use std::sync::Arc;

use pretty_assertions::assert_eq as assert_deep_eq;

use super::*;
use crate::preferences::MuxFormat;

async fn acquire_subtitle_and_observe(
	directory: &TestDirectory,
	adapter: &ScriptedAdapter,
) -> Observation {
	let (_cancel_tx, mut cancel) = watch::channel(false);
	acquire_subtitle_with_cancel(directory, adapter, &mut cancel).await
}

async fn acquire_subtitle_with_cancel(
	directory: &TestDirectory,
	adapter: &ScriptedAdapter,
	cancel: &mut watch::Receiver<bool>,
) -> Observation {
	let mut release = release();
	release.subtitle_url = Some(SUBTITLE_URL.into());
	let result = acquire(
		adapter,
		&release,
		&directory.path("episode.mp4"),
		&directory.path("episode.ass"),
		&ProgressBar::hidden(),
		ProgressBar::hidden,
		cancel,
	)
	.await;
	observe(&directory.0, result, adapter).await
}

fn subtitle_requests(attempts: usize) -> Vec<ObservedRequest> {
	let mut requests = vec![
		ObservedRequest::Head(URL.into()),
		ObservedRequest::Get(URL.into()),
	];
	requests.extend((0..attempts).flat_map(|_| {
		[
			ObservedRequest::Head(SUBTITLE_URL.into()),
			ObservedRequest::Get(SUBTITLE_URL.into()),
		]
	}));
	requests
}

#[tokio::test]
async fn acquires_and_finalizes_separate_subtitle_after_video() {
	let directory = TestDirectory::new();
	let adapter = ScriptedAdapter::new([
		known_response(4, []),
		known_response(4, [b"good".as_slice()]),
		tagged_response(3, SUBTITLE_ETAG, []),
		tagged_response(3, SUBTITLE_ETAG, [b"ass".as_slice()]),
	]);
	assert_deep_eq!(
		acquire_subtitle_and_observe(&directory, &adapter).await,
		Observation {
			result: Ok(Acquisition {
				status: AcquisitionStatus::Downloaded,
				bytes: 4,
				has_subtitle_asset: true,
			}),
			requests: subtitle_requests(1),
			files: vec![
				("episode.ass".into(), b"ass".to_vec()),
				("episode.mp4".into(), b"good".to_vec()),
			],
		}
	);
}

#[tokio::test(start_paused = true)]
async fn subtitle_failure_leaves_episode_incomplete_with_video_bytes() {
	let directory = TestDirectory::new();
	let adapter = ScriptedAdapter::new(
		[
			known_response(4, []),
			known_response(4, [b"good".as_slice()]),
		]
		.into_iter()
		.chain((0..8).map(|_| tagged_response(3, SUBTITLE_ETAG, []))),
	);
	assert_deep_eq!(
		acquire_subtitle_and_observe(&directory, &adapter).await,
		Observation {
			result: Err(TransferError {
				error: Error::new(
					"Subtitle download failed: The media server returned an empty file.",
				),
				bytes: 4,
				retry: true,
				retry_after: None,
			}),
			requests: subtitle_requests(4),
			files: vec![
				("episode.ass.part".into(), Vec::new()),
				(
					"episode.ass.part.state".into(),
					format!("3\n{SUBTITLE_ETAG}").into_bytes(),
				),
				("episode.mp4".into(), b"good".to_vec()),
			],
		}
	);
}

#[tokio::test]
async fn subtitle_interruption_returns_incomplete_episode_with_partial_asset() {
	let directory = TestDirectory::new();
	let (cancel, mut cancellation) = watch::channel(false);
	let mut subtitle_response =
		tagged_response(3, SUBTITLE_ETAG, [b"a".as_slice()]);
	subtitle_response.body.push_back(BodyStep::Cancel(cancel));
	let adapter = ScriptedAdapter::new([
		known_response(4, []),
		known_response(4, [b"good".as_slice()]),
		tagged_response(3, SUBTITLE_ETAG, []),
		subtitle_response,
	]);
	assert_deep_eq!(
		acquire_subtitle_with_cancel(&directory, &adapter, &mut cancellation)
			.await,
		Observation {
			result: Ok(Acquisition {
				status: AcquisitionStatus::Interrupted,
				bytes: 4,
				has_subtitle_asset: true,
			}),
			requests: subtitle_requests(1),
			files: vec![
				("episode.ass.part".into(), b"a".to_vec()),
				(
					"episode.ass.part.state".into(),
					format!("3\n{SUBTITLE_ETAG}").into_bytes(),
				),
				("episode.mp4".into(), b"good".to_vec()),
			],
		}
	);
}

