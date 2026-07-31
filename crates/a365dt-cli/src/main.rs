mod api;
mod app_files;
mod auth;
mod cache;
mod command_line;
mod doctor;
mod download;
mod error;
mod poster;
mod search;
mod select;
mod series_search;
mod sqlite;
mod startup;
mod stats;
mod telemetry;
mod ui;

#[cfg(test)]
#[path = "search_tests.rs"]
mod search_tests;

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;

use std::{
	collections::VecDeque,
	num::NonZeroUsize,
	path::PathBuf,
	process::{self, ExitCode},
	sync::{Arc, Mutex},
};

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::aot::{Shell, generate};
use console::style;
use indicatif::{HumanBytes, HumanDuration};
use tokio::{fs, process::Command, signal, sync::watch, task::JoinSet};

use crate::{
	api::{Anime365, Episode, Translation},
	command_line::OwnerRoute,
	download::{Job, Status},
	error::Error,
	select::Release,
};

#[derive(Parser)]
#[command(
	name = "a365dt",
	version,
	about = "Download Anime365 episodes without guessing translations"
)]
struct Args {
	#[command(subcommand)]
	command: Option<Commands>,

	#[arg(value_name = "QUERY_OR_URL", num_args = 0..)]
	query: Vec<String>,

	/// Search for a title even when it matches a command name.
	#[arg(
		long = "query",
		value_name = "QUERY",
		num_args = 1..,
		conflicts_with = "query"
	)]
	forced_query: Vec<String>,

	#[arg(short, long, default_value = ".", value_name = "DIR")]
	output: PathBuf,

	#[arg(short, long, default_value = "4")]
	jobs: NonZeroUsize,

	/// Mux separate ASS subtitles into MKV without confirmation.
	#[arg(
		long,
		visible_aliases = ["burn-subtitles", "as-single-file"]
	)]
	mux: bool,

	/// Show technical error details.
	#[arg(long, global = true)]
	debug: bool,
}

#[derive(Subcommand)]
enum Commands {
	/// Manage the local cache.
	Cache {
		#[command(subcommand)]
		command: CacheCommand,
	},

	/// Generate shell completions.
	Completions {
		#[arg(value_name = "SHELL", num_args = 1..)]
		arguments: Vec<String>,
	},

	/// Check a365dt, Anime365, cache, and telemetry health.
	Doctor {
		#[arg(value_name = "QUERY", num_args = 0.., hide = true)]
		query: Vec<String>,
	},

	/// Permanently remove all local a365dt application data.
	Purge {
		/// Purge without asking for confirmation.
		#[arg(short, long)]
		yes: bool,
	},

	/// Show local cache, usage, and performance statistics.
	Stats {
		#[arg(value_name = "QUERY", num_args = 0.., hide = true)]
		query: Vec<String>,
	},

	/// Inspect or control local usage telemetry.
	Telemetry {
		#[command(subcommand)]
		command: TelemetryCommand,
	},

	/// Check whether a newer stable a365dt release is available.
	Update {
		#[arg(value_name = "QUERY", num_args = 0.., hide = true)]
		query: Vec<String>,
	},
}

#[derive(Subcommand)]
enum CacheCommand {
	/// Clear the local cache.
	Prune {
		/// Rebuild damaged cache storage without confirmation.
		#[arg(short, long)]
		yes: bool,

		#[arg(value_name = "QUERY", num_args = 0.., hide = true)]
		query: Vec<String>,
	},

	#[command(external_subcommand)]
	Query(Vec<String>),
}

