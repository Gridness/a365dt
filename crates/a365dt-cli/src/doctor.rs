use std::process::ExitCode;

use indicatif::{HumanBytes, HumanDuration};

use crate::{
	error::Error,
	series_cache,
	startup::{self, Update},
	telemetry::{self, PerformanceMetric, Snapshot},
};

mod cache;
mod report;
mod server;

pub(crate) use cache::{
	Inspection as CacheInspection, inspect as inspect_cache,
};
pub(crate) use report::{Check, Status};
use report::{Report, Section};
use server::Probe as ServerProbe;

pub async fn run(debug: bool) -> ExitCode {
	let (server, update) = tokio::join!(server::probe(), startup::check());
	let cache = inspect_cache();
	let telemetry = telemetry::snapshot();
	let mut sections = vec![
		Section {
			title: "Health",
			debug: false,
			checks: health_checks(&server, &cache, &telemetry, debug),
		},
		Section {
			title: "Build",
			debug: false,
			checks: build_checks(&update),
		},
	];
	if debug {
		sections.push(Section {
			title: "Debug diagnostics",
			debug: true,
			checks: debug_checks(&server, &cache, &telemetry, &update),
		});
	}
	let report = Report { sections };
	report.print();
	report.exit_code()
}

fn health_checks(
	server: &ServerProbe,
	cache: &CacheInspection,
	snapshot: &Result<Snapshot, Error>,
	debug: bool,
) -> Vec<Check> {
	let server = match server.status {
		Status::Error => Check::new("Anime365", &server.summary, server.status)
			.remedy("Check the network or Anime365 status, then retry"),
		Status::Warning => {
			Check::new("Anime365", &server.summary, server.status)
				.remedy("Retry; check the network if latency remains elevated")
		}
		Status::Healthy | Status::Info => {
			Check::new("Anime365", &server.summary, server.status)
		}
	};
	let cache = match cache {
		CacheInspection::Ready { cache, .. } if cache.is_fresh() => {
			Check::new("Series cache", "Fresh", Status::Healthy)
		}
		CacheInspection::Ready { .. } => {
			Check::new("Series cache", "Stale", Status::Warning)
				.remedy("Run a title search to refresh it")
		}
		CacheInspection::Missing(_) => {
			Check::new("Series cache", "Not created yet", Status::Info)
				.remedy("Run a title search to create it")
		}
		CacheInspection::Broken { .. } => {
			Check::new("Series cache", "Unreadable", Status::Error)
				.remedy("Run `a365dt cache prune` to reset it")
		}
	};
	let telemetry = match snapshot {
		Ok(snapshot) if snapshot.enabled => {
			Check::new("Local telemetry", "Enabled", Status::Healthy)
		}
		Ok(_) => Check::new("Local telemetry", "Disabled", Status::Warning)
			.remedy("Run `a365dt telemetry enable` to resume observations"),
		Err(error) => {
			Check::new("Local telemetry", error.render(debug), Status::Error)
				.remedy("Run `a365dt telemetry clear` to reset it")
		}
	};
	vec![server, cache, telemetry]
}

fn build_checks(update: &Result<Option<Update>, Error>) -> Vec<Check> {
	vec![
		version_check(update),
		Check::new("Commit", env!("A365DT_COMMIT_SHA"), Status::Info),
		Check::new("Profile", env!("A365DT_BUILD_PROFILE"), Status::Info),
		Check::new(
			"Platform",
			format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
			Status::Info,
		),
		Check::new("Compiler", env!("A365DT_RUSTC"), Status::Info),
	]
}

fn version_check(update: &Result<Option<Update>, Error>) -> Check {
	match update {
		Ok(Some(update)) => Check::new(
			"Version",
			format!("{} → {} available", update.installed, update.available),
			Status::Warning,
		)
		.remedy("Run `a365dt update`"),
		Ok(None) => Check::new(
			"Version",
			concat!(env!("CARGO_PKG_VERSION"), " · up to date"),
			Status::Healthy,
		),
		Err(_) => Check::new(
			"Version",
			concat!(env!("CARGO_PKG_VERSION"), " · update check unavailable"),
			Status::Warning,
		)
		.remedy("Check the network or GitHub status, then retry"),
	}
}

