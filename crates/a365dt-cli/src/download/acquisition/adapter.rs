use std::future::Future;

use bytes::Bytes;
use reqwest::{Method, StatusCode, header::HeaderMap};

use crate::{api::Anime365, error::Error};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::download) enum Request<'a> {
	Head(&'a str),
	Get(&'a str),
	Resume {
		url: &'a str,
		start: u64,
		validator: &'a str,
	},
}

pub(in crate::download) struct Response<B> {
	pub(super) status: StatusCode,
	pub(super) headers: HeaderMap,
	pub(super) content_length: Option<u64>,
	pub(super) body: B,
}

impl<B> Response<B> {
	pub(super) fn status(&self) -> StatusCode {
		self.status
	}

	pub(super) fn headers(&self) -> &HeaderMap {
		&self.headers
	}

	pub(super) fn content_length(&self) -> Option<u64> {
		self.content_length
	}
}

/// Supplies media responses to acquisition without exposing the Anime365
/// client or its HTTP response type to the acquisition interface.
pub(in crate::download) trait Adapter: Sync {
	type Body: Send;

	fn send(
		&self,
		request: Request<'_>,
	) -> impl Future<Output = Result<Response<Self::Body>, Error>> + Send;

	fn chunk(
		&self,
		body: &mut Self::Body,
	) -> impl Future<Output = Result<Option<Bytes>, Error>> + Send;
}

pub(in crate::download) struct Anime365Adapter<'a> {
	api: &'a Anime365,
}

impl<'a> Anime365Adapter<'a> {
	pub(in crate::download) fn new(api: &'a Anime365) -> Self {
		Self { api }
	}
}

impl Adapter for Anime365Adapter<'_> {
	type Body = reqwest::Response;

	async fn send(
		&self,
		request: Request<'_>,
	) -> Result<Response<Self::Body>, Error> {
		let response = match request {
			Request::Head(url) => self.api.asset(Method::HEAD, url).await,
			Request::Get(url) => self.api.asset(Method::GET, url).await,
			Request::Resume {
				url,
				start,
				validator,
			} => self.api.asset_from(url, start, validator).await,
		}?;
		Ok(Response {
			status: response.status(),
			headers: response.headers().clone(),
			content_length: response.content_length(),
			body: response,
		})
	}

	async fn chunk(
		&self,
		body: &mut Self::Body,
	) -> Result<Option<Bytes>, Error> {
		body.chunk().await.map_err(|error| {
			Error::with_debug(
				"The media download was interrupted by a network error.",
				error.without_url(),
			)
		})
	}
}
