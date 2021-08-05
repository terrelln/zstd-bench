extern crate fasthash;
extern crate glob;
extern crate serde;
use crate::config::BenchmarkConfig;
use crate::config::{Config, DataSetConfig, DataSetMode};
use glob::glob;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::ops::Add;
use std::time::{Duration, Instant};

pub struct Datum {
	bytes: Vec<u8>,
	id: u64,
}

impl Datum {
	pub fn bytes(&self) -> &[u8] {
		&self.bytes
	}

	pub fn len(&self) -> usize {
		self.bytes().len()
	}

	pub fn as_ptr(&self) -> *const u8 {
		self.bytes().as_ptr()
	}

	pub fn id(&self) -> u64 {
		self.id
	}

	fn new(bytes: Vec<u8>) -> Self {
		let id = fasthash::xx::hash64(&bytes);
		Datum { bytes, id }
	}
}

pub struct DataSet {
	data: Vec<Datum>,
	name: String,
}

impl DataSet {
	pub fn data(&self) -> &[Datum] {
		&self.data
	}

	pub fn name(&self) -> &str {
		&self.name
	}

	pub fn load(config: &DataSetConfig) -> Self {
		let mut file_data = Vec::new();
		for g in &config.globs {
			for file in glob(g).unwrap().map(|x| x.unwrap()) {
				let bytes = fs::read(file).unwrap();
				file_data.push(bytes);
			}
		}

		assert_ne!(file_data.len(), 0);

		let data = match config.mode {
			DataSetMode::ConcatenateFiles => vec![Datum::new(file_data.concat())],
			DataSetMode::Cut(size) => {
				let mut chunks = Vec::new();
				for datum in file_data {
					chunks.extend(datum
						.chunks(size)
						.map(|x| Datum::new(x.to_owned())));
				}
				chunks
			}
			DataSetMode::SeparateFiles => {
				file_data.into_iter().map(|x| Datum::new(x)).collect()
			}
		};

		let mut ids = HashSet::new();
		let data: Vec<_> = data.into_iter().filter(|d| ids.insert(d.id())).collect();

		assert_ne!(data.len(), 0);

		DataSet {
			name: config.name.clone(),
			data,
		}
	}
}

#[derive(Default)]
pub struct Metrics {
	pub uncompressed_size: Option<u64>,
	pub compressed_size: Option<u64>,
	pub duration: Option<Duration>,
}

impl Metrics {
	pub fn zero() -> Metrics {
		Metrics {
			uncompressed_size: Some(0),
			compressed_size: Some(0),
			duration: Some(Duration::default()),
		}
	}
}

fn add_opt<T: Add>(x: Option<T>, y: Option<T>) -> Option<T::Output> {
	if let Some(x) = x {
		if let Some(y) = y {
			return Some(x + y);
		}
	}
	None
}

impl std::ops::Add for Metrics {
	type Output = Self;

	fn add(self, other: Self) -> Self {
		let uncompressed_size = add_opt(self.uncompressed_size, other.uncompressed_size);
		let compressed_size = add_opt(self.compressed_size, other.compressed_size);
		let duration = add_opt(self.duration, other.duration);
		Metrics {
			uncompressed_size,
			compressed_size,
			duration,
		}
	}
}

#[derive(Debug, PartialEq, Eq)]
enum TimerState {
	Stopped,
	Started,
}

pub struct Timer {
	elapsed: Duration,
	checkpoint: Instant,
	state: TimerState,
}

impl Timer {
	pub fn new() -> Self {
		Timer {
			elapsed: Duration::new(0, 0),
			checkpoint: Instant::now(),
			state: TimerState::Started,
		}
	}

	pub fn reset(&mut self) {
		self.elapsed = Duration::new(0, 0);
		self.state = TimerState::Stopped;
		self.start();
	}

	pub fn start(&mut self) {
		assert_eq!(self.state, TimerState::Stopped);
		self.state = TimerState::Started;
		self.checkpoint = Instant::now();
	}

