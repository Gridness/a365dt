use pretty_assertions::assert_eq;

use super::{State, input_window, query_and_choice};
use crate::search::Search;

fn state(count: usize) -> State {
	let rows = (1..=count)
		.map(|number| [format!("Choice {number}")])
		.collect::<Vec<_>>();
	State::new(Search::new(&rows))
}

#[test]
fn scrolls_at_edges_and_wraps_across_all_results() {
	let mut state = state(12);
	for _ in 0..10 {
		state.down(10);
	}
	assert_eq!(
		(state.selected, state.offset),
		(10, 1),
		"moving below the viewport reveals one more row"
	);

	state.up(10);
	assert_eq!(
		(state.selected, state.offset),
		(9, 1),
		"moving upward inside the viewport does not scroll it"
	);

	state.selected = 11;
	state.offset = 2;
	state.down(10);
	assert_eq!((state.selected, state.offset), (0, 0));
	state.up(10);
	assert_eq!((state.selected, state.offset), (11, 2));
}

#[test]
fn parses_filtered_choices_and_numeric_queries() {
	assert_eq!(
		[
			query_and_choice("dub ru #2"),
			query_and_choice("#3"),
			query_and_choice("2"),
			query_and_choice("1080"),
			query_and_choice("title #wrong"),
		],
		[
			("dub ru", Some(2)),
			("", Some(3)),
			("2", None),
			("1080", None),
			("title #wrong", None),
		]
	);

	let rows = [["1080p".into()], ["720p".into()]];
	let mut state = State::new(Search::new(&rows));
	for character in "1080".chars() {
		state.insert(character);
	}
	assert_eq!((state.matches.clone(), state.choice()), (vec![0], Some(0)));
}

#[test]
fn keeps_the_cursor_visible_in_long_unicode_input() {
	let input = "Врата Штейна";

	assert_eq!(input_window(input, input.len(), 5), ("тейна", 5));
}

#[test]
fn preserves_the_query_when_live_results_arrive() {
	let mut state = State::from_matches("битва магическая".into(), Vec::new());
	assert_eq!((state.query(), state.choice()), ("битва магическая", None));

	state.replace_matches(vec![0]);

	assert_eq!(
		(state.query(), state.choice()),
		("битва магическая", Some(0))
	);
}

#[test]
fn preserves_the_selection_and_viewport_when_rows_are_appended() {
	let mut state = state(12);
	for _ in 0..10 {
		state.down(10);
	}

	state.replace_matches((0..13).collect());

	assert_eq!(
		(state.matches[state.selected], state.selected, state.offset),
		(10, 10, 1)
	);
}
