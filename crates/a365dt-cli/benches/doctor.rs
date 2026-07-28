use std::sync::LazyLock;

mod telemetry {
	pub struct PerformanceMetric {
		pub operation: String,
		pub count: u64,
		pub total_us: u64,
		pub work_units: u64,
		pub samples_us: Vec<u64>,
	}
}

#[path = "../src/doctor/metrics.rs"]
mod metrics;

use metrics::{Aggregate, aggregate};
use telemetry::PerformanceMetric;

static OBSERVATIONS: LazyLock<Vec<PerformanceMetric>> = LazyLock::new(|| {
	(0..64)
		.map(|index| PerformanceMetric {
			operation: if index % 3 == 0 {
				"request.api.search"
			} else if index % 3 == 1 {
				"cache.retrieve"
			} else {
				"search.rank"
			}
			.into(),
			count: 100,
			total_us: 50_000,
			work_units: 30_000,
			samples_us: (0..101).collect(),
		})
		.collect()
});

fn main() {
	divan::main();
}

#[divan::bench]
fn aggregate_request_statistics() -> Option<Aggregate> {
	aggregate(divan::black_box(&OBSERVATIONS), "request.")
}
