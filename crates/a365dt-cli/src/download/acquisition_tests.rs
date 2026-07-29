use std::{
	collections::VecDeque,
	path::{Path, PathBuf},
	sync::Mutex,
};

use pretty_assertions::assert_eq;
use reqwest::{StatusCode, header::HeaderMap};
use tokio::sync::watch;

use super::{
	Adapter, AssetRequest, AssetResponse, EpisodeRequest, Outcome,
	ProgressEvent, RefreshedMedia, ResumeState, TransferProgress, acquire,
	finalize, first_nonzero, part_path, protect_mismatch, resume_start,
	retryable, valid_content_range, verified_size,
};
use crate::{
	api::{Episode, Translation},
	error::Error,
	select::PlannedRelease,
};

struct ScriptedAdapter {
	responses: Mutex<VecDeque<ScriptedResponse>>,
}

struct NoProgress;

impl TransferProgress for NoProgress {
	fn total(&self) -> Option<u64> {
		None
	}

	fn report(&self, _event: ProgressEvent<'_>) {}
}

impl ScriptedAdapter {
	fn new(responses: impl IntoIterator<Item = ScriptedResponse>) -> Self {
		Self {
			responses: Mutex::new(responses.into_iter().collect()),
		}
	}
}

impl Adapter for ScriptedAdapter {
	type Response = ScriptedResponse;

	async fn asset(
		&self,
		_request: AssetRequest<'_>,
	) -> Result<Self::Response, Error> {
		self.responses
			.lock()
			.unwrap()
			.pop_front()
			.ok_or_else(|| Error::new("Unexpected asset request."))
	}

	async fn refresh(
		&self,
		_translation_id: u64,
		_height: u16,
	) -> Result<RefreshedMedia, Error> {
		Err(Error::new("Unexpected media refresh."))
	}
}

struct ScriptedResponse {
	content_length: Option<u64>,
	headers: HeaderMap,
	chunks: VecDeque<Vec<u8>>,
}

impl ScriptedResponse {
	fn metadata(content_length: u64) -> Self {
		Self {
			content_length: Some(content_length),
			headers: HeaderMap::new(),
			chunks: VecDeque::new(),
		}
	}

	fn body(bytes: &[u8]) -> Self {
		Self {
			content_length: Some(bytes.len() as u64),
			headers: HeaderMap::new(),
			chunks: VecDeque::from([bytes.to_vec()]),
		}
	}
}

impl AssetResponse for ScriptedResponse {
	type Chunk = Vec<u8>;

	fn status(&self) -> StatusCode {
		StatusCode::OK
	}

	fn headers(&self) -> &HeaderMap {
		&self.headers
	}

	fn content_length(&self) -> Option<u64> {
		self.content_length
	}

	async fn chunk(&mut self) -> Result<Option<Self::Chunk>, Error> {
		Ok(self.chunks.pop_front())
	}
}

struct TestDirectory(PathBuf);

impl TestDirectory {
	fn new() -> Self {
		let unique = format!(
			"a365dt-test-{}-{}",
			std::process::id(),
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap()
				.as_nanos()
		);
		let path = std::env::temp_dir().join(unique);
		std::fs::create_dir(&path).unwrap();
		Self(path)
	}

	fn join(&self, path: impl AsRef<Path>) -> PathBuf {
		self.0.join(path)
	}

