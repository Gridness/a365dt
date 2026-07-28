use crate::telemetry::PerformanceMetric;

#[derive(Debug, Eq, PartialEq)]
pub(super) struct Aggregate {
	pub count: u64,
	pub total_us: u64,
	pub median_us: u64,
	pub work_units: u64,
	pub samples: usize,
}

pub(super) fn aggregate(
	metrics: &[PerformanceMetric],
	prefix: &str,
) -> Option<Aggregate> {
	let mut samples = Vec::new();
	let mut aggregate = Aggregate {
		count: 0,
		total_us: 0,
		median_us: 0,
		work_units: 0,
		samples: 0,
	};
	for metric in metrics
		.iter()
		.filter(|metric| metric.operation.starts_with(prefix))
	{
		aggregate.count = aggregate.count.saturating_add(metric.count);
		aggregate.total_us = aggregate.total_us.saturating_add(metric.total_us);
		aggregate.work_units =
			aggregate.work_units.saturating_add(metric.work_units);
		samples.extend_from_slice(&metric.samples_us);
	}
	if aggregate.count == 0 {
		return None;
	}
	samples.sort_unstable();
	aggregate.samples = samples.len();
	aggregate.median_us =
		samples.get(samples.len() / 2).copied().unwrap_or_default();
	Some(aggregate)
}
