use std::{
	collections::VecDeque,
	path::{Path, PathBuf},
	sync::Mutex,
	time::{SystemTime, UNIX_EPOCH},
};

use bytes::Bytes;
use indicatif::ProgressBar;
use pretty_assertions::assert_eq;
use reqwest::{
	StatusCode,
	header::{self, HeaderMap, HeaderValue},
};
use tokio::sync::watch;

use super::adapter::{Adapter, Request, Response};
use super::{Acquisition, AcquisitionStatus, TransferError, acquire};
use crate::error::Error;

const URL: &str = "https://media.test/episode.mp4";
const ETAG: &str = "\"asset\"";

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

	fn path(&self, name: &str) -> PathBuf {
		self.0.join(name)
	}
}

impl Drop for TestDirectory {
	fn drop(&mut self) {
		std::fs::remove_dir_all(&self.0).unwrap();
	}
}

#[derive(Debug, Eq, PartialEq)]
struct Observation {
	result: Result<Acquisition, TransferError>,
	requests: Vec<ObservedRequest>,
	files: Vec<(String, Vec<u8>)>,
}

fn body(chunks: impl IntoIterator<Item = &'static [u8]>) -> Body {
	chunks
		.into_iter()
		.map(|chunk| Ok(Bytes::from_static(chunk)))
		.collect()
}

fn known_response(
	content_length: u64,
	chunks: impl IntoIterator<Item = &'static [u8]>,
) -> Response<Body> {
	Response {
		status: StatusCode::OK,
		headers: HeaderMap::new(),
		content_length: Some(content_length),
		body: body(chunks),
	}
}

fn unknown_response(
	chunks: impl IntoIterator<Item = &'static [u8]>,
) -> Response<Body> {
	Response {
		status: StatusCode::OK,
		headers: HeaderMap::new(),
		content_length: None,
		body: body(chunks),
	}
}

fn tagged_response(
	content_length: u64,
	validator: &'static str,
	chunks: impl IntoIterator<Item = &'static [u8]>,
) -> Response<Body> {
	let mut response = known_response(content_length, chunks);
	response
		.headers
		.insert(header::ETAG, HeaderValue::from_static(validator));
	response
}

fn partial_response(
	content_range: &'static str,
	chunks: impl IntoIterator<Item = &'static [u8]>,
) -> Response<Body> {
	let mut headers = HeaderMap::new();
	headers.insert(
		header::CONTENT_RANGE,
		HeaderValue::from_static(content_range),
	);
	Response {
		status: StatusCode::PARTIAL_CONTENT,
		headers,
		content_length: None,
		body: body(chunks),
	}
}

async fn write_partial(
	directory: &TestDirectory,
	bytes: &[u8],
	total: u64,
	validator: &str,
) {
	tokio::fs::write(directory.path("episode.mp4.part"), bytes)
		.await
		.unwrap();
	tokio::fs::write(
		directory.path("episode.mp4.part.state"),
		format!("{total}\n{validator}"),
	)
	.await
	.unwrap();
}

async fn acquire_and_observe(
	directory: &TestDirectory,
	adapter: &ScriptedAdapter,
) -> Observation {
	let (_cancel_tx, mut cancel) = watch::channel(false);
	let result = acquire(
		adapter,
		URL,
		&directory.path("episode.mp4"),
		&ProgressBar::hidden(),
		&mut cancel,
	)
	.await;
	observe(&directory.0, result, adapter).await
}

async fn observe(
	directory: &Path,
	result: Result<Acquisition, TransferError>,
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
		result,
		requests: adapter.requests(),
		files,
	}
}

fn downloaded(bytes: u64) -> Result<Acquisition, TransferError> {
	Ok(Acquisition {
		status: AcquisitionStatus::Downloaded,
		bytes,
	})
}

