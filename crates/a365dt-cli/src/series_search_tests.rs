use pretty_assertions::assert_eq;

use super::api_query;

#[test]
fn uses_the_longest_query_word_for_fallback_search() {
	assert_eq!(
		[
			api_query("битва магическая"),
			api_query("jujutsu kaisen"),
			api_query("one"),
		],
		["магическая", "jujutsu", "one"]
	);
}
