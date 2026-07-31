use pretty_assertions::assert_eq;

use std::{collections::BTreeMap, path::PathBuf};

use super::{Aggregate, aggregate, cache_statistics, statistic_checks};
use crate::{
	cache::Inspection as CacheInspection,
	doctor::{Check, Status},
	telemetry::{PerformanceMetric, Snapshot},
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

#[test]
fn preserves_the_complete_rendered_telemetry_projection() {
	let cache = CacheInspection::Missing {
		path: PathBuf::from("cache.sqlite"),
		bytes: 0,
	};
	let telemetry = Ok(Snapshot {
		enabled: false,
		data_path: "telemetry.json".into(),
		disabled_path: "telemetry-disabled".into(),
		data_bytes: Some(1),
		schema_version: 1,
		first_recorded_at: Some(1),
		last_recorded_at: Some(2),
		first_download_at: Some(1),
		last_download_at: Some(2),
		last_enabled_at: Some(1),
		last_disabled_at: Some(2),
		last_cleared_at: None,
		counters: BTreeMap::from([
			("catalogue.hits".into(), 3),
			("catalogue.misses".into(), 1),
			("commands.download.success".into(), 1),
			("commands.update.failure".into(), 1),
			("downloads.batches".into(), 1),
			("downloads.bytes".into(), 2_048),
			("downloads.episodes.downloaded".into(), 2),
			("downloads.episodes.failed".into(), 1),
			("downloads.episodes.skipped".into(), 1),
		]),
		samples: BTreeMap::new(),
		performance: vec![
			PerformanceMetric {
				operation: "request.api.search".into(),
				count: 2,
				total_us: 4_000,
				work_units: 0,
				samples_us: vec![1_000, 3_000],
			},
			PerformanceMetric {
				operation: "request.asset.get".into(),
				count: 1,
				total_us: 1_000,
				work_units: 0,
				samples_us: vec![1_000],
			},
			PerformanceMetric {
				operation: "cache.retrieve".into(),
				count: 1,
				total_us: 500,
				work_units: 0,
				samples_us: vec![500],
			},
			PerformanceMetric {
				operation: "search.rank".into(),
				count: 2,
				total_us: 100,
				work_units: 1_000,
				samples_us: vec![40, 60],
			},
		],
	});

	assert_eq!(
		statistic_checks(&cache, &telemetry),
		vec![
			Check::new("Cache path", "cache.sqlite", Status::Info),
			Check::new("Last cache update", "Never", Status::Info)
				.remedy("Run a title search to create the cache"),
			Check::new("Cached Series", "0 · 0 B", Status::Info),
			Check::new(
				"Catalogue hit rate",
				"75.0% · 4 observations (historical)",
				Status::Info,
			),
			Check::new(
				"API requests",
				"average 2.000 ms · median 3.000 ms · 2 observations (historical)",
				Status::Info,
			),
			Check::new(
				"Media requests",
				"average 1.000 ms · median 1.000 ms · 1 observations (historical)",
				Status::Info,
			),
			Check::new(
				"Cache retrieval",
				"average 0.500 ms · median 0.500 ms · 1 observations (historical)",
				Status::Info,
			),
			Check::new(
				"Search",
				"average 0.050 ms · median 0.060 ms · 2 observations (historical)",
				Status::Info,
			),
			Check::new(
				"Search throughput",
				"10000000 Series/s (historical)",
				Status::Info,
			),
			Check::new(
				"Downloads",
				"75.0% · 4 observations (historical)",
				Status::Info,
			),
			Check::new(
				"Download volume",
				"1 batches · 4 Episodes · 2.00 KiB (historical)",
				Status::Info,
			),
			Check::new(
				"Command usage",
				"2 commands (historical)",
				Status::Info
			),
		]
	);
}
