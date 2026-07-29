use std::{collections::HashSet, fs, process, time::SystemTime};

use pretty_assertions::assert_eq;

use super::{Catalogue, MAX_AGE, decode, encode, prune_directory};
use crate::{
	api::Series,
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

fn matching_series(
	catalogue: &mut Catalogue,
	query: &str,
	server_matches: &[u64],
) -> Vec<Series> {
	catalogue
		.suggestions(query, server_matches, &Recorder::default())
		.matching_series()
		.cloned()
		.collect()
}

#[test]
fn serializes_catalogue_without_episode_details() {
	let expected = series(7, "Магическая битва", 2020);
	let mut catalogue = Catalogue::refreshed(vec![expected.clone()]);
	catalogue.remember_alias("jjk", 7);
	let json = String::from_utf8(encode(&catalogue).unwrap()).unwrap();

	assert!(!json.contains("episodes"));
	assert!(!json.contains("posterUrlSmall"));
	let mut decoded = decode(json.as_bytes()).unwrap();
	assert_eq!(
		(
			matching_series(&mut decoded, "", &[]),
			matching_series(&mut decoded, "jjk", &[]),
		),
		(vec![expected.clone()], vec![expected])
	);
}

#[test]
fn expires_catalogue_after_one_day() {
	let catalogue = Catalogue::default();

	assert!(catalogue.is_fresh_at(MAX_AGE.as_secs() - 1));
	assert!(!catalogue.is_fresh_at(MAX_AGE.as_secs()));
}

#[test]
fn updates_existing_series_and_deduplicates_new_results() {
	let mut catalogue = Catalogue::new(vec![series(1, "Old title", 2020)]);
	let expected = vec![
		series(1, "Current title", 2021),
		series(2, "Final title", 2023),
	];

	catalogue.upsert(vec![
		expected[0].clone(),
		series(2, "Superseded title", 2022),
		expected[1].clone(),
	]);

	assert_eq!(matching_series(&mut catalogue, "", &[]), expected);
}

#[test]
fn prioritizes_learned_and_server_suggestions_without_duplicates() {
	let expected = vec![
		series(1, "Jujutsu Kaisen", 2020),
		series(2, "Tengen Toppa Gurren Lagann", 2007),
	];
	let mut catalogue = Catalogue::new(expected.clone());

	assert_eq!(
		matching_series(&mut catalogue, "jjk", &[1]),
		vec![expected[0].clone()]
	);

	catalogue.remember_alias("  JJK!!!  ", 2);
	assert_eq!(
		matching_series(&mut catalogue, "jjk", &[1, 2]),
		vec![expected[1].clone(), expected[0].clone()]
	);
}

#[test]
fn removes_a_series_and_its_aliases_together() {
	let remaining = series(1, "Jujutsu Kaisen", 2020);
	let mut catalogue = Catalogue::new(vec![
		remaining.clone(),
		series(2, "Tengen Toppa Gurren Lagann", 2007),
	]);
	catalogue.remember_alias("gurren", 2);

	catalogue.remove_series(2);

	assert_eq!(
		(
			matching_series(&mut catalogue, "", &[]),
			matching_series(&mut catalogue, "gurren", &[]),
		),
		(vec![remaining], Vec::new())
	);
}

#[test]
fn merges_refreshes_with_current_results_and_valid_aliases() {
	let aliased = series(1, "Tengen Toppa Gurren Lagann", 2007);
	let current = series(2, "Current query result", 2024);
	let mut catalogue = Catalogue::new(vec![
		aliased.clone(),
		current.clone(),
		series(4, "Stale result", 1999),
	]);
	catalogue.remember_alias("ttgl", 1);
	let refreshed = Catalogue::refreshed(vec![
		series(3, "Old refreshed title", 2020),
		series(3, "Current refreshed title", 2021),
	]);

	catalogue.merge_refresh(refreshed, &HashSet::from([2]));

	assert_eq!(
		(
			matching_series(&mut catalogue, "", &[]),
			matching_series(&mut catalogue, "ttgl", &[]),
			catalogue.is_fresh(),
		),
		(
			vec![
				series(3, "Old refreshed title", 2020),
				aliased.clone(),
				current,
			],
			vec![aliased],
			true,
		)
	);
}

#[test]
fn ranks_series_and_classifies_catalogue_use_through_the_interface() {
	let expected = series(1, "Магическая битва", 2020);
	let mut catalogue = Catalogue::new(vec![
		expected.clone(),
		series(2, "Битва через пять секунд после встречи", 2021),
	]);
	catalogue.upsert(vec![series(3, "New result", 2024)]);
	let rows = catalogue
		.suggestions("битва магическая", &[], &Recorder::default())
		.matching_rows(10);

	assert_eq!(
		(
			matching_series(&mut catalogue, "битва магическая", &[]),
			rows,
			[catalogue.catalogue_use(1), catalogue.catalogue_use(3),],
		),
		(
			vec![expected],
			vec![[
				"Магическая битва".into(),
				"2020".into(),
				"TV".into(),
				"24 episodes".into(),
			]],
			[CatalogueUse::Hit, CatalogueUse::Miss],
		)
	);
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
