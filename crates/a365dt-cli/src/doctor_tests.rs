use pretty_assertions::assert_eq;
use semver::Version;

use super::{
	Aggregate, Check, Report, Section, Status, aggregate, version_check,
};
use crate::{error::Error, startup::Update, telemetry::PerformanceMetric};

#[test]
fn aggregates_matching_operation_totals_and_recent_samples() {
	let metrics = vec![
		PerformanceMetric {
			operation: "request.api.search".into(),
			count: 2,
			total_us: 2_000,
			work_units: 0,
			samples_us: vec![800, 1_200],
		},
		PerformanceMetric {
			operation: "request.asset.get".into(),
			count: 1,
			total_us: 3_000,
			work_units: 4,
			samples_us: vec![3_000],
		},
		PerformanceMetric {
			operation: "search.rank".into(),
			count: 10,
			total_us: 1,
			work_units: 30_000,
			samples_us: vec![1],
		},
	];

	assert_eq!(
		aggregate(&metrics, "request."),
		Some(Aggregate {
			count: 3,
			total_us: 5_000,
			median_us: 1_200,
			work_units: 4,
			samples: 3,
		})
	);
}

#[test]
fn reports_the_worst_health_status() {
	let report = Report {
		sections: vec![Section {
			title: "Health",
			debug: false,
			checks: vec![
				Check::new("API", "Available", Status::Healthy),
				Check::new("Cache", "Stale", Status::Warning),
				Check::new("Telemetry", "Unavailable", Status::Error),
			],
		}],
	};

	assert_eq!(report.status(), Status::Error);
}

#[test]
fn reports_current_available_and_unknown_versions() {
	let installed = env!("CARGO_PKG_VERSION");

	assert_eq!(
		[
			version_check(&Ok(None)),
			version_check(&Ok(Some(Update {
				installed: Version::new(0, 9, 0),
				available: Version::new(0, 10, 0),
				release_url: "https://example.com/release".into(),
			}))),
			version_check(&Err(Error::new("unavailable"))),
		],
		[
			Check::new(
				"Version",
				format!("{installed} · up to date"),
				Status::Healthy,
			),
			Check::new("Version", "0.9.0 → 0.10.0 available", Status::Warning,)
				.remedy("Run `a365dt update`"),
			Check::new(
				"Version",
				format!("{installed} · update check unavailable"),
				Status::Warning,
			)
			.remedy("Check the network or GitHub status, then retry"),
		]
	);
}
