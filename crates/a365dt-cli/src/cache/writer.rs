use std::collections::HashSet;

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
	catalogue: Catalogue,
	discovered: HashSet<u64>,
}

enum WriterState {
	Available(State),
	Unavailable,
}

impl LoadedCatalogue {
	pub(super) fn new(catalogue: Catalogue) -> Self {
		Self {
			state: WriterState::Available(State {
				catalogue: catalogue.clone(),
				discovered: HashSet::new(),
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
		state.apply(message);
		let _measurement = telemetry
			.measure_items(Operation::CacheStore, state.catalogue.len());
		if let Err(error) = store.save_catalogue(&state.catalogue).await
			&& first_error.is_none()
		{
			first_error = Some(error);
		}
	}
	first_error.map_or(Ok(()), Err)
}

impl State {
	fn apply(&mut self, message: Message) {
		match message {
			Message::Discover(series) => {
				self.discovered
					.extend(series.iter().map(|series| series.id));
				self.catalogue.upsert(series);
			}
			Message::RememberAlias { query, series } => {
				self.discovered.insert(series.id);
				self.catalogue.upsert(vec![series.clone()]);
				self.catalogue.remember_alias(&query, series.id);
			}
			Message::RemoveMissing(series_id) => {
				self.discovered.remove(&series_id);
				self.catalogue.remove_series(series_id);
			}
			Message::CommitRefresh(series) => {
				self.catalogue.merge_refresh(
					Catalogue::refreshed(series),
					&self.discovered,
				);
			}
		}
	}
}

fn writer_stopped(error: JoinError) -> Error {
	Error::with_debug("The local cache writer stopped unexpectedly.", error)
}
