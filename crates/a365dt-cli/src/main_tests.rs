use std::sync::Mutex;

use clap::Parser;
use pretty_assertions::assert_eq;
use tokio::sync::watch;

use super::{
	Args, CacheCommand, Commands, TelemetryCommand, cancel_download,
	command_line::{OwnerRoute, owner_route, route_title_query},
};

#[test]
fn routes_interrupts_to_active_downloads() {
	let (cancel, mut cancellation) = watch::channel(false);
	let active_download = Mutex::new(Some(cancel));

	assert_eq!(
		(
			cancel_download(&active_download),
			*cancellation.borrow_and_update(),
		),
		(true, true)
	);
}

#[test]
fn forces_multi_word_command_names_through_title_search() {
	let args =
		Args::try_parse_from(["a365dt", "--query", "cache", "prune"]).unwrap();

	assert_eq!(
		(args.forced_query, args.query, args.command.is_none(),),
		(
			vec!["cache".to_owned(), "prune".to_owned()],
			Vec::<String>::new(),
			true
		)
	);
}

#[test]
fn parses_mux_after_query_and_its_aliases() {
	for option in ["--mux", "--burn-subtitles", "--as-single-file"] {
		let args = Args::try_parse_from(["a365dt", "Frieren", option]).unwrap();

		assert_eq!((args.query, args.mux), (vec!["Frieren".to_owned()], true));
	}
}

#[test]
fn parses_telemetry_control_commands() {
	let args =
		Args::try_parse_from(["a365dt", "telemetry", "disable"]).unwrap();

	assert!(matches!(
		args.command,
		Some(Commands::Telemetry {
			command: TelemetryCommand::Disable { query }
		})
			if query.is_empty()
	));
}

#[test]
fn parses_guarded_full_and_partial_telemetry_clears() {
	assert_eq!(
		[
			clear_args(&["--since", "30m"]),
			clear_args(&["--since", "30", "minutes"]),
			clear_args(&["--since", "this year"]),
		],
		[
			(false, Some(vec!["30m".into()])),
			(false, Some(vec!["30".into(), "minutes".into()])),
			(false, Some(vec!["this year".into()])),
		]
	);
	for option in ["-y", "--yes"] {
		assert_eq!(clear_args(&[option]), (true, None));
	}
}

#[test]
fn rejects_conflicting_or_oversized_telemetry_clear_options() {
	for arguments in [
		&["--yes", "--since", "30m"][..],
		&["--since", "30", "minutes", "ago"][..],
		&["--since", "30m", "--since", "1h"][..],
	] {
		let arguments = ["a365dt", "telemetry", "clear"]
			.into_iter()
			.chain(arguments.iter().copied());
		assert!(Args::try_parse_from(arguments).is_err());
	}
}

fn clear_args(arguments: &[&str]) -> (bool, Option<Vec<String>>) {
	let arguments = ["a365dt", "telemetry", "clear"]
		.into_iter()
		.chain(arguments.iter().copied());
	let args = Args::try_parse_from(arguments).unwrap();
	let Some(Commands::Telemetry {
		command: TelemetryCommand::Clear { yes, since, query },
	}) = args.command
	else {
		panic!("expected telemetry clear");
	};
	assert!(query.is_empty());
	(yes, since)
}

#[test]
fn parses_purge_confirmation_options() {
	for (arguments, expected) in [
		(&["a365dt", "purge"][..], false),
		(&["a365dt", "purge", "-y"][..], true),
		(&["a365dt", "purge", "--yes"][..], true),
	] {
		let args = Args::try_parse_from(arguments.iter().copied()).unwrap();

		assert!(matches!(
			args.command,
			Some(Commands::Purge { yes }) if yes == expected
		));
	}
}

#[test]
fn routes_unknown_command_arguments_through_title_search() {
	for arguments in [
		&["a365dt", "cache", "this"][..],
		&["a365dt", "cache", "prune", "this"][..],
		&["a365dt", "completions", "this"][..],
		&["a365dt", "completions", "zsh", "this"][..],
		&["a365dt", "doctor", "elise"][..],
		&["a365dt", "stats", "this"][..],
		&["a365dt", "telemetry", "this"][..],
		&["a365dt", "telemetry", "show", "this"][..],
		&["a365dt", "update", "this"][..],
	] {
		let mut args = Args::try_parse_from(arguments.iter().copied()).unwrap();

		route_title_query(&mut args);

		assert_eq!(
			(args.query, args.command.is_none()),
			(
				arguments[1..].iter().copied().map(str::to_owned).collect(),
				true
			)
		);
	}
}

#[test]
fn preserves_existing_commands() {
	let mut cache = Args::try_parse_from(["a365dt", "cache", "prune"]).unwrap();
	let mut clear = Args::try_parse_from([
		"a365dt",
		"telemetry",
		"clear",
		"--since",
		"yesterday",
	])
	.unwrap();
	let mut completions =
		Args::try_parse_from(["a365dt", "completions", "zsh"]).unwrap();
	let mut doctor = Args::try_parse_from(["a365dt", "doctor"]).unwrap();
	let mut stats = Args::try_parse_from(["a365dt", "stats"]).unwrap();
	let mut update = Args::try_parse_from(["a365dt", "update"]).unwrap();

	route_title_query(&mut cache);
	route_title_query(&mut clear);
	route_title_query(&mut completions);
	route_title_query(&mut doctor);
	route_title_query(&mut stats);
	route_title_query(&mut update);

	assert!(matches!(
		cache.command,
		Some(Commands::Cache {
			command: CacheCommand::Prune { query, .. }
		})
			if query.is_empty()
	));
	assert!(matches!(
		clear.command,
		Some(Commands::Telemetry {
			command: TelemetryCommand::Clear { since: Some(_), .. }
		})
	));
	assert!(matches!(
		completions.command,
		Some(Commands::Completions { arguments })
			if arguments == ["zsh"]
	));
	assert!(matches!(
		doctor.command,
		Some(Commands::Doctor { query }) if query.is_empty()
	));
	assert!(matches!(
		stats.command,
		Some(Commands::Stats { query }) if query.is_empty()
	));
	assert!(matches!(
		update.command,
		Some(Commands::Update { query }) if query.is_empty()
	));
}

#[test]
fn accepts_preauthorized_cache_rebuilds() {
	let args =
		Args::try_parse_from(["a365dt", "cache", "prune", "--yes"]).unwrap();

	assert!(matches!(
		args.command,
		Some(Commands::Cache {
			command: CacheCommand::Prune {
				yes: true,
				query,
			}
		}) if query.is_empty()
	));
}

#[test]
fn routes_commands_to_only_their_required_owners() {
	for (arguments, expected) in [
		(&["a365dt", "purge", "--yes"][..], OwnerRoute::Purge),
		(
			&["a365dt", "telemetry", "show"][..],
			OwnerRoute::TelemetryControl,
		),
		(&["a365dt", "completions", "zsh"][..], OwnerRoute::Stateless),
		(
			&["a365dt", "cache", "prune", "--yes"][..],
			OwnerRoute::CachePruneAndTelemetry,
		),
		(&["a365dt", "doctor"][..], OwnerRoute::CacheAndTelemetry),
		(&["a365dt", "stats"][..], OwnerRoute::CacheAndTelemetry),
		(&["a365dt", "update"][..], OwnerRoute::CacheAndTelemetry),
		(&["a365dt", "Frieren"][..], OwnerRoute::CacheAndTelemetry),
	] {
		let mut args = Args::try_parse_from(arguments.iter().copied()).unwrap();
		route_title_query(&mut args);

		assert_eq!(owner_route(&args), expected);
	}
}
