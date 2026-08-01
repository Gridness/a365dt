use std::{
	fmt,
	time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use tokio::sync::mpsc;
use uuid::Uuid;

use crate::{
	api::Series,
	download::{self, Status},
};

#[derive(Clone, Copy)]
enum Work {
	Items(u64),
	None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
	CachePrune,
	Doctor,
	Download,
	Stats,
	TelemetryDisable,
	TelemetryEnable,
	TelemetryShow,
	Update,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandOutcome {
	Cancelled,
	Failure,
	Success,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogueUse {
	Bypassed,
	Hit,
	Miss,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
	ApiEmbed,
	ApiSearch,
	ApiSeries,
	ApiSeriesPage,
	ApiTranslations,
	ApiValidate,
	AssetGet,
	AssetHead,
	AssetResume,
	CacheRetrieve,
	CacheStore,
	SearchIndex,
	SearchRank,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvocationId(Uuid);

#[derive(Clone, Debug, Default)]
pub struct Recorder {
	invocation_id: Option<InvocationId>,
	observations: Option<mpsc::UnboundedSender<Observation>>,
}

pub struct Measurement<'a> {
	recorder: &'a Recorder,
	operation: Operation,
	started: Option<Instant>,
	work: Work,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct Observation {
	pub invocation_id: InvocationId,
	pub observed_at_ms: u64,
	pub kind: ObservationKind,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) enum ObservationKind {
	Command {
		command: Command,
		outcome: CommandOutcome,
	},
	SeriesSelection {
		series_id: u64,
		series_title: String,
		catalogue: Option<CatalogueUse>,
	},
	DownloadBatch {
		series_id: u64,
		series_title: String,
		duration_us: u64,
		outcomes: Vec<DownloadOutcome>,
	},
	Performance {
		operation: Operation,
		duration_us: u64,
		work_units: Option<u64>,
	},
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct DownloadOutcome {
	pub status: Status,
	pub bytes: Option<u64>,
}

impl InvocationId {
	pub fn new() -> Self {
		Self(Uuid::now_v7())
	}
}

impl Observation {
	pub(super) fn command(
		invocation_id: InvocationId,
		command: Command,
		outcome: CommandOutcome,
	) -> Self {
		Self {
			invocation_id,
			observed_at_ms: now_ms(),
			kind: ObservationKind::Command { command, outcome },
		}
	}
}

impl fmt::Display for InvocationId {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.hyphenated().fmt(formatter)
	}
}

impl Command {
	pub(super) fn database_name(self) -> &'static str {
		match self {
			Self::CachePrune => "cache_prune",
			Self::Doctor => "doctor",
			Self::Download => "download",
			Self::Stats => "stats",
			Self::TelemetryDisable => "telemetry_disable",
			Self::TelemetryEnable => "telemetry_enable",
			Self::TelemetryShow => "telemetry_show",
			Self::Update => "update",
		}
	}
}

impl CommandOutcome {
	pub(super) fn name(self) -> &'static str {
		match self {
			Self::Cancelled => "cancelled",
			Self::Failure => "failure",
			Self::Success => "success",
		}
	}
}

impl CatalogueUse {
	pub(super) fn database_name(self) -> Option<&'static str> {
		match self {
			Self::Bypassed => None,
			Self::Hit => Some("hit"),
			Self::Miss => Some("miss"),
		}
	}
}

impl Operation {
	pub(super) fn name(self) -> &'static str {
		match self {
			Self::ApiEmbed => "request.api.embed",
			Self::ApiSearch => "request.api.search",
			Self::ApiSeries => "request.api.series",
			Self::ApiSeriesPage => "request.api.series_page",
			Self::ApiTranslations => "request.api.translations",
			Self::ApiValidate => "request.api.validate",
			Self::AssetGet => "request.asset.get",
			Self::AssetHead => "request.asset.head",
			Self::AssetResume => "request.asset.resume",
			Self::CacheRetrieve => "cache.retrieve",
			Self::CacheStore => "cache.store",
			Self::SearchIndex => "search.index",
			Self::SearchRank => "search.rank",
		}
	}
}

impl Recorder {
	pub(super) fn connected(
		invocation_id: InvocationId,
		observations: mpsc::UnboundedSender<Observation>,
	) -> Self {
		Self {
			invocation_id: Some(invocation_id),
			observations: Some(observations),
		}
	}

	pub fn record_command(&self, command: Command, outcome: CommandOutcome) {
		self.record(ObservationKind::Command { command, outcome });
	}

	pub fn record_series(&self, series: &Series, catalogue: CatalogueUse) {
		self.record(ObservationKind::SeriesSelection {
			series_id: series.id,
			series_title: series.title.clone(),
			catalogue: match catalogue {
				CatalogueUse::Bypassed => None,
				CatalogueUse::Hit | CatalogueUse::Miss => Some(catalogue),
			},
		});
	}

	pub fn record_download(
		&self,
		series: &Series,
		summary: &download::Summary,
	) {
		self.record(ObservationKind::DownloadBatch {
			series_id: series.id,
			series_title: series.title.clone(),
			duration_us: u64::try_from(summary.elapsed.as_micros())
				.unwrap_or(u64::MAX),
			outcomes: summary
				.outcomes
				.iter()
				.map(|outcome| DownloadOutcome {
					status: outcome.status,
					bytes: (outcome.status == Status::Downloaded)
						.then_some(outcome.bytes),
				})
				.collect(),
		});
	}

	pub fn measure(&self, operation: Operation) -> Measurement<'_> {
		Measurement {
			recorder: self,
			operation,
			started: self.observations.as_ref().map(|_| Instant::now()),
			work: Work::None,
		}
	}

	pub fn measure_items(
		&self,
		operation: Operation,
		items: usize,
	) -> Measurement<'_> {
		Measurement {
			recorder: self,
			operation,
			started: self.observations.as_ref().map(|_| Instant::now()),
			work: Work::Items(u64::try_from(items).unwrap_or(u64::MAX)),
		}
	}

	fn record_performance(
		&self,
		operation: Operation,
		duration: Duration,
		work: Work,
	) {
		self.record(ObservationKind::Performance {
			operation,
			duration_us: u64::try_from(duration.as_micros())
				.unwrap_or(u64::MAX),
			work_units: match work {
				Work::Items(items) => Some(items),
				Work::None => None,
			},
		});
	}

	fn record(&self, kind: ObservationKind) {
		let (Some(invocation_id), Some(observations)) =
			(self.invocation_id, &self.observations)
		else {
			return;
		};
		let _ = observations.send(Observation {
			invocation_id,
			observed_at_ms: now_ms(),
			kind,
		});
	}
}

impl Drop for Measurement<'_> {
	fn drop(&mut self) {
		if let Some(started) = self.started {
			let elapsed = started.elapsed();
			self.recorder.record_performance(
				self.operation,
				elapsed,
				self.work,
			);
		}
	}
}

pub(super) fn now_ms() -> u64 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap_or_default()
		.as_millis()
		.try_into()
		.unwrap_or(u64::MAX)
}
