use std::{collections::BTreeMap, fs, process, time::SystemTime};

use pretty_assertions::assert_eq;

use super::{Cache, MAX_AGE, prune_directory};
use crate::api::Series;

#[test]
fn serializes_catalogue_without_episode_details() {
	let cache = Cache {
		refreshed_at: 42,
		series: vec![Series {
			id: 7,
			title: "Магическая битва".into(),
			year: Some(2020),
			type_title: Some("TV".into()),
			number_of_episodes: Some(24),
			poster_url_small: None,
			episodes: Vec::new(),
		}],
		aliases: BTreeMap::from([("jjk".into(), 7)]),
	};
	let json = serde_json::to_string(&cache).unwrap();

	assert!(!json.contains("episodes"));
	assert!(!json.contains("posterUrlSmall"));
	assert_eq!(serde_json::from_str::<Cache>(&json).unwrap(), cache);
}

#[test]
fn expires_catalogue_after_one_day() {
	let cache = Cache {
		refreshed_at: 100,
		series: Vec::new(),
		aliases: BTreeMap::new(),
	};

	assert!(cache.is_fresh_at(100 + MAX_AGE.as_secs() - 1));
	assert!(!cache.is_fresh_at(100 + MAX_AGE.as_secs()));
}

#[test]
fn remembers_normalized_aliases_until_the_series_is_removed() {
	let mut cache = Cache {
		series: vec![Series {
			id: 7,
			title: "Jujutsu Kaisen".into(),
			year: Some(2020),
			type_title: Some("TV".into()),
			number_of_episodes: Some(47),
			poster_url_small: None,
			episodes: Vec::new(),
		}],
		..Cache::default()
	};

	cache.remember_alias("  JJK!!!  ", 7);
	assert_eq!(cache.alias("jjk"), Some(7));

	cache.remove_series(7);
	assert_eq!(cache, Cache::default());
}

#[test]
fn prunes_cache_directory_idempotently() {
	let directory = std::env::temp_dir().join(format!(
		"a365dt-cache-prune-{}-{}",
		process::id(),
		SystemTime::now()
			.duration_since(SystemTime::UNIX_EPOCH)
			.unwrap()
			.as_nanos()
	));
	fs::create_dir_all(&directory).unwrap();
	fs::write(directory.join("series.json"), b"cached").unwrap();

	prune_directory(&directory).unwrap();
	prune_directory(&directory).unwrap();

	assert!(!directory.exists());
}