fn debug_checks(
	server: &ServerProbe,
	cache: &CacheInspection,
	snapshot: &Result<Snapshot, Error>,
	update: &Result<Option<Update>, Error>,
) -> Vec<Check> {
	let mut checks = vec![
		Check::new("Server endpoint", server::URL, Status::Info),
		Check::new(
			"Server response",
			format!(
				"{} · {}",
				server.http_status.map_or_else(
					|| "No HTTP response".into(),
					|status| status.to_string()
				),
				milliseconds(server.latency.as_micros() as u64)
			),
			Status::Info,
		),
		Check::new(
			"Latency warning threshold",
			HumanDuration(server::LATENCY_WARNING).to_string(),
			Status::Info,
		),
	];
	if let Some(detail) = &server.detail {
		checks.push(Check::new("Server detail", detail, Status::Info));
	}
	if let Err(error) = update {
		checks.push(Check::new(
			"Update check detail",
			error.render(true),
			Status::Info,
		));
	}
	let (cache_path, cache_detail) = match cache {
		CacheInspection::Ready { path, cache, .. } => (
			path,
			format!(
				"{} old · TTL {}",
				HumanDuration(cache::age(cache)),
				HumanDuration(series_cache::MAX_AGE)
			),
		),
		CacheInspection::Missing(path) => (path, "Missing".into()),
		CacheInspection::Broken { path, detail } => (path, detail.clone()),
	};
	checks.push(Check::new(
		"Cache path",
		cache_path.display().to_string(),
		Status::Info,
	));
	checks.push(Check::new("Cache detail", cache_detail, Status::Info));
	match snapshot {
		Ok(snapshot) => {
			checks.extend([
				Check::new(
					"Telemetry data",
					format!(
						"{} · {}",
						snapshot.data_path.display(),
						snapshot.data_bytes.map_or_else(
							|| "missing".into(),
							|bytes| HumanBytes(bytes).to_string()
						)
					),
					Status::Info,
				),
				Check::new(
					"Telemetry opt-out",
					snapshot.disabled_path.display().to_string(),
					Status::Info,
				),
				Check::new(
					"Telemetry schema",
					snapshot.schema_version.to_string(),
					Status::Info,
				),
				Check::new(
					"First observation",
					telemetry::format_timestamp(snapshot.first_recorded_at),
					Status::Info,
				),
				Check::new(
					"Last observation",
					telemetry::format_timestamp(snapshot.last_recorded_at),
					Status::Info,
				),
				Check::new(
					"Last enabled",
					telemetry::format_timestamp(snapshot.last_enabled_at),
					Status::Info,
				),
				Check::new(
					"Last disabled",
					telemetry::format_timestamp(snapshot.last_disabled_at),
					Status::Info,
				),
				Check::new(
					"Last cleared",
					telemetry::format_timestamp(snapshot.last_cleared_at),
					Status::Info,
				),
			]);
			if snapshot.performance.is_empty() {
				checks.push(
					Check::new(
						"Per-operation latency",
						"Unavailable",
						Status::Info,
					)
					.remedy(
						"Collect telemetry by running searches or downloads",
					),
				);
			} else {
				checks.extend(snapshot.performance.iter().map(|metric| {
					Check::new(
						format!("Latency · {}", metric.operation),
						performance_detail(metric),
						Status::Info,
					)
				}));
			}
			if snapshot.counters.is_empty() {
				checks.push(
					Check::new("Usage counters", "Unavailable", Status::Info)
						.remedy("Run commands with telemetry enabled"),
				);
			} else {
				checks.extend(snapshot.counters.iter().map(
					|(counter, value)| {
						Check::new(
							format!("Counter · {counter}"),
							value.to_string(),
							Status::Info,
						)
					},
				));
			}
		}
		Err(error) => checks.push(Check::new(
			"Telemetry detail",
			error.render(true),
			Status::Error,
		)),
	}
	let overhead = telemetry::benchmark_overhead();
	checks.push(Check::new(
		"Telemetry overhead",
		format!(
			"enabled {} ns · disabled {} ns · added {} ns",
			overhead.enabled_ns, overhead.disabled_ns, overhead.added_ns
		),
		if overhead.added_ns <= 10_000 {
			Status::Healthy
		} else {
			Status::Warning
		},
	));
	checks
}

fn performance_detail(metric: &PerformanceMetric) -> String {
	let median = metric
		.samples_us
		.get(metric.samples_us.len() / 2)
		.copied()
		.unwrap_or_default();
	format!(
		"average {} · median {} · total {} · {} samples · {} work units",
		milliseconds(metric.total_us / metric.count.max(1)),
		milliseconds(median),
		milliseconds(metric.total_us),
		metric.samples_us.len(),
		metric.work_units
	)
}

fn milliseconds(microseconds: u64) -> String {
	format!("{:.3} ms", microseconds as f64 / 1_000.0)
}

#[cfg(test)]
#[path = "doctor_tests.rs"]
mod tests;
