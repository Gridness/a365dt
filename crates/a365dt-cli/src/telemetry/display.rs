use std::{fmt, time::Duration};

use console::style;
use indicatif::HumanDuration;

use super::{Paths, Stats};
use crate::ui;

pub(super) fn print(paths: &Paths, stats: &Stats, disabled: bool) {
	ui::heading("Local telemetry");
	ui::grid(&[
		row("Collection", if disabled { "Disabled" } else { "Enabled" }),
		row("Data", paths.data.display()),
		row("Opt-out", paths.disabled.display()),
		row("Schema", stats.schema_version),
		row(
			"First observation",
			format_timestamp(stats.usage.first_recorded_at),
		),
		row(
			"Last observation",
			format_timestamp(stats.usage.last_recorded_at),
		),
		row("Last enabled", format_timestamp(stats.last_enabled_at)),
		row("Last disabled", format_timestamp(stats.last_disabled_at)),
		row("Last cleared", format_timestamp(stats.last_cleared_at)),
		row(
			"First download",
			format_timestamp(stats.usage.first_download_at),
		),
		row(
			"Last download",
			format_timestamp(stats.usage.last_download_at),
		),
	]);

	ui::heading("Collected counters");
	if stats.usage.counters.is_empty() {
		ui::note("No counters recorded");
	} else {
		let rows = stats
			.usage
			.counters
			.iter()
			.map(|(key, value)| row(key, value))
			.collect::<Vec<_>>();
		ui::grid(&rows);
	}

	ui::heading("Calculated statistics");
	ui::grid(&[
		row(
			"catalogue.hit_rate",
			rate(
				counter(stats, "catalogue.hits"),
				counter(stats, "catalogue.misses"),
			),
		),
		row(
			"downloads.success_rate",
			rate(
				counter(stats, "downloads.episodes.downloaded").saturating_add(
					counter(stats, "downloads.episodes.skipped"),
				),
				counter(stats, "downloads.episodes.failed")
					.saturating_add(counter(
						stats,
						"downloads.episodes.mux_failed",
					))
					.saturating_add(counter(
						stats,
						"downloads.episodes.interrupted",
					)),
			),
		),
	]);

	ui::heading("Performance observations");
	if stats.usage.performance.is_empty() {
		ui::note("No performance observations recorded");
	} else {
		let mut rows = vec![[
			style("Operation").bold().to_string(),
			style("Count").bold().to_string(),
			style("Total").bold().to_string(),
			style("Average").bold().to_string(),
			style("Median").bold().to_string(),
			style("Work units").bold().to_string(),
		]];
		rows.extend(stats.usage.performance.iter().map(
			|(operation, metric)| {
				[
					operation.to_owned(),
					metric.count().to_string(),
					duration_us(metric.total_us()),
					duration_us(
						metric
							.total_us()
							.checked_div(metric.count())
							.unwrap_or_default(),
					),
					median(metric.samples_us())
						.map_or_else(|| "Unavailable".into(), duration_us),
					metric.work_units().to_string(),
				]
			},
		));
		ui::grid(&rows);
	}

	ui::heading("Recent samples");
	if stats.usage.samples.is_empty() {
		ui::note("No samples recorded");
	} else {
		let mut rows = vec![[
			style("Metric").bold().to_string(),
			style("Samples").bold().to_string(),
			style("Median").bold().to_string(),
		]];
		rows.extend(stats.usage.samples.iter().map(|(key, samples)| {
			[
				key.clone(),
				samples.len().to_string(),
				median(samples).map_or_else(
					|| "Unavailable".into(),
					|value| {
						HumanDuration(Duration::from_millis(value)).to_string()
					},
				),
			]
		}));
		ui::grid(&rows);
	}
}

fn row(label: impl fmt::Display, value: impl fmt::Display) -> [String; 2] {
	[style(label).bold().to_string(), value.to_string()]
}

fn counter(stats: &Stats, key: &str) -> u64 {
	stats.usage.counters.get(key).copied().unwrap_or_default()
}

fn rate(success: u64, failure: u64) -> String {
	let total = success.saturating_add(failure);
	if total == 0 {
		"Unavailable (no observations)".into()
	} else {
		format!("{:.1}%", success as f64 / total as f64 * 100.0)
	}
}

fn median(samples: &[u64]) -> Option<u64> {
	let mut samples = samples.to_vec();
	samples.sort_unstable();
	samples.get(samples.len() / 2).copied()
}

fn duration_us(microseconds: u64) -> String {
	format!("{:.3} ms", microseconds as f64 / 1_000.0)
}

pub(crate) fn format_timestamp(timestamp: Option<u64>) -> String {
	let Some(timestamp) = timestamp else {
		return "Never".into();
	};
	let seconds_per_day = 24 * 60 * 60;
	let days = i64::try_from(timestamp / seconds_per_day).unwrap_or(i64::MAX);
	let seconds = timestamp % seconds_per_day;
	let (year, month, day) = civil_date(days);
	format!(
		"{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
		seconds / 3600,
		seconds % 3600 / 60,
		seconds % 60
	)
}

fn civil_date(days_since_epoch: i64) -> (i64, i64, i64) {
	let days = days_since_epoch.saturating_add(719_468);
	let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
	let day_of_era = days - era * 146_097;
	let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524
		- day_of_era / 146_096)
		/ 365;
	let mut year = year_of_era + era * 400;
	let day_of_year =
		day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
	let month_prime = (5 * day_of_year + 2) / 153;
	let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
	let month = month_prime + if month_prime < 10 { 3 } else { -9 };
	year += i64::from(month <= 2);
	(year, month, day)
}
