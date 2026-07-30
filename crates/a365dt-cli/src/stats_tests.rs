use pretty_assertions::assert_eq;

use std::path::PathBuf;

use super::{Aggregate, aggregate, cache_statistics};
use crate::{
	cache::Inspection as CacheInspection,
	doctor::{Check, Status},
	telemetry::PerformanceMetric,
};

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
fn reports_a_valid_empty_cache_as_zero_series() {
	assert_eq!(
		cache_statistics(&CacheInspection::Missing {
			path: PathBuf::from("cache.sqlite"),
			bytes: 0,
		}),
		vec![
			Check::new("Cache path", "cache.sqlite", Status::Info),
			Check::new("Last cache update", "Never", Status::Info)
				.remedy("Run a title search to create the cache"),
			Check::new("Cached Series", "0 · 0 B", Status::Info),
		]
	);
}
