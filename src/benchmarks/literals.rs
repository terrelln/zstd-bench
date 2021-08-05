use crate::benchmark::{Benchmark, ConfigurableBenchmark, DataSet, Metrics, Timer};
use crate::config::BenchmarkConfig;
use crate::zstd;

#[derive(Eq, PartialEq)]
enum LiteralsMode {
	Compress,
	Decompress,
}

pub struct LiteralsBenchmark<const MODE: i32> {
	compressor: zstd::LiteralsBlockCompressor,
	decompressor: zstd::LiteralsBlockDecompressor,
	c_data: Vec<Vec<u8>>,
	c_literals: Vec<Vec<u8>>,
	d_literals: Vec<Vec<u8>>,
	quantization: Option<i32>,
	level: i32,
}

pub type CompressLiteralsBenchmark = LiteralsBenchmark<0>;
pub type DecompressLiteralsBenchmark = LiteralsBenchmark<1>;

impl<const MODE: i32> LiteralsBenchmark<MODE> {
	fn mode() -> LiteralsMode {
		match MODE {
			0 => LiteralsMode::Compress,
			1 => LiteralsMode::Decompress,
			_ => panic!("Unsupported mode"),
		}
	}

	fn new(level: i32, quantization: Option<i32>) -> Self {
		LiteralsBenchmark {
			compressor: zstd::LiteralsBlockCompressor::new(),
			decompressor: zstd::LiteralsBlockDecompressor::new(),
			c_data: Vec::new(),
			c_literals: Vec::new(),
			d_literals: Vec::new(),
			level,
			quantization,
		}
	}

	fn run_one(&mut self) -> (usize, usize) {
		match Self::mode() {
			LiteralsMode::Compress => {
				let mut d_size = 0;
				let mut c_size = 0;
				for d_lits in &self.d_literals {
					d_size += d_lits.len();
					c_size += self.compressor.compress(&d_lits);
				}
				(d_size, c_size)
			},
			LiteralsMode::Decompress => {
				let mut d_size = 0;
				let mut c_size = 0;
				if self.quantization.is_none() {
					for c_data in &self.c_data {
						zstd::for_each_literals_block(&c_data, |c_lits, d_lits, _lits_type| {
							d_size += d_lits.len();
							c_size += c_lits.len();
							zstd::IterationCommand::Continue
						});
					}
				} else {
					for c_lits in &self.c_literals {
						c_size += c_lits.len();
						d_size += self.decompressor.decompress(&c_lits);
					}
				}
				(d_size, c_size)
			}
		}
	}
}

impl<const MODE: i32> ConfigurableBenchmark for LiteralsBenchmark<MODE> {
	fn name() -> String {
		match Self::mode() {
			LiteralsMode::Compress => String::from("compress_literals"),
			LiteralsMode::Decompress => String::from("decompress_literals"),
		}
	}

	fn from_config(config: &BenchmarkConfig) -> Box<dyn Benchmark> {
		let level = config
			.get_parameter("ZSTD_c_compressionLevel")
			.map(|v| v.unwrap_integer())
			.unwrap_or(0);
		let quantization = config
			.get_parameter("Q")
			.map(|v| v.unwrap_integer() as i32);
		let bm = LiteralsBenchmark::<MODE>::new(level as i32, quantization);
		Box::new(bm)
	}
}

impl<const MODE: i32> Benchmark for LiteralsBenchmark<MODE> {
	fn initialize_data_set(&mut self, data_set: &DataSet) {
		println!("Initializing dataset...");
		self.c_data.clear();
		self.c_literals.clear();
		self.d_literals.clear();
		for datum in data_set.data() {
			let mut cdata = Vec::new();
			cdata.resize(zstd::compress_bound(datum.len()), 0);
			let csize = zstd::compress(&mut cdata, datum.bytes(), self.level);
			assert_eq!(zstd::is_error(csize), false);
			cdata.resize(csize, 0);

			let nblocks = zstd::for_each_block(&cdata, |_block, _block_type| {
				zstd::IterationCommand::Continue
			});
			assert_eq!(zstd::is_error(nblocks), false);

			let nblocks = zstd::for_each_literals_block(&cdata, |c_lits, d_lits, lits_type| {
				let q = if c_lits.len() >= d_lits.len() { 15 } else { (c_lits.len() * 16 / d_lits.len()) as i32 };
				if lits_type == zstd::LiteralsBlockType::Compressed && self.quantization == Some(q) {
					self.c_literals.push(c_lits.to_owned());
				}
				self.d_literals.push(d_lits.to_owned());
				zstd::IterationCommand::Continue
			});
			assert_eq!(zstd::is_error(nblocks), false);

			self.c_data.push(cdata);
		}
		if self.quantization.is_some() && self.c_literals.is_empty() {
			panic!("No data with the given quantization!");
		}
		println!("initialized");
	}

	fn run_data_set(&mut self, _dataset: &DataSet, iters: u64) -> Metrics {
		let mut timer = Timer::new();
		let mut compressed_size = 0;
		let mut decompressed_size = 0;
		for _ in 0..iters {
			let (d_size, c_size) = self.run_one();
			decompressed_size += d_size;
			compressed_size += c_size;
		}
		let duration = timer.stop();

		Metrics {
			uncompressed_size: Some(decompressed_size as u64),
			compressed_size: Some(compressed_size as u64),
			duration: Some(duration),
		}
	}
}