	pub fn stop(&mut self) -> Duration {
		self.elapsed += self.checkpoint.elapsed();
		assert_eq!(self.state, TimerState::Started);
		self.state = TimerState::Stopped;
		self.elapsed
	}
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Statistic {
	pub min: u64,
	pub max: u64,
	pub mean: u64,
	pub median: u64,
	pub std_dev: u64,
}

fn median(data: &[u64]) -> u64 {
	let mut sorted = data.to_owned();
	sorted.sort();
	let odd = (data.len()) % 2 != 0;
	if odd {
		sorted[data.len() / 2]
	} else {
		let x = sorted[data.len() / 2 - 1];
		let y = sorted[data.len() / 2];
		(x + y) / 2
	}
}

fn std_dev(data: &[u64], mean: u64) -> u64 {
	let variance =
		data.iter().map(|x| (x - mean) * (x - mean)).sum::<u64>() / (data.len() as u64);
	(variance as f64).sqrt() as u64
}

impl Statistic {
	pub fn compute(data: &[u64]) -> Self {
		assert_ne!(data.len(), 0);
		let mean = data.iter().sum::<u64>() / (data.len() as u64);
		Statistic {
			min: *data.iter().min().unwrap(),
			max: *data.iter().max().unwrap(),
			mean,
			median: median(data),
			std_dev: std_dev(data, mean),
		}
	}
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct BenchmarkResult {
	pub zstd_commit: String,
	pub zstd_revision: String,
	pub zstd_tag: Option<String>,
	pub zstd_branch: Option<String>,
	pub zstd_commit_timestamp: i64,
	pub cc: String,
	pub cc_version: String,
	pub cflags: String,

	pub command_prefix: Vec<String>,

	pub benchmark_name: String,
	pub config_name: Option<String>,

	pub data_set: String,

	pub iters_per_run: u64,
	pub runs: u64,

	pub uncompressed_bytes: Option<u64>,
	pub compressed_bytes: Option<u64>,
	pub duration_ns: Statistic,
}

impl BenchmarkResult {
	pub fn new(config: &Config) -> Self {
		let mut result = BenchmarkResult::default();

		result.zstd_commit = option_env!("ZSTD_COMMIT").unwrap().to_owned();
		result.zstd_revision = option_env!("ZSTD_REV").unwrap().to_owned();
		result.zstd_tag = option_env!("ZSTD_TAG").map(|tag| tag.to_owned());
		result.zstd_branch = option_env!("ZSTD_BRANCH").map(|branch| branch.to_owned());
		result.zstd_commit_timestamp = option_env!("ZSTD_COMMIT_TIMESTAMP")
			.unwrap()
			.parse::<i64>()
			.unwrap();
		result.cc = option_env!("CC").unwrap().to_owned();
		result.cc_version = option_env!("CC_VERSION").unwrap().to_owned();
		result.cflags = option_env!("CFLAGS").unwrap().to_owned();

		result.command_prefix = config.command_prefix().to_owned();

		result
	}
}

fn compute_iters_and_runs(config: &Config, benchmark: &mut dyn Benchmark, data_set: &DataSet) -> (u64, u64) {
	let target_run_duration = Duration::from_millis(config.min_ms_per_run());
	let target_total_duration = Duration::from_secs(config.min_secs());
	let mut iters = config.min_iters_per_run();
	loop {
		let mut duration = benchmark.run_data_set(data_set, iters).duration.unwrap();
		assert_ne!(duration, Duration::default());
		if duration < target_run_duration / 10 {
			iters *= 10;
			continue;
		}
		if duration < target_run_duration {
			let mult = (target_run_duration.as_nanos() / duration.as_nanos()) as u64;
			let mult = std::cmp::max(mult, 2);
			iters *= mult;
			duration *= mult as u32;
		}
		assert_ne!(iters, 0);
		let mut runs = if duration < target_total_duration {
			(target_total_duration.as_nanos() / duration.as_nanos()) as u64
		} else {
			1
		};
		assert_eq!(iters >= config.min_iters_per_run(), true);
		runs = std::cmp::max(runs, config.min_runs());

		assert_ne!(runs, 0);
		return (iters, runs);
	}
}

fn assert_opt_eq(prev: &Option<u64>, curr: Option<u64>) -> Option<u64> {
	if let Some(prev) = prev {
		assert_eq!(*prev, curr.unwrap());
	}
	curr
}

pub fn run_benchmark(
	config: &Config,
	benchmark_name: &str,
	config_name: Option<&str>,
	benchmark: &mut dyn Benchmark,
	data_set: &DataSet,
) -> BenchmarkResult {
	let mut result = BenchmarkResult::new(config);
	result.benchmark_name = benchmark_name.to_owned();
	result.config_name = config_name.map(|s| s.to_owned());

	result.data_set = data_set.name.clone();

	benchmark.initialize_data_set(data_set);

	let (iters, runs) = compute_iters_and_runs(config, benchmark, data_set);
	result.iters_per_run = iters;
	result.runs = runs;

	println!("{} runs @ {} iters/run", result.runs, result.iters_per_run);

	let mut duration_ns = Vec::new();
	let mut uncompressed_bytes = None;
	let mut compressed_bytes = None;
	for _ in 0..runs {
		let metrics = benchmark.run_data_set(&data_set, result.iters_per_run);
		uncompressed_bytes =
			assert_opt_eq(&uncompressed_bytes, metrics.uncompressed_size);
		compressed_bytes =
			assert_opt_eq(&compressed_bytes, metrics.compressed_size);
		duration_ns.push(metrics.duration.unwrap().as_nanos() as u64);
	}
	result.uncompressed_bytes = uncompressed_bytes;
	result.compressed_bytes = compressed_bytes;
	result.duration_ns = Statistic::compute(&duration_ns);

	benchmark.finalize_data_set(data_set);

	result
}

pub trait Benchmark {
	fn initialize_data_set(&mut self, data_set: &DataSet) {
		data_set.data()
			.iter()
			.for_each(|datum| self.initialize_datum(&datum));
	}

	fn finalize_data_set(&mut self, data_set: &DataSet) {
		data_set.data()
			.iter()
			.for_each(|datum| self.finalize_datum(&datum));
	}

	fn initialize_datum(&mut self, _datum: &Datum) {}

	fn finalize_datum(&mut self, _datum: &Datum) {}

	fn run_data_set(&mut self, data_set: &DataSet, iters: u64) -> Metrics {
		data_set.data().iter().fold(Metrics::zero(), |acc, datum| {
			acc + self.run_datum(&datum, iters)
		})
	}

	fn run_datum(&mut self, _datum: &Datum, _iters: u64) -> Metrics {
		Metrics::default()
	}
}

pub trait ConfigurableBenchmark: Benchmark {
	fn name() -> String;

	fn from_config(config: &BenchmarkConfig) -> Box<dyn Benchmark>;
}
