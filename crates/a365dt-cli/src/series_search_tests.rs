use std::collections::BTreeMap;

use pretty_assertions::assert_eq;

use super::{api_query, catalogue_use, ranked_series, suggestions, upsert};
use crate::{
	api::Series,
	series_cache::Cache,
	telemetry::{CatalogueUse, Recorder},
};

fn series(id: u64, title: &str, year: u16) -> Series {
	Series {
		id,
		title: title.into(),
		year: Some(year),
		type_title: Some("TV".into()),
		number_of_episodes: Some(24),
		poster_url_small: None,
		episodes: Vec::new(),
	}
}

#[test]
fn finds_titles_with_reversed_words() {
	let catalogue = vec![
		series(1, "Магическая битва", 2020),
		series(2, "Битва через пять секунд после встречи", 2021),
	];

	assert_eq!(
		ranked_series(&catalogue, "битва магическая", 10, &Recorder::default(),),
		vec![catalogue[0].clone()]
	);
	assert_eq!(api_query("битва магическая"), "магическая");
}

#[test]
fn updates_existing_series_and_appends_new_results() {
	let mut catalogue = vec![series(1, "Old title", 2020)];
	let expected = vec![
		series(1, "Current title", 2021),
		series(2, "Another title", 2022),
	];

	upsert(&mut catalogue, expected.clone());

	assert_eq!(catalogue, expected);
}

#[test]
fn prioritizes_learned_and_server_suggestions_without_title_matches() {
	let catalogue = vec![
		series(1, "Jujutsu Kaisen", 2020),
		series(2, "Tengen Toppa Gurren Lagann", 2007),
	];
	let mut cache = Cache {
		refreshed_at: 0,
		series: catalogue.clone(),
		aliases: BTreeMap::new(),
	};

	assert_eq!(
		suggestions(&cache, "jjk", &[1], 10, &Recorder::default()),
		vec![catalogue[0].clone()]
	);

	cache.aliases.insert("jjk".into(), 2);
	assert_eq!(
		suggestions(&cache, "jjk", &[1, 2], 10, &Recorder::default(),),
		vec![catalogue[1].clone(), catalogue[0].clone()]
	);
}

#[test]
fn classifies_selection_by_the_persisted_catalogue() {
	assert_eq!(
		[catalogue_use(&[1, 2], 2), catalogue_use(&[1, 2], 3)],
		[CatalogueUse::Hit, CatalogueUse::Miss]
	);
}
