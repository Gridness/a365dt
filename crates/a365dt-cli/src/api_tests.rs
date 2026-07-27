use pretty_assertions::assert_eq;

use super::{Envelope, Episode, Series, normalize_url, series_id_from_url};

#[test]
fn resolves_relative_assets_on_media_origin() {
	assert_eq!(
		normalize_url("/episodeTranslations/3954619.ass?willcache").unwrap(),
		"https://smotret-anime.org/episodeTranslations/3954619.ass?willcache"
			.parse()
			.unwrap()
	);
}

#[test]
fn routes_posters_around_the_challenged_web_origin() {
	assert_eq!(
		normalize_url("https://anime365.ru/posters/30887.small.jpg").unwrap(),
		"https://smotret-anime.org/posters/30887.small.jpg"
			.parse()
			.unwrap()
	);
}

#[test]
fn parses_only_official_series_urls() {
	assert_eq!(
		series_id_from_url(
			"https://smotret-anime.org/catalog/road-of-naruto-30887/"
		),
		Some(30887)
	);
	assert_eq!(
		series_id_from_url("https://example.com/catalog/title-1"),
		None
	);
	assert_eq!(
		series_id_from_url("https://anime365.ru/catalog/title-1/2-seriya-3"),
		None
	);
}

#[test]
fn parses_official_series_fixture() {
	let actual: Envelope<Vec<Series>> = serde_json::from_str(
        r#"{"data":[{"id":30887,"title":"ROAD OF NARUTO","year":2022,"typeTitle":"ONA","numberOfEpisodes":1,"posterUrlSmall":"https://anime365.ru/posters/30887.small.jpg","episodes":[{"id":292232,"episodeInt":"1","episodeFull":"ONA 1"}]}]}"#,
    )
    .unwrap();

	assert_eq!(
		actual.data.unwrap(),
		vec![Series {
			id: 30887,
			title: "ROAD OF NARUTO".into(),
			year: Some(2022),
			type_title: Some("ONA".into()),
			number_of_episodes: Some(1),
			poster_url_small: Some(
				"https://anime365.ru/posters/30887.small.jpg".into(),
			),
			episodes: vec![Episode {
				id: 292232,
				episode_int: "1".into(),
				episode_full: "ONA 1".into(),
			}],
		}]
	);
}
