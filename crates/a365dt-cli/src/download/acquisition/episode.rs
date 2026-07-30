use std::path::Path;

use indicatif::ProgressBar;
use tokio::{sync::watch, time::sleep};

use super::{
	Acquisition, AcquisitionStatus, Adapter, RETRIES, TransferError,
	TransferMode, backoff, file_len, part_path, transfer,
};
use crate::select::PlannedRelease;

pub(in crate::download) async fn acquire<A: Adapter>(
	adapter: &A,
	release: &PlannedRelease,
	path: &Path,
	bar: &ProgressBar,
	cancel: &mut watch::Receiver<bool>,
) -> Result<Acquisition, TransferError> {
	let mut video_url = release.media_url.clone();
	let mut subtitle_url = release.subtitle_url.clone();
	for attempt in 0..=RETRIES {
		if attempt > 0 {
			match adapter.refresh(release.translation.id).await {
				Ok(embed) => {
					video_url = embed
						.download
						.into_iter()
						.find(|item| item.height == release.height)
						.and_then(|item| item.url)
						.unwrap_or(video_url);
					subtitle_url = embed.subtitles_url.or(subtitle_url);
				}
				Err(error) => bar.set_message(format!(
					"{} • refresh failed: {}",
					release.episode.episode_full,
					error.render(false)
				)),
			}
		}
		match transfer(
			adapter,
			&video_url,
			path,
			TransferMode::Resumable,
			bar,
			cancel,
		)
		.await
		{
			Ok(mut acquisition) => {
				acquisition.subtitle_url = subtitle_url;
				return Ok(acquisition);
			}
			Err(error) if error.error.message() == "interrupted" => {
				return Ok(Acquisition {
					status: AcquisitionStatus::Interrupted,
					bytes: file_len(&part_path(path)).await,
					subtitle_url,
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
