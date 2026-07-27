use pretty_assertions::assert_eq;

use super::{api_query, ranked_series, upsert};
use crate::api::Series;

fn series(id: u64, title: &str, year: u16) -> Series {
	Series {
		id,
		title: title.into(),
		year: Some(year),
		type_title: Some("TV".into()),
		number_of_episodes: Some(24),
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
		ranked_series(&catalogue, "битва магическая", 10),
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
