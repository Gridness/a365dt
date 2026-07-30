use std::path::Path;

use indicatif::ProgressBar;
use tokio::{sync::watch, time::sleep};

use super::{
	Acquisition, AcquisitionStatus, Adapter, RETRIES, TransferError, backoff,
	file_len, part_path, retry_transfer, transfer,
};
use crate::select::PlannedRelease;

pub(in crate::download) async fn acquire<A: Adapter>(
	adapter: &A,
	release: &PlannedRelease,
	video_path: &Path,
	subtitle_path: &Path,
	bar: &ProgressBar,
	subtitle_bar: impl FnOnce() -> ProgressBar,
	cancel: &mut watch::Receiver<bool>,
) -> Result<Acquisition, TransferError> {
	let mut video_url = release.media_url.clone();
	let mut subtitle_url = release.subtitle_url.clone();
	let mut video_acquisition = acquire_video(
		adapter,
		release,
		video_path,
		bar,
		cancel,
		&mut video_url,
		&mut subtitle_url,
	)
	.await?;
	if video_acquisition.status == AcquisitionStatus::Interrupted {
		return Ok(video_acquisition);
	}
	let Some(subtitle_url) = subtitle_url else {
		return Ok(video_acquisition);
	};
	let subtitle_bar = subtitle_bar();
	let subtitle_acquisition = retry_transfer(
		adapter,
		&subtitle_url,
		subtitle_path,
		&subtitle_bar,
		cancel,
	)
	.await;
	subtitle_bar.finish_and_clear();
	let subtitle_acquisition = match subtitle_acquisition {
		Ok(acquisition) => acquisition,
		Err(error) if error.is_interrupted() => {
			return Ok(Acquisition {
				status: AcquisitionStatus::Interrupted,
				bytes: video_acquisition.bytes,
				has_subtitle_asset: true,
			});
		}
		Err(mut error) => {
			error.error = error.error.context("Subtitle download failed");
			error.bytes = video_acquisition.bytes;
			return Err(error);
		}
	};
	if subtitle_acquisition.status != AcquisitionStatus::Skipped {
		video_acquisition.status = subtitle_acquisition.status;
	}
	video_acquisition.has_subtitle_asset = true;
	Ok(video_acquisition)
}

async fn acquire_video<A: Adapter>(
	adapter: &A,
	release: &PlannedRelease,
	path: &Path,
	bar: &ProgressBar,
	cancel: &mut watch::Receiver<bool>,
	video_url: &mut String,
	subtitle_url: &mut Option<String>,
) -> Result<Acquisition, TransferError> {
	for attempt in 0..=RETRIES {
		if attempt > 0 {
			match adapter.refresh(release.translation.id).await {
				Ok(embed) => {
					*video_url = embed
						.download
						.into_iter()
						.find(|item| item.height == release.height)
						.and_then(|item| item.url)
						.unwrap_or_else(|| video_url.clone());
					*subtitle_url =
						embed.subtitles_url.or_else(|| subtitle_url.clone());
				}
				Err(error) => bar.set_message(format!(
					"{} • refresh failed: {}",
					release.episode.episode_full,
					error.render(false)
				)),
			}
		}
		match transfer(adapter, video_url, path, bar, cancel).await {
			Ok(acquisition) => return Ok(acquisition),
			Err(error) if error.is_interrupted() => {
				return Ok(Acquisition {
					status: AcquisitionStatus::Interrupted,
					bytes: file_len(&part_path(path)).await,
					has_subtitle_asset: false,
				});
			}
			Err(error) if error.retry && attempt < RETRIES => {
				bar.set_message(format!(
					"{} • retry {}/{}",
					release.episode.episode_full,
					attempt + 1,
					RETRIES
				));
				sleep(
					error.retry_after.unwrap_or_else(|| {
						backoff(attempt, release.episode.id)
					}),
				)
				.await;
			}
			Err(error) => return Err(error),
		}
	}
	unreachable!()
}
