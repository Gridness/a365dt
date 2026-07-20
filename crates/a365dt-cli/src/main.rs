mod api;
mod download;
mod error;
mod select;
mod ui;

use std::{
	collections::VecDeque, num::NonZeroUsize, path::PathBuf, process::ExitCode,
};

use clap::Parser;
use indicatif::{HumanBytes, HumanDuration};
use tokio::{fs, process::Command, task::JoinSet};

use crate::{
	api::{Anime365, Episode, Translation, series_id_from_url},
	download::{Job, Status},
	error::Error,
	select::Release,
};

const ACCESS_TOKEN_HELP: &str = r#"ANIME365_ACCESS_TOKEN is missing or empty.

Obtain an access token:
1. Sign in to https://anime365.ru in your browser.
2. Open https://anime365.ru/api-clients.
3. Enter a client name and click "Создать клиент".
4. Follow the displayed link to get your access token.

Save the token in a password manager or OS credential store. Do not put it in
project files, shell profiles, command arguments, or shell history.

Pass it to a365dt through the environment for the current shell:

macOS/Linux (bash or zsh):
  printf 'Anime365 access token: '; read -rs ANIME365_ACCESS_TOKEN; printf '\n'
  export ANIME365_ACCESS_TOKEN
  a365dt
  unset ANIME365_ACCESS_TOKEN

PowerShell 7:
  $env:ANIME365_ACCESS_TOKEN = Read-Host 'Anime365 access token' -MaskInput
  a365dt
  Remove-Item Env:ANIME365_ACCESS_TOKEN"#;

#[derive(Parser)]
#[command(
	version,
	about = "Download Anime365 episodes without guessing translations"
)]
struct Args {
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

#[tokio::main]
async fn main() -> ExitCode {
	ui::init();
	let args = Args::parse();
	let debug = args.debug;
	match run(args).await {
		Ok(code) => code,
		Err(error) => {
			ui::failure(error.render(debug));
			ExitCode::FAILURE
		}
	}
}

async fn run(args: Args) -> Result<ExitCode, Error> {
	ui::heading("a365dt  ◆  Anime365 downloader");
	let token = std::env::var("ANIME365_ACCESS_TOKEN")
		.map_err(|_| Error::new(ACCESS_TOKEN_HELP))?;
	if token.trim().is_empty() {
		return Err(Error::new(ACCESS_TOKEN_HELP));
	}
	let api = Anime365::new(token)?;
	ui::note("Validating Anime365 access…");
	api.validate().await?;
	ui::success("Authenticated");

	let input = if args.query.is_empty() {
		ui::prompt("Search title or Anime365 catalogue URL:")?
	} else {
		args.query.join(" ")
	};
	let series =
		if input.starts_with("http://") || input.starts_with("https://") {
			let id = series_id_from_url(&input).ok_or_else(|| {
				"Enter an official Anime365 series catalogue URL.".to_owned()
			})?;
			api.series(id).await?
		} else {
			let spinner = ui::spinner("Searching Anime365…");
			let matches = api.search(&input).await;
			spinner.finish_and_clear();
			select::choose_series(&matches?)?
		};
	ui::success(format!("Selected {}", series.title));
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
	let summary = download::run(api, jobs, args.jobs.get(), args.debug).await;
	print_summary(&summary, &directory, args.debug);
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
	ui::success(format!(
		"{} downloaded • {} skipped • {} failed • {} interrupted",
		count(Status::Downloaded),
		count(Status::Skipped),
		count(Status::Failed) + count(Status::MuxFailed),
		count(Status::Interrupted)
	));
	ui::note(format!(
		"{} • {} • {}",
		HumanBytes(bytes),
		HumanDuration(summary.elapsed),
		directory.display()
	));
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
