use crate::benchmark::{Benchmark, ConfigurableBenchmark, Datum, Metrics, Timer};
use crate::config::BenchmarkConfig;
use crate::zstd;

pub struct CompressBenchmark {
	level: i32,
	out: Vec<u8>,
}

impl CompressBenchmark {
	fn run_one(&mut self, datum: &Datum) -> usize {
		let csize = zstd::compress(&mut self.out, datum.bytes(), self.level);
		assert_eq!(zstd::is_error(csize), false);
		csize
	}
}

impl ConfigurableBenchmark for CompressBenchmark {
	fn name() -> String {
		String::from("compress")
	}

	fn from_config(config: &BenchmarkConfig) -> Box<dyn Benchmark> {
		let level = config
			.get_parameter("ZSTD_c_compressionLevel")
			.map(|v| v.unwrap_integer())
			.unwrap_or(0);
		let bm = CompressBenchmark {
			level: level as i32,
			out: Vec::new(),
		};
		Box::new(bm)
	}
}

impl Benchmark for CompressBenchmark {
	fn run_datum(&mut self, datum: &Datum, iters: u64) -> Metrics {
		let cbound = zstd::compress_bound(datum.len());
		if self.out.len() < cbound {
			self.out.resize(cbound, 0);
		}

		let mut compressed_size = 0;
		let mut timer = Timer::new();
		for _ in 0..iters {
			compressed_size += self.run_one(&datum);
		}
		let duration = timer.stop();

		Metrics {
			uncompressed_size: Some(datum.len() as u64 * iters),
			compressed_size: Some(compressed_size as u64),
			duration: Some(duration),
		}
	}
}
