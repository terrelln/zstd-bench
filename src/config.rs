extern crate toml;
use std::collections::{HashMap, HashSet};
use std::fs::read;
use std::path::Path;
use toml::Value;

pub enum Parameter {
	String(String),
	Integer(i64),
	Bool(bool),
}

impl Parameter {
	fn from_value(value: Value) -> Self {
		match value {
			Value::String(s) => Parameter::String(s),
			Value::Integer(i) => Parameter::Integer(i),
			Value::Boolean(b) => Parameter::Bool(b),
			_ => panic!("Unsupported value type"),
		}
	}

	pub fn as_string(&self) -> Option<&str> {
		if let Parameter::String(s) = self {
			Some(&s)
		} else {
			None
		}
	}

	pub fn as_integer(&self) -> Option<i64> {
		if let Parameter::Integer(i) = self {
			Some(*i)
		} else {
			None
		}
	}

	pub fn as_bool(&self) -> Option<bool> {
		if let Parameter::Bool(b) = self {
			Some(*b)
		} else {
			None
		}
	}

	pub fn unwrap_string(&self) -> &str {
		self.as_string().unwrap()
	}

	pub fn unwrap_integer(&self) -> i64 {
		self.as_integer().unwrap()
	}

	pub fn unwrap_bool(&self) -> bool {
		self.as_bool().unwrap()
	}
}

#[derive(Default)]
pub struct BenchmarkConfig {
	parameters: HashMap<String, Parameter>,
	data_sets: Option<HashSet<String>>,
}

impl BenchmarkConfig {
	pub fn get_parameter(&self, param: &str) -> Option<&Parameter> {
		self.parameters.get(param)
	}

	pub fn get_data_sets(&self) -> &Option<HashSet<String>> {
		&self.data_sets
	}
}

pub enum DataSetMode {
	SeparateFiles,
	ConcatenateFiles,
	Cut(usize),
}

impl DataSetMode {
	fn load(toml: &Value) -> DataSetMode {
		if toml.is_table() {
			let table = toml.as_table().unwrap();
			assert_eq!(table.len(), 1);
			let cut = table.get("cut").unwrap();
			DataSetMode::Cut(cut.as_integer().unwrap() as usize)
		} else {
			let s = toml.as_str().unwrap();
			if s.starts_with("cat") || s.starts_with("concat") {
				DataSetMode::ConcatenateFiles
			} else {
				assert_eq!(s.starts_with("sep"), true);
				DataSetMode::SeparateFiles
			}
		}
	}
}

impl Default for DataSetMode {
	fn default() -> Self {
		DataSetMode::SeparateFiles
	}
}

pub struct DataSetConfig {
	pub name: String,
	pub globs: Vec<String>,
	pub mode: DataSetMode,
}

pub struct Config {
	repo: String,
	commits: Vec<String>,
	command_prefix: Vec<String>,
	benchmark_configs: HashMap<String, Vec<(Option<String>, BenchmarkConfig)>>,
	dataset_configs: Vec<DataSetConfig>,
	min_secs: Option<u64>,
	min_runs: Option<u64>,
	min_ms_per_run: Option<u64>,
	min_iters_per_run: Option<u64>,
}

fn load_opt_int(dst: &mut Option<u64>, val: Option<&Value>) {
	*dst = val.map(|x| x.as_integer().unwrap() as u64);
}

impl Config {
	pub fn new() -> Self {
		Config {
			repo: String::new(),
			commits: Vec::new(),
			command_prefix: Vec::new(),
			benchmark_configs: HashMap::new(),
			dataset_configs: Vec::new(),
			min_secs: None,
			min_runs: None,
			min_ms_per_run: None,
			min_iters_per_run: None,
		}
	}

