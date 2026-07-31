use std::{fs, process, time::SystemTime};

use pretty_assertions::assert_eq;

use super::remember_failure;
use crate::telemetry::{
	Command, CommandOutcome, InvocationId, Paths,
	recording::{Observation, ObservationKind},
	snapshot,
	storage::Store,
};

#[tokio::test]
async fn remembers_the_first_failure_and_keeps_committing() {
	let paths = paths();
	let store = Store::open(paths.clone()).await.unwrap();
	let invocation_id = InvocationId::new();
	let mut watermark = None;
	let mut first_error = None;
	remember_failure(
		store
			.commit(
				&mut watermark,
				vec![Observation {
					invocation_id,
					observed_at_ms: 1,
					kind: ObservationKind::SeriesSelection {
						series_id: 0,
						series_title: "invalid".into(),
						catalogue: None,
					},
				}],
			)
			.await,
		&mut first_error,
	);
	remember_failure(
		store
			.commit(
				&mut watermark,
				vec![Observation {
					invocation_id,
					observed_at_ms: 2,
					kind: ObservationKind::Command {
						command: Command::Update,
						outcome: CommandOutcome::Success,
					},
				}],
			)
			.await,
		&mut first_error,
	);

	assert_eq!(
		(
			first_error.unwrap().to_string(),
			snapshot::capture(&store).await.unwrap().counters,
		),
		(
			"Could not update the local telemetry. Close other a365dt processes and retry."
				.into(),
			std::collections::BTreeMap::from([(
				"commands.update.success".into(),
				1,
			)]),
		)
	);
	store.close().await;
	fs::remove_dir_all(paths.data.parent().unwrap().parent().unwrap()).unwrap();
}

fn paths() -> Paths {
	let root = std::env::temp_dir().join(format!(
		"a365dt-telemetry-writer-error-{}-{}",
		process::id(),
		SystemTime::now()
			.duration_since(SystemTime::UNIX_EPOCH)
			.unwrap()
			.as_nanos()
	));
	Paths {
		data: root.join("data/telemetry.sqlite"),
		lock: root.join("data/telemetry.lock"),
		disabled: root.join("config/telemetry-disabled"),
	}
}
