use std::{ffi::OsString, process::ExitCode};

use clap::{
	Arg, ArgAction, Command, CommandFactory, FromArgMatches,
	error::{ContextKind, ErrorKind},
};
use fluent_bundle::FluentValue;
use rapidfuzz::distance::osa;

use super::{Args, CacheCommand, Commands, TelemetryCommand, completion_shell};
use crate::{
	l10n::{tr, tr_args},
	search::typo_budget,
	ui,
};

pub fn closest_match<'candidate>(
	query: &str,
	candidates: &'candidate [&str],
) -> Option<&'candidate str> {
	candidates
		.iter()
		.filter_map(|candidate| {
			let distance = osa::distance(
				query.to_ascii_lowercase().chars(),
				candidate.to_ascii_lowercase().chars(),
			);
			(distance <= typo_budget(query.chars().count()))
				.then_some((distance, *candidate))
		})
		.min_by_key(|(distance, _)| *distance)
		.map(|(_, candidate)| candidate)
}

pub fn parse(arguments: Vec<OsString>) -> Result<Args, ExitCode> {
	let mut command = localized_command();
	let mut matches = match command.try_get_matches_from_mut(arguments) {
		Ok(matches) => matches,
		Err(error)
			if matches!(
				error.kind(),
				ErrorKind::DisplayHelp
					| ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
					| ErrorKind::DisplayVersion
			) =>
		{
			print!("{error}");
			return Err(ExitCode::SUCCESS);
		}
		Err(error) => {
			ui::failure(error_message(&error));
			return Err(ExitCode::from(2));
		}
	};
	Args::from_arg_matches_mut(&mut matches).map_err(|error| {
		ui::failure(error_message(&error));
		ExitCode::from(2)
	})
}

pub fn localized_command() -> Command {
	let help = Arg::new("help")
		.short('h')
		.long("help")
		.global(true)
		.action(ArgAction::Help)
		.help(tr("cli-option-help"));
	let version = Arg::new("version")
		.short('V')
		.long("version")
		.global(true)
		.action(ArgAction::Version)
		.help(tr("cli-option-version"));
	let mut command = Args::command()
		.propagate_version(true)
		.about(tr("cli-about"))
		.override_help(tr("cli-help-root"))
		.arg(help)
		.arg(version)
		.mut_arg("query", |arg| arg.help(tr("cli-option-query-or-url")))
		.mut_arg("forced_query", |arg| arg.help(tr("cli-option-query")))
		.mut_arg("output", |arg| arg.help(tr("cli-option-output")))
		.mut_arg("jobs", |arg| arg.help(tr("cli-option-jobs")))
		.mut_arg("debug", |arg| arg.help(tr("cli-option-debug")))
		.mut_arg("lang", |arg| arg.help(tr("cli-option-lang")))
		.mut_subcommand("cache", |command| {
			command
				.about(tr("cli-command-cache"))
				.override_help(tr("cli-help-cache"))
				.mut_subcommand("prune", |command| {
					command
						.about(tr("cli-command-cache-prune"))
						.override_help(tr("cli-help-cache-prune"))
				})
		})
		.mut_subcommand("completions", |command| {
			command
				.about(tr("cli-command-completions"))
				.override_help(tr("cli-help-completions"))
				.mut_arg("arguments", |arg| {
					arg.help(tr("cli-option-completion-shell"))
				})
		})
		.mut_subcommand("doctor", |command| {
			command
				.about(tr("cli-command-doctor"))
				.override_help(tr("cli-help-doctor"))
		})
		.mut_subcommand("purge", |command| {
			command
				.about(tr("cli-command-purge"))
				.override_help(tr("cli-help-purge"))
				.mut_arg("yes", |arg| arg.help(tr("cli-option-purge-yes")))
		})
		.mut_subcommand("telemetry", |command| {
			command
				.about(tr("cli-command-telemetry"))
				.override_help(tr("cli-help-telemetry"))
				.mut_subcommand("clear", |command| {
					command
						.about(tr("cli-command-telemetry-clear"))
						.override_help(tr("cli-help-telemetry-clear"))
				})
				.mut_subcommand("disable", |command| {
					command
						.about(tr("cli-command-telemetry-disable"))
						.override_help(tr("cli-help-telemetry-disable"))
				})
				.mut_subcommand("enable", |command| {
					command
						.about(tr("cli-command-telemetry-enable"))
						.override_help(tr("cli-help-telemetry-enable"))
				})
				.mut_subcommand("show", |command| {
					command
						.about(tr("cli-command-telemetry-show"))
						.override_help(tr("cli-help-telemetry-show"))
				})
		});
	command.build();
	localize_help_subcommands(&mut command);
	command
}

