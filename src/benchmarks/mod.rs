use crate::benchmark::{Benchmark, ConfigurableBenchmark};
use crate::config::BenchmarkConfig;
use std::collections::HashMap;

mod compress;
mod literals;

type BenchmarkMap = HashMap<String, Box<dyn Fn(&BenchmarkConfig) -> Box<dyn Benchmark>>>;

fn add<B: ConfigurableBenchmark>(bms: &mut BenchmarkMap) {
	let func = |config: &BenchmarkConfig| B::from_config(config);
	bms.insert(B::name(), Box::new(func));
}

pub fn get_all_benchmarks() -> BenchmarkMap {
	let mut benchmarks = HashMap::new();

	add::<compress::CompressBenchmark>(&mut benchmarks);
	add::<literals::CompressLiteralsBenchmark>(&mut benchmarks);
	add::<literals::DecompressLiteralsBenchmark>(&mut benchmarks);

	benchmarks
}
