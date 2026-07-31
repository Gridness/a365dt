use std::{
	collections::BTreeMap,
	fs,
	time::{Duration, SystemTime},
};

use pretty_assertions::assert_eq;
use tokio::sync::mpsc;
use uuid::{Uuid, Version};

use super::{
	CatalogueUse, Command, CommandOutcome, InvocationId, Operation, Paths,
	Recorder, Stats, Usage, Writer, clear_at, commit_observations, disable_at,
	display::format_timestamp,
	enable_at,
	performance::{Performance, Work},
	push_sample, read_stats_locked,
	recording::{DownloadOutcome, Observation, ObservationKind},
};
use crate::{
	api::{Episode, Series},
	download::{Outcome, Status, Summary},
	error::Error,
};

#[tokio::test]
async fn records_aggregate_usage_without_download_identity() {
	let paths = paths("aggregate");
	let (recorder, writer) =
		Writer::at(paths.clone(), InvocationId::new()).unwrap();
	recorder.record_command(Command::Download, CommandOutcome::Success);
	recorder.record_command(Command::Update, CommandOutcome::Failure);
	recorder.record_series(&series(), CatalogueUse::Hit);
	recorder.record_download(
		&series(),
		&Summary {
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
		},
	);
	writer.finish().await.unwrap();

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

#[tokio::test]
async fn opt_out_and_clear_have_independent_lifecycles() {
	let paths = paths("lifecycle");
	let (recorder, writer) =
		Writer::at(paths.clone(), InvocationId::new()).unwrap();
	recorder.record_command(Command::Download, CommandOutcome::Failure);
	writer.finish().await.unwrap();

	disable_at(&paths, InvocationId::new()).unwrap();
	clear_at(&paths).unwrap();
	let disabled = read_stats_locked(&paths, false).unwrap();
	assert_eq!(disabled.usage, Usage::default());
	assert!(paths.disabled.exists());
	assert!(disabled.last_disabled_at.is_some());
	assert!(disabled.last_cleared_at.is_some());

	enable_at(&paths, InvocationId::new()).unwrap();
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

#[tokio::test]
async fn clones_share_the_same_writer() {
	let paths = paths("clones");
	let (recorder, writer) =
		Writer::at(paths.clone(), InvocationId::new()).unwrap();
	recorder.record_command(Command::Download, CommandOutcome::Success);
	recorder
		.clone()
		.record_command(Command::Update, CommandOutcome::Failure);
	writer.finish().await.unwrap();

	assert_eq!(
		read_stats_locked(&paths, true).unwrap().usage.counters,
		BTreeMap::from([
			("commands.download.success".into(), 1),
			("commands.update.failure".into(), 1),
		])
	);
	cleanup(&paths);
}

#[test]
fn recorder_sends_complete_typed_privacy_safe_observations_from_clones() {
	let invocation_id = InvocationId::new();
	let (observations, mut receiver) = mpsc::unbounded_channel();
	let recorder = Recorder::connected(invocation_id, observations);
	recorder.record_command(Command::Download, CommandOutcome::Success);
	recorder
		.clone()
		.record_series(&series(), CatalogueUse::Miss);
	recorder.record_download(
		&series(),
		&Summary {
			outcomes: vec![
				Outcome {
					episode: "secret episode".into(),
					status: Status::Downloaded,
					bytes: 42,
					detail: Error::new("secret path"),
				},
				Outcome {
					episode: "secret existing episode".into(),
					status: Status::Skipped,
					bytes: 100,
					detail: Error::new("secret existing path"),
				},
			],
			elapsed: Duration::from_micros(12_345),
		},
	);
	drop(recorder.measure_items(Operation::SearchRank, 30_000));
	drop(recorder);
	let mut observations =
		std::iter::from_fn(|| receiver.try_recv().ok()).collect::<Vec<_>>();

	assert!(
		observations
			.iter()
			.all(|observation| observation.invocation_id == invocation_id)
	);
	let performance = observations.pop().unwrap().kind;
	assert_eq!(
		observations
			.into_iter()
			.map(|observation| observation.kind)
			.collect::<Vec<_>>(),
		vec![
			ObservationKind::Command {
				command: Command::Download,
				outcome: CommandOutcome::Success,
			},
			ObservationKind::SeriesSelection {
				series_id: 365,
				series_title: "Private Series title".into(),
				catalogue: Some(CatalogueUse::Miss),
			},
			ObservationKind::DownloadBatch {
				series_id: 365,
				series_title: "Private Series title".into(),
				duration_us: 12_345,
				outcomes: vec![
					DownloadOutcome {
						status: Status::Downloaded,
						bytes: Some(42),
					},
					DownloadOutcome {
						status: Status::Skipped,
						bytes: None,
					},
				],
			},
		]
	);
	assert!(matches!(
		performance,
		ObservationKind::Performance {
			operation: Operation::SearchRank,
			duration_us: _,
			work_units: Some(30_000),
		}
	));
	let parsed = Uuid::parse_str(&invocation_id.to_string()).unwrap();
	assert_eq!(
		(parsed.get_version(), invocation_id.to_string().len()),
		(Some(Version::SortRand), 36)
	);
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

#[tokio::test(start_paused = true)]
async fn writer_commits_one_second_batches_and_finish_drains_the_tail() {
	let paths = paths("batching");
	let (recorder, writer) =
		Writer::at(paths.clone(), InvocationId::new()).unwrap();
	recorder.record_command(Command::Download, CommandOutcome::Success);
	tokio::task::yield_now().await;

	tokio::time::advance(Duration::from_millis(999)).await;
	assert_eq!(
		read_stats_locked(&paths, true).unwrap().usage,
		Usage::default()
	);
	tokio::time::advance(Duration::from_millis(1)).await;
	tokio::task::yield_now().await;
	assert_eq!(
		writer.snapshot().unwrap().counters,
		BTreeMap::from([("commands.download.success".into(), 1)])
	);

	recorder.record_command(Command::Update, CommandOutcome::Failure);
	writer.finish().await.unwrap();
	assert_eq!(
		read_stats_locked(&paths, true).unwrap().usage.counters,
		BTreeMap::from([
			("commands.download.success".into(), 1),
			("commands.update.failure".into(), 1),
		])
	);
	cleanup(&paths);
}

#[tokio::test(start_paused = true)]
async fn writer_rechecks_collection_state_for_each_batch() {
	let paths = paths("collection-state");
	let invocation_id = InvocationId::new();
	let (recorder, writer) = Writer::at(paths.clone(), invocation_id).unwrap();
	recorder.record_command(Command::Download, CommandOutcome::Success);
	tokio::task::yield_now().await;

	disable_at(&paths, invocation_id).unwrap();
	tokio::time::advance(Duration::from_secs(1)).await;
	tokio::task::yield_now().await;
	enable_at(&paths, invocation_id).unwrap();
	recorder.record_command(Command::Update, CommandOutcome::Failure);
	tokio::task::yield_now().await;
	tokio::time::advance(Duration::from_secs(1)).await;
	tokio::task::yield_now().await;

	assert_eq!(
		writer.snapshot().unwrap().counters,
		BTreeMap::from([
			("commands.telemetry.disable.success".into(), 1),
			("commands.telemetry.enable.success".into(), 1),
			("commands.update.failure".into(), 1),
		])
	);
	writer.finish().await.unwrap();
	cleanup(&paths);
}

#[tokio::test(start_paused = true)]
async fn writer_reports_the_first_failure_after_continuing_later_batches() {
	let paths = paths("background-failure");
	let (recorder, writer) =
		Writer::at(paths.clone(), InvocationId::new()).unwrap();
	recorder.record_command(Command::Download, CommandOutcome::Success);
	fs::write(&paths.data, b"{").unwrap();
	tokio::task::yield_now().await;
	tokio::time::advance(Duration::from_secs(1)).await;
	tokio::task::yield_now().await;

	fs::write(&paths.data, serde_json::to_vec(&Stats::default()).unwrap())
		.unwrap();
	recorder.record_command(Command::Update, CommandOutcome::Failure);
	tokio::task::yield_now().await;
	tokio::time::advance(Duration::from_secs(1)).await;
	tokio::task::yield_now().await;
	assert_eq!(
		writer.snapshot().unwrap().counters,
		BTreeMap::from([("commands.update.failure".into(), 1)])
	);

	let error = writer.finish().await.unwrap_err();
	assert_eq!(
		error.message(),
		"Could not read the local telemetry because it is invalid. Run `a365dt telemetry clear` to reset it."
	);
	cleanup(&paths);
}

#[test]
fn clear_watermark_prevents_pending_observations_from_reappearing() {
	let paths = paths("clear-watermark");
	clear_at(&paths).unwrap();
	let watermark = read_stats_locked(&paths, true)
		.unwrap()
		.last_cleared_at_ms
		.unwrap();
	let mut before = Observation::command(
		InvocationId::new(),
		Command::Download,
		CommandOutcome::Success,
	);
	before.observed_at_ms = watermark;
	let mut after = Observation::command(
		InvocationId::new(),
		Command::Update,
		CommandOutcome::Failure,
	);
	after.observed_at_ms = watermark + 1;

	commit_observations(&paths, vec![before, after]).unwrap();

	assert_eq!(
		read_stats_locked(&paths, true).unwrap().usage.counters,
		BTreeMap::from([("commands.update.failure".into(), 1)])
	);
	cleanup(&paths);
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

fn series() -> Series {
	Series {
		id: 365,
		title: "Private Series title".into(),
		year: None,
		type_title: None,
		number_of_episodes: None,
		poster_url_small: Some("secret poster URL".into()),
		episodes: vec![Episode {
			id: 999,
			episode_int: "secret number".into(),
			episode_full: "secret episode identity".into(),
		}],
	}
}

fn cleanup(paths: &Paths) {
	let root = paths.data.parent().unwrap().parent().unwrap();
	fs::remove_dir_all(root).unwrap();
}