#[derive(Subcommand)]
enum TelemetryCommand {
	/// Clear collected telemetry without changing collection state.
	Clear {
		/// Clear all telemetry without asking for confirmation.
		#[arg(short, long, conflicts_with = "since")]
		yes: bool,

		/// Clear telemetry since 30m, 30 minutes, today, this week, this month,
		/// or this year.
		#[arg(
			long,
			value_name = "EXPRESSION",
			num_args = 1..=2,
			action = clap::ArgAction::Set,
			conflicts_with = "query"
		)]
		since: Option<Vec<String>>,

		#[arg(
			value_name = "QUERY",
			num_args = 0..,
			hide = true,
			conflicts_with = "yes"
		)]
		query: Vec<String>,
	},

	/// Stop collecting local telemetry.
	Disable {
		#[arg(value_name = "QUERY", num_args = 0.., hide = true)]
		query: Vec<String>,
	},

	/// Resume collecting local telemetry.
	Enable {
		#[arg(value_name = "QUERY", num_args = 0.., hide = true)]
		query: Vec<String>,
	},

	/// Show every collected field and its current value.
	Show {
		#[arg(value_name = "QUERY", num_args = 0.., hide = true)]
		query: Vec<String>,
	},

	#[command(external_subcommand)]
	Query(Vec<String>),
}

#[tokio::main]
async fn main() -> ExitCode {
	let mut args = Args::parse();
	let invocation_id = telemetry::InvocationId::new();
	ui::init();
	let debug = args.debug;
	if !args.forced_query.is_empty() && args.command.is_some() {
		ui::failure(
			"`--query` cannot be combined with a command. Remove the command or search terms.",
		);
		return ExitCode::FAILURE;
	}
	let suggestions = command_line::suggestions(&args);
	if !suggestions.is_empty() {
		ui::failure(command_line::suggestion_message(&suggestions));
		return ExitCode::FAILURE;
	}
	command_line::route_title_query(&mut args);
	let owner_route = command_line::owner_route(&args);
	if owner_route == OwnerRoute::Purge {
		let Some(Commands::Purge { yes }) = args.command.as_ref() else {
			unreachable!("the purge route contains a purge command")
		};
		let confirmed = if *yes {
			true
		} else {
			match ui::confirm(
				&ui::red(
					"Permanently remove all local a365dt application data and saved credentials?",
				),
				false,
			) {
				Ok(confirmed) => confirmed,
				Err(error) => {
					ui::failure(error.render(debug));
					return ExitCode::FAILURE;
				}
			}
		};
		if !confirmed {
			ui::note("Purge cancelled.");
			return ExitCode::SUCCESS;
		}
		let files = app_files::purge().map_err(|error| {
			Error::with_debug(
				"Could not remove all local a365dt application files.",
				error,
			)
		});
		let token = auth::remove_stored_token();
		return match files.and(token) {
			Ok(()) => {
				ui::success("Local a365dt application data removed");
				ExitCode::SUCCESS
			}
			Err(error) => {
				ui::failure(error.render(debug));
				ExitCode::FAILURE
			}
		};
	}
	if owner_route == OwnerRoute::TelemetryControl {
		let Some(Commands::Telemetry { command }) = args.command.as_ref()
		else {
			unreachable!("the Telemetry control route contains its command")
		};
		return match run_telemetry(command, invocation_id).await {
			Ok(()) => ExitCode::SUCCESS,
			Err(error) => {
				ui::failure(error.render(debug));
				ExitCode::FAILURE
			}
		};
	}
	let command = telemetry_command(&args);
	let (telemetry, telemetry_writer) =
		telemetry::Writer::open(invocation_id).await;
	if let Some(error) = telemetry_writer.initialization_warning() {
		ui::warning(error.render(debug));
	}
	let active_download = Arc::new(Mutex::new(None));
	let interrupt_download = Arc::clone(&active_download);
	drop(tokio::spawn(async move {
		match signal::ctrl_c().await {
			Ok(()) => {
				eprintln!();
				ui::failure("Cancelled.");
				if !cancel_download(&interrupt_download) {
					process::exit(130);
				}
			}
			Err(error) => {
				ui::failure(
					Error::with_debug("Could not listen for Ctrl+C.", error)
						.render(debug),
				);
				process::exit(1);
			}
		}
	}));
	let result = match owner_route {
		OwnerRoute::TelemetryOnly => {
			let Some(Commands::Completions { arguments }) =
				args.command.as_ref()
			else {
				unreachable!("the Telemetry-only route generates completions")
			};
			generate(
				completion_shell(arguments)
					.expect("invalid completion shells return to title search"),
				&mut Args::command(),
				"a365dt",
				&mut std::io::stdout(),
			);
			Ok(ExitCode::SUCCESS)
		}
		OwnerRoute::CachePruneAndTelemetry => {
			let Some(Commands::Cache {
				command: CacheCommand::Prune { yes, .. },
			}) = args.command.as_ref()
			else {
				unreachable!("the cache-prune route contains its command")
			};
			prune_cache(if *yes {
				cache::RebuildPermission::Preauthorized
			} else {
				cache::RebuildPermission::Ask
			})
			.await
		}
		OwnerRoute::CacheAndTelemetry => {
			let store = cache::Store::open().await;
			if let Some(error) = store.initialization_warning() {
				ui::warning(error);
			}
			let result = if let Some(Commands::Doctor { .. }) =
				args.command.as_ref()
			{
				Ok(doctor::run(&store, &telemetry_writer, debug).await)
			} else if let Some(Commands::Stats { .. }) = args.command.as_ref() {
				stats::run(&store, &telemetry_writer).await;
				Ok(ExitCode::SUCCESS)
			} else if let Some(Commands::Update { .. }) = args.command.as_ref()
			{
				startup::check(&store).await.map(|update| {
					if let Some(update) = update {
						startup::show_update(&update);
					} else {
						ui::success("Already up to date");
					}
					ExitCode::SUCCESS
				})
			} else {
				run(args, active_download, &store, &telemetry).await
			};
			store.close().await;
			result
		}
		OwnerRoute::Purge | OwnerRoute::TelemetryControl => {
			unreachable!("early-return routes do not open ordinary owners")
		}
	};
	let (code, outcome) = match result {
		Ok(code) if code == ExitCode::SUCCESS => {
			(code, telemetry::CommandOutcome::Success)
		}
		Ok(code) if code == ExitCode::from(130) => {
			(code, telemetry::CommandOutcome::Cancelled)
		}
		Ok(code) => (code, telemetry::CommandOutcome::Failure),
		Err(error) => {
			let outcome = if error.message() == "Cancelled." {
				telemetry::CommandOutcome::Cancelled
			} else {
				telemetry::CommandOutcome::Failure
			};
			ui::failure(error.render(debug));
			(ExitCode::FAILURE, outcome)
		}
	};
	telemetry.record_command(command, outcome);
	if let Err(error) = telemetry_writer.finish().await {
		ui::warning(error.render(debug));
	}
	code
}

