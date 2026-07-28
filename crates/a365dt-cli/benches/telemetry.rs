use std::{
	sync::{LazyLock, Mutex},
	time::Instant,
};

#[allow(dead_code)]
#[path = "../src/telemetry/performance.rs"]
mod performance;

use performance::{Performance, Work};

static PERFORMANCE: LazyLock<Mutex<Performance>> =
	LazyLock::new(Mutex::default);

fn main() {
	divan::main();
}

#[divan::bench]
fn measure_and_record() {
	let started = Instant::now();
	divan::black_box(());
	PERFORMANCE.lock().unwrap().record(
		"request.api.search",
		started.elapsed(),
		Work::None,
	);
}

#[divan::bench]
fn skip_when_disabled() {
	let started = divan::black_box(false).then(Instant::now);
	divan::black_box(started);
}

#[divan::bench]
fn doctor_debug_overhead_sample() {
	let performance = Mutex::new(Performance::default());
	for _ in 0..1_001 {
		let started = Instant::now();
		performance.lock().unwrap().record(
			"search.rank",
			started.elapsed(),
			Work::None,
		);
	}
	divan::black_box(performance);
}
