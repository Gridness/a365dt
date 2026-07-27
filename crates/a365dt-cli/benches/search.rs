use std::sync::LazyLock;

#[path = "../src/search.rs"]
mod search;

use search::Search;

static ROWS: LazyLock<Vec<[String; 4]>> = LazyLock::new(|| {
	(0..30_000)
		.map(|index| {
			if index == 914 {
				[
					"Врата Штейна / Steins;Gate".into(),
					"2011".into(),
					"TV".into(),
					"24 episodes".into(),
				]
			} else {
				[
					format!("Series {index}: Chronicle of the Silver Horizon"),
					(1980 + index % 47).to_string(),
					if index % 5 == 0 { "Movie" } else { "TV" }.into(),
					format!("{} episodes", 1 + index % 48),
				]
			}
		})
		.collect()
});
static CATALOGUE: LazyLock<Search> =
	LazyLock::new(|| Search::new(ROWS.as_slice()));

fn main() {
	divan::main();
}

#[divan::bench(args = [
	"steins gate",
	"stiens gate",
	"gate steins",
	"definitely absent",
])]
fn filter_catalogue(query: &str) -> Vec<usize> {
	divan::black_box(CATALOGUE.len());
	CATALOGUE.ranked(divan::black_box(query))
}

#[divan::bench]
fn index_catalogue() -> Search {
	Search::new(divan::black_box(ROWS.as_slice()))
}