	fn files(&self) -> Vec<(String, Vec<u8>)> {
		let mut files = std::fs::read_dir(&self.0)
			.unwrap()
			.map(|entry| {
				let path = entry.unwrap().path();
				(
					path.file_name().unwrap().to_string_lossy().into_owned(),
					std::fs::read(path).unwrap(),
				)
			})
			.collect::<Vec<_>>();
		files.sort_by(|left, right| left.0.cmp(&right.0));
		files
	}
}

impl Drop for TestDirectory {
	fn drop(&mut self) {
		std::fs::remove_dir_all(&self.0).unwrap();
	}
}

fn release() -> PlannedRelease {
	PlannedRelease {
		episode: Episode {
			id: 42,
			episode_int: "1".into(),
			episode_full: "1 серия".into(),
		},
		translation: Translation {
			id: 365,
			episode_id: 42,
			kind: "sub".into(),
			language: "ru".into(),
			authors_summary: "Team".into(),
		},
		height: 1080,
		media_url: "https://media.example/episode.mp4".into(),
		subtitle_url: None,
	}
}

#[tokio::test]
async fn acquires_and_finalizes_a_new_video_through_the_episode_interface() {
	let adapter = ScriptedAdapter::new([
		ScriptedResponse::metadata(4),
		ScriptedResponse::body(b"good"),
	]);
	let directory = TestDirectory::new();
	let video = directory.join("episode.mp4");
	let progress = NoProgress;
	let (_cancel, mut cancellation) = watch::channel(false);

	let outcome = acquire(
		&adapter,
		EpisodeRequest::new(&release(), &video, &progress),
		&mut cancellation,
	)
	.await;

	assert_eq!(
		(outcome, directory.files()),
		(
			Outcome::Downloaded {
				bytes: 4,
				subtitle_url: None,
			},
			vec![("episode.mp4".into(), b"good".to_vec())],
		)
	);
}

#[tokio::test]
async fn skips_a_matching_final_video_without_transferring_its_body() {
	let adapter = ScriptedAdapter::new([ScriptedResponse::metadata(4)]);
	let directory = TestDirectory::new();
	let video = directory.join("episode.mp4");
	std::fs::write(&video, b"good").unwrap();
	let progress = NoProgress;
	let (_cancel, mut cancellation) = watch::channel(false);

	let outcome = acquire(
		&adapter,
		EpisodeRequest::new(&release(), &video, &progress),
		&mut cancellation,
	)
	.await;

	assert_eq!(
		(outcome, directory.files()),
		(
			Outcome::Skipped {
				bytes: 4,
				subtitle_url: None,
			},
			vec![("episode.mp4".into(), b"good".to_vec())],
		)
	);
}

#[test]
fn validates_resumed_content_range() {
	assert_eq!(valid_content_range("bytes 50-99/100", 50, 100), true);
	assert_eq!(valid_content_range("bytes 0-99/100", 50, 100), false);
	assert_eq!(valid_content_range("bytes 50-99/101", 50, 100), false);
}

#[test]
fn resumes_only_when_the_partial_file_belongs_to_the_current_asset() {
	let old = ResumeState {
		total: 100,
		validator: "old".into(),
	};
	let current = ResumeState {
		total: 100,
		validator: "current".into(),
	};

	assert_eq!(
		[
			resume_start(50, Some(100), Some(&current), Some(&current)),
			resume_start(50, Some(100), Some(&old), Some(&current)),
			resume_start(50, Some(100), None, Some(&current)),
			resume_start(101, Some(100), Some(&current), Some(&current)),
		],
		[50, 0, 0, 0]
	);
}

#[test]
fn accepts_transfer_without_declared_size() {
	assert_eq!(verified_size(None, 42).unwrap(), 42);
}

#[test]
fn preserves_known_size_when_retry_reports_zero() {
	assert_eq!(
		[
			first_nonzero(Some(200), Some(100)),
			first_nonzero(Some(0), Some(100)),
			first_nonzero(None, Some(100)),
			first_nonzero(Some(0), None),
			first_nonzero(None, None),
		],
		[Some(200), Some(100), Some(100), None, None]
	);
}

#[test]
fn rejects_empty_transfer_with_relevant_error() {
	assert_eq!(
		verified_size(Some(0), 0).unwrap_err(),
		retryable("The media server returned an empty file.", None)
	);
}

#[tokio::test]
async fn replaces_mismatched_file_and_cleans_backup() {
	let directory = TestDirectory::new();
	let final_path = directory.join("episode.mp4");
	tokio::fs::write(&final_path, b"bad").await.unwrap();

	protect_mismatch(&final_path, 4).await.unwrap();
	let part = part_path(&final_path);
	tokio::fs::write(&part, b"good").await.unwrap();
	finalize(&part, &final_path).await.unwrap();

	assert_eq!(
		directory.files(),
		vec![("episode.mp4".into(), b"good".to_vec())]
	);
}
