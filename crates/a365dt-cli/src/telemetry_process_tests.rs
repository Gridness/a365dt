use std::{
	collections::BTreeMap,
	io::{self, Write},
};

use pretty_assertions::assert_eq;

use super::{
	CatalogueUse, Command, CommandOutcome, InvocationId, Operation, Paths,
	Writer,
	recording::{Observation, ObservationKind},
	snapshot,
	storage::Store,
};
use crate::{
	api::Series,
	download::{Outcome, Status},
	error::Error,
};

#[tokio::test]
#[ignore]
async fn worker_concurrent_first_open() {
	wait_for_input("OPEN");
	let store = Store::open(paths()).await.unwrap();
	assert_eq!(
		store.collection_state().await.unwrap(),
		super::storage::CollectionState {
			enabled: false,
			last_enabled_at_ms: None,
			last_disabled_at_ms: Some(123_000),
			last_cleared_at_ms: None,
		}
	);
	barrier("OPENED");
	store.close().await;
}

#[tokio::test]
#[ignore]
async fn worker_writer_drain() {
	let (recorder, writer) =
		Writer::at(paths(), InvocationId::new()).await.unwrap();
	barrier("READY");
	wait_for_input("RECORD");
	let series = series();
	recorder.record_command(Command::Download, CommandOutcome::Success);
	recorder.record_series(&series, CatalogueUse::Hit);
	recorder.record_download(
		&series,
		&crate::download::Summary {
			outcomes: vec![
				Outcome {
					episode: "private".into(),
					status: Status::Downloaded,
					bytes: 42,
					detail: Error::new("private"),
				},
				Outcome {
					episode: "private".into(),
					status: Status::Skipped,
					bytes: 0,
					detail: Error::new("private"),
				},
			],
			elapsed: std::time::Duration::from_millis(5),
		},
	);
	drop(recorder.measure_items(Operation::SearchRank, 10));
	barrier("RECORDED");
	wait_for_input("FINISH");
	writer.finish().await.unwrap();
	barrier("FINISHED");
}

#[tokio::test]
#[ignore]
async fn worker_verify_writer_drain() {
	let store = Store::open(paths()).await.unwrap();
	assert_eq!(
		(
			root_counts(&store).await,
			sqlx::query_as::<_, (i64, i64)>(
				"SELECT COUNT(*), COUNT(DISTINCT invocation_id) \
				 FROM command_events",
			)
			.fetch_one(&store.pool)
			.await
			.unwrap(),
			sqlx::query_as::<_, (i64, i64)>(
				"SELECT COUNT(*), COUNT(DISTINCT invocation_id) \
				 FROM series_selection_events",
			)
			.fetch_one(&store.pool)
			.await
			.unwrap(),
			sqlx::query_as::<_, (i64, i64)>(
				"SELECT COUNT(*), COUNT(DISTINCT invocation_id) \
				 FROM download_batches",
			)
			.fetch_one(&store.pool)
			.await
			.unwrap(),
			sqlx::query_as::<_, (i64, i64)>(
				"SELECT COUNT(*), COUNT(DISTINCT invocation_id) \
				 FROM performance_events",
			)
			.fetch_one(&store.pool)
			.await
			.unwrap(),
			sqlx::query_as::<_, (i64, i64)>(
				"SELECT COUNT(*), COUNT(DISTINCT batch_id) \
				 FROM download_outcomes",
			)
			.fetch_one(&store.pool)
			.await
			.unwrap(),
		),
		((2, 2, 2, 2), (2, 2), (2, 2), (2, 2), (2, 2), (4, 2),)
	);
	store.close().await;
}

#[tokio::test]
#[ignore]
async fn worker_state_batches() {
	let store = Store::open(paths()).await.unwrap();
	let mut watermark =
		store.collection_state().await.unwrap().last_cleared_at_ms;
	let invocation_id = InvocationId::new();
	barrier("READY");
	wait_for_input("DISABLED_BATCH");
	store
		.commit(
			&mut watermark,
			vec![observation(invocation_id, 1, Command::Download)],
		)
		.await
		.unwrap();
	barrier("DISABLED_BATCHED");
	wait_for_input("ENABLED_BATCH");
	store
		.commit(
			&mut watermark,
			vec![observation(invocation_id, 2, Command::Update)],
		)
		.await
		.unwrap();
	barrier("ENABLED_BATCHED");
	wait_for_input("WATERMARK_BATCH");
	let cleared_at = store
		.collection_state()
		.await
		.unwrap()
		.last_cleared_at_ms
		.unwrap();
	store
		.commit(
			&mut watermark,
			vec![
				observation(invocation_id, cleared_at, Command::Doctor),
				observation(invocation_id, cleared_at + 1, Command::Stats),
			],
		)
		.await
		.unwrap();
	assert_eq!(
		snapshot::capture(&store).await.unwrap().counters,
		BTreeMap::from([("commands.stats.success".into(), 1)])
	);
	barrier("VERIFIED");
	store.close().await;
}

#[tokio::test]
#[ignore]
async fn worker_state_control() {
	let store = Store::open(paths()).await.unwrap();
	barrier("READY");
	loop {
		match read_input().as_str() {
			"DISABLE" => {
				store.disable(InvocationId::new()).await.unwrap();
				barrier("DISABLED");
			}
			"ENABLE" => {
				store.enable(InvocationId::new()).await.unwrap();
				barrier("ENABLED");
			}
			"CLEAR" => {
				store.clear().await.unwrap();
				barrier("CLEARED");
			}
			"FINISH" => break,
			input => panic!("unexpected control input: {input}"),
		}
	}
	store.close().await;
}

async fn root_counts(store: &Store) -> (i64, i64, i64, i64) {
	(
		sqlx::query_scalar("SELECT COUNT(*) FROM command_events")
			.fetch_one(&store.pool)
			.await
			.unwrap(),
		sqlx::query_scalar("SELECT COUNT(*) FROM series_selection_events")
			.fetch_one(&store.pool)
			.await
			.unwrap(),
		sqlx::query_scalar("SELECT COUNT(*) FROM download_batches")
			.fetch_one(&store.pool)
			.await
			.unwrap(),
		sqlx::query_scalar("SELECT COUNT(*) FROM performance_events")
			.fetch_one(&store.pool)
			.await
			.unwrap(),
	)
}

fn paths() -> Paths {
	let root = std::env::current_dir().unwrap();
	Paths {
		data: root.join("telemetry.sqlite"),
		lock: root.join("telemetry.lock"),
		disabled: root.join("telemetry-disabled"),
	}
}

fn observation(
	invocation_id: InvocationId,
	observed_at_ms: u64,
	command: Command,
) -> Observation {
	Observation {
		invocation_id,
		observed_at_ms,
		kind: ObservationKind::Command {
			command,
			outcome: CommandOutcome::Success,
		},
	}
}

fn series() -> Series {
	Series {
		id: 365,
		title: "Series".into(),
		year: None,
		type_title: None,
		number_of_episodes: None,
		poster_url_small: None,
		episodes: Vec::new(),
	}
}

fn wait_for_input(expected: &str) {
	assert_eq!(read_input(), expected);
}

fn read_input() -> String {
	let mut line = String::new();
	io::stdin().read_line(&mut line).unwrap();
	line.trim().into()
}

fn barrier(token: &str) {
	println!("\n{token}");
	io::stdout().flush().unwrap();
}
