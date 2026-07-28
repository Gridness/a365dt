use std::{
	collections::BTreeMap,
	fs::{self, File, OpenOptions},
	io,
	path::PathBuf,
	sync::{Arc, Mutex},
	time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{
	app_files,
	download::{self, Status},
	error::Error,
	l10n::{tr, tr_args},
	ui,
};

mod display;
mod performance;
mod snapshot;

pub(crate) use display::format_timestamp;
use performance::{Performance, Work};
pub(crate) use snapshot::{
	PerformanceMetric, Snapshot, benchmark_overhead, capture as snapshot,
};

const SCHEMA_VERSION: u16 = 1;
const SAMPLE_LIMIT: usize = 101;

#[derive(Clone, Copy)]
pub enum Command {
	CachePrune,
	Completions,
	Doctor,
	Download,
	TelemetryDisable,
	TelemetryEnable,
	TelemetryShow,
}

#[derive(Clone, Copy)]
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

#[derive(Clone, Copy)]
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

#[derive(Clone, Debug, Default)]
pub struct Recorder {
	enabled: bool,
	paths: Option<Paths>,
	usage: Arc<Mutex<Usage>>,
}

pub struct Measurement<'a> {
	recorder: &'a Recorder,
	operation: Operation,
	started: Option<Instant>,
	work: Work,
}

#[derive(Clone, Debug)]
struct Paths {
	data: PathBuf,
	disabled: PathBuf,
	lock: PathBuf,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
struct Stats {
	schema_version: u16,
	last_enabled_at: Option<u64>,
	last_disabled_at: Option<u64>,
	last_cleared_at: Option<u64>,
	usage: Usage,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
struct Usage {
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
			usage: Usage::default(),
		}
	}
}

impl Command {
	fn name(self) -> &'static str {
		match self {
			Self::CachePrune => "cache prune",
			Self::Completions => "completions",
			Self::Doctor => "doctor",
			Self::Download => "download",
			Self::TelemetryDisable => "telemetry disable",
			Self::TelemetryEnable => "telemetry enable",
			Self::TelemetryShow => "telemetry show",
		}
	}
}

impl CommandOutcome {
	fn name(self) -> &'static str {
		match self {
			Self::Cancelled => "cancelled",
			Self::Failure => "failure",
			Self::Success => "success",
		}
	}
}

impl Operation {
	fn name(self) -> &'static str {
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
	pub fn new() -> Result<Self, Error> {
		Self::from_paths(Paths::discover()?)
	}

	fn from_paths(paths: Paths) -> Result<Self, Error> {
		Ok(Self {
			enabled: !is_disabled(&paths)?,
			paths: Some(paths),
			usage: Arc::default(),
		})
	}

	pub fn record_command(&self, command: Command, outcome: CommandOutcome) {
		if self.enabled {
			self.usage
				.lock()
				.unwrap()
				.record_command(command, outcome, now());
		}
	}

	pub fn record_catalogue(&self, usage: CatalogueUse) {
		if self.enabled {
			self.usage.lock().unwrap().record_catalogue(usage, now());
		}
	}

	pub fn record_download(&self, summary: &download::Summary) {
		if self.enabled {
			self.usage.lock().unwrap().record_download(summary, now());
		}
	}

	pub fn measure(&self, operation: Operation) -> Measurement<'_> {
		Measurement {
			recorder: self,
			operation,
			started: self.enabled.then(Instant::now),
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
			started: self.enabled.then(Instant::now),
			work: Work::Items(u64::try_from(items).unwrap_or(u64::MAX)),
		}
	}

	fn record_performance(
		&self,
		operation: Operation,
		duration: Duration,
		work: Work,
	) {
		self.usage.lock().unwrap().performance.record(
			operation.name(),
			duration,
			work,
		);
	}

	pub fn flush(&self) -> Result<(), Error> {
		let Some(paths) = &self.paths else {
			return Ok(());
		};
		let usage = self.usage.lock().unwrap().clone();
		if !self.enabled || usage == Usage::default() {
			return Ok(());
		}
		if is_disabled(paths)? {
			return Ok(());
		}
		update_stats(paths, true, |stats| stats.usage.merge(&usage))
	}
}

impl Drop for Measurement<'_> {
	fn drop(&mut self) {
		if let Some(started) = self.started {
			self.recorder.record_performance(
				self.operation,
				started.elapsed(),
				self.work,
			);
		}
	}
}

