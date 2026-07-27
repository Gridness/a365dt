use std::time::Duration;

use reqwest::{Client, Method, RequestBuilder, Response, Url, header};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::error::Error;

const ASSET_ORIGIN: &str = "https://smotret-anime.org";
const API: &str = "https://anime365.ru/api";
const SERIES_FIELDS: &str = "id,title,year,typeTitle,numberOfEpisodes";
pub const SERIES_PAGE_SIZE: usize = 1_000;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone)]
pub struct Anime365 {
	http: Client,
	token: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Series {
	pub id: u64,
	pub title: String,
	pub year: Option<u16>,
	pub type_title: Option<String>,
	pub number_of_episodes: Option<u32>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub poster_url_small: Option<String>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub episodes: Vec<Episode>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Episode {
	pub id: u64,
	pub episode_int: String,
	pub episode_full: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Translation {
	pub id: u64,
	pub episode_id: u64,
	#[serde(rename = "typeKind")]
	pub kind: String,
	#[serde(rename = "typeLang")]
	pub language: String,
	#[serde(default)]
	pub authors_summary: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Embed {
	#[serde(default)]
	pub download: Vec<MediaOption>,
	pub subtitles_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MediaOption {
	pub height: u16,
	pub url: Option<String>,
}

#[derive(Deserialize)]
struct Envelope<T> {
	data: Option<T>,
	error: Option<ApiError>,
}

#[derive(Deserialize)]
struct ApiError {
	code: u16,
	message: String,
}

impl Anime365 {
	pub fn new(token: String) -> Result<Self> {
		let http = Client::builder()
			.https_only(true)
			.connect_timeout(Duration::from_secs(30))
			.user_agent(concat!("a365dt/", env!("CARGO_PKG_VERSION")))
			.build()
			.map_err(|error| {
				request_error(
					"Could not initialize the secure HTTP client.",
					error,
				)
			})?;
		Ok(Self { http, token })
	}

	pub async fn validate(&self) -> Result<()> {
		self.get::<serde::de::IgnoredAny>("/me", &[], true)
			.await
			.map(drop)
	}

	pub async fn search(&self, query: &str) -> Result<Vec<Series>> {
		self.get(
			"/series/",
			&[
				("query", query.to_owned()),
				("limit", "10".into()),
				("fields", SERIES_FIELDS.into()),
			],
			false,
		)
		.await
	}

	pub async fn series(&self, id: u64) -> Result<Option<Series>> {
		self.get_optional(&format!("/series/{id}"), &[], false)
			.await
	}

	pub async fn series_page(&self, offset: usize) -> Result<Vec<Series>> {
		self.get(
			"/series/",
			&[
				("limit", SERIES_PAGE_SIZE.to_string()),
				("offset", offset.to_string()),
				("fields", SERIES_FIELDS.into()),
			],
			false,
		)
		.await
	}

	pub async fn translations(
		&self,
		series_id: u64,
	) -> Result<Vec<Translation>> {
		let mut translations = Vec::new();
		loop {
			let page: Vec<Translation> = self
				.get(
					"/translations/",
					&[
						("seriesId", series_id.to_string()),
						("limit", "1000".into()),
						("offset", translations.len().to_string()),
						(
							"fields",
							"id,episodeId,typeKind,typeLang,authorsSummary"
								.into(),
						),
					],
					false,
				)
				.await?;
			let done = page.len() < 1000;
			translations.extend(page);
			if done {
				return Ok(translations);
			}
			if translations.len() >= 100_000 {
				return Err("Anime365 returned too many translations.".into());
			}
		}
	}

	pub async fn embed(&self, translation_id: u64) -> Result<Embed> {
		self.get(&format!("/translations/embed/{translation_id}"), &[], true)
			.await
	}

	pub async fn asset(&self, method: Method, url: &str) -> Result<Response> {
		send_asset(self.asset_request(method, url)?).await
	}

	pub async fn asset_from(
		&self,
		url: &str,
		start: u64,
		validator: &str,
	) -> Result<Response> {
		send_asset(
			self.asset_request(Method::GET, url)?
				.header(header::RANGE, format!("bytes={start}-"))
				.header(header::IF_RANGE, validator),
		)
		.await
	}

	fn asset_request(
		&self,
		method: Method,
		url: &str,
	) -> Result<RequestBuilder> {
		let url = normalize_url(url)?;
		let mut request = self.http.request(method, url.clone());
		if is_official(&url) {
			request = request.query(&[("access_token", &self.token)]);
		}
		Ok(request)
	}

	async fn get<T: DeserializeOwned>(
		&self,
		path: &str,
		query: &[(&str, String)],
		authenticated: bool,
	) -> Result<T> {
		self.get_optional(path, query, authenticated)
			.await?
			.ok_or_else(|| {
				Error::new("Anime365 did not return the requested API data.")
			})
	}

	async fn get_optional<T: DeserializeOwned>(
		&self,
		path: &str,
		query: &[(&str, String)],
		authenticated: bool,
	) -> Result<Option<T>> {
		let mut request = self
			.http
			.get(format!("{API}{path}"))
			.query(query)
			.timeout(Duration::from_secs(30));
		if authenticated {
			request = request.query(&[("access_token", &self.token)]);
		}
		let response = request.send().await.map_err(|error| {
			request_error("The request to the Anime365 API failed.", error)
		})?;
		let status = response.status();
		let body: Envelope<T> = response.json().await.map_err(|error| {
			request_error(
				"Anime365 returned a response a365dt could not read.",
				error,
			)
		})?;
		if status == reqwest::StatusCode::NOT_FOUND
			|| body.error.as_ref().is_some_and(|error| error.code == 404)
		{
			return Ok(None);
		}
		if let Some(error) = body.error {
			return Err(Error::new(format!(
				"Anime365 error {}: {}",
				error.code, error.message
			)));
		}
		if !status.is_success() {
			return Err(Error::new(format!(
				"Anime365 rejected the API request (HTTP {status})."
			)));
		}
		Ok(body.data)
	}
}

pub fn series_id_from_url(input: &str) -> Option<u64> {
	let url = Url::parse(input).ok()?;
	if !is_official(&url) {
		return None;
	}
	let parts: Vec<_> = url
		.path_segments()?
		.filter(|part| !part.is_empty())
		.collect();
	(parts.len() == 2 && parts[0] == "catalog")
		.then(|| parts[1].rsplit('-').next()?.parse().ok())?
}

fn normalize_url(input: &str) -> Result<Url> {
	let mut url = Url::parse(input)
		.or_else(|_| Url::parse(ASSET_ORIGIN).and_then(|base| base.join(input)))
		.map_err(|error| {
			Error::with_debug("Anime365 returned an invalid media URL.", error)
		})?;
	if matches!(url.host_str(), Some("anime365.ru" | "www.anime365.ru"))
		&& url.path().starts_with("/posters/")
	{
		url.set_host(Some("smotret-anime.org")).map_err(|error| {
			Error::with_debug("Anime365 returned an invalid poster URL.", error)
		})?;
	}
	Ok(url)
}

fn is_official(url: &Url) -> bool {
	matches!(
		url.host_str(),
		Some(
			"anime365.ru"
				| "www.anime365.ru"
				| "smotret-anime.org"
				| "www.smotret-anime.org"
		)
	)
}

fn request_error(message: &str, error: reqwest::Error) -> Error {
	Error::with_debug(message, error.without_url())
}

async fn send_asset(request: RequestBuilder) -> Result<Response> {
	request.send().await.map_err(|error| {
		request_error("The request to the Anime365 media server failed.", error)
	})
}

#[cfg(test)]
#[path = "api_tests.rs"]
mod tests;
