use std::{
	collections::VecDeque,
	io::IsTerminal,
	path::PathBuf,
	sync::Arc,
	time::{Duration, Instant},
};

use console::style;
use indicatif::{
	MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle,
};
use tokio::{fs, sync::watch, task::JoinSet};

mod acquisition;
mod mux;

use crate::{
	api::{Anime365, Episode},
	error::Error,
	select::PlannedRelease,
};
use acquisition::{
	AcquisitionStatus, Adapter, Anime365Adapter, acquire, file_len,
};

#[derive(Clone)]
pub struct Job {
	pub release: PlannedRelease,
	pub directory: PathBuf,
	pub mux: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
	Downloaded,
	Skipped,
	Failed,
	MuxFailed,
	Interrupted,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Outcome {
	pub episode: String,
	pub status: Status,
	pub bytes: u64,
	pub detail: Error,
}

pub struct Summary {
	pub outcomes: Vec<Outcome>,
	pub elapsed: Duration,
}

struct Bars {
	multi: MultiProgress,
	overall: ProgressBar,
	debug: bool,
}

impl Job {
	pub fn new(release: PlannedRelease, directory: PathBuf, mux: bool) -> Self {
		Self {
			release,
			directory,
			mux,
		}
	}

	fn stem(&self) -> String {
		format!(
			"{} [{}-{}] [{}] [{}p]",
			episode_tag(&self.release.episode),
			sanitize(&self.release.translation.kind, 20),
			sanitize(&self.release.translation.language, 10),
			sanitize(&self.release.translation.authors_summary, 64),
			self.release.height
		)
	}
}

pub async fn run(
	api: Anime365,
	jobs: Vec<Job>,
	concurrency: usize,
	debug: bool,
	cancel: watch::Receiver<bool>,
) -> Summary {
	let bars = Arc::new(Bars::new(jobs.len() as u64, debug));
	run_with_adapter(
		Arc::new(Anime365Adapter::new(api)),
		jobs,
		concurrency,
		bars,
		cancel,
	)
	.await
}

async fn run_with_adapter<A: Adapter + 'static>(
	adapter: Arc<A>,
	jobs: Vec<Job>,
	concurrency: usize,
	bars: Arc<Bars>,
	mut cancel: watch::Receiver<bool>,
) -> Summary {
	let started = Instant::now();
	let mut pending = VecDeque::from(jobs);
	let mut active = JoinSet::new();
	for _ in 0..concurrency {
		spawn_next(&mut active, &mut pending, &adapter, &bars, &cancel);
	}
	let mut interrupted = false;
	let mut outcomes = Vec::new();
	while !active.is_empty() {
		tokio::select! {
			result = active.join_next() => {
				let outcome = match result {
					Some(Ok(outcome)) => outcome,
					Some(Err(error)) => Outcome {
						episode: "Download task".into(),
						status: Status::Failed,
						bytes: 0,
						detail: Error::with_debug(
							"An internal download task stopped unexpectedly.",
							error,
						),
					},
					None => break,
				};
				bars.overall.inc(1);
				bars.line(&outcome);
				outcomes.push(outcome);
				if !interrupted {
					spawn_next(&mut active, &mut pending, &adapter, &bars, &cancel);
				}
			}
			result = cancel.changed(), if !interrupted => {
				interrupted = true;
				match result {
					Ok(()) => bars.message("Stopping cleanly; flushing partial files…"),
					Err(error) => bars.message(&Error::with_debug(
						"The cancellation channel closed; stopping active downloads.",
						error,
					).render(bars.debug)),
				}
			}
		}
	}
	if interrupted {
		outcomes.extend(pending.into_iter().map(|job| Outcome {
			episode: job.release.episode.episode_full,
			status: Status::Interrupted,
			bytes: 0,
			detail: "Not started because the download was interrupted.".into(),
		}));
	}
	bars.overall.finish_and_clear();
	Summary {
		outcomes,
		elapsed: started.elapsed(),
	}
}

fn spawn_next<A: Adapter + 'static>(
	active: &mut JoinSet<Outcome>,
	pending: &mut VecDeque<Job>,
	adapter: &Arc<A>,
	bars: &Arc<Bars>,
	cancel: &watch::Receiver<bool>,
) {
	if let Some(job) = pending.pop_front() {
		let adapter = Arc::clone(adapter);
		let bars = Arc::clone(bars);
		let cancel = cancel.clone();
		active.spawn(download_job(adapter, job, bars, cancel));
	}
}