fn localize_help_subcommands(command: &mut Command) {
	for subcommand in command.get_subcommands_mut() {
		if subcommand.get_name() == "help" {
			*subcommand = subcommand.clone().about(tr("cli-command-help"));
		}
		localize_help_subcommands(subcommand);
	}
}

fn error_message(error: &clap::Error) -> String {
	let value =
		|kind| error.get(kind).map(ToString::to_string).unwrap_or_default();
	let message = match error.kind() {
		ErrorKind::ArgumentConflict => tr_args(
			"cli-error-conflict",
			&[
				(
					"argument",
					FluentValue::from(value(ContextKind::InvalidArg)),
				),
				("other", FluentValue::from(value(ContextKind::PriorArg))),
			],
		),
		ErrorKind::InvalidSubcommand => tr_args(
			"cli-error-subcommand",
			&[(
				"value",
				FluentValue::from(value(ContextKind::InvalidSubcommand)),
			)],
		),
		ErrorKind::InvalidValue | ErrorKind::ValueValidation => tr_args(
			"cli-error-value",
			&[
				("value", FluentValue::from(value(ContextKind::InvalidValue))),
				(
					"argument",
					FluentValue::from(value(ContextKind::InvalidArg)),
				),
			],
		),
		ErrorKind::MissingRequiredArgument => tr_args(
			"cli-error-required",
			&[(
				"argument",
				FluentValue::from(value(ContextKind::InvalidArg)),
			)],
		),
		ErrorKind::MissingSubcommand => tr_args(
			"cli-error-missing-subcommand",
			&[(
				"command",
				FluentValue::from(value(ContextKind::InvalidSubcommand)),
			)],
		),
		ErrorKind::NoEquals => tr_args(
			"cli-error-equals",
			&[(
				"argument",
				FluentValue::from(value(ContextKind::InvalidArg)),
			)],
		),
		ErrorKind::TooManyValues => tr_args(
			"cli-error-too-many",
			&[(
				"argument",
				FluentValue::from(value(ContextKind::InvalidArg)),
			)],
		),
		ErrorKind::TooFewValues | ErrorKind::WrongNumberOfValues => tr_args(
			"cli-error-too-few",
			&[(
				"argument",
				FluentValue::from(value(ContextKind::InvalidArg)),
			)],
		),
		ErrorKind::UnknownArgument => tr_args(
			"cli-error-argument",
			&[(
				"argument",
				FluentValue::from(value(ContextKind::InvalidArg)),
			)],
		),
		ErrorKind::InvalidUtf8 => tr("cli-error-utf8"),
		ErrorKind::DisplayHelp
		| ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
		| ErrorKind::DisplayVersion
		| ErrorKind::Io
		| ErrorKind::Format => tr("cli-error-generic"),
		_ => tr("cli-error-generic"),
	};
	let suggestion = [
		ContextKind::SuggestedArg,
		ContextKind::SuggestedSubcommand,
		ContextKind::SuggestedValue,
	]
	.into_iter()
	.find_map(|kind| error.get(kind))
	.map(ToString::to_string);
	match suggestion {
		Some(suggestion) => tr_args(
			"cli-error-with-suggestion",
			&[
				("message", FluentValue::from(message)),
				("suggestion", FluentValue::from(suggestion)),
			],
		),
		None => tr_args(
			"cli-error-with-help",
			&[("message", FluentValue::from(message))],
		),
	}
}

pub fn route_title_query(args: &mut Args) {
	if let Some(query) = title_query(args) {
		args.query = query;
		args.command = None;
	}
}

