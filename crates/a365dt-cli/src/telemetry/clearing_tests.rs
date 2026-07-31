use chrono::{FixedOffset, NaiveDate, TimeZone, Timelike};
use pretty_assertions::assert_eq;

use super::{
	FullClearPermission,
	clearing::{
		PreparedClear, TerminalAccess, authorize_full_clear, prepare_since,
		resolve_boundary,
	},
};

#[test]
fn accepts_only_the_decided_elapsed_duration_grammar() {
	let timezone = FixedOffset::east_opt(3 * 60 * 60).unwrap();
	let now = timezone
		.with_ymd_and_hms(2026, 7, 31, 12, 34, 56)
		.unwrap()
		.with_nanosecond(789_000_000)
		.unwrap();

	for (values, expression, cutoff_ms) in [
		(&["30m"][..], "30m", 1_785_488_696_789),
		(&["30", "minutes"][..], "30 minutes", 1_785_488_696_789),
		(&[" 30\tMINUTES "][..], "30 minutes", 1_785_488_696_789),
		(&["3h"][..], "3h", 1_785_479_696_789),
		(&["1d"][..], "1d", 1_785_404_096_789),
		(&["1w"][..], "1w", 1_784_885_696_789),
		(&["1", "week"][..], "1 week", 1_784_885_696_789),
	] {
		assert_eq!(
			prepare_since(values, now).unwrap(),
			PreparedClear::Since {
				cleared_at_ms: 1_785_490_496_789,
				cutoff_ms,
				expression: expression.into(),
			}
		);
	}
}

#[test]
fn rejects_ambiguous_or_out_of_range_elapsed_durations() {
	let timezone = FixedOffset::east_opt(0).unwrap();
	let now = timezone.timestamp_millis_opt(7_200_000).unwrap();

	for values in [
		&["0m"][..],
		&["+1h"][..],
		&["-1h"][..],
		&["1.5h"][..],
		&["1h30m"][..],
		&["1s"][..],
		&["2026-01-01"][..],
		&["18446744073709551615w"][..],
		&["3h"][..],
	] {
		assert!(prepare_since(values, now).is_err());
	}
	assert!(
		prepare_since(
			&["this year"],
			timezone.with_ymd_and_hms(1969, 1, 1, 0, 0, 0).unwrap(),
		)
		.is_err()
	);
}

#[test]
fn resolves_local_calendar_boundaries() {
	let timezone = FixedOffset::east_opt(3 * 60 * 60).unwrap();
	let now = timezone.with_ymd_and_hms(2026, 7, 31, 12, 34, 56).unwrap();

	for (values, expression, cutoff_ms) in [
		(&["today"][..], "today", 1_785_445_200_000),
		(&["this", "week"][..], "this week", 1_785_099_600_000),
		(&["this month"][..], "this month", 1_782_853_200_000),
		(&["THIS", "YEAR"][..], "this year", 1_767_214_800_000),
	] {
		assert_eq!(
			prepare_since(values, now).unwrap(),
			PreparedClear::Since {
				cleared_at_ms: 1_785_490_496_000,
				cutoff_ms,
				expression: expression.into(),
			}
		);
	}
}

#[test]
fn chooses_the_earlier_repeated_midnight_and_first_valid_skipped_instant() {
	let date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
	let midnight = date.and_hms_opt(0, 0, 0).unwrap();

	assert_eq!(
		(
			resolve_boundary(date, |local| if local == midnight {
				chrono::LocalResult::Ambiguous(2, 1)
			} else {
				chrono::LocalResult::Single(3)
			}),
			resolve_boundary(date, |local| {
				let second = (local - midnight).num_seconds();
				if second < 42 {
					chrono::LocalResult::None
				} else {
					chrono::LocalResult::Single(second)
				}
			}),
		),
		(Some(1), Some(42))
	);
}

#[test]
fn full_clear_requires_preauthorization_or_two_terminals() {
	let preauthorized = authorize_full_clear(
		FullClearPermission::Preauthorized,
		TerminalAccess::NonInteractive,
		|| panic!("preauthorization must not prompt"),
	)
	.unwrap();
	let declined = authorize_full_clear(
		FullClearPermission::Ask,
		TerminalAccess::Interactive,
		|| Ok(false),
	)
	.unwrap();
	let accepted = authorize_full_clear(
		FullClearPermission::Ask,
		TerminalAccess::Interactive,
		|| Ok(true),
	)
	.unwrap();
	let refused = authorize_full_clear(
		FullClearPermission::Ask,
		TerminalAccess::NonInteractive,
		|| panic!("non-interactive use must not read input"),
	)
	.unwrap_err();
	assert_eq!(
		(
			preauthorized,
			declined,
			accepted,
			refused.message().contains("--yes")
		),
		(true, false, true, true)
	);
}