#[tokio::test]
async fn acquires_and_finalizes_new_video() {
	let directory = TestDirectory::new();
	let adapter = ScriptedAdapter::new([
		known_response(4, []),
		known_response(4, [b"good".as_slice()]),
	]);

	assert_eq!(
		acquire_and_observe(&directory, &adapter).await,
		Observation {
			result: downloaded(4),
			requests: vec![
				ObservedRequest::Head(URL.into()),
				ObservedRequest::Get(URL.into()),
			],
			files: vec![("episode.mp4".into(), b"good".to_vec())],
		}
	);
}

#[tokio::test]
async fn skips_matching_final_video_without_requesting_its_body() {
	let directory = TestDirectory::new();
	tokio::fs::write(directory.path("episode.mp4"), b"good")
		.await
		.unwrap();
	let adapter = ScriptedAdapter::new([known_response(4, [])]);

	assert_eq!(
		acquire_and_observe(&directory, &adapter).await,
		Observation {
			result: Ok(Acquisition {
				status: AcquisitionStatus::Skipped,
				bytes: 4,
			}),
			requests: vec![ObservedRequest::Head(URL.into())],
			files: vec![("episode.mp4".into(), b"good".to_vec())],
		}
	);
}

#[tokio::test]
async fn resumes_matching_partial_video_and_finalizes_it() {
	let directory = TestDirectory::new();
	write_partial(&directory, b"go", 4, ETAG).await;
	let adapter = ScriptedAdapter::new([
		tagged_response(4, ETAG, []),
		partial_response("bytes 2-3/4", [b"od".as_slice()]),
	]);

	assert_eq!(
		acquire_and_observe(&directory, &adapter).await,
		Observation {
			result: downloaded(4),
			requests: vec![
				ObservedRequest::Head(URL.into()),
				ObservedRequest::Resume {
					url: URL.into(),
					start: 2,
					validator: ETAG.into(),
				},
			],
			files: vec![("episode.mp4".into(), b"good".to_vec())],
		}
	);
}

async fn restarted_partial_observation(
	bytes: &[u8],
	total: u64,
	saved_validator: &str,
	head: Response<Body>,
	get_validator: &'static str,
) -> Observation {
	let directory = TestDirectory::new();
	write_partial(&directory, bytes, total, saved_validator).await;
	let adapter = ScriptedAdapter::new([
		head,
		tagged_response(4, get_validator, [b"good".as_slice()]),
	]);
	acquire_and_observe(&directory, &adapter).await
}

#[tokio::test]
async fn restarts_partial_video_when_it_cannot_be_resumed_safely() {
	let observations = [
		restarted_partial_observation(
			b"go",
			4,
			"\"old\"",
			tagged_response(4, "\"new\"", []),
			"\"new\"",
		)
		.await,
		restarted_partial_observation(
			b"go",
			4,
			ETAG,
			known_response(4, []),
			ETAG,
		)
		.await,
		restarted_partial_observation(
			b"stale",
			4,
			ETAG,
			tagged_response(4, ETAG, []),
			ETAG,
		)
		.await,
	];
	let expected = || Observation {
		result: downloaded(4),
		requests: vec![
			ObservedRequest::Head(URL.into()),
			ObservedRequest::Get(URL.into()),
		],
		files: vec![("episode.mp4".into(), b"good".to_vec())],
	};

	assert_eq!(observations, [expected(), expected(), expected()]);
}

async fn invalid_range_observation(content_range: &'static str) -> Observation {
	let directory = TestDirectory::new();
	write_partial(&directory, b"go", 4, ETAG).await;
	let adapter = ScriptedAdapter::new([
		tagged_response(4, ETAG, []),
		partial_response(content_range, [b"od".as_slice()]),
	]);
	acquire_and_observe(&directory, &adapter).await
}

