use std::time::{Duration, Instant};

use reqwest::StatusCode;

use super::{milliseconds, report::Status};
use crate::l10n::{tr, tr_args};

pub(super) const URL: &str =
	"https://anime365.ru/api/series/?limit=1&fields=id";
pub(super) const TIMEOUT: Duration = Duration::from_secs(5);
pub(super) const LATENCY_WARNING: Duration = Duration::from_secs(1);

pub(super) struct Probe {
	pub status: Status,
	pub summary: String,
	pub http_status: Option<StatusCode>,
	pub latency: Duration,
	pub detail: Option<String>,
}

pub(super) async fn probe() -> Probe {
	let started = Instant::now();
	let client = match reqwest::Client::builder()
		.https_only(true)
		.connect_timeout(TIMEOUT)
		.timeout(TIMEOUT)
		.user_agent(concat!("a365dt/", env!("CARGO_PKG_VERSION")))
		.build()
	{
		Ok(client) => client,
		Err(error) => {
			return Probe {
				status: Status::Error,
				summary: tr("unavailable"),
				http_status: None,
				latency: started.elapsed(),
				detail: Some(error.to_string()),
			};
		}
	};
	match client.get(URL).send().await {
		Ok(response) => {
			let http_status = response.status();
			let body = response.bytes().await;
			let latency = started.elapsed();
			if !http_status.is_success() {
				Probe {
					status: Status::Error,
					summary: tr_args(
						"doctor-server-http-unavailable",
						&[("status", http_status.as_u16().into())],
					),
					http_status: Some(http_status),
					latency,
					detail: None,
				}
			} else if let Err(error) = body {
				Probe {
					status: Status::Error,
					summary: tr("doctor-server-read-error"),
					http_status: Some(http_status),
					latency,
					detail: Some(error.to_string()),
				}
			} else {
				let status = if latency >= LATENCY_WARNING {
					Status::Warning
				} else {
					Status::Healthy
				};
				Probe {
					status,
					summary: tr_args(
						if status == Status::Warning {
							"doctor-server-available-slow"
						} else {
							"doctor-server-available"
						},
						&[(
							"latency",
							milliseconds(latency.as_micros() as u64).into(),
						)],
					),
					http_status: Some(http_status),
					latency,
					detail: None,
				}
			}
		}
		Err(error) => Probe {
			status: Status::Error,
			summary: if error.is_timeout() {
				tr("doctor-server-timeout")
			} else {
				tr("doctor-server-request-error")
			},
			http_status: None,
			latency: started.elapsed(),
			detail: Some(error.without_url().to_string()),
		},
	}
}
