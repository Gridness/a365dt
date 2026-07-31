use std::time::Duration;

use tokio::{
	sync::{mpsc, oneshot},
	task::{JoinError, JoinHandle},
	time::sleep,
};

use super::{
	Error, InvocationId, Observation, Paths, Recorder, Snapshot,
	commit_observations, is_disabled, read_stats_locked, snapshot,
	snapshot::Overhead,
};

const BATCH_INTERVAL: Duration = Duration::from_secs(1);

pub struct Writer {
	invocation_id: InvocationId,
	state: State,
}

enum State {
	Ready {
		paths: Paths,
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
	pub fn open(invocation_id: InvocationId) -> (Recorder, Self) {
		match Paths::discover().and_then(|paths| Self::at(paths, invocation_id))
		{
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

	pub(super) fn at(
		paths: Paths,
		invocation_id: InvocationId,
	) -> Result<(Recorder, Self), Error> {
		let enabled = !is_disabled(&paths)?;
		read_stats_locked(&paths, enabled)?;
		let (recorder, background) = if enabled {
			let (observations, receiver) = mpsc::unbounded_channel();
			let (finish, finishing) = oneshot::channel();
			(
				Recorder::connected(invocation_id, observations),
				Background::Running {
					finish,
					task: tokio::spawn(run(receiver, finishing, paths.clone())),
				},
			)
		} else {
			(Recorder::default(), Background::Disabled)
		};
		Ok((
			recorder,
			Self {
				invocation_id,
				state: State::Ready { paths, background },
			},
		))
	}

	pub fn initialization_warning(&self) -> Option<&Error> {
		match &self.state {
			State::Ready { .. } => None,
			State::Unavailable(error) => Some(error),
		}
	}

	pub fn snapshot(&self) -> Result<Snapshot, Error> {
		match &self.state {
			State::Ready { paths, .. } => snapshot::capture(paths),
			State::Unavailable(error) => Err(error.clone()),
		}
	}

	pub fn benchmark_overhead(&self) -> Overhead {
		snapshot::benchmark_overhead(self.invocation_id)
	}

	pub async fn finish(self) -> Result<(), Error> {
		match self.state {
			State::Ready {
				background: Background::Running { finish, task },
				..
			} => {
				let _ = finish.send(());
				task.await.map_err(writer_stopped)?
			}
			State::Ready {
				background: Background::Disabled,
				..
			}
			| State::Unavailable(_) => Ok(()),
		}
	}
}

async fn run(
	mut observations: mpsc::UnboundedReceiver<Observation>,
	mut finishing: oneshot::Receiver<()>,
	paths: Paths,
) -> Result<(), Error> {
	let mut first_error = None;
	loop {
		let first = tokio::select! {
			observation = observations.recv() => {
				let Some(observation) = observation else {
					break;
				};
				observation
			}
			_ = &mut finishing => {
				observations.close();
				let mut batch = Vec::new();
				while let Some(observation) = observations.recv().await {
					batch.push(observation);
				}
				if !batch.is_empty() {
					remember_failure(
						commit_observations(&paths, batch),
						&mut first_error,
					);
				}
				break;
			}
		};
		let mut batch = vec![first];
		let timer = sleep(BATCH_INTERVAL);
		tokio::pin!(timer);
		let closed = loop {
			tokio::select! {
				biased;
				_ = &mut finishing => {
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
		remember_failure(commit_observations(&paths, batch), &mut first_error);
		if closed {
			break;
		}
	}
	first_error.map_or(Ok(()), Err)
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
