use std::{
	collections::BTreeMap,
	fs,
	time::{Duration, SystemTime},
};

use pretty_assertions::assert_eq;

use super::{
	CatalogueUse, Command, CommandOutcome, Operation, Paths, Recorder, Usage,
	clear_at, disable_at,
	display::format_timestamp,
	enable_at,
	performance::{Performance, Work},
	push_sample, read_stats_locked,
};
use crate::{
	download::{Outcome, Status, Summary},
	error::Error,
};

#[test]
fn records_aggregate_usage_without_download_identity() {
	let paths = paths("aggregate");
	let recorder = Recorder::from_paths(paths.clone()).unwrap();
	recorder.record_command(Command::Download, CommandOutcome::Success);
	recorder.record_command(Command::Update, CommandOutcome::Failure);
	recorder.record_catalogue(CatalogueUse::Hit);
	recorder.record_download(&Summary {
		outcomes: vec![
			Outcome {
				episode: "secret episode".into(),
				status: Status::Downloaded,
				bytes: 42,
				detail: Error::new("secret path"),
			},
			Outcome {
				episode: "existing episode".into(),
				status: Status::Skipped,
				bytes: 100,
				detail: Error::new("secret existing path"),
			},
		],
		elapsed: Duration::from_millis(12),
	});
	recorder.flush().unwrap();

	let stats = read_stats_locked(&paths, true).unwrap();
	let json = serde_json::to_string(&stats).unwrap();
	assert_eq!(
		stats.usage.counters,
		BTreeMap::from([
			("catalogue.hits".into(), 1),
			("commands.download.success".into(), 1),
			("commands.update.failure".into(), 1),
			("downloads.batches".into(), 1),
			("downloads.bytes".into(), 42),
			("downloads.episodes.downloaded".into(), 1),
			("downloads.episodes.skipped".into(), 1),
		])
	);
	assert_eq!(
		stats.usage.samples,
		BTreeMap::from([("downloads.batch_duration_ms".into(), vec![12])])
	);
	assert!(!json.contains("secret"));
	cleanup(&paths);
}

#[test]
fn opt_out_and_clear_have_independent_lifecycles() {
	let paths = paths("lifecycle");
	let recorder = Recorder::from_paths(paths.clone()).unwrap();
	recorder.record_command(Command::Download, CommandOutcome::Failure);
	recorder.flush().unwrap();

	disable_at(&paths).unwrap();
	clear_at(&paths).unwrap();
	let disabled = read_stats_locked(&paths, false).unwrap();
	assert_eq!(disabled.usage, Usage::default());
	assert!(paths.disabled.exists());
	assert!(disabled.last_disabled_at.is_some());
	assert!(disabled.last_cleared_at.is_some());

	enable_at(&paths).unwrap();
	let enabled = read_stats_locked(&paths, true).unwrap();
	assert_eq!(enabled.schema_version, super::SCHEMA_VERSION);
	assert!(!paths.disabled.exists());
	assert!(enabled.last_enabled_at.is_some());
	cleanup(&paths);
}

#[test]
fn formats_utc_calendar_dates() {
	assert_eq!(
		[
			format_timestamp(None),
			format_timestamp(Some(0)),
			format_timestamp(Some(951_782_400)),
		],
		[
			"Never",
			"1970-01-01 00:00:00 UTC",
			"2000-02-29 00:00:00 UTC",
		]
	);
}

#[test]
fn keeps_only_the_latest_latency_samples() {
	let mut samples = (0..super::SAMPLE_LIMIT as u64).collect::<Vec<_>>();

	push_sample(&mut samples, super::SAMPLE_LIMIT as u64);

	assert_eq!(
		samples,
		(1..=super::SAMPLE_LIMIT as u64).collect::<Vec<_>>()
	);
}

#[test]
fn shares_and_flushes_performance_observations_from_clones() {
	let paths = paths("performance");
	let recorder = Recorder::from_paths(paths.clone()).unwrap();
	recorder.record_performance(
		Operation::ApiSearch,
		Duration::from_micros(1_200),
		Work::None,
	);
	recorder.clone().record_performance(
		Operation::SearchRank,
		Duration::from_micros(300),
		Work::Items(30_000),
	);
	recorder.flush().unwrap();

	let stats = read_stats_locked(&paths, true).unwrap();
	let mut expected = Performance::default();
	expected.record(
		"request.api.search",
		Duration::from_micros(1_200),
		Work::None,
	);
	expected.record(
		"search.rank",
		Duration::from_micros(300),
		Work::Items(30_000),
	);
	assert_eq!(stats.usage.performance, expected);
	cleanup(&paths);
}

#[test]
fn keeps_bounded_recent_performance_samples() {
	let mut performance = Performance::default();
	for microseconds in 0..=101 {
		performance.record(
			"search.rank",
			Duration::from_micros(microseconds),
			Work::Items(30_000),
		);
	}

	let (operation, metric) = performance.iter().next().unwrap();
	assert_eq!(
		(
			operation,
			metric.count(),
			metric.total_us(),
			metric.work_units(),
			metric.samples_us().to_vec(),
		),
		("search.rank", 102, 5_151, 3_060_000, (1..=101).collect(),)
	);
}

fn paths(name: &str) -> Paths {
	let root = std::env::temp_dir().join(format!(
		"a365dt-telemetry-{name}-{}-{}",
		std::process::id(),
		SystemTime::now()
			.duration_since(SystemTime::UNIX_EPOCH)
			.unwrap()
			.as_nanos()
	));
	Paths {
		data: root.join("data/telemetry.json"),
		lock: root.join("data/telemetry.lock"),
		disabled: root.join("config/telemetry-disabled"),
	}
}

fn cleanup(paths: &Paths) {
	let root = paths.data.parent().unwrap().parent().unwrap();
	fs::remove_dir_all(root).unwrap();
}
