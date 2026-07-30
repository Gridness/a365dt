use clap::Parser;
use pretty_assertions::assert_eq;

use super::suggestions;
use crate::Args;

#[test]
fn suggests_likely_command_and_subcommand_typos() {
	for (arguments, expected) in [
		(&["a365dt", "telemtry", "show"][..], "telemetry show"),
		(&["a365dt", "telemetry", "shwo"][..], "telemetry show"),
		(&["a365dt", "cach", "prune"][..], "cache prune"),
		(&["a365dt", "cache", "prne"][..], "cache prune"),
		(&["a365dt", "doctro"][..], "doctor"),
		(&["a365dt", "purg"][..], "purge"),
		(&["a365dt", "sttas"][..], "stats"),
		(&["a365dt", "udpate"][..], "update"),
	] {
		let args = Args::try_parse_from(arguments.iter().copied()).unwrap();

		assert_eq!(suggestions(&args), vec![expected.to_owned()]);
	}
}

#[test]
fn keeps_unrelated_words_and_forced_queries_as_title_searches() {
	for arguments in [
		&["a365dt", "telemetry", "this"][..],
		&["a365dt", "cache", "this"][..],
		&["a365dt", "show", "telemetry"][..],
		&["a365dt", "update", "this"][..],
		&["a365dt", "--query", "telemtry show"][..],
	] {
		let args = Args::try_parse_from(arguments.iter().copied()).unwrap();

		assert_eq!(suggestions(&args), Vec::<String>::new());
	}
}
