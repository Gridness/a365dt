use std::{
	collections::VecDeque,
	io::{ErrorKind, IsTerminal},
	path::{Path, PathBuf},
	sync::Arc,
	time::{Duration, Instant},
};

use console::style;
use indicatif::{
	MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle,
};
use reqwest::{Method, Response, StatusCode, header};
use tokio::{
	fs::{self, OpenOptions},
	io::AsyncWriteExt,
	sync::watch,
	task::JoinSet,
	time::sleep,
};

mod mux;

use crate::{
	api::{Anime365, Episode},
	error::Error,
	select::PlannedRelease,
};

const RETRIES: usize = 3;

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

#[derive(Debug)]
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

#[derive(Debug, Eq, PartialEq)]
struct TransferError {
	error: Error,
	retry: bool,
	retry_after: Option<Duration>,
}

#[derive(Debug, Eq, PartialEq)]
struct ResumeState {
	total: u64,
	validator: String,
}

impl ResumeState {
	fn from_response(response: &Response, total: Option<u64>) -> Option<Self> {
		let validator = response
			.headers()
			.get(header::ETAG)
			.and_then(|value| value.to_str().ok())
			.filter(|value| !value.starts_with("W/"))
			.or_else(|| {
				response
					.headers()
					.get(header::LAST_MODIFIED)
					.and_then(|value| value.to_str().ok())
			})?;
		Some(Self {
			total: total?,
			validator: validator.into(),
		})
	}

	fn parse(value: &str) -> Option<Self> {
		let (total, validator) = value.split_once('\n')?;
		Some(Self {
			total: total.parse().ok()?,
			validator: validator.into(),
		})
	}