#[tokio::test(start_paused = true)]
async fn download_batch_continues_after_episode_acquisition_failure() {
	let directory = TestDirectory::new();
	let adapter = Arc::new(ScriptedAdapter::new(
		(0..4)
			.flat_map(|_| [known_response(4, []), known_response(4, [])])
			.chain([
				known_response(4, []),
				known_response(4, [b"good".as_slice()]),
			]),
	));
	let mut second = release();
	second.episode.episode_int = "2".into();
	second.episode.episode_full = "2 серия".into();
	let mux = Mux::Disabled;
	let jobs = vec![
		Job::new(release(), directory.0.clone(), mux),
		Job::new(second, directory.0.clone(), mux),
	];
	let (_cancel_tx, cancel) = watch::channel(false);
	let debug = false;
	let bars = Arc::new(Bars::new(jobs.len() as u64, debug));
	let summary = run_with_adapter(adapter, jobs, 1, bars, cancel).await;
	let failed = "The media server returned an empty file.";
	let completed = directory
		.path("E02 [voice-ru] [Test] [1080p].mp4")
		.display()
		.to_string();
	let outcome = |episode: &str, status, bytes, detail: Error| Outcome {
		episode: episode.into(),
		status,
		bytes,
		detail,
	};
	assert_deep_eq!(
		summary.outcomes,
		vec![
			outcome("1 серия", Status::Failed, 0, failed.into()),
			outcome("2 серия", Status::Downloaded, 4, completed.into()),
		]
	);
}

#[tokio::test]
async fn mp4_mux_preserves_existing_separate_files_on_failure() {
	let directory = TestDirectory::new();
	let mut release = release();
	release.subtitle_url = Some(SUBTITLE_URL.into());
	let stem = "E01 [voice-ru] [Test] [1080p]";
	tokio::fs::write(directory.path(&format!("{stem}.video.mp4")), b"raw!")
		.await
		.unwrap();
	let adapter = Arc::new(ScriptedAdapter::new([
		known_response(4, []),
		known_response(3, []),
		known_response(3, [b"ass".as_slice()]),
	]));
	let (_cancel_tx, cancel) = watch::channel(false);
	let summary = run_with_adapter(
		Arc::clone(&adapter),
		vec![Job::new(
			release,
			directory.0.clone(),
			Mux::Enabled(MuxFormat::Mp4),
		)],
		1,
		Arc::new(Bars::new(1, false)),
		cancel,
	)
	.await;
	let files = observe(&directory.0, downloaded(0), &adapter).await.files;
	let outcomes = summary
		.outcomes
		.into_iter()
		.map(|outcome| (outcome.episode, outcome.status, outcome.bytes))
		.collect::<Vec<_>>();

	assert_deep_eq!(
		(outcomes, adapter.requests(), files),
		(
			vec![("1 серия".into(), Status::MuxFailed, 4)],
			vec![
				ObservedRequest::Head(URL.into()),
				ObservedRequest::Head(SUBTITLE_URL.into()),
				ObservedRequest::Get(SUBTITLE_URL.into()),
			],
			vec![
				(format!("{stem}.ass"), b"ass".to_vec()),
				(format!("{stem}.video.mp4"), b"raw!".to_vec()),
			],
		)
	);
}

#[tokio::test(start_paused = true)]
async fn mp4_mux_uses_subtitle_discovered_during_video_retry() {
	let directory = TestDirectory::new();
	let adapter = Arc::new(
		ScriptedAdapter::new([
			known_response(4, []),
			known_response(4, []),
			known_response(4, []),
			known_response(4, [b"raw!".as_slice()]),
			known_response(3, []),
			known_response(3, [b"ass".as_slice()]),
		])
		.with_refreshes([Ok(Embed {
			download: vec![MediaOption {
				height: 1080,
				url: Some(URL.into()),
			}],
			subtitles_url: Some(SUBTITLE_URL.into()),
		})]),
	);
	let (_cancel_tx, cancel) = watch::channel(false);
	let summary = run_with_adapter(
		Arc::clone(&adapter),
		vec![Job::new(
			release(),
			directory.0.clone(),
			Mux::Enabled(MuxFormat::Mp4),
		)],
		1,
		Arc::new(Bars::new(1, false)),
		cancel,
	)
	.await;
	let files = observe(&directory.0, downloaded(0), &adapter).await.files;

	assert_deep_eq!(
		(summary.outcomes[0].status, files),
		(
			Status::MuxFailed,
			vec![
				("E01 [voice-ru] [Test] [1080p].ass".into(), b"ass".to_vec()),
				(
					"E01 [voice-ru] [Test] [1080p].video.mp4".into(),
					b"raw!".to_vec(),
				),
			],
		)
	);
}