async fn download_job<A: Adapter>(
	adapter: Arc<A>,
	job: Job,
	bars: Arc<Bars>,
	mut cancel: watch::Receiver<bool>,
) -> Outcome {
	let episode = job.release.episode.episode_full.clone();
	let stem = job.stem();
	let video = job.directory.join(format!("{stem}.mp4"));
	let subtitle = job.directory.join(format!("{stem}.ass"));
	let mkv = job.directory.join(format!("{stem}.mkv"));
	if job.mux && mkv.exists() {
		let _ = fs::remove_file(&video).await;
		let _ = fs::remove_file(&subtitle).await;
		let _ = fs::remove_file(mkv.with_extension("part.mkv")).await;
		return Outcome {
			episode,
			status: Status::Skipped,
			bytes: file_len(&mkv).await,
			detail: "MKV already exists.".into(),
		};
	}
	let bar =
		bars.transfer_bar(&format!("{episode} • {}p", job.release.height));
	let acquisition = match acquire(
		adapter.as_ref(),
		&job.release,
		&video,
		&subtitle,
		&bar,
		|| {
			bar.finish_and_clear();
			bars.spinner(&format!("{episode} • ASS"))
		},
		&mut cancel,
	)
	.await
	{
		Ok(acquisition) => acquisition,
		Err(error) => {
			bar.finish_and_clear();
			return Outcome {
				episode,
				status: Status::Failed,
				bytes: error.bytes,
				detail: error.error,
			};
		}
	};
	let video_skipped = match acquisition.status {
		AcquisitionStatus::Downloaded => false,
		AcquisitionStatus::Skipped => true,
		AcquisitionStatus::Interrupted => {
			bar.finish_and_clear();
			return Outcome {
				episode,
				status: Status::Interrupted,
				bytes: acquisition.bytes,
				detail: if acquisition.has_subtitle_asset {
					"Subtitle download failed: interrupted".into()
				} else {
					"Interrupted; the resumable partial file was saved.".into()
				},
			};
		}
	};
	let bytes = acquisition.bytes;
	let has_subtitle_asset = acquisition.has_subtitle_asset;
	bar.finish_and_clear();
	if job.mux && has_subtitle_asset {
		let mux_bar = bars.spinner(&format!("{episode} • muxing"));
		let result = mux::run(&video, &subtitle, &mkv).await;
		mux_bar.finish_and_clear();
		if let Err(error) = result {
			return Outcome {
				episode,
				status: Status::MuxFailed,
				bytes,
				detail: error,
			};
		}
		return Outcome {
			episode,
			status: Status::Downloaded,
			bytes,
			detail: format!("{}", mkv.display()).into(),
		};
	}
	Outcome {
		episode,
		status: if video_skipped {
			Status::Skipped
		} else {
			Status::Downloaded
		},
		bytes,
		detail: format!("{}", video.display()).into(),
	}
}

pub fn sanitize(input: &str, max: usize) -> String {
	let mut result = input
		.chars()
		.map(|character| {
			if character.is_control() || "<>:\"/\\|?*".contains(character) {
				'_'
			} else {
				character
			}
		})
		.take(max)
		.collect::<String>();
	result = result.trim_matches([' ', '.']).to_owned();
	if result.is_empty() {
		result.push_str("Unknown");
	}
	let upper = result.to_uppercase();
	if matches!(
		upper.as_str(),
		"CON"
			| "PRN" | "AUX"
			| "NUL" | "COM1"
			| "COM2" | "COM3"
			| "LPT1" | "LPT2"
			| "LPT3"
	) {
		result.insert(0, '_');
	}
	result
}

fn episode_tag(episode: &Episode) -> String {
	let label = episode
		.episode_full
		.strip_suffix(" серия")
		.unwrap_or(&episode.episode_full);
	if label != episode.episode_int {
		return sanitize(label, 32);
	}
	let (whole, fraction) = episode
		.episode_int
		.split_once('.')
		.map_or((label, None), |(whole, fraction)| (whole, Some(fraction)));
	let whole = whole
		.parse::<u64>()
		.map_or_else(|_| sanitize(whole, 12), |number| format!("E{number:02}"));
	fraction.map_or(whole.clone(), |fraction| format!("{whole}.{fraction}"))
}

impl Bars {
	fn new(total: u64, debug: bool) -> Self {
		let visible = std::io::stderr().is_terminal();
		let target = if visible {
			ProgressDrawTarget::stderr()
		} else {
			ProgressDrawTarget::hidden()
		};
		let multi = MultiProgress::with_draw_target(target);
		let overall = multi.add(ProgressBar::new(total));
		overall.set_style(
			ProgressStyle::with_template(
				"{prefix:.bold.cyan} [{bar:32.cyan/blue}] {pos}/{len} episodes",
			)
			.expect("valid style")
			.progress_chars("━━╸"),
		);
		overall.set_prefix("Batch");
		Self {
			multi,
			overall,
			debug,
		}
	}

	fn transfer_bar(&self, message: &str) -> ProgressBar {
		let bar = self.multi.add(ProgressBar::new(0));
		bar.set_style(ProgressStyle::with_template("{spinner:.cyan} {msg:24!} [{bar:24.cyan/blue}] {bytes:>11}/{total_bytes:11} {bytes_per_sec:>13} ETA {eta:>3}").expect("valid style").progress_chars("━━╸"));
		bar.enable_steady_tick(Duration::from_millis(100));
		bar.set_message(message.to_owned());
		bar
	}

	fn spinner(&self, message: &str) -> ProgressBar {
		let bar = self.multi.add(ProgressBar::new_spinner());
		bar.set_style(
			ProgressStyle::with_template("{spinner:.magenta} {msg}")
				.expect("valid style"),
		);
		bar.enable_steady_tick(Duration::from_millis(100));
		bar.set_message(message.to_owned());
		bar
	}

	fn line(&self, outcome: &Outcome) {
		let (icon, color) = match outcome.status {
			Status::Downloaded => ("✓", "green"),
			Status::Skipped => ("↷", "cyan"),
			Status::Failed | Status::MuxFailed => ("✗", "red"),
			Status::Interrupted => ("■", "yellow"),
		};
		let line = format!(
			"{} {} • {}",
			style(icon)
				.color256(match color {
					"green" => 2,
					"cyan" => 6,
					"red" => 1,
					_ => 3,
				})
				.bold(),
			outcome.episode,
			outcome.detail.render(self.debug)
		);
		self.message(&line);
	}

	fn message(&self, message: &str) {
		if self.multi.is_hidden() {
			eprintln!("{message}");
		} else {
			let _ = self.multi.println(message);
		}
	}
}

#[cfg(test)]
#[path = "download_tests.rs"]
mod tests;
