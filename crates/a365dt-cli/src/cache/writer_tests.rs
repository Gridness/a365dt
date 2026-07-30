use std::{fs, process, time::SystemTime};

use pretty_assertions::assert_eq;

use super::{Catalogue, Store};
use crate::{api::Series, telemetry::Recorder};

fn series(id: u64, title: &str) -> Series {
	Series {
		id,
		title: title.into(),
		year: Some(2024),
		type_title: Some("TV".into()),
		number_of_episodes: Some(12),
		poster_url_small: None,
		episodes: Vec::new(),
	}
}

fn matching_series(catalogue: &mut Catalogue, query: &str) -> Vec<Series> {
	let suggestions = catalogue.suggestions(query, &[], &Recorder::default());
	(0..suggestions.matches().len())
		.filter_map(|position| suggestions.series(position))
		.cloned()
		.collect()
}

#[tokio::test]
async fn semantic_writer_drains_every_mutation_before_finishing() {
	let directory = std::env::temp_dir().join(format!(
		"a365dt-cache-writer-{}-{}",
		process::id(),
		SystemTime::now()
			.duration_since(SystemTime::UNIX_EPOCH)
			.unwrap()
			.as_nanos()
	));
	let store = Store::at(directory.clone());
	let (mut catalogue, writer) = store
		.load_catalogue()
		.await
		.unwrap()
		.into_session(&store, Recorder::default());
	let discovered = series(1, "Discovered");
	let missing = series(2, "Missing");
	let refreshed = series(3, "Refreshed");

	catalogue.upsert(vec![discovered.clone(), missing.clone()]);
	writer.discover(vec![discovered.clone(), missing]);
	catalogue.remember_alias("known", discovered.id);
	writer.remember_alias("known".into(), discovered.clone());
	catalogue.remove_series(2);
	writer.remove_missing(2);
	catalogue.merge_refresh(
		Catalogue::refreshed(vec![refreshed.clone()]),
		&[discovered.id].into(),
	);
	writer.commit_refresh(vec![refreshed.clone()]);
	writer.finish().await.unwrap();

	let (mut loaded, loaded_writer) = store
		.load_catalogue()
		.await
		.unwrap()
		.into_session(&store, Recorder::default());
	loaded_writer.finish().await.unwrap();
	assert_eq!(
		(
			matching_series(&mut loaded, ""),
			matching_series(&mut loaded, "known"),
		),
		(vec![refreshed, discovered.clone()], vec![discovered],)
	);

	fs::remove_dir_all(directory).unwrap();
}
