use std::collections::{HashMap, HashSet};

use tokio::{
	sync::mpsc,
	task::{JoinError, JoinHandle},
};

use super::{Catalogue, Store};
use crate::{
	api::Series,
	error::Error,
	telemetry::{Operation, Recorder},
};

pub(crate) struct LoadedCatalogue {
	catalogue: Catalogue,
	state: WriterState,
}

pub(crate) struct Writer {
	messages: mpsc::UnboundedSender<Message>,
	task: JoinHandle<Result<(), Error>>,
}

enum Message {
	Discover(Vec<Series>),
	RememberAlias { query: String, series: Series },
	RemoveMissing(u64),
	CommitRefresh(Vec<Series>),
}

struct State {
	base_revision: i64,
	revisions: HashMap<u64, i64>,
	series: HashSet<u64>,
}

enum WriterState {
	Available(State),
	Unavailable,
}

impl LoadedCatalogue {
	pub(super) fn new(
		catalogue: Catalogue,
		base_revision: i64,
		revisions: HashMap<u64, i64>,
	) -> Self {
		Self {
			state: WriterState::Available(State {
				base_revision,
				series: revisions.keys().copied().collect(),
				revisions,
			}),
			catalogue,
		}
	}

	pub(crate) fn unavailable() -> Self {
		Self {
			catalogue: Catalogue::default(),
			state: WriterState::Unavailable,
		}
	}

	pub(crate) fn into_session(
		self,
		store: &Store,
		telemetry: Recorder,
	) -> (Catalogue, Writer) {
		let writer = Writer::start(store.clone(), self.state, telemetry);
		(self.catalogue, writer)
	}
}

impl Writer {
	fn start(store: Store, state: WriterState, telemetry: Recorder) -> Self {
		let (messages, receiver) = mpsc::unbounded_channel();
		let task = tokio::spawn(run(receiver, store, state, telemetry));
		Self { messages, task }
	}

	pub(crate) fn discover(&self, series: Vec<Series>) {
		let _ = self.messages.send(Message::Discover(series));
	}

	pub(crate) fn remember_alias(&self, query: String, series: Series) {
		let _ = self.messages.send(Message::RememberAlias { query, series });
	}

	pub(crate) fn remove_missing(&self, series_id: u64) {
		let _ = self.messages.send(Message::RemoveMissing(series_id));
	}

	pub(crate) fn commit_refresh(&self, ordered_series: Vec<Series>) {
		let _ = self.messages.send(Message::CommitRefresh(ordered_series));
	}

	pub(crate) async fn finish(self) -> Result<(), Error> {
		drop(self.messages);
		self.task.await.map_err(writer_stopped)?
	}
}

async fn run(
	mut messages: mpsc::UnboundedReceiver<Message>,
	store: Store,
	state: WriterState,
	telemetry: Recorder,
) -> Result<(), Error> {
	let WriterState::Available(mut state) = state else {
		while messages.recv().await.is_some() {}
		return Ok(());
	};
	let mut first_error = None;
	while let Some(message) = messages.recv().await {
		let _measurement =
			telemetry.measure_items(Operation::CacheStore, state.series.len());
		let result = state.apply(message, &store).await;
		if let Err(error) = result
			&& first_error.is_none()
		{
			first_error = Some(error);
		}
	}
	first_error.map_or(Ok(()), Err)
}

impl State {
	async fn apply(
		&mut self,
		message: Message,
		store: &Store,
	) -> Result<(), Error> {
		match message {
			Message::Discover(series) => {
				let ids = series
					.iter()
					.map(|series| series.id)
					.collect::<HashSet<_>>();
				if let Some(revision) = store.discover(series).await? {
					for id in ids {
						self.series.insert(id);
						self.revisions.insert(id, revision);
					}
				}
			}
			Message::RememberAlias { query, series } => {
				let id = series.id;
				if let Some(revision) =
					store.remember_alias(query, series).await?
				{
					self.series.insert(id);
					self.revisions.insert(id, revision);
				}
			}
			Message::RemoveMissing(series_id) => {
				store
					.remove_missing(
						series_id,
						self.revisions.get(&series_id).copied(),
					)
					.await?;
				self.series.remove(&series_id);
				self.revisions.remove(&series_id);
			}
			Message::CommitRefresh(series) => {
				let ids = series
					.iter()
					.map(|series| series.id)
					.collect::<HashSet<_>>();
				if let Some(revision) =
					store.commit_refresh(series, self.base_revision).await?
				{
					self.base_revision = revision;
					self.series.extend(&ids);
					for id in self.series.iter().copied() {
						self.revisions.insert(id, revision);
					}
				}
			}
		}
		Ok(())
	}
}

fn writer_stopped(error: JoinError) -> Error {
	Error::with_debug("The local cache writer stopped unexpectedly.", error)
}