pub fn suggestions(args: &Args) -> Vec<String> {
	if !args.forced_query.is_empty() {
		return Vec::new();
	}
	let query = match &args.command {
		None => args.query.clone(),
		Some(_) => match title_query(args) {
			Some(query) => query,
			None => return Vec::new(),
		},
	};
	let mut suggestions = command_paths()
		.into_iter()
		.filter_map(|command| {
			command_distance(&query, &command)
				.map(|distance| (distance, command))
		})
		.collect::<Vec<_>>();
	suggestions.sort_by_key(|(distance, _)| *distance);
	suggestions
		.into_iter()
		.take(5)
		.map(|(_, command)| command)
		.collect()
}

pub fn suggestion_message(suggestions: &[String]) -> String {
	let mut commands = String::new();
	for suggestion in suggestions {
		commands.push_str("\n  a365dt ");
		commands.push_str(suggestion);
	}
	tr_args(
		"command-suggestions",
		&[("commands", FluentValue::from(commands))],
	)
}

fn title_query(args: &Args) -> Option<Vec<String>> {
	let (command, query) = match &args.command {
		Some(Commands::Cache {
			command: CacheCommand::Prune { query },
		}) if !query.is_empty() => (
			"cache",
			std::iter::once("prune".to_owned())
				.chain(query.clone())
				.collect(),
		),
		Some(Commands::Cache {
			command: CacheCommand::Query(query),
		}) => ("cache", query.clone()),
		Some(Commands::Completions { arguments })
			if completion_shell(arguments).is_none() =>
		{
			("completions", arguments.clone())
		}
		Some(Commands::Doctor { query }) if !query.is_empty() => {
			("doctor", query.clone())
		}
		Some(Commands::Telemetry {
			command: TelemetryCommand::Clear { query },
		}) if !query.is_empty() => (
			"telemetry",
			std::iter::once("clear".to_owned())
				.chain(query.clone())
				.collect(),
		),
		Some(Commands::Telemetry {
			command: TelemetryCommand::Disable { query },
		}) if !query.is_empty() => (
			"telemetry",
			std::iter::once("disable".to_owned())
				.chain(query.clone())
				.collect(),
		),
		Some(Commands::Telemetry {
			command: TelemetryCommand::Enable { query },
		}) if !query.is_empty() => (
			"telemetry",
			std::iter::once("enable".to_owned())
				.chain(query.clone())
				.collect(),
		),
		Some(Commands::Telemetry {
			command: TelemetryCommand::Show { query },
		}) if !query.is_empty() => (
			"telemetry",
			std::iter::once("show".to_owned())
				.chain(query.clone())
				.collect(),
		),
		Some(Commands::Telemetry {
			command: TelemetryCommand::Query(query),
		}) => ("telemetry", query.clone()),
		_ => return None,
	};
	Some(std::iter::once(command.to_owned()).chain(query).collect())
}

fn command_paths() -> Vec<String> {
	let mut paths = Vec::new();
	collect_paths(&Args::command(), &mut Vec::new(), &mut paths);
	paths
}

fn collect_paths(
	command: &Command,
	prefix: &mut Vec<String>,
	paths: &mut Vec<String>,
) {
	let mut children = command
		.get_subcommands()
		.filter(|command| !command.is_hide_set())
		.peekable();
	if children.peek().is_none() {
		if !prefix.is_empty() {
			paths.push(prefix.join(" "));
		}
		return;
	}
	for child in children {
		prefix.push(child.get_name().to_owned());
		collect_paths(child, prefix, paths);
		prefix.pop();
	}
}

fn command_distance(query: &[String], command: &str) -> Option<usize> {
	let mut distance = 0;
	for (query, command) in query.iter().zip(command.split_whitespace()) {
		let current =
			osa::distance(query.to_ascii_lowercase().chars(), command.chars());
		if current > typo_budget(query.chars().count()) {
			return None;
		}
		distance += current;
	}
	(distance > 0).then_some(distance)
}

#[cfg(test)]
#[path = "command_line_tests.rs"]
mod tests;
