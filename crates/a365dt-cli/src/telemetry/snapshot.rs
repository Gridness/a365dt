use std::{
	collections::BTreeMap, fs, io, path::PathBuf, sync::Arc, time::Instant,
};

use super::{
	Error, Operation, Paths, Recorder, is_disabled, read_stats_locked,
	storage_error,
};

pub struct Snapshot {
	pub enabled: bool,
	pub data_path: PathBuf,
	pub disabled_path: PathBuf,
	pub data_bytes: Option<u64>,
	pub schema_version: u16,
	pub first_recorded_at: Option<u64>,
	pub last_recorded_at: Option<u64>,
	pub last_enabled_at: Option<u64>,
	pub last_disabled_at: Option<u64>,
	pub last_cleared_at: Option<u64>,
	pub counters: BTreeMap<String, u64>,
	pub performance: Vec<PerformanceMetric>,
}

pub struct PerformanceMetric {
	pub operation: String,
	pub count: u64,
	pub total_us: u64,
	pub work_units: u64,
	pub samples_us: Vec<u64>,
}

pub struct Overhead {
	pub enabled_ns: u64,
	pub disabled_ns: u64,
	pub added_ns: u64,
}

pub fn capture() -> Result<Snapshot, Error> {
	let paths = Paths::discover()?;
	let enabled = !is_disabled(&paths)?;
	let stats = read_stats_locked(&paths, enabled)?;
	let data_bytes = match fs::metadata(&paths.data) {
		Ok(metadata) => Some(metadata.len()),
		Err(error) if error.kind() == io::ErrorKind::NotFound => None,
		Err(error) => {
			return Err(storage_error(
				"Could not inspect the local telemetry data.",
				error,
			));
		}
	};
	let performance = stats
		.usage
		.performance
		.iter()
		.map(|(operation, metric)| {
			let mut samples = metric.samples_us().to_vec();
			samples.sort_unstable();
			PerformanceMetric {
				operation: operation.to_owned(),
				count: metric.count(),
				total_us: metric.total_us(),
				work_units: metric.work_units(),
				samples_us: samples,
			}
		})
		.collect();
	Ok(Snapshot {
		enabled,
		data_path: paths.data,
		disabled_path: paths.disabled,
		data_bytes,
		schema_version: stats.schema_version,
		first_recorded_at: stats.usage.first_recorded_at,
		last_recorded_at: stats.usage.last_recorded_at,
		last_enabled_at: stats.last_enabled_at,
		last_disabled_at: stats.last_disabled_at,
		last_cleared_at: stats.last_cleared_at,
		counters: stats.usage.counters,
		performance,
	})
}

pub fn benchmark_overhead() -> Overhead {
	let enabled = Recorder {
		enabled: true,
		paths: None,
		usage: Arc::default(),
	};
	let disabled = Recorder::default();
	let enabled_ns = median_overhead(&enabled);
	let disabled_ns = median_overhead(&disabled);
	Overhead {
		enabled_ns,
		disabled_ns,
		added_ns: enabled_ns.saturating_sub(disabled_ns),
	}
}

fn median_overhead(recorder: &Recorder) -> u64 {
	let mut samples = Vec::with_capacity(1_001);
	for _ in 0..1_001 {
		let started = Instant::now();
		drop(recorder.measure(Operation::SearchRank));
		std::hint::black_box(());
		samples.push(
			u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX),
		);
	}
	samples.sort_unstable();
	samples[samples.len() / 2]
}
