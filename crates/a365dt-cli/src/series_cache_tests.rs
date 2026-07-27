use pretty_assertions::assert_eq;

use super::{Cache, MAX_AGE};
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
			episodes: Vec::new(),
		}],
	};
	let json = serde_json::to_string(&cache).unwrap();

	assert!(!json.contains("episodes"));
	assert_eq!(serde_json::from_str::<Cache>(&json).unwrap(), cache);
}

#[test]
fn expires_catalogue_after_one_day() {
	let cache = Cache {
		refreshed_at: 100,
		series: Vec::new(),
	};

	assert!(cache.is_fresh_at(100 + MAX_AGE.as_secs() - 1));
	assert!(!cache.is_fresh_at(100 + MAX_AGE.as_secs()));
}
