use std::{fs, process, time::SystemTime};

use pretty_assertions::assert_eq;

use super::{CompletedRelease, Release, ReleaseState, Store, prune_directory};
use crate::{
	api::{Episode, Series},
	cache::Catalogue,
	telemetry::Recorder,
};

fn temporary_directory(name: &str) -> std::path::PathBuf {
	std::env::temp_dir().join(format!(
		"a365dt-{name}-{}-{}",
		process::id(),
		SystemTime::now()
			.duration_since(SystemTime::UNIX_EPOCH)
			.unwrap()
			.as_nanos()
	))
}

fn series(id: u64, title: &str) -> Series {
	Series {
		id,
		title: title.into(),
		year: Some(2020),
		type_title: Some("TV".into()),
		number_of_episodes: Some(24),
		poster_url_small: Some("https://example.com/poster.jpg".into()),
		episodes: vec![Episode {
			id: 70,
			episode_int: "1".into(),
			episode_full: "Episode 1".into(),
		}],
	}
}

#[tokio::test]
async fn stores_the_catalogue_projection_without_episode_or_poster_details() {
	let directory = temporary_directory("cache-storage");
	let store = Store::at(directory.clone());
	let stored = series(7, "Магическая битва");
	let mut expected = stored.clone();
	expected.poster_url_small = None;
	expected.episodes.clear();
	let mut catalogue = Catalogue::refreshed(vec![stored]);
	catalogue.remember_alias("jjk", expected.id);

	store.save_catalogue(&catalogue).await.unwrap();

	let (mut loaded, writer) = store
		.load_catalogue()
		.await
		.unwrap()
		.into_session(&store, Recorder::default());
	writer.finish().await.unwrap();
	let suggestions = loaded.suggestions("jjk", &[], &Recorder::default());
	assert_eq!(
		(0..suggestions.matches().len())
			.filter_map(|position| suggestions.series(position))
			.cloned()
			.collect::<Vec<_>>(),
		vec![expected]
	);
	fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn retains_the_latest_completed_release() {
	let directory = temporary_directory("release-storage");
	let store = Store::at(directory.clone());
	let expected = Release {
		tag_name: "v1.2.3".into(),
		html_url: "https://example.com/release".into(),
	};
	let older = Release {
		tag_name: "v1.2.2".into(),
		html_url: "https://example.com/older".into(),
	};

	store
		.save_release(CompletedRelease {
			release: expected.clone(),
			completed_at_ms: 2,
		})
		.await
		.unwrap();
	store
		.save_release(CompletedRelease {
			release: older,
			completed_at_ms: 1,
		})
		.await
		.unwrap();

	assert_eq!(
		store.load_release().await.unwrap(),
		ReleaseState::Fresh(expected)
	);
	fs::remove_dir_all(directory).unwrap();
}

#[test]
fn prunes_cache_directory_idempotently() {
	let directory = temporary_directory("cache-prune");
	fs::create_dir_all(&directory).unwrap();
	fs::write(directory.join("series.json"), b"cached").unwrap();

	prune_directory(&directory).unwrap();
	prune_directory(&directory).unwrap();

	assert!(!directory.exists());
}