fn completion_shell(arguments: &[String]) -> Option<Shell> {
	let [shell] = arguments else {
		return None;
	};
	shell.parse().ok()
}

async fn run_telemetry(
	command: &TelemetryCommand,
	invocation_id: telemetry::InvocationId,
) -> Result<(), Error> {
	match command {
		TelemetryCommand::Clear { yes, since, .. } => {
			let request = match (*yes, since) {
				(true, None) => telemetry::ClearRequest::All(
					telemetry::FullClearPermission::Preauthorized,
				),
				(false, None) => telemetry::ClearRequest::All(
					telemetry::FullClearPermission::Ask,
				),
				(false, Some(since)) => {
					telemetry::ClearRequest::Since(since.clone())
				}
				(true, Some(_)) => {
					unreachable!("clap rejects --yes with --since")
				}
			};
			telemetry::clear(request).await
		}
		TelemetryCommand::Disable { .. } => {
			telemetry::disable(invocation_id).await
		}
		TelemetryCommand::Enable { .. } => {
			telemetry::enable(invocation_id).await
		}
		TelemetryCommand::Show { .. } => telemetry::show(invocation_id).await,
		TelemetryCommand::Query(_) => {
			unreachable!("telemetry queries return to title search")
		}
	}
}

fn telemetry_command(args: &Args) -> telemetry::Command {
	match args.command {
		Some(Commands::Cache {
			command: CacheCommand::Prune { .. },
		}) => telemetry::Command::CachePrune,
		Some(Commands::Cache {
			command: CacheCommand::Query(_),
		}) => unreachable!("cache queries return to title search"),
		Some(Commands::Completions { .. }) => telemetry::Command::Completions,
		Some(Commands::Doctor { .. }) => telemetry::Command::Doctor,
		Some(Commands::Purge { .. }) => {
			unreachable!("purge returns before recording")
		}
		Some(Commands::Stats { .. }) => telemetry::Command::Stats,
		Some(Commands::Telemetry { .. }) => {
			unreachable!("telemetry commands return before recording")
		}
		Some(Commands::Update { .. }) => telemetry::Command::Update,
		None => telemetry::Command::Download,
	}
}

