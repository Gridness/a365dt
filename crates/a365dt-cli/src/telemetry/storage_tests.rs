use std::{fs, process, time::SystemTime};

use pretty_assertions::assert_eq;

use super::Store;
use crate::{
	download::Status,
	telemetry::{
		CatalogueUse, Command, CommandOutcome, InvocationId, Operation, Paths,
		recording::{DownloadOutcome, Observation, ObservationKind},
	},
};

#[tokio::test]
async fn opens_the_durable_typed_telemetry_store() {
	let paths = paths("settings");
	let store = Store::open(paths.clone()).await.unwrap();

	assert_eq!(
		(
			sqlx::query_scalar::<_, String>("PRAGMA journal_mode")
				.fetch_one(&store.pool)
				.await
				.unwrap(),
			sqlx::query_scalar::<_, i64>("PRAGMA synchronous")
				.fetch_one(&store.pool)
				.await
				.unwrap(),
			sqlx::query_scalar::<_, i64>("PRAGMA foreign_keys")
				.fetch_one(&store.pool)
				.await
				.unwrap(),
			sqlx::query_scalar::<_, i64>("PRAGMA busy_timeout")
				.fetch_one(&store.pool)
				.await
				.unwrap(),
			sqlx::query_scalar::<_, i64>(
				"SELECT COUNT(*) FROM pragma_table_list \
				 WHERE name IN (\
				 'collection_state', 'command_events', \
				 'series_selection_events', 'download_batches', \
				 'download_outcomes', 'performance_events'\
				 ) AND strict = 1",
			)
			.fetch_one(&store.pool)
			.await
			.unwrap(),
		),
		("wal".into(), 2, 1, 5_000, 6)
	);

	store.close().await;
	cleanup(&paths);
}

