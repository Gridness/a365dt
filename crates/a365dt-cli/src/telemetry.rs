use std::{
	collections::BTreeMap,
	fs::{self, File, OpenOptions},
	io,
	path::PathBuf,
	time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{app_files, download::Status, error::Error, ui};

mod display;
mod performance;
mod recording;
mod snapshot;
mod writer;

pub(crate) use display::format_timestamp;
use performance::{Performance, Work};
use recording::now_ms;
pub(crate) use recording::{
	CatalogueUse, Command, CommandOutcome, InvocationId, Operation, Recorder,
};
use recording::{Observation, ObservationKind};
pub(crate) use snapshot::{PerformanceMetric, Snapshot};
pub(crate) use writer::Writer;

const SCHEMA_VERSION: u16 = 1;
const SAMPLE_LIMIT: usize = 101;

#[derive(Clone, Debug)]
pub(super) struct Paths {
	data: PathBuf,
	disabled: PathBuf,
	lock: PathBuf,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub(super) struct Stats {
	schema_version: u16,
	last_enabled_at: Option<u64>,
	last_disabled_at: Option<u64>,
	last_cleared_at: Option<u64>,
	last_cleared_at_ms: Option<u64>,
	usage: Usage,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub(super) struct Usage {
	first_recorded_at: Option<u64>,
	last_recorded_at: Option<u64>,
	first_download_at: Option<u64>,
	last_download_at: Option<u64>,
	counters: BTreeMap<String, u64>,
	samples: BTreeMap<String, Vec<u64>>,
	performance: Performance,
}

impl Default for Stats {
	fn default() -> Self {
		Self {
			schema_version: SCHEMA_VERSION,
			last_enabled_at: None,
			last_disabled_at: None,
			last_cleared_at: None,
			last_cleared_at_ms: None,
			usage: Usage::default(),
		}
	}
}

impl Paths {
	fn discover() -> Result<Self, Error> {
		let directories = app_files::directories().ok_or_else(|| {
			Error::new("Could not resolve the local telemetry directory.")
		})?;
		let data_directory = directories.data_local_dir();
		Ok(Self {
			data: data_directory.join("telemetry.json"),
			lock: data_directory.join("telemetry.lock"),
			disabled: directories.config_dir().join("telemetry-disabled"),
		})
	}
}

impl Stats {
	fn new(enabled: bool, disabled_at: Option<u64>) -> Self {
		Self {
			last_enabled_at: enabled.then(now),
			last_disabled_at: disabled_at,
			..Self::default()
		}
	}
}

impl Usage {
	fn record_observation(&mut self, observation: Observation) {
		let at = observation.observed_at_ms / 1_000;
		match observation.kind {
			ObservationKind::Command { command, outcome } => {
				self.record_command(command, outcome, at);
			}
			ObservationKind::SeriesSelection { catalogue, .. } => {
				if let Some(catalogue) = catalogue {
					self.record_catalogue(catalogue, at);
				} else {
					self.touch(at);
				}
			}
			ObservationKind::DownloadBatch {
				duration_us,
				outcomes,
				..
			} => self.record_download(duration_us, outcomes, at),
			ObservationKind::Performance {
				operation,
				duration_us,
				work_units,
			} => {
				self.performance.record(
					operation.name(),
					Duration::from_micros(duration_us),
					work_units.map_or(Work::None, Work::Items),
				);
				self.touch(at);
			}
		}
	}

	fn record_command(
		&mut self,
		command: Command,
		outcome: CommandOutcome,
		at: u64,
	) {
		self.increment(
			format!(
				"commands.{}.{}",
				command.name().replace(' ', "."),
				outcome.name()
			),
			1,
		);
		self.touch(at);
	}

	fn record_catalogue(&mut self, usage: CatalogueUse, at: u64) {
		let counter = match usage {
			CatalogueUse::Bypassed => return,
			CatalogueUse::Hit => "catalogue.hits",
			CatalogueUse::Miss => "catalogue.misses",
		};
		self.increment(counter, 1);
		self.touch(at);
	}

	fn record_download(
		&mut self,
		duration_us: u64,
		outcomes: Vec<recording::DownloadOutcome>,
		at: u64,
	) {
		self.increment("downloads.batches", 1);
		for outcome in outcomes {
			let counter = match outcome.status {
				Status::Downloaded => {
					self.increment(
						"downloads.bytes",
						outcome.bytes.unwrap_or_default(),
					);
					"downloads.episodes.downloaded"
				}
				Status::Skipped => "downloads.episodes.skipped",
				Status::Failed => "downloads.episodes.failed",
				Status::MuxFailed => "downloads.episodes.mux_failed",
				Status::Interrupted => "downloads.episodes.interrupted",
			};
			self.increment(counter, 1);
		}
		push_sample(
			self.samples
				.entry("downloads.batch_duration_ms".into())
				.or_default(),
			duration_us / 1_000,
		);
		self.first_download_at = earliest(self.first_download_at, Some(at));
		self.last_download_at = latest(self.last_download_at, Some(at));
		self.touch(at);
	}

	fn increment(&mut self, key: impl Into<String>, value: u64) {
		let current = self.counters.entry(key.into()).or_default();
		*current = current.saturating_add(value);
	}

	fn touch(&mut self, at: u64) {
		self.first_recorded_at = earliest(self.first_recorded_at, Some(at));
		self.last_recorded_at = latest(self.last_recorded_at, Some(at));
	}
}

pub fn show(invocation_id: InvocationId) -> Result<(), Error> {
	let paths = Paths::discover()?;
	let disabled = is_disabled(&paths)?;
	let stats = read_stats_locked(&paths, !disabled)?;
	display::print(&paths, &stats, disabled);
	if !disabled {
		commit_observations(
			&paths,
			vec![Observation::command(
				invocation_id,
				Command::TelemetryShow,
				CommandOutcome::Success,
			)],
		)?;
	}
	Ok(())
}

pub fn clear() -> Result<(), Error> {
	let paths = Paths::discover()?;
	clear_at(&paths)?;
	ui::success("Local telemetry cleared");
	Ok(())
}

fn clear_at(paths: &Paths) -> Result<(), Error> {
	let disabled = is_disabled(paths)?;
	let disabled_at = marker_timestamp(paths);
	let _lock = lock(paths)?;
	let mut stats = read_stats(paths, !disabled)
		.unwrap_or_else(|_| Stats::new(!disabled, disabled_at));
	stats.usage = Usage::default();
	let cleared_at_ms = now_ms();
	stats.last_cleared_at = Some(cleared_at_ms / 1_000);
	stats.last_cleared_at_ms = Some(cleared_at_ms);
	write_stats(paths, &stats)?;
	Ok(())
}

pub fn disable(invocation_id: InvocationId) -> Result<(), Error> {
	let paths = Paths::discover()?;
	disable_at(&paths, invocation_id)?;
	ui::success("Local telemetry disabled");
	Ok(())
}

fn disable_at(paths: &Paths, invocation_id: InvocationId) -> Result<(), Error> {
	let already_disabled = is_disabled(paths)?;
	let at = now();
	write_marker(paths, at)?;
	update_stats(paths, false, |stats| {
		if !already_disabled {
			stats.usage.record_observation(Observation::command(
				invocation_id,
				Command::TelemetryDisable,
				CommandOutcome::Success,
			));
			stats.last_enabled_at.get_or_insert(at);
		}
		stats.last_disabled_at = Some(at);
	})?;
	Ok(())
}

pub fn enable(invocation_id: InvocationId) -> Result<(), Error> {
	let paths = Paths::discover()?;
	enable_at(&paths, invocation_id)?;
	ui::success("Local telemetry enabled");
	Ok(())
}

fn enable_at(paths: &Paths, invocation_id: InvocationId) -> Result<(), Error> {
	let at = now();
	update_stats(paths, false, |stats| {
		stats.last_enabled_at = Some(at);
		stats.usage.record_observation(Observation::command(
			invocation_id,
			Command::TelemetryEnable,
			CommandOutcome::Success,
		));
	})?;
	remove_marker(paths)?;
	Ok(())
}

fn read_stats_locked(paths: &Paths, enabled: bool) -> Result<Stats, Error> {
	let _lock = lock(paths)?;
	read_stats(paths, enabled)
}

fn update_stats(
	paths: &Paths,
	enabled: bool,
	update: impl FnOnce(&mut Stats),
) -> Result<(), Error> {
	let _lock = lock(paths)?;
	let mut stats = read_stats(paths, enabled)?;
	update(&mut stats);
	write_stats(paths, &stats)
}

fn commit_observations(
	paths: &Paths,
	observations: Vec<Observation>,
) -> Result<(), Error> {
	let _lock = lock(paths)?;
	if is_disabled(paths)? {
		return Ok(());
	}
	let mut stats = read_stats(paths, true)?;
	let watermark = stats.last_cleared_at_ms.or_else(|| {
		stats
			.last_cleared_at
			.map(|timestamp| timestamp.saturating_mul(1_000))
	});
	for observation in observations.into_iter().filter(|observation| {
		watermark.is_none_or(|at| observation.observed_at_ms > at)
	}) {
		stats.usage.record_observation(observation);
	}
	write_stats(paths, &stats)
}

fn lock(paths: &Paths) -> Result<File, Error> {
	let Some(directory) = paths.lock.parent() else {
		return Err(Error::new(
			"Could not resolve the local telemetry directory.",
		));
	};
	fs::create_dir_all(directory).map_err(|error| {
		storage_error("Could not create the local telemetry directory.", error)
	})?;
	let file = OpenOptions::new()
		.create(true)
		.truncate(false)
		.read(true)
		.write(true)
		.open(&paths.lock)
		.map_err(|error| {
			storage_error("Could not open the local telemetry lock.", error)
		})?;
	file.lock().map_err(|error| {
		storage_error("Could not lock the local telemetry.", error)
	})?;
	Ok(file)
}

fn read_stats(paths: &Paths, enabled: bool) -> Result<Stats, Error> {
	let contents = match fs::read(&paths.data) {
		Ok(contents) => contents,
		Err(error) if error.kind() == io::ErrorKind::NotFound => {
			return Ok(Stats::new(enabled, marker_timestamp(paths)));
		}
		Err(error) => {
			return Err(storage_error(
				"Could not read the local telemetry.",
				error,
			));
		}
	};
	let stats: Stats = serde_json::from_slice(&contents).map_err(|error| {
		Error::with_debug(
			"Could not read the local telemetry because it is invalid. Run `a365dt telemetry clear` to reset it.",
			error,
		)
	})?;
	if stats.schema_version != SCHEMA_VERSION {
		return Err(Error::new(format!(
			"Local telemetry schema {} is unsupported. Run `a365dt telemetry clear` to reset it.",
			stats.schema_version
		)));
	}
	Ok(stats)
}

fn write_stats(paths: &Paths, stats: &Stats) -> Result<(), Error> {
	let Some(directory) = paths.data.parent() else {
		return Err(Error::new(
			"Could not resolve the local telemetry directory.",
		));
	};
	fs::create_dir_all(directory).map_err(|error| {
		storage_error("Could not create the local telemetry directory.", error)
	})?;
	let contents = serde_json::to_vec(stats).map_err(|error| {
		Error::with_debug("Could not prepare the local telemetry.", error)
	})?;
	// ponytail: telemetry tolerates a lost file on a crash; add atomic
	// replacement if telemetry ever becomes recovery-critical.
	fs::write(&paths.data, contents).map_err(|error| {
		storage_error("Could not store the local telemetry.", error)
	})
}

fn is_disabled(paths: &Paths) -> Result<bool, Error> {
	paths.disabled.try_exists().map_err(|error| {
		storage_error("Could not inspect the local telemetry opt-out.", error)
	})
}

fn marker_timestamp(paths: &Paths) -> Option<u64> {
	fs::read_to_string(&paths.disabled)
		.ok()
		.and_then(|contents| contents.parse().ok())
}

fn write_marker(paths: &Paths, at: u64) -> Result<(), Error> {
	let Some(directory) = paths.disabled.parent() else {
		return Err(Error::new(
			"Could not resolve the local telemetry opt-out directory.",
		));
	};
	fs::create_dir_all(directory).map_err(|error| {
		storage_error(
			"Could not create the local telemetry opt-out directory.",
			error,
		)
	})?;
	fs::write(&paths.disabled, at.to_string()).map_err(|error| {
		storage_error("Could not disable the local telemetry.", error)
	})
}

fn remove_marker(paths: &Paths) -> Result<(), Error> {
	match fs::remove_file(&paths.disabled) {
		Ok(()) => Ok(()),
		Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
		Err(error) => Err(storage_error(
			"Could not enable the local telemetry.",
			error,
		)),
	}
}

fn storage_error(message: &str, error: io::Error) -> Error {
	Error::with_debug(message, error)
}

fn push_sample(samples: &mut Vec<u64>, value: u64) {
	if samples.len() >= SAMPLE_LIMIT {
		let excess = samples.len() + 1 - SAMPLE_LIMIT;
		samples.drain(..excess);
	}
	samples.push(value);
}

fn earliest(left: Option<u64>, right: Option<u64>) -> Option<u64> {
	left.into_iter().chain(right).min()
}

fn latest(left: Option<u64>, right: Option<u64>) -> Option<u64> {
	left.into_iter().chain(right).max()
}

fn now() -> u64 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap_or_default()
		.as_secs()
}

#[cfg(test)]
#[path = "telemetry_tests.rs"]
mod tests;
