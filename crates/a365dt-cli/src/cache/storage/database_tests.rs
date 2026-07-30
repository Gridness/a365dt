use super::{FailureContext, is_structural, primary_result_code};

#[test]
fn extended_result_codes_keep_their_primary_classification() {
	assert!(!is_structural(
		Some(primary_result_code(266)),
		FailureContext::Schema
	));
	assert!(is_structural(
		Some(primary_result_code(267)),
		FailureContext::Opening
	));
}
