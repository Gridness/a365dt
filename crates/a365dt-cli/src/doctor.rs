use std::process::ExitCode;

use indicatif::{HumanBytes, HumanDuration};

use crate::{
	error::Error,
	l10n::{tr, tr_args},
	series_cache,
	telemetry::{self, PerformanceMetric, Snapshot},
};

mod cache;
mod metrics;
mod report;
mod server;

use cache::Inspection as CacheInspection;
use metrics::{Aggregate, aggregate};
use report::{Check, Report, Section, Status};
use server::Probe as ServerProbe;

pub async fn run(debug: bool) -> ExitCode {
	let server = server::probe().await;
	let cache = cache::inspect();
	let telemetry = telemetry::snapshot();
	let mut sections = vec![
		Section {
			title: tr("doctor-section-health"),
			debug: false,
			checks: health_checks(&server, &cache, &telemetry, debug),
		},
		Section {
			title: tr("doctor-section-statistics"),
			debug: false,
			checks: statistic_checks(&cache, &telemetry),
		},
		Section {
			title: tr("doctor-section-build"),
			debug: false,
			checks: build_checks(),
		},
	];
	if debug {
		sections.push(Section {
			title: tr("doctor-section-debug"),
			debug: true,
			checks: debug_checks(&server, &cache, &telemetry),
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
			.remedy(tr("doctor-remedy-server-error")),
		Status::Warning => {
			Check::new("Anime365", &server.summary, server.status)
				.remedy(tr("doctor-remedy-server-slow"))
		}
		Status::Healthy | Status::Info => {
			Check::new("Anime365", &server.summary, server.status)
		}
	};
	let cache = match cache {
		CacheInspection::Ready { cache, .. } if cache.is_fresh() => Check::new(
			tr("doctor-series-cache"),
			tr("doctor-fresh"),
			Status::Healthy,
		),
		CacheInspection::Ready { .. } => Check::new(
			tr("doctor-series-cache"),
			tr("doctor-stale"),
			Status::Warning,
		)
		.remedy(tr("doctor-remedy-refresh-cache")),
		CacheInspection::Missing(_) => Check::new(
			tr("doctor-series-cache"),
			tr("doctor-not-created"),
			Status::Info,
		)
		.remedy(tr("doctor-remedy-create-cache")),
		CacheInspection::Broken { .. } => Check::new(
			tr("doctor-series-cache"),
			tr("doctor-unreadable"),
			Status::Error,
		)
		.remedy(tr("doctor-remedy-reset-cache")),
	};
	let telemetry = match snapshot {
		Ok(snapshot) if snapshot.enabled => Check::new(
			tr("telemetry-heading"),
			tr("telemetry-state-enabled"),
			Status::Healthy,
		),
		Ok(_) => Check::new(
			tr("telemetry-heading"),
			tr("telemetry-state-disabled"),
			Status::Warning,
		)
		.remedy(tr("doctor-remedy-enable-telemetry")),
		Err(error) => Check::new(
			tr("telemetry-heading"),
			error.render(debug),
			Status::Error,
		)
		.remedy(tr("doctor-remedy-reset-telemetry")),
	};
	vec![server, cache, telemetry]
}

fn statistic_checks(
	cache: &CacheInspection,
	snapshot: &Result<Snapshot, Error>,
) -> Vec<Check> {
	let mut checks = cache_statistics(cache);
	let Ok(snapshot) = snapshot else {
		for id in [
			"doctor-catalogue-hit-rate",
			"doctor-api-requests",
			"doctor-media-requests",
			"doctor-cache-retrieval",
			"doctor-search",
			"doctor-search-throughput",
			"doctor-downloads",
			"doctor-download-volume",
			"doctor-command-usage",
		] {
			checks.push(
				Check::new(tr(id), tr("unavailable"), Status::Info)
					.remedy(tr("doctor-remedy-reset-observations")),
			);
		}
		return checks;
	};
	let suffix = if snapshot.enabled {
		String::new()
	} else {
		tr("doctor-historical")
	};
	let hits = counter(snapshot, "catalogue.hits");
	let misses = counter(snapshot, "catalogue.misses");
	checks.push(rate_check(
		&tr("doctor-catalogue-hit-rate"),
		hits,
		misses,
		&suffix,
	));
	checks.push(performance_check(
		&tr("doctor-api-requests"),
		aggregate(&snapshot.performance, "request.api."),
		&suffix,
	));
	checks.push(performance_check(
		&tr("doctor-media-requests"),
		aggregate(&snapshot.performance, "request.asset."),
		&suffix,
	));
	checks.push(performance_check(
		&tr("doctor-cache-retrieval"),
		aggregate(&snapshot.performance, "cache.retrieve"),
		&suffix,
	));
	checks.push(performance_check(
		&tr("doctor-search"),
		aggregate(&snapshot.performance, "search."),
		&suffix,
	));
	let rank = aggregate(&snapshot.performance, "search.rank");
	checks.push(match rank {
		Some(metric) => Check::new(
			tr("doctor-search-throughput"),
			tr_args(
				"doctor-search-rate",
				&[
					(
						"rate",
						format!(
							"{:.0}",
							metric.work_units as f64 * 1_000_000.0
								/ metric.total_us.max(1) as f64
						)
						.into(),
					),
					("suffix", suffix.clone().into()),
				],
			),
			Status::Info,
		),
		None => Check::new(
			tr("doctor-search-throughput"),
			tr("unavailable-no-observations"),
			Status::Info,
		)
		.remedy(tr("doctor-remedy-run-searches")),
	});
	let downloaded = counter(snapshot, "downloads.episodes.downloaded");
	let skipped = counter(snapshot, "downloads.episodes.skipped");
	let failed = counter(snapshot, "downloads.episodes.failed")
		.saturating_add(counter(snapshot, "downloads.episodes.mux_failed"))
		.saturating_add(counter(snapshot, "downloads.episodes.interrupted"));
	checks.push(rate_check(
		&tr("doctor-downloads"),
		downloaded.saturating_add(skipped),
		failed,
		&suffix,
	));
	let batches = counter(snapshot, "downloads.batches");
	let bytes = counter(snapshot, "downloads.bytes");
	let episodes = downloaded.saturating_add(skipped).saturating_add(failed);
	checks.push(if batches == 0 {
		Check::new(
			tr("doctor-download-volume"),
			tr("unavailable-no-observations"),
			Status::Info,
		)
		.remedy(tr("doctor-remedy-run-downloads"))
	} else {
		Check::new(
			tr("doctor-download-volume"),
			tr_args(
				"doctor-download-volume-value",
				&[
					("batches", batches.into()),
					("episodes", episodes.into()),
					("bytes", HumanBytes(bytes).to_string().into()),
					("suffix", suffix.clone().into()),
				],
			),
			Status::Info,
		)
	});
	let commands = snapshot
		.counters
		.iter()
		.filter(|(key, _)| key.starts_with("commands."))
		.map(|(_, count)| count)
		.sum::<u64>();
	checks.push(Check::new(
		tr("doctor-command-usage"),
		tr_args(
			"doctor-command-count",
			&[("commands", commands.into()), ("suffix", suffix.into())],
		),
		Status::Info,
	));
	checks
}

fn cache_statistics(cache: &CacheInspection) -> Vec<Check> {
	match cache {
		CacheInspection::Ready { cache, bytes, .. } => vec![
			Check::new(
				tr("doctor-last-cache-update"),
				telemetry::format_timestamp(Some(cache.refreshed_at)),
				Status::Info,
			),
			Check::new(
				tr("doctor-cached-series"),
				format!("{} · {}", cache.series.len(), HumanBytes(*bytes)),
				Status::Info,
			),
		],
		CacheInspection::Missing(_) => vec![
			Check::new(
				tr("doctor-last-cache-update"),
				tr("never"),
				Status::Info,
			)
			.remedy(tr("doctor-remedy-create-cache")),
			Check::new(
				tr("doctor-cached-series"),
				tr("unavailable"),
				Status::Info,
			)
			.remedy(tr("doctor-remedy-create-cache")),
		],
		CacheInspection::Broken { .. } => vec![
			Check::new(
				tr("doctor-last-cache-update"),
				tr("unavailable"),
				Status::Info,
			)
			.remedy(tr("doctor-remedy-cache-prune")),
			Check::new(
				tr("doctor-cached-series"),
				tr("unavailable"),
				Status::Info,
			)
			.remedy(tr("doctor-remedy-cache-prune")),
		],
	}
}

fn build_checks() -> Vec<Check> {
	vec![
		Check::new(
			tr("doctor-version"),
			env!("CARGO_PKG_VERSION"),
			Status::Info,
		),
		Check::new(
			tr("doctor-commit"),
			env!("A365DT_COMMIT_SHA"),
			Status::Info,
		),
		Check::new(
			tr("doctor-profile"),
			env!("A365DT_BUILD_PROFILE"),
			Status::Info,
		),
		Check::new(
			tr("doctor-platform"),
			format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
			Status::Info,
		),
		Check::new(tr("doctor-compiler"), env!("A365DT_RUSTC"), Status::Info),
	]
}

fn debug_checks(
	server: &ServerProbe,
	cache: &CacheInspection,
	snapshot: &Result<Snapshot, Error>,
) -> Vec<Check> {
	let response_status = server.http_status.map_or_else(
		|| tr("doctor-no-http-response"),
		|status| status.to_string(),
	);
	let response_latency = milliseconds(server.latency.as_micros() as u64);
	let mut checks = vec![
		Check::new(tr("doctor-server-endpoint"), server::URL, Status::Info),
		Check::new(
			tr("doctor-server-response"),
			tr_args(
				"doctor-server-response-value",
				&[
					("status", response_status.into()),
					("latency", response_latency.into()),
				],
			),
			Status::Info,
		),
		Check::new(
			tr("doctor-latency-threshold"),
			HumanDuration(server::LATENCY_WARNING).to_string(),
			Status::Info,
		),
	];
	if let Some(detail) = &server.detail {
		checks.push(Check::new(
			tr("doctor-server-detail"),
			detail,
			Status::Info,
		));
	}
	let (cache_path, cache_detail) = match cache {
		CacheInspection::Ready { path, cache, .. } => (
			path,
			tr_args(
				"doctor-cache-age",
				&[
					(
						"age",
						HumanDuration(cache::age(cache)).to_string().into(),
					),
					(
						"ttl",
						HumanDuration(series_cache::MAX_AGE).to_string().into(),
					),
				],
			),
		),
		CacheInspection::Missing(path) => (path, tr("doctor-missing")),
		CacheInspection::Broken { path, detail } => (path, detail.clone()),
	};
	checks.push(Check::new(
		tr("doctor-cache-path"),
		cache_path.display().to_string(),
		Status::Info,
	));
	checks.push(Check::new(
		tr("doctor-cache-detail"),
		cache_detail,
		Status::Info,
	));
	match snapshot {
		Ok(snapshot) => {
			let data_size = snapshot.data_bytes.map_or_else(
				|| tr("doctor-missing-lowercase"),
				|bytes| HumanBytes(bytes).to_string(),
			);
			checks.extend([
				Check::new(
					tr("telemetry-data"),
					tr_args(
						"doctor-telemetry-data-value",
						&[
							(
								"path",
								snapshot.data_path.display().to_string().into(),
							),
							("size", data_size.into()),
						],
					),
					Status::Info,
				),
				Check::new(
					tr("telemetry-opt-out"),
					snapshot.disabled_path.display().to_string(),
					Status::Info,
				),
				Check::new(
					tr("telemetry-schema"),
					snapshot.schema_version.to_string(),
					Status::Info,
				),
				Check::new(
					tr("telemetry-first-observation"),
					telemetry::format_timestamp(snapshot.first_recorded_at),
					Status::Info,
				),
				Check::new(
					tr("telemetry-last-observation"),
					telemetry::format_timestamp(snapshot.last_recorded_at),
					Status::Info,
				),
				Check::new(
					tr("telemetry-last-enabled"),
					telemetry::format_timestamp(snapshot.last_enabled_at),
					Status::Info,
				),
				Check::new(
					tr("telemetry-last-disabled"),
					telemetry::format_timestamp(snapshot.last_disabled_at),
					Status::Info,
				),
				Check::new(
					tr("telemetry-last-cleared"),
					telemetry::format_timestamp(snapshot.last_cleared_at),
					Status::Info,
				),
			]);
			if snapshot.performance.is_empty() {
				checks.push(
					Check::new(
						tr("doctor-operation-latency"),
						tr("unavailable"),
						Status::Info,
					)
					.remedy(tr("doctor-remedy-collect-telemetry")),
				);
			} else {
				checks.extend(snapshot.performance.iter().map(|metric| {
					Check::new(
						tr_args(
							"doctor-latency-operation",
							&[("operation", metric.operation.clone().into())],
						),
						performance_detail(metric),
						Status::Info,
					)
				}));
			}
			if snapshot.counters.is_empty() {
				checks.push(
					Check::new(
						tr("doctor-usage-counters"),
						tr("unavailable"),
						Status::Info,
					)
					.remedy(tr("doctor-remedy-run-commands")),
				);
			} else {
				checks.extend(snapshot.counters.iter().map(
					|(counter, value)| {
						Check::new(
							tr_args(
								"doctor-counter",
								&[("counter", counter.clone().into())],
							),
							value.to_string(),
							Status::Info,
						)
					},
				));
			}
		}
		Err(error) => checks.push(Check::new(
			tr("doctor-telemetry-detail"),
			error.render(true),
			Status::Error,
		)),
	}
	let overhead = telemetry::benchmark_overhead();
	checks.push(Check::new(
		tr("doctor-telemetry-overhead"),
		tr_args(
			"doctor-telemetry-overhead-value",
			&[
				("enabled", overhead.enabled_ns.into()),
				("disabled", overhead.disabled_ns.into()),
				("added", overhead.added_ns.into()),
			],
		),
		if overhead.added_ns <= 10_000 {
			Status::Healthy
		} else {
			Status::Warning
		},
	));
	checks
}

fn performance_check(
	label: &str,
	metric: Option<Aggregate>,
	suffix: &str,
) -> Check {
	match metric {
		Some(metric) => Check::new(
			label,
			tr_args(
				"doctor-performance-value",
				&[
					(
						"average",
						milliseconds(metric.total_us / metric.count).into(),
					),
					("median", milliseconds(metric.median_us).into()),
					("count", metric.count.into()),
					("suffix", suffix.into()),
				],
			),
			Status::Info,
		),
		None => {
			Check::new(label, tr("unavailable-no-observations"), Status::Info)
				.remedy(tr("doctor-remedy-run-activity"))
		}
	}
}

fn performance_detail(metric: &PerformanceMetric) -> String {
	let median = metric
		.samples_us
		.get(metric.samples_us.len() / 2)
		.copied()
		.unwrap_or_default();
	tr_args(
		"doctor-performance-detail",
		&[
			(
				"average",
				milliseconds(metric.total_us / metric.count.max(1)).into(),
			),
			("median", milliseconds(median).into()),
			("total", milliseconds(metric.total_us).into()),
			("samples", metric.samples_us.len().into()),
			("work_units", metric.work_units.into()),
		],
	)
}

fn rate_check(label: &str, success: u64, failure: u64, suffix: &str) -> Check {
	let total = success.saturating_add(failure);
	if total == 0 {
		Check::new(label, tr("unavailable-no-observations"), Status::Info)
			.remedy(tr("doctor-remedy-run-activity"))
	} else {
		Check::new(
			label,
			tr_args(
				"doctor-rate-value",
				&[
					(
						"percent",
						format!("{:.1}", success as f64 / total as f64 * 100.0)
							.into(),
					),
					("total", total.into()),
					("suffix", suffix.into()),
				],
			),
			Status::Info,
		)
	}
}

fn counter(snapshot: &Snapshot, key: &str) -> u64 {
	snapshot.counters.get(key).copied().unwrap_or_default()
}

fn milliseconds(microseconds: u64) -> String {
	format!("{:.3} ms", microseconds as f64 / 1_000.0)
}

#[cfg(test)]
#[path = "doctor_tests.rs"]
mod tests;
