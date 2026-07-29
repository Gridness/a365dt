use std::{future::Future, path::Path};

use bytes::Bytes;
use reqwest::{Method, Response, StatusCode, header::HeaderMap};
use tokio::{sync::watch, time::sleep};

use super::{
	ProgressEvent, RETRIES, TransferMode, TransferProgress, backoff, file_len,
	transfer,
};
use crate::{api::Anime365, error::Error, select::PlannedRelease};

/// Supplies refreshed Anime365 media locations and streaming asset responses.
///
/// Implementations perform external requests while acquisition retains
/// ownership of verification, filesystem state, and finalization.
pub(in crate::download) trait Adapter: Sync {
	type Response: AssetResponse;

	fn asset(
		&self,
		request: AssetRequest<'_>,
	) -> impl Future<Output = Result<Self::Response, Error>> + Send;

	fn refresh(
		&self,
		translation_id: u64,
		height: u16,
	) -> impl Future<Output = Result<RefreshedMedia, Error>> + Send;
}

/// Exposes the HTTP metadata and ordered body chunks required by acquisition.
///
/// Implementations translate their transport response into these operations;
/// each call to `chunk` yields the next bytes or the transport error.
pub(in crate::download) trait AssetResponse: Send {
	type Chunk: AsRef<[u8]> + Send;

	fn status(&self) -> StatusCode;
	fn headers(&self) -> &HeaderMap;
	fn content_length(&self) -> Option<u64>;
	fn chunk(
		&mut self,
	) -> impl Future<Output = Result<Option<Self::Chunk>, Error>> + Send;
}

pub(in crate::download) enum AssetRequest<'a> {
	Metadata {
		url: &'a str,
	},
	Download {
		url: &'a str,
	},
	Resume {
		url: &'a str,
		start: u64,
		validator: &'a str,
	},
}

pub(in crate::download) struct RefreshedMedia {
	video_url: Option<String>,
	subtitle_url: Option<String>,
}

pub(in crate::download) struct EpisodeRequest<'a> {
	release: &'a PlannedRelease,
	video_path: &'a Path,
	progress: &'a dyn TransferProgress,
	debug: bool,
}

impl<'a> EpisodeRequest<'a> {
	pub(in crate::download) fn new(
		release: &'a PlannedRelease,
		video_path: &'a Path,
		progress: &'a dyn TransferProgress,
	) -> Self {
		Self {
			release,
			video_path,
			progress,
			debug: false,
		}
	}

	pub(in crate::download) fn with_debug_errors(mut self) -> Self {
		self.debug = true;
		self
	}
}

#[derive(Debug, Eq, PartialEq)]
pub(in crate::download) enum Outcome {
	Downloaded {
		bytes: u64,
		subtitle_url: Option<String>,
	},
	Skipped {
		bytes: u64,
		subtitle_url: Option<String>,
	},
	Failed(Error),
	Interrupted {
		bytes: u64,
	},
}

impl Adapter for Anime365 {
	type Response = Response;

	async fn asset(
		&self,
		request: AssetRequest<'_>,
	) -> Result<Self::Response, Error> {
		match request {
			AssetRequest::Metadata { url } => {
				Anime365::asset(self, Method::HEAD, url).await
			}
			AssetRequest::Download { url } => {
				Anime365::asset(self, Method::GET, url).await
			}
			AssetRequest::Resume {
				url,
				start,
				validator,
			} => Anime365::asset_from(self, url, start, validator).await,
		}
	}

	async fn refresh(
		&self,
		translation_id: u64,
		height: u16,
	) -> Result<RefreshedMedia, Error> {
		let embed = Anime365::embed(self, translation_id).await?;
		Ok(RefreshedMedia {
			video_url: embed
				.download
				.into_iter()
				.find(|item| item.height == height)
				.and_then(|item| item.url),
			subtitle_url: embed.subtitles_url,
		})
	}
}

impl AssetResponse for Response {
	type Chunk = Bytes;

	fn status(&self) -> StatusCode {
		Response::status(self)
	}

	fn headers(&self) -> &HeaderMap {
		Response::headers(self)
	}

	fn content_length(&self) -> Option<u64> {
		Response::content_length(self)
	}

	async fn chunk(&mut self) -> Result<Option<Self::Chunk>, Error> {
		Response::chunk(self).await.map_err(|error| {
			Error::with_debug(
				"The media download was interrupted by a network error.",
				error.without_url(),
			)
		})
	}
}

pub(in crate::download) async fn acquire(
	adapter: &impl Adapter,
	request: EpisodeRequest<'_>,
	cancel: &mut watch::Receiver<bool>,
) -> Outcome {
	let release = request.release;
	let mut video_url = release.media_url.clone();
	let mut subtitle_url = release.subtitle_url.clone();
	for attempt in 0..=RETRIES {
		if attempt > 0 {
			match adapter
				.refresh(release.translation.id, release.height)
				.await
			{
				Ok(media) => {
					video_url = media.video_url.unwrap_or(video_url);
					subtitle_url = media.subtitle_url.or(subtitle_url);
				}
				Err(error) => {
					request.progress.report(ProgressEvent::RefreshFailed {
						episode: &release.episode.episode_full,
						error: &error,
						debug: request.debug,
					});
				}
			}
		}
		match transfer(
			adapter,
			&video_url,
			request.video_path,
			TransferMode::Resumable,
			request.progress,
			cancel,
		)
		.await
		{
			Ok((skipped, bytes)) => {
				return if skipped {
					Outcome::Skipped {
						bytes,
						subtitle_url,
					}
				} else {
					Outcome::Downloaded {
						bytes,
						subtitle_url,
					}
				};
			}
			Err(error) if error.error.message() == "interrupted" => {
				return Outcome::Interrupted {
					bytes: file_len(request.video_path).await,
				};
			}
			Err(error) if error.retry && attempt < RETRIES => {
				request.progress.report(ProgressEvent::Retry {
					episode: &release.episode.episode_full,
					attempt: attempt + 1,
					retries: RETRIES,
				});
				sleep(
					error.retry_after.unwrap_or_else(|| {
						backoff(attempt, release.episode.id)
					}),
				)
				.await;
			}
			Err(error) => return Outcome::Failed(error.error),
		}
	}
	unreachable!()
}