	fn load_benchmark(&mut self, name: &str, config_name: Option<&str>, toml: &Value) {
		let table = toml.as_table().unwrap();
		let parameters = table
			.iter()
			.filter(|(_key, value)| !value.is_array())
			.map(|(key, value)| (key.clone(), Parameter::from_value(value.clone())))
			.collect();
		let data_sets = table.get("datasets").map(|ds| {
			ds.as_array()
				.unwrap()
				.iter()
				.map(|v| v.as_str().unwrap().to_owned())
				.collect()
		});
		let bm_config = BenchmarkConfig {
			parameters,
			data_sets,
		};
		let config_name = config_name.map(|s| s.to_string());

		self.benchmark_configs
			.entry(name.to_string())
			.or_insert(Vec::new())
			.push((config_name, bm_config));
	}

	fn load_benchmarks(&mut self, toml: &Value) {
		let benchmarks = toml.as_table().unwrap();
		for (name, benchmark) in benchmarks {
			let table = benchmark.as_table().unwrap();
			if table.values().any(|value| value.is_table()) {
				for (config, benchmark) in table {
					self.load_benchmark(name, Some(config), benchmark);
				}
			} else {
				self.load_benchmark(name, None, benchmark);
			}
		}
	}

	fn load_data_sets(&mut self, toml: &Value) {
		let data_sets = toml.as_table().unwrap();
		for (name, data_set) in data_sets {
			let data_set = data_set.as_table().unwrap();
			let globs: Vec<_> = data_set
				.get("files")
				.unwrap()
				.as_array()
				.unwrap()
				.iter()
				.map(|v| v.as_str().unwrap().to_owned())
				.collect();
			let mode = data_set.get("mode").map(|v| DataSetMode::load(v)).unwrap_or_default();
			self.dataset_configs.push(DataSetConfig {
				name: name.to_owned(),
				globs,
				mode
			});
		}
	}

	pub fn load<P: AsRef<Path>>(path: P) -> Self {
		let mut config = Config::new();
		let bytes = read(path).unwrap();
		let toml: Value = toml::from_slice(&bytes).unwrap();

		if let Some(command_prefix) = toml.get("command_prefix") {
			config.command_prefix = command_prefix
				.as_array()
				.unwrap()
				.iter()
				.map(|v| v.as_str().unwrap().to_owned())
				.collect();
		}

		if let Some(benchmarks) = toml.get("benchmarks") {
			config.load_benchmarks(benchmarks);
		}

		if let Some(commits) = toml.get("commits") {
			config.commits = commits
				.as_array()
				.unwrap()
				.iter()
				.map(|value| value.as_str().unwrap().to_string())
				.collect();
		}

		if let Some(data_sets) = toml.get("datasets") {
			config.load_data_sets(data_sets);
		}

		if let Some(repo) = toml.get("repo") {
			config.repo = repo.as_str().unwrap().to_string();
		}

		load_opt_int(&mut config.min_secs, toml.get("min_secs"));
		load_opt_int(&mut config.min_runs, toml.get("min_runs"));
		load_opt_int(&mut config.min_ms_per_run, toml.get("min_ms_per_run"));
		load_opt_int(&mut config.min_iters_per_run, toml.get("min_iters_per_run"));

		config
	}

	pub fn configs_for_benchmark(&self, name: &str) -> &[(Option<String>, BenchmarkConfig)] {
		match self.benchmark_configs.get(name) {
			Some(x) => x,
			None => &[],
		}
	}

	pub fn benchmarks(&self) -> impl Iterator<Item = &str> {
		self.benchmark_configs.iter().map(|(name, _)| &name as &str)
	}

	pub fn dataset_configs(&self) -> &[DataSetConfig] {
		&self.dataset_configs
	}

	pub fn repo(&self) -> &str {
		&self.repo
	}

	pub fn commits(&self) -> &[String] {
		&self.commits
	}

	pub fn command_prefix(&self) -> &[String] {
		&self.command_prefix
	}

	pub fn min_secs(&self) -> u64 {
		self.min_secs.unwrap_or(10)
	}

	pub fn min_runs(&self) -> u64 {
		self.min_runs.unwrap_or(3)
	}

	pub fn min_iters_per_run(&self) -> u64 {
		self.min_iters_per_run.unwrap_or(1)
	}

	pub fn min_ms_per_run(&self) -> u64 {
		self.min_ms_per_run.unwrap_or(100)
	}
}