fn cancel_download(
	active_download: &Mutex<Option<watch::Sender<bool>>>,
) -> bool {
	active_download
		.lock()
		.unwrap()
		.as_ref()
		.is_some_and(|cancel| cancel.send(true).is_ok())
}

async fn prune_cache(
	permission: cache::RebuildPermission,
) -> Result<ExitCode, Error> {
	ui::heading("a365dt  ◆  Anime365 downloader");
	cache::prune(permission).await?;
	ui::success("Local cache cleared");
	Ok(ExitCode::SUCCESS)
}

async fn run(
	args: Args,
	active_download: Arc<Mutex<Option<watch::Sender<bool>>>>,
	store: &cache::Store,
	telemetry: &telemetry::Recorder,
) -> Result<ExitCode, Error> {
	ui::heading("a365dt  ◆  Anime365 downloader");
	startup::show(store).await;
	let access_token = auth::access_token()?;
	let api =
		Anime365::new(access_token.value().to_owned(), telemetry.clone())?;
	ui::note("Validating Anime365 access…");
	api.validate().await?;
	ui::success("Authenticated");
	auth::store_if_requested(&access_token)?;

	let query = if args.forced_query.is_empty() {
		args.query.join(" ")
	} else {
		args.forced_query.join(" ")
	};
	let selected = series_search::choose(&api, store, query, telemetry).await?;
	telemetry.record_series(&selected.series, selected.catalogue);
	let series = selected.series;
	ui::success(format!("Selected {}", series.title));
	poster::show(&api, &series).await;
	let episodes = select::choose_episodes(&series.episodes)?;
	let translations = api.translations(series.id).await?;
	let (track, releases) = select::choose_track(translations, &episodes)?;
	ui::success(format!(
		"Selected {}-{} by {}",
		track.kind, track.language, track.authors
	));

	ui::note("Loading available media…");
	let releases = fetch_embeds(&api, releases, args.jobs.get()).await?;
	let planned = select::choose_resolutions(releases)?;
	let separate_subtitles = planned
		.iter()
		.filter(|release| release.subtitle_url.is_some())
		.count();
	let embedded = planned.len() - separate_subtitles;
	if track.kind == "sub" && embedded > 0 {
		ui::note(format!(
			"{embedded} episode(s) have subtitles contained in the MP4."
		));
	}
	let mux = if separate_subtitles > 0 && ffmpeg_available().await {
		args.mux
			|| ui::confirm(
				"Mux separate ASS subtitles into MKV after download?",
				false,
			)?
	} else {
		if separate_subtitles > 0 {
			ui::warning(
				"ffmpeg is unavailable; keeping MP4 and ASS files separate.",
			);
		}
		false
	};

	let directory = args.output.join(download::sanitize(&series.title, 100));
	fs::create_dir_all(&directory).await.map_err(|error| {
		Error::with_debug(
			format!(
				"Could not create output directory {}.",
				directory.display()
			),
			error,
		)
	})?;
	ui::note(format!("Output: {}", directory.display()));
	let jobs = planned
		.into_iter()
		.map(|release| Job::new(release, directory.clone(), mux))
		.collect();
	let (cancel, cancellation) = watch::channel(false);
	*active_download.lock().unwrap() = Some(cancel);
	let summary =
		download::run(api, jobs, args.jobs.get(), args.debug, cancellation)
			.await;
	*active_download.lock().unwrap() = None;
	telemetry.record_download(&series, &summary);
	print_summary(&summary, &directory, args.debug);
	ui::alert();
	let interrupted = summary
		.outcomes
		.iter()
		.any(|outcome| outcome.status == Status::Interrupted);
	let failed = summary.outcomes.iter().any(|outcome| {
		matches!(
			outcome.status,
			Status::Failed | Status::MuxFailed | Status::Interrupted
		)
	});
	Ok(if interrupted {
		ExitCode::from(130)
	} else if failed {
		ExitCode::FAILURE
	} else {
		ExitCode::SUCCESS
	})
}