impl Paths {
	fn discover() -> Result<Self, Error> {
		let directories = app_files::directories()
			.ok_or_else(|| Error::new(tr("telemetry-directory-error")))?;
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

	fn record_download(&mut self, summary: &download::Summary, at: u64) {
		self.increment("downloads.batches", 1);
		for outcome in &summary.outcomes {
			let counter = match outcome.status {
				Status::Downloaded => {
					self.increment("downloads.bytes", outcome.bytes);
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
			u64::try_from(summary.elapsed.as_millis()).unwrap_or(u64::MAX),
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

	fn merge(&mut self, newer: &Self) {
		self.first_recorded_at =
			earliest(self.first_recorded_at, newer.first_recorded_at);
		self.last_recorded_at =
			latest(self.last_recorded_at, newer.last_recorded_at);
		self.first_download_at =
			earliest(self.first_download_at, newer.first_download_at);
		self.last_download_at =
			latest(self.last_download_at, newer.last_download_at);
		for (key, value) in &newer.counters {
			self.increment(key.clone(), *value);
		}
		for (key, samples) in &newer.samples {
			let current = self.samples.entry(key.clone()).or_default();
			for sample in samples {
				push_sample(current, *sample);
			}
		}
		self.performance.merge(&newer.performance);
	}
}

pub fn show() -> Result<(), Error> {
	let paths = Paths::discover()?;
	let disabled = is_disabled(&paths)?;
	let stats = read_stats_locked(&paths, !disabled)?;
	display::print(&paths, &stats, disabled);
	if !disabled {
		let mut usage = Usage::default();
		usage.record_command(
			Command::TelemetryShow,
			CommandOutcome::Success,
			now(),
		);
		update_stats(&paths, true, |stats| stats.usage.merge(&usage))?;
	}
	Ok(())
}

pub fn clear() -> Result<(), Error> {
	let paths = Paths::discover()?;
	clear_at(&paths)?;
	ui::success(tr("telemetry-cleared"));
	Ok(())
}

fn clear_at(paths: &Paths) -> Result<(), Error> {
	let disabled = is_disabled(paths)?;
	let disabled_at = marker_timestamp(paths);
	let _lock = lock(paths)?;
	let mut stats = read_stats(paths, !disabled)
		.unwrap_or_else(|_| Stats::new(!disabled, disabled_at));
	stats.usage = Usage::default();
	stats.last_cleared_at = Some(now());
	write_stats(paths, &stats)?;
	Ok(())
}

pub fn disable() -> Result<(), Error> {
	let paths = Paths::discover()?;
	disable_at(&paths)?;
	ui::success(tr("telemetry-disabled"));
	Ok(())
}

fn disable_at(paths: &Paths) -> Result<(), Error> {
	let already_disabled = is_disabled(paths)?;
	let at = now();
	write_marker(paths, at)?;
	update_stats(paths, false, |stats| {
		if !already_disabled {
			stats.usage.record_command(
				Command::TelemetryDisable,
				CommandOutcome::Success,
				at,
			);
			stats.last_enabled_at.get_or_insert(at);
		}
		stats.last_disabled_at = Some(at);
	})?;
	Ok(())
}

pub fn enable() -> Result<(), Error> {
	let paths = Paths::discover()?;
	enable_at(&paths)?;
	ui::success(tr("telemetry-enabled"));
	Ok(())
}

fn enable_at(paths: &Paths) -> Result<(), Error> {
	let at = now();
	update_stats(paths, false, |stats| {
		stats.last_enabled_at = Some(at);
		stats.usage.record_command(
			Command::TelemetryEnable,
			CommandOutcome::Success,
			at,
		);
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

fn lock(paths: &Paths) -> Result<File, Error> {
	let Some(directory) = paths.lock.parent() else {
		return Err(Error::new(tr("telemetry-directory-error")));
	};
	fs::create_dir_all(directory).map_err(|error| {
		storage_error(&tr("telemetry-directory-create-error"), error)
	})?;
	let file = OpenOptions::new()
		.create(true)
		.truncate(false)
		.read(true)
		.write(true)
		.open(&paths.lock)
		.map_err(|error| {
			storage_error(&tr("telemetry-lock-open-error"), error)
		})?;
	file.lock()
		.map_err(|error| storage_error(&tr("telemetry-lock-error"), error))?;
	Ok(file)
}

fn read_stats(paths: &Paths, enabled: bool) -> Result<Stats, Error> {
	let contents = match fs::read(&paths.data) {
		Ok(contents) => contents,
		Err(error) if error.kind() == io::ErrorKind::NotFound => {
			return Ok(Stats::new(enabled, marker_timestamp(paths)));
		}
		Err(error) => {
			return Err(storage_error(&tr("telemetry-read-error"), error));
		}
	};
	let stats: Stats = serde_json::from_slice(&contents).map_err(|error| {
		Error::with_debug(tr("telemetry-invalid-error"), error)
	})?;
	if stats.schema_version != SCHEMA_VERSION {
		return Err(Error::new(tr_args(
			"telemetry-schema-unsupported",
			&[("version", stats.schema_version.into())],
		)));
	}
	Ok(stats)
}

fn write_stats(paths: &Paths, stats: &Stats) -> Result<(), Error> {
	let Some(directory) = paths.data.parent() else {
		return Err(Error::new(tr("telemetry-directory-error")));
	};
	fs::create_dir_all(directory).map_err(|error| {
		storage_error(&tr("telemetry-directory-create-error"), error)
	})?;
	let contents = serde_json::to_vec(stats).map_err(|error| {
		Error::with_debug(tr("telemetry-prepare-error"), error)
	})?;
	// ponytail: telemetry tolerates a lost file on a crash; add atomic
	// replacement if telemetry ever becomes recovery-critical.
	fs::write(&paths.data, contents)
		.map_err(|error| storage_error(&tr("telemetry-store-error"), error))
}

fn is_disabled(paths: &Paths) -> Result<bool, Error> {
	paths.disabled.try_exists().map_err(|error| {
		storage_error(&tr("telemetry-opt-out-inspect-error"), error)
	})
}

fn marker_timestamp(paths: &Paths) -> Option<u64> {
	fs::read_to_string(&paths.disabled)
		.ok()
		.and_then(|contents| contents.parse().ok())
}

fn write_marker(paths: &Paths, at: u64) -> Result<(), Error> {
	let Some(directory) = paths.disabled.parent() else {
		return Err(Error::new(tr("telemetry-opt-out-directory-error")));
	};
	fs::create_dir_all(directory).map_err(|error| {
		storage_error(&tr("telemetry-opt-out-create-error"), error)
	})?;
	fs::write(&paths.disabled, at.to_string())
		.map_err(|error| storage_error(&tr("telemetry-disable-error"), error))
}

fn remove_marker(paths: &Paths) -> Result<(), Error> {
	match fs::remove_file(&paths.disabled) {
		Ok(()) => Ok(()),
		Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
		Err(error) => Err(storage_error(&tr("telemetry-enable-error"), error)),
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
