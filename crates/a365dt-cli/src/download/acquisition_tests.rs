use std::{
	collections::VecDeque,
	path::{Path, PathBuf},
	sync::Mutex,
	time::{SystemTime, UNIX_EPOCH},
};

use bytes::Bytes;
use indicatif::ProgressBar;
use pretty_assertions::assert_eq;
use reqwest::{StatusCode, header::HeaderMap};
use tokio::sync::watch;

use super::adapter::{Adapter, Request, Response};
use super::{
	Acquisition, AcquisitionStatus, ResumeState, acquire, finalize,
	first_nonzero, part_path, protect_mismatch, resume_start, retryable,
	valid_content_range, verified_size,
};
use crate::error::Error;

type Body = VecDeque<Result<Bytes, Error>>;

struct ScriptedAdapter {
	responses: Mutex<VecDeque<Response<Body>>>,
	requests: Mutex<Vec<ObservedRequest>>,
}

impl ScriptedAdapter {
	fn new(responses: impl IntoIterator<Item = Response<Body>>) -> Self {
		Self {
			responses: Mutex::new(responses.into_iter().collect()),
			requests: Mutex::new(Vec::new()),
		}
	}

	fn requests(&self) -> Vec<ObservedRequest> {
		self.requests.lock().unwrap().clone()
	}
}

impl Adapter for ScriptedAdapter {
	type Body = Body;

	async fn send(
		&self,
		request: Request<'_>,
	) -> Result<Response<Self::Body>, Error> {
		self.requests.lock().unwrap().push(request.into());
		self.responses
			.lock()
			.unwrap()
			.pop_front()
			.ok_or_else(|| Error::new("Unexpected media request."))
	}

	async fn chunk(
		&self,
		body: &mut Self::Body,
	) -> Result<Option<Bytes>, Error> {
		body.pop_front().transpose()
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ObservedRequest {
	Head(String),
	Get(String),
	Resume {
		url: String,
		start: u64,
		validator: String,
	},
}

impl From<Request<'_>> for ObservedRequest {
	fn from(request: Request<'_>) -> Self {
		match request {
			Request::Head(url) => Self::Head(url.into()),
			Request::Get(url) => Self::Get(url.into()),
			Request::Resume {
				url,
				start,
				validator,
			} => Self::Resume {
				url: url.into(),
				start,
				validator: validator.into(),
			},
		}
	}
}

struct TestDirectory(PathBuf);

impl TestDirectory {
	fn new() -> Self {
		let unique = format!(
			"a365dt-test-{}-{}",
			std::process::id(),
			SystemTime::now()
				.duration_since(UNIX_EPOCH)
				.unwrap()
				.as_nanos()
		);
		let path = std::env::temp_dir().join(unique);
		std::fs::create_dir(&path).unwrap();
		Self(path)
	}
}

impl Drop for TestDirectory {
	fn drop(&mut self) {
		std::fs::remove_dir_all(&self.0).unwrap();
	}
}

#[derive(Debug, Eq, PartialEq)]
struct Observation {
	acquisition: Acquisition,
	requests: Vec<ObservedRequest>,
	files: Vec<(String, Vec<u8>)>,
}

fn response(
	content_length: u64,
	body: impl IntoIterator<Item = &'static [u8]>,
) -> Response<Body> {
	Response {
		status: StatusCode::OK,
		headers: HeaderMap::new(),
		content_length: Some(content_length),
		body: body
			.into_iter()
			.map(|chunk| Ok(Bytes::from_static(chunk)))
			.collect(),
	}
}

async fn observe(
	directory: &Path,
	acquisition: Acquisition,
	adapter: &ScriptedAdapter,
) -> Observation {
	let mut entries = tokio::fs::read_dir(directory).await.unwrap();
	let mut files = Vec::new();
	while let Some(entry) = entries.next_entry().await.unwrap() {
		files.push((
			entry.file_name().to_string_lossy().into_owned(),
			tokio::fs::read(entry.path()).await.unwrap(),
		));
	}
	files.sort_unstable_by(|left, right| left.0.cmp(&right.0));
	Observation {
		acquisition,
		requests: adapter.requests(),
		files,
	}
}

#[tokio::test]
async fn acquires_and_finalizes_new_video() {
	let directory = TestDirectory::new();
	let final_path = directory.0.join("episode.mp4");
	let adapter = ScriptedAdapter::new([
		response(4, []),
		response(4, [b"good".as_slice()]),
	]);
	let (_cancel_tx, mut cancel) = watch::channel(false);

	let acquisition = acquire(
		&adapter,
		"https://media.test/episode.mp4",
		&final_path,
		&ProgressBar::hidden(),
		&mut cancel,
	)
	.await
	.unwrap();

	assert_eq!(
		observe(&directory.0, acquisition, &adapter).await,
		Observation {
			acquisition: Acquisition {
				status: AcquisitionStatus::Downloaded,
				bytes: 4,
			},
			requests: vec![
				ObservedRequest::Head("https://media.test/episode.mp4".into()),
				ObservedRequest::Get("https://media.test/episode.mp4".into()),
			],
			files: vec![("episode.mp4".into(), b"good".to_vec())],
		}
	);
}

#[tokio::test]
async fn skips_matching_final_video_without_requesting_its_body() {
	let directory = TestDirectory::new();
	let final_path = directory.0.join("episode.mp4");
	tokio::fs::write(&final_path, b"good").await.unwrap();
	let adapter = ScriptedAdapter::new([response(4, [])]);
	let (_cancel_tx, mut cancel) = watch::channel(false);

	let acquisition = acquire(
		&adapter,
		"https://media.test/episode.mp4",
		&final_path,
		&ProgressBar::hidden(),
		&mut cancel,
	)
	.await
	.unwrap();

	assert_eq!(
		observe(&directory.0, acquisition, &adapter).await,
		Observation {
			acquisition: Acquisition {
				status: AcquisitionStatus::Skipped,
				bytes: 4,
			},
			requests: vec![ObservedRequest::Head(
				"https://media.test/episode.mp4".into()
			)],
			files: vec![("episode.mp4".into(), b"good".to_vec())],
		}
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
	let unique = format!(
		"a365dt-test-{}-{}",
		std::process::id(),
		std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_nanos()
	);
	let directory = std::env::temp_dir().join(unique);
	tokio::fs::create_dir(&directory).await.unwrap();
	let final_path = directory.join("episode.mp4");
	tokio::fs::write(&final_path, b"bad").await.unwrap();

	protect_mismatch(&final_path, 4).await.unwrap();
	let part = part_path(&final_path);
	tokio::fs::write(&part, b"good").await.unwrap();
	finalize(&part, &final_path).await.unwrap();

	assert_eq!(tokio::fs::read(&final_path).await.unwrap(), b"good");
	assert_eq!(directory.join("episode.mp4.corrupt").exists(), false);
	tokio::fs::remove_dir_all(directory).await.unwrap();
}
