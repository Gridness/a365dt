mod api;
mod auth;
mod download;
mod error;
mod poster;
mod search;
mod select;
mod series_cache;
mod series_search;
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

	#[arg(short, long, default_value = ".", value_name = "DIR")]
	output: PathBuf,

	#[arg(short, long, default_value = "4")]
	jobs: NonZeroUsize,

	/// Show technical error details.
	#[arg(long)]
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
	Completions { shell: Shell },
}

#[derive(Subcommand)]
enum CacheCommand {
	/// Clear the local cache.
	Prune,
}

#[tokio::main]
async fn main() -> ExitCode {
	let args = Args::parse();
	ui::init();
	let debug = args.debug;
	let active_download = Arc::new(Mutex::new(None));
	let interrupt_download = Arc::clone(&active_download);
	drop(tokio::spawn(async move {
		match signal::ctrl_c().await {
			Ok(()) => {
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
	if let Some(Commands::Completions { shell }) = args.command.as_ref() {
		generate(
			*shell,
			&mut Args::command(),
			"a365dt",
			&mut std::io::stdout(),
		);
		return ExitCode::SUCCESS;
	}
	match run(args, active_download).await {
		Ok(code) => code,
		Err(error) => {
			ui::failure(error.render(debug));
			ExitCode::FAILURE
		}
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

async fn run(
	args: Args,
	active_download: Arc<Mutex<Option<watch::Sender<bool>>>>,
) -> Result<ExitCode, Error> {
	ui::heading("a365dt  ◆  Anime365 downloader");
	if let Some(Commands::Cache {
		command: CacheCommand::Prune,
	}) = args.command
	{
		series_cache::prune().map_err(|error| {
			Error::with_debug("Could not clear the local cache.", error)
		})?;
		ui::success("Local cache cleared");
		return Ok(ExitCode::SUCCESS);
	}
	let access_token = auth::access_token()?;
	let api = Anime365::new(access_token.value().to_owned())?;
	ui::note("Validating Anime365 access…");
	api.validate().await?;
	ui::success("Authenticated");
	auth::store_if_requested(&access_token)?;

	let series = series_search::choose(&api, args.query.join(" ")).await?;
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
		ui::confirm(
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
