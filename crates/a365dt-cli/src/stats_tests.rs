use pretty_assertions::assert_eq;

use super::{Aggregate, aggregate};
use crate::telemetry::PerformanceMetric;

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
