use std::{collections::BTreeMap, time::Duration};

use serde::{Deserialize, Serialize};

const SAMPLE_LIMIT: usize = 101;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Performance {
	metrics: BTreeMap<String, Metric>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Metric {
	count: u64,
	total_us: u64,
	work_units: u64,
	samples_us: Vec<u64>,
}

#[derive(Clone, Copy)]
pub enum Work {
	Items(u64),
	None,
}

impl Performance {
	pub fn record(&mut self, operation: &str, duration: Duration, work: Work) {
		let metric = self.metrics.entry(operation.to_owned()).or_default();
		let duration = u64::try_from(duration.as_micros()).unwrap_or(u64::MAX);
		metric.count = metric.count.saturating_add(1);
		metric.total_us = metric.total_us.saturating_add(duration);
		if let Work::Items(items) = work {
			metric.work_units = metric.work_units.saturating_add(items);
		}
		push_sample(&mut metric.samples_us, duration);
	}

	pub fn is_empty(&self) -> bool {
		self.metrics.is_empty()
	}

	pub fn iter(&self) -> impl Iterator<Item = (&str, &Metric)> {
		self.metrics
			.iter()
			.map(|(operation, metric)| (operation.as_str(), metric))
	}
}

impl Metric {
	pub fn count(&self) -> u64 {
		self.count
	}

	pub fn total_us(&self) -> u64 {
		self.total_us
	}

	pub fn work_units(&self) -> u64 {
		self.work_units
	}

	pub fn samples_us(&self) -> &[u64] {
		&self.samples_us
	}
}

fn push_sample(samples: &mut Vec<u64>, value: u64) {
	if samples.len() >= SAMPLE_LIMIT {
		let excess = samples.len() + 1 - SAMPLE_LIMIT;
		samples.drain(..excess);
	}
	samples.push(value);
}