async fn fetch_embeds(
	api: &Anime365,
	releases: Vec<(Episode, Translation)>,
	concurrency: usize,
) -> Result<Vec<Release>, Error> {
	let mut pending = VecDeque::from(releases);
	let mut active = JoinSet::new();
	for _ in 0..concurrency {
		spawn_embed(&mut active, &mut pending, api);
	}
	let mut result = Vec::new();
	while let Some(joined) = active.join_next().await {
		result.push(joined.map_err(|error| {
			Error::with_debug(
				"An internal task stopped while loading episode media.",
				error,
			)
		})??);
		spawn_embed(&mut active, &mut pending, api);
	}
	result.sort_by(|left, right| {
		let number = |episode: &Episode| {
			episode.episode_int.parse::<f64>().unwrap_or(f64::MAX)
		};
		number(&left.episode).total_cmp(&number(&right.episode))
	});
	Ok(result)
}

fn spawn_embed(
	active: &mut JoinSet<Result<Release, Error>>,
	pending: &mut VecDeque<(Episode, Translation)>,
	api: &Anime365,
) {
	if let Some((episode, translation)) = pending.pop_front() {
		let api = api.clone();
		active.spawn(async move {
			let embed = api.embed(translation.id).await?;
			Ok(Release {
				episode,
				translation,
				embed,
			})
		});
	}
}

async fn ffmpeg_available() -> bool {
	Command::new("ffmpeg")
		.arg("-version")
		.stdout(std::process::Stdio::null())
		.stderr(std::process::Stdio::null())
		.status()
		.await
		.is_ok_and(|status| status.success())
}

fn print_summary(
	summary: &download::Summary,
	directory: &std::path::Path,
	debug: bool,
) {
	let count = |status| {
		summary
			.outcomes
			.iter()
			.filter(|outcome| outcome.status == status)
			.count()
	};
	let bytes = summary.outcomes.iter().map(|outcome| outcome.bytes).sum();
	ui::heading("Batch summary");
	ui::grid(&[
		[
			style("Downloaded").green().bold().to_string(),
			count(Status::Downloaded).to_string(),
		],
		[
			style("Skipped").cyan().bold().to_string(),
			count(Status::Skipped).to_string(),
		],
		[
			style("Failed").red().bold().to_string(),
			(count(Status::Failed) + count(Status::MuxFailed)).to_string(),
		],
		[
			style("Interrupted").yellow().bold().to_string(),
			count(Status::Interrupted).to_string(),
		],
		[
			style("Size").bold().to_string(),
			HumanBytes(bytes).to_string(),
		],
		[
			style("Elapsed").bold().to_string(),
			HumanDuration(summary.elapsed).to_string(),
		],
		[
			style("Output").bold().to_string(),
			directory.display().to_string(),
		],
	]);
	for outcome in summary.outcomes.iter().filter(|outcome| {
		matches!(
			outcome.status,
			Status::Failed | Status::MuxFailed | Status::Interrupted
		)
	}) {
		ui::failure(format!(
			"{}: {}",
			outcome.episode,
			outcome.detail.render(debug)
		));
	}
	if summary
		.outcomes
		.iter()
		.any(|outcome| outcome.status == Status::Failed)
	{
		ui::note("Run the same command again to resume preserved .part files.");
	}
}
