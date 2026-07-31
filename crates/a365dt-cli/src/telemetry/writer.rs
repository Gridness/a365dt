use std::time::Duration;

use tokio::{
	sync::{mpsc, oneshot},
	task::{JoinError, JoinHandle},
	time::sleep,
};

use super::{
	Error, InvocationId, Observation, Paths, Recorder, Snapshot, snapshot,
	snapshot::Overhead, storage::Store,
};

const BATCH_INTERVAL: Duration = Duration::from_secs(1);

pub struct Writer {
	invocation_id: InvocationId,
	state: State,
}

enum State {
	Ready {
		store: Store,
		background: Background,
	},
	Unavailable(Error),
}

enum Background {
	Running {
		finish: oneshot::Sender<()>,
		task: JoinHandle<Result<(), Error>>,
	},
	Disabled,
}

impl Writer {
	pub async fn open(invocation_id: InvocationId) -> (Recorder, Self) {
		let result = match Paths::discover() {
			Ok(paths) => Self::at(paths, invocation_id).await,
			Err(error) => Err(error),
		};
		match result {
			Ok(owner) => owner,
			Err(error) => (
				Recorder::default(),
				Self {
					invocation_id,
					state: State::Unavailable(error),
				},
			),
		}
	}

	pub(super) async fn at(
		paths: Paths,
		invocation_id: InvocationId,
	) -> Result<(Recorder, Self), Error> {
		let store = Store::open(paths).await?;
		let state = store.collection_state().await?;
		let (recorder, background) = if state.enabled {
			let (observations, receiver) = mpsc::unbounded_channel();
			let (finish, finishing) = oneshot::channel();
			(
				Recorder::connected(invocation_id, observations),
				Background::Running {
					finish,
					task: tokio::spawn(run(
						receiver,
						finishing,
						store.clone(),
						state.last_cleared_at_ms,
					)),
				},
			)
		} else {
			(Recorder::default(), Background::Disabled)
		};
		Ok((
			recorder,
			Self {
				invocation_id,
				state: State::Ready { store, background },
			},
		))
	}

	pub fn initialization_warning(&self) -> Option<Error> {
		match &self.state {
			State::Ready { store, .. } => store.warning(),
			State::Unavailable(error) => Some(error.clone()),
		}
	}

	pub async fn snapshot(&self) -> Result<Snapshot, Error> {
		match &self.state {
			State::Ready { store, .. } => snapshot::capture(store).await,
			State::Unavailable(error) => Err(error.clone()),
		}
	}

	pub fn benchmark_overhead(&self) -> Overhead {
		snapshot::benchmark_overhead(self.invocation_id)
	}

	pub async fn finish(self) -> Result<(), Error> {
		match self.state {
			State::Ready { store, background } => {
				let result = match background {
					Background::Running { finish, task } => {
						let _ = finish.send(());
						match task.await {
							Ok(result) => result,
							Err(error) => Err(writer_stopped(error)),
						}
					}
					Background::Disabled => Ok(()),
				};
				store.close().await;
				result
			}
			State::Unavailable(_) => Ok(()),
		}
	}
}

async fn run(
	mut observations: mpsc::UnboundedReceiver<Observation>,
	mut finishing: oneshot::Receiver<()>,
	store: Store,
	mut watermark: Option<u64>,
) -> Result<(), Error> {
	let mut first_error = None;
	while let Some((batch, closed)) =
		receive_batch(&mut observations, &mut finishing).await
	{
		remember_failure(
			store.commit(&mut watermark, batch).await,
			&mut first_error,
		);
		if closed {
			break;
		}
	}
	first_error.map_or(Ok(()), Err)
}

async fn receive_batch(
	observations: &mut mpsc::UnboundedReceiver<Observation>,
	finishing: &mut oneshot::Receiver<()>,
) -> Option<(Vec<Observation>, bool)> {
	let first = tokio::select! {
		observation = observations.recv() => observation?,
		_ = &mut *finishing => {
			observations.close();
			let mut batch = Vec::new();
			while let Some(observation) = observations.recv().await {
				batch.push(observation);
			}
			return (!batch.is_empty()).then_some((batch, true));
		}
	};
	let mut batch = vec![first];
	let timer = sleep(BATCH_INTERVAL);
	tokio::pin!(timer);
	let closed = loop {
		tokio::select! {
			biased;
			_ = &mut *finishing => {
				observations.close();
				while let Some(observation) = observations.recv().await {
					batch.push(observation);
				}
				break true;
			},
			() = &mut timer => break false,
			observation = observations.recv() => match observation {
				Some(observation) => batch.push(observation),
				None => break true,
			},
		}
	};
	Some((batch, closed))
}

fn remember_failure(
	result: Result<(), Error>,
	first_error: &mut Option<Error>,
) {
	if let Err(error) = result
		&& first_error.is_none()
	{
		*first_error = Some(error);
	}
}

fn writer_stopped(error: JoinError) -> Error {
	Error::with_debug("The local telemetry writer stopped unexpectedly.", error)
}

#[cfg(test)]
#[path = "writer_tests.rs"]
mod tests;