#[tokio::test]
async fn stores_complete_typed_observations() {
	let paths = paths("events");
	let store = Store::open(paths.clone()).await.unwrap();
	let invocation_id = InvocationId::new();
	let invocation = invocation_id.to_string();
	let mut watermark = None;
	store
		.commit(
			&mut watermark,
			vec![
				Observation {
					invocation_id,
					observed_at_ms: 10_000,
					kind: ObservationKind::Command {
						command: Command::Download,
						outcome: CommandOutcome::Success,
					},
				},
				Observation {
					invocation_id,
					observed_at_ms: 20_000,
					kind: ObservationKind::SeriesSelection {
						series_id: 365,
						series_title: "Private Series title".into(),
						catalogue: Some(CatalogueUse::Miss),
					},
				},
				Observation {
					invocation_id,
					observed_at_ms: 30_000,
					kind: ObservationKind::DownloadBatch {
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
				},
				Observation {
					invocation_id,
					observed_at_ms: 40_000,
					kind: ObservationKind::Performance {
						operation: Operation::SearchRank,
						duration_us: 50,
						work_units: Some(30_000),
					},
				},
			],
		)
		.await
		.unwrap();

	assert_eq!(
		(
			sqlx::query_as::<_, (String, i64, String, String)>(
				"SELECT invocation_id, observed_at_ms, command, outcome \
				 FROM command_events",
			)
			.fetch_all(&store.pool)
			.await
			.unwrap(),
			sqlx::query_as::<_, (String, i64, i64, String, Option<String>)>(
				"SELECT invocation_id, observed_at_ms, series_id, \
				 series_title, catalogue_result FROM series_selection_events",
			)
			.fetch_all(&store.pool)
			.await
			.unwrap(),
			sqlx::query_as::<_, (String, i64, i64, String, i64)>(
				"SELECT invocation_id, observed_at_ms, series_id, \
				 series_title, duration_us FROM download_batches",
			)
			.fetch_all(&store.pool)
			.await
			.unwrap(),
			sqlx::query_as::<_, (String, Option<i64>)>(
				"SELECT status, downloaded_bytes FROM download_outcomes \
				 ORDER BY id",
			)
			.fetch_all(&store.pool)
			.await
			.unwrap(),
			sqlx::query_as::<_, (String, i64, String, i64, Option<i64>)>(
				"SELECT invocation_id, observed_at_ms, operation, duration_us, \
				 work_units FROM performance_events",
			)
			.fetch_all(&store.pool)
			.await
			.unwrap(),
		),
		(
			vec![(
				invocation.clone(),
				10_000,
				"download".into(),
				"success".into(),
			)],
			vec![(
				invocation.clone(),
				20_000,
				365,
				"Private Series title".into(),
				Some("miss".into()),
			)],
			vec![(
				invocation.clone(),
				30_000,
				365,
				"Private Series title".into(),
				12_345,
			)],
			vec![("downloaded".into(), Some(42)), ("skipped".into(), None),],
			vec![(invocation, 40_000, "search.rank".into(), 50, Some(30_000),)],
		)
	);
	let snapshot = crate::telemetry::snapshot::capture(&store).await.unwrap();
	assert_eq!(
		(
			snapshot.first_recorded_at,
			snapshot.last_recorded_at,
			snapshot.first_download_at,
			snapshot.last_download_at,
			snapshot.counters,
			snapshot.samples,
			snapshot
				.performance
				.into_iter()
				.map(|metric| (
					metric.operation,
					metric.count,
					metric.total_us,
					metric.work_units,
					metric.samples_us,
				))
				.collect::<Vec<_>>(),
		),
		(
			Some(10),
			Some(40),
			Some(30),
			Some(30),
			std::collections::BTreeMap::from([
				("catalogue.misses".into(), 1),
				("commands.download.success".into(), 1),
				("downloads.batches".into(), 1),
				("downloads.bytes".into(), 42),
				("downloads.episodes.downloaded".into(), 1),
				("downloads.episodes.skipped".into(), 1),
			]),
			std::collections::BTreeMap::from([(
				"downloads.batch_duration_ms".into(),
				vec![12],
			)]),
			vec![("search.rank".into(), 1, 50, 30_000, vec![50])],
		)
	);
	store.clear().await.unwrap();
	assert!(
		crate::telemetry::snapshot::capture(&store)
			.await
			.unwrap()
			.counters
			.is_empty()
	);

	store.close().await;
	cleanup(&paths);
}

#[tokio::test]
async fn rolls_back_incomplete_download_batches_and_enforces_scalars() {
	let paths = paths("atomic");
	let store = Store::open(paths.clone()).await.unwrap();
	let invocation_id = InvocationId::new();
	let mut watermark = None;
	let error = store
		.commit(
			&mut watermark,
			vec![
				Observation {
					invocation_id,
					observed_at_ms: 1,
					kind: ObservationKind::Command {
						command: Command::Download,
						outcome: CommandOutcome::Success,
					},
				},
				Observation {
					invocation_id,
					observed_at_ms: 2,
					kind: ObservationKind::DownloadBatch {
						series_id: 365,
						series_title: "Series".into(),
						duration_us: 3,
						outcomes: vec![DownloadOutcome {
							status: Status::Downloaded,
							bytes: None,
						}],
					},
				},
			],
		)
		.await
		.unwrap_err();

	assert!(error.to_string().contains("Could not update"));
	assert_eq!(
		(
			sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM command_events")
				.fetch_one(&store.pool)
				.await
				.unwrap(),
			sqlx::query_scalar::<_, i64>(
				"SELECT COUNT(*) FROM download_batches"
			)
			.fetch_one(&store.pool)
			.await
			.unwrap(),
			sqlx::query_scalar::<_, i64>(
				"SELECT COUNT(*) FROM download_outcomes",
			)
			.fetch_one(&store.pool)
			.await
			.unwrap(),
			sqlx::query(
				"INSERT INTO series_selection_events \
				 (invocation_id, observed_at_ms, series_id, series_title) \
				 VALUES ('00000000-0000-7000-8000-000000000000', 0, 0, 'x')",
			)
			.execute(&store.pool)
			.await
			.is_err(),
		),
		(0, 0, 0, true)
	);

	store.close().await;
	cleanup(&paths);
}

fn paths(name: &str) -> Paths {
	let root = std::env::temp_dir().join(format!(
		"a365dt-telemetry-storage-{name}-{}-{}",
		process::id(),
		SystemTime::now()
			.duration_since(SystemTime::UNIX_EPOCH)
			.unwrap()
			.as_nanos()
	));
	Paths {
		data: root.join("data/telemetry.sqlite"),
		disabled: root.join("config/telemetry-disabled"),
		lock: root.join("data/telemetry.lock"),
	}
}

fn cleanup(paths: &Paths) {
	fs::remove_dir_all(paths.data.parent().unwrap().parent().unwrap()).unwrap();
}
