use super::*;
use pretty_assertions::assert_eq as assert_deep_eq;

fn media(options: &[(u16, &str)]) -> Embed {
	Embed {
		download: options
			.iter()
			.map(|(height, url)| MediaOption {
				height: *height,
				url: Some((*url).into()),
			})
			.collect(),
		subtitles_url: None,
	}
}

#[tokio::test(start_paused = true)]
async fn retries_empty_video_and_never_finalizes_it() {
	let directory = TestDirectory::new();
	let adapter = ScriptedAdapter::new((0..4).flat_map(|_| {
		[tagged_response(4, ETAG, []), tagged_response(4, ETAG, [])]
	}));

	assert_deep_eq!(
		acquire_and_observe(&directory, &adapter).await,
		failed(
			"The media server returned an empty file.",
			vec![
				ObservedRequest::Head(URL.into()),
				ObservedRequest::Get(URL.into()),
				refresh(1_042),
				ObservedRequest::Head(URL.into()),
				ObservedRequest::Get(URL.into()),
				refresh(3_084),
				ObservedRequest::Head(URL.into()),
				ObservedRequest::Get(URL.into()),
				refresh(7_126),
				ObservedRequest::Head(URL.into()),
				ObservedRequest::Get(URL.into()),
			],
			b"",
		)
	);
}

#[tokio::test(start_paused = true)]
async fn recovers_partial_video_after_network_and_throttling_failures() {
	let directory = TestDirectory::new();
	let mut interrupted = tagged_response(4, ETAG, [b"go".as_slice()]);
	interrupted
		.body
		.push_back(BodyStep::Error(Error::new("network interrupted")));
	let adapter = ScriptedAdapter::new([
		tagged_response(4, ETAG, []),
		interrupted,
		retry_after_response(StatusCode::TOO_MANY_REQUESTS, 5),
		tagged_response(4, ETAG, []),
		partial_response("bytes 2-3/4", [b"od".as_slice()]),
	])
	.with_refreshes([
		Ok(media(&[
			(720, "https://media.test/wrong-resolution.mp4"),
			(1080, "https://media.test/throttled.mp4"),
		])),
		Ok(media(&[(1080, REFRESHED_URL)])),
	]);

	assert_deep_eq!(
		acquire_and_observe(&directory, &adapter).await,
		Observation {
			result: downloaded(4),
			requests: vec![
				ObservedRequest::Head(URL.into()),
				ObservedRequest::Get(URL.into()),
				refresh(1_042),
				ObservedRequest::Head(
					"https://media.test/throttled.mp4".into()
				),
				refresh(6_042),
				ObservedRequest::Head(REFRESHED_URL.into()),
				resume(REFRESHED_URL, 2),
			],
			files: vec![("episode.mp4".into(), b"good".to_vec())],
		}
	);
}

#[tokio::test(start_paused = true)]
async fn video_interruption_returns_saved_bytes_without_requesting_subtitle() {
	let directory = TestDirectory::new();
	let (cancel, mut cancellation) = watch::channel(false);
	let mut video = tagged_response(4, ETAG, [b"go".as_slice()]);
	video.body.push_back(BodyStep::Cancel(cancel));
	let adapter = ScriptedAdapter::new([tagged_response(4, ETAG, []), video]);
	let mut release = release();
	release.subtitle_url = Some(SUBTITLE_URL.into());
	let result = acquire(
		&adapter,
		&release,
		&directory.path("episode.mp4"),
		&directory.path("episode.ass"),
		&ProgressBar::hidden(),
		ProgressBar::hidden,
		&mut cancellation,
	)
	.await;

	assert_deep_eq!(
		observe(&directory.0, result, &adapter).await,
		Observation {
			result: Ok(Acquisition {
				status: AcquisitionStatus::Interrupted,
				bytes: 2,
				has_subtitle_asset: false,
			}),
			requests: vec![
				ObservedRequest::Head(URL.into()),
				ObservedRequest::Get(URL.into()),
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