	fn serialize(&self) -> String {
		format!("{}\n{}", self.total, self.validator)
	}
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
	mut cancel: watch::Receiver<bool>,
) -> Summary {
	let started = Instant::now();
	let bars = Arc::new(Bars::new(jobs.len() as u64, debug));
	let mut pending = VecDeque::from(jobs);
	let mut active = JoinSet::new();
	for _ in 0..concurrency {
		spawn_next(&mut active, &mut pending, &api, &bars, &cancel);
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
					spawn_next(&mut active, &mut pending, &api, &bars, &cancel);
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

fn spawn_next(
	active: &mut JoinSet<Outcome>,
	pending: &mut VecDeque<Job>,
	api: &Anime365,
	bars: &Arc<Bars>,
	cancel: &watch::Receiver<bool>,
) {
	if let Some(job) = pending.pop_front() {
		let api = api.clone();
		let bars = Arc::clone(bars);
		let cancel = cancel.clone();
		active.spawn(async move { download_job(api, job, bars, cancel).await });
	}
}

async fn download_job(
	api: Anime365,
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
	let mut video_url = job.release.media_url.clone();
	let mut subtitle_url = job.release.subtitle_url.clone();
	let mut video_result = None;
	for attempt in 0..=RETRIES {
		if attempt > 0 {
			match api.embed(job.release.translation.id).await {
				Ok(embed) => {
					video_url = embed
						.download
						.into_iter()
						.find(|item| item.height == job.release.height)
						.and_then(|item| item.url)
						.unwrap_or(video_url);
					subtitle_url = embed.subtitles_url.or(subtitle_url);
				}
				Err(error) => bar.set_message(format!(
					"{episode} • refresh failed: {}",
					error.render(bars.debug)
				)),
			}
		}
		match transfer(&api, &video_url, &video, true, &bar, &mut cancel).await
		{
			Ok(result) => {
				video_result = Some(result);
				break;
			}
			Err(error) if error.error.message() == "interrupted" => {
				bar.finish_and_clear();
				return Outcome {
					episode,
					status: Status::Interrupted,
					bytes: file_len(&video).await,
					detail:
						"Interrupted; the resumable partial file was saved."
							.into(),
				};
			}
			Err(error) if error.retry && attempt < RETRIES => {
				bar.set_message(format!(
					"{episode} • retry {}/{}",
					attempt + 1,
					RETRIES
				));
				sleep(error.retry_after.unwrap_or_else(|| {
					backoff(attempt, job.release.episode.id)
				}))
				.await;
			}
			Err(error) => {
				bar.finish_and_clear();
				return Outcome {
					episode,
					status: Status::Failed,
					bytes: 0,
					detail: error.error,
				};
			}
		}
	}
	let (video_skipped, bytes) =
		video_result.expect("retry loop always returns or succeeds");
	bar.finish_and_clear();
	let mut subtitle_skipped = true;
	if let Some(url) = &subtitle_url {
		let sub_bar = bars.spinner(&format!("{episode} • ASS"));
		match retry_transfer(&api, url, &subtitle, false, &sub_bar, &mut cancel)
			.await
		{
			Ok((skipped, _)) => subtitle_skipped = skipped,
			Err(error) => {
				sub_bar.finish_and_clear();
				let status = if error.error.message() == "interrupted" {
					Status::Interrupted
				} else {
					Status::Failed
				};
				return Outcome {
					episode,
					status,
					bytes,
					detail: error.error.context("Subtitle download failed"),
				};
			}
		}
		sub_bar.finish_and_clear();
	}
	if job.mux && subtitle_url.is_some() {
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
		status: if video_skipped && subtitle_skipped {
			Status::Skipped
		} else {
			Status::Downloaded
		},
		bytes,
		detail: format!("{}", video.display()).into(),
	}
}

async fn retry_transfer(
	api: &Anime365,
	url: &str,
	path: &Path,
	resume: bool,
	bar: &ProgressBar,
	cancel: &mut watch::Receiver<bool>,
) -> Result<(bool, u64), TransferError> {
	for attempt in 0..=RETRIES {
		match transfer(api, url, path, resume, bar, cancel).await {
			Ok(result) => return Ok(result),
			Err(error) if error.retry && attempt < RETRIES => {
				sleep(error.retry_after.unwrap_or_else(|| backoff(attempt, 0)))
					.await;
			}
			Err(error) => return Err(error),
		}
	}
	unreachable!()
}

async fn transfer(
	api: &Anime365,
	url: &str,
	final_path: &Path,
	resume: bool,
	bar: &ProgressBar,
	cancel: &mut watch::Receiver<bool>,
) -> Result<(bool, u64), TransferError> {
	let part = part_path(final_path);
	let mut current_state = None;
	let total = if resume {
		let head = api.asset(Method::HEAD, url).await.map_err(network)?;
		check_status(&head)?;
		let total = first_nonzero(head.content_length(), bar.length());
		current_state = ResumeState::from_response(&head, total);
		if let Some(total) = total {
			bar.set_length(total);
			protect_mismatch(final_path, total)
				.await
				.map_err(io_error)?;
			if final_path.exists() {
				remove_resume_state(&part).await.map_err(io_error)?;
				remove_corrupt_backups(final_path).await.map_err(io_error)?;
				bar.set_position(total);
				return Ok((true, total));
			}
		} else {
			bar.unset_length();
		}
		total
	} else {
		None
	};
	let part_len = file_len(&part).await;
	let saved_state = if part_len > 0 {
		read_resume_state(&part).await.map_err(io_error)?
	} else {
		None
	};
	let mut start = resume_start(
		part_len,
		total,
		saved_state.as_ref(),
		current_state.as_ref(),
	);
	if total == Some(start) && start > 0 {
		finalize(&part, final_path).await.map_err(io_error)?;
		bar.set_position(start);
		return Ok((false, start));
	}
	let mut response = if start > 0 {
		api.asset_from(
			url,
			start,
			&saved_state
				.expect("resumed part has matching state")
				.validator,
		)
		.await
	} else {
		api.asset(Method::GET, url).await
	}
	.map_err(network)?;
	check_status(&response)?;
	if start > 0 && response.status() != StatusCode::PARTIAL_CONTENT {
		start = 0;
	}
	let total = if response.status() == StatusCode::PARTIAL_CONTENT {
		total
	} else {
		first_nonzero(response.content_length(), total)
	};
	if let Some(total) = total {
		bar.set_length(total);
	}
	if response.status() == StatusCode::PARTIAL_CONTENT {
		let total = total.ok_or_else(|| {
			fatal(
				"The media server did not provide the file size needed to resume the download.",
			)
		})?;
		validate_content_range(&response, start, total)?;
	}
	let mut file = OpenOptions::new()
		.create(true)
		.write(true)
		.append(start > 0)
		.truncate(start == 0)
		.open(&part)
		.await
		.map_err(io_error)?;
	if resume && start == 0 {
		write_resume_state(
			&part,
			ResumeState::from_response(&response, total).as_ref(),
		)
		.await
		.map_err(io_error)?;
	}
	bar.set_position(start);
	bar.reset_eta();
	loop {
		tokio::select! {
			changed = cancel.changed() => {
				if changed.is_err() || *cancel.borrow() {
					file.sync_all().await.map_err(io_error)?;
					return Err(fatal("interrupted"));
				}
			}
			chunk = response.chunk() => match chunk.map_err(|error| {
				network(Error::with_debug(
					"The media download was interrupted by a network error.",
					error.without_url(),
				))
			})? {
				Some(chunk) => {
					file.write_all(&chunk).await.map_err(io_error)?;
					bar.inc(chunk.len() as u64);
				}
				None => break,
			}
		}
	}
	file.flush().await.map_err(io_error)?;
	file.sync_all().await.map_err(io_error)?;
	let bytes = verified_size(total, file_len(&part).await)?;
	protect_mismatch(final_path, bytes)
		.await
		.map_err(io_error)?;
	if final_path.exists() {
		fs::remove_file(&part).await.map_err(io_error)?;
		remove_corrupt_backups(final_path).await.map_err(io_error)?;
		bar.set_position(bytes);
		return Ok((true, bytes));
	}
	finalize(&part, final_path).await.map_err(io_error)?;
	Ok((false, bytes))
}

async fn protect_mismatch(path: &Path, expected: u64) -> std::io::Result<()> {
	if !path.exists() || file_len(path).await == expected {
		return Ok(());
	}
	for suffix in 0.. {
		let suffix = if suffix == 0 {
			".corrupt".into()
		} else {
			format!(".corrupt.{suffix}")
		};
		let backup = PathBuf::from(format!("{}{suffix}", path.display()));
		if !backup.exists() {
			fs::rename(path, &backup).await?;
			return Ok(());
		}
	}
	unreachable!()
}

async fn finalize(part: &Path, final_path: &Path) -> std::io::Result<()> {
	fs::rename(part, final_path).await?;
	remove_resume_state(part).await?;
	remove_corrupt_backups(final_path).await
}

fn resume_start(
	part_len: u64,
	total: Option<u64>,
	saved: Option<&ResumeState>,
	current: Option<&ResumeState>,
) -> u64 {
	let Some(total) = total else {
		return 0;
	};
	if part_len > total || (part_len > 0 && saved != current) {
		return 0;
	}
	part_len
}

async fn read_resume_state(
	part: &Path,
) -> std::io::Result<Option<ResumeState>> {
	match fs::read_to_string(resume_state_path(part)).await {
		Ok(value) => Ok(ResumeState::parse(&value)),
		Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
		Err(error) => Err(error),
	}
}

async fn write_resume_state(
	part: &Path,
	state: Option<&ResumeState>,
) -> std::io::Result<()> {
	if let Some(state) = state {
		fs::write(resume_state_path(part), state.serialize()).await
	} else {
		remove_resume_state(part).await
	}
}

async fn remove_resume_state(part: &Path) -> std::io::Result<()> {
	match fs::remove_file(resume_state_path(part)).await {
		Ok(()) => Ok(()),
		Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
		Err(error) => Err(error),
	}
}

async fn remove_corrupt_backups(path: &Path) -> std::io::Result<()> {
	let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
		return Ok(());
	};
	let prefix = format!("{name}.corrupt");
	let Some(parent) = path.parent() else {
		return Ok(());
	};
	let mut entries = fs::read_dir(parent).await?;
	while let Some(entry) = entries.next_entry().await? {
		let name = entry.file_name();
		let name = name.to_string_lossy();
		if name == prefix
			|| name
				.strip_prefix(&prefix)
				.is_some_and(|suffix| suffix.starts_with('.'))
		{
			fs::remove_file(entry.path()).await?;
		}
	}
	Ok(())
}

fn validate_content_range(
	response: &Response,
	start: u64,
	total: u64,
) -> Result<(), TransferError> {
	let value = response
		.headers()
		.get(header::CONTENT_RANGE)
		.and_then(|value| value.to_str().ok())
		.unwrap_or("");
	if !valid_content_range(value, start, total) {
		return Err(fatal(Error::with_debug(
			"The media server returned invalid resume information.",
			format!("invalid Content-Range: {value}"),
		)));
	}
	Ok(())
}

fn valid_content_range(value: &str, start: u64, total: u64) -> bool {
	value.starts_with(&format!("bytes {start}-"))
		&& value.ends_with(&format!("/{total}"))
}

fn check_status(response: &Response) -> Result<(), TransferError> {
	let status = response.status();
	if status.is_success() {
		return Ok(());
	}
	let delay = response
		.headers()
		.get(header::RETRY_AFTER)
		.and_then(|value| value.to_str().ok())
		.and_then(|value| value.parse().ok())
		.map(Duration::from_secs);
	let retry = status == StatusCode::REQUEST_TIMEOUT
		|| status == StatusCode::TOO_MANY_REQUESTS
		|| status.is_server_error();
	Err(TransferError {
		error: Error::new(format!(
			"The media server rejected the download (HTTP {status})."
		)),
		retry,
		retry_after: delay,
	})
}

fn verified_size(
	expected: Option<u64>,
	received: u64,
) -> Result<u64, TransferError> {
	if received == 0 {
		return Err(retryable(
			"The media server returned an empty file.",
			None,
		));
	}
	if let Some(expected) = expected
		&& received != expected
	{
		return Err(retryable(
			format!(
				"The downloaded file was incomplete ({received} of {expected} bytes)."
			),
			None,
		));
	}
	Ok(received)
}

fn first_nonzero(first: Option<u64>, second: Option<u64>) -> Option<u64> {
	first
		.filter(|value| *value > 0)
		.or_else(|| second.filter(|value| *value > 0))
}

fn network(error: Error) -> TransferError {
	retryable(error, None)
}
fn io_error(error: std::io::Error) -> TransferError {
	retryable(
		Error::with_debug("Could not read or write a download file.", error),
		None,
	)
}
fn fatal(error: impl Into<Error>) -> TransferError {
	TransferError {
		error: error.into(),
		retry: false,
		retry_after: None,
	}
}
fn retryable(
	error: impl Into<Error>,
	retry_after: Option<Duration>,
) -> TransferError {
	TransferError {
		error: error.into(),
		retry: true,
		retry_after,
	}
}
fn backoff(attempt: usize, seed: u64) -> Duration {
	Duration::from_millis((1_u64 << attempt) * 1000 + seed % 251)
}
fn part_path(path: &Path) -> PathBuf {
	PathBuf::from(format!("{}.part", path.display()))
}
fn resume_state_path(part: &Path) -> PathBuf {
	PathBuf::from(format!("{}.state", part.display()))
}
async fn file_len(path: &Path) -> u64 {
	fs::metadata(path)
		.await
		.map_or(0, |metadata| metadata.len())
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
		bar.set_style(ProgressStyle::with_template("{spinner:.cyan} {msg:24!} [{bar:24.cyan/blue}] {bytes}/{total_bytes} {bytes_per_sec} ETA {eta}").expect("valid style").progress_chars("━━╸"));
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
