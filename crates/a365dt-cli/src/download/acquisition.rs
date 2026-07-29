use std::{
	io::ErrorKind,
	path::{Path, PathBuf},
	time::Duration,
};

use indicatif::ProgressBar;
use reqwest::{Method, Response, StatusCode, header};
use tokio::{
	fs::{self, OpenOptions},
	io::AsyncWriteExt,
	sync::watch,
	time::sleep,
};

use super::RETRIES;
use crate::{api::Anime365, error::Error};

#[derive(Debug, Eq, PartialEq)]
pub(super) struct TransferError {
	pub(super) error: Error,
	pub(super) retry: bool,
	pub(super) retry_after: Option<Duration>,
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

pub(super) async fn retry_transfer(
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

pub(super) async fn transfer(
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
pub(super) fn backoff(attempt: usize, seed: u64) -> Duration {
	Duration::from_millis((1_u64 << attempt) * 1000 + seed % 251)
}
fn part_path(path: &Path) -> PathBuf {
	PathBuf::from(format!("{}.part", path.display()))
}
fn resume_state_path(part: &Path) -> PathBuf {
	PathBuf::from(format!("{}.state", part.display()))
}
pub(super) async fn file_len(path: &Path) -> u64 {
	fs::metadata(path)
		.await
		.map_or(0, |metadata| metadata.len())
}

#[cfg(test)]
#[path = "acquisition_tests.rs"]
mod tests;
