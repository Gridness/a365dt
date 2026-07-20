use pretty_assertions::assert_eq;

use super::Error;

#[test]
fn hides_technical_details_unless_debug_is_enabled() {
	let error = Error::with_debug(
		"Could not connect to Anime365.",
		"client error (Connect)",
	);

	assert_eq!(
		[error.render(false), error.render(true)],
		["Could not connect to Anime365.", "client error (Connect)",]
	);
}