#[tokio::test]
async fn rejects_invalid_resume_start_or_total_without_changing_partial() {
	for content_range in ["bytes 0-3/4", "bytes 2-3/5"] {
		assert_eq!(
			invalid_range_observation(content_range).await,
			Observation {
				result: Err(TransferError {
					error: Error::with_debug(
						"The media server returned invalid resume information.",
						format!("invalid Content-Range: {content_range}"),
					),
					retry: false,
					retry_after: None,
				}),
				requests: vec![
					ObservedRequest::Head(URL.into()),
					ObservedRequest::Resume {
						url: URL.into(),
						start: 2,
						validator: ETAG.into(),
					},
				],
				files: vec![
					("episode.mp4.part".into(), b"go".to_vec()),
					(
						"episode.mp4.part.state".into(),
						format!("4\n{ETAG}").into_bytes(),
					),
				],
			}
		);
	}
}

#[tokio::test]
async fn restarts_from_zero_when_server_ignores_resume_request() {
	let directory = TestDirectory::new();
	write_partial(&directory, b"go", 4, ETAG).await;
	let adapter = ScriptedAdapter::new([
		tagged_response(4, ETAG, []),
		tagged_response(4, ETAG, [b"good".as_slice()]),
	]);

	assert_eq!(
		acquire_and_observe(&directory, &adapter).await,
		Observation {
			result: downloaded(4),
			requests: vec![
				ObservedRequest::Head(URL.into()),
				ObservedRequest::Resume {
					url: URL.into(),
					start: 2,
					validator: ETAG.into(),
				},
			],
			files: vec![("episode.mp4".into(), b"good".to_vec())],
		}
	);
}

#[tokio::test]
async fn finalizes_complete_partial_without_requesting_body() {
	let directory = TestDirectory::new();
	write_partial(&directory, b"good", 4, ETAG).await;
	let adapter = ScriptedAdapter::new([tagged_response(4, ETAG, [])]);

	assert_eq!(
		acquire_and_observe(&directory, &adapter).await,
		Observation {
			result: downloaded(4),
			requests: vec![ObservedRequest::Head(URL.into())],
			files: vec![("episode.mp4".into(), b"good".to_vec())],
		}
	);
}

#[tokio::test]
async fn accepts_nonempty_video_without_declared_total() {
	let directory = TestDirectory::new();
	let adapter = ScriptedAdapter::new([
		unknown_response([]),
		unknown_response([b"good".as_slice()]),
	]);

	assert_eq!(
		acquire_and_observe(&directory, &adapter).await,
		Observation {
			result: downloaded(4),
			requests: vec![
				ObservedRequest::Head(URL.into()),
				ObservedRequest::Get(URL.into()),
			],
			files: vec![("episode.mp4".into(), b"good".to_vec())],
		}
	);
}

#[tokio::test]
async fn keeps_resumable_state_when_video_does_not_match_known_total() {
	let directory = TestDirectory::new();
	let adapter = ScriptedAdapter::new([
		tagged_response(4, ETAG, []),
		tagged_response(4, ETAG, [b"bad".as_slice()]),
	]);

	assert_eq!(
		acquire_and_observe(&directory, &adapter).await,
		Observation {
			result: Err(TransferError {
				error: Error::new(
					"The downloaded file was incomplete (3 of 4 bytes).",
				),
				retry: true,
				retry_after: None,
			}),
			requests: vec![
				ObservedRequest::Head(URL.into()),
				ObservedRequest::Get(URL.into()),
			],
			files: vec![
				("episode.mp4.part".into(), b"bad".to_vec()),
				(
					"episode.mp4.part.state".into(),
					format!("4\n{ETAG}").into_bytes(),
				),
			],
		}
	);
}

#[tokio::test]
async fn replaces_mismatched_final_video_and_removes_backup() {
	let directory = TestDirectory::new();
	tokio::fs::write(directory.path("episode.mp4"), b"bad")
		.await
		.unwrap();
	let adapter = ScriptedAdapter::new([
		tagged_response(4, ETAG, []),
		tagged_response(4, ETAG, [b"good".as_slice()]),
	]);

	assert_eq!(
		acquire_and_observe(&directory, &adapter).await,
		Observation {
			result: downloaded(4),
			requests: vec![
				ObservedRequest::Head(URL.into()),
				ObservedRequest::Get(URL.into()),
			],
			files: vec![("episode.mp4".into(), b"good".to_vec())],
		}
	);
}
