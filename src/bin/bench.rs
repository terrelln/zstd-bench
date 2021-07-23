extern crate clap;
extern crate serde_json;
use clap::{App, Arg};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use zstd_bench::benchmark::{run_benchmark, BenchmarkResult, DataSet};
use zstd_bench::benchmarks::get_all_benchmarks;
use zstd_bench::config::Config;
use zstd_bench::print::Format;
use std::os::unix::fs as unix_fs;

fn benchmark_command(config: &Config, bin: &Path) -> Command {
	if config.command_prefix().is_empty() {
		Command::new(bin)
	} else {
		let prefix = config.command_prefix();
		let mut cmd = Command::new(&prefix[0]);
		for arg in &prefix[1..] {
			cmd.arg(arg);
		}
		cmd.arg(bin);
		for arg in std::env::args().skip(1) {
			cmd.arg(arg);
		}
		cmd
	}
}

struct BenchArgs {
	config: Config,
	cargo_dir: PathBuf,
	bin_dir: Option<PathBuf>,
	benchmark: bool,
	output_file: PathBuf,
	archive_file: PathBuf,
	print: bool,
	print_format: Format,
	print_keys: Vec<String>,
}

fn parse_args() -> Option<BenchArgs> {
	let cwd = std::env::current_dir()
		.unwrap()
		.to_str()
		.unwrap()
		.to_owned();
	let matches = App::new("Benchmark")
		.arg(Arg::with_name("config")
			.short("c")
			.long("config")
			.value_name("FILE")
			.help("Sets the benchmark config file")
			.takes_value(true)
			.default_value("config.toml"))
		.arg(Arg::with_name("cargo")
			.long("cargo")
			.value_name("DIR")
			.help("Set the directory where the cargo project lives")
			.takes_value(true)
			.default_value(&cwd))
		.arg(Arg::with_name("bin")
			.short("b")
			.long("bin")
			.value_name("DIR")
			.help("Store the binaries for each commit here")
			.takes_value(true))
		.arg(Arg::with_name("no_benchmark")
			.short("n")
			.long("no-benchmark")
			.help("Skip benchmarking"))
		.arg(Arg::with_name("output")
			.short("o")
			.long("output")
			.value_name("FILE")
			.help("Write the results here (overwritten)")
			.takes_value(true)
			.default_value("results.json"))
		.arg(Arg::with_name("archive")
			.short("a")
			.long("archive")
			.value_name("FILE")
			.help("Archive the results here (appended)")
			.takes_value(true)
			.default_value("archive.json"))
		.arg(Arg::with_name("print")
			.short("p")
			.long("print")
			.help("Print results from output"))
		.arg(Arg::with_name("print_format")
			.short("f")
			.long("print-format")
			.value_name("FORMAT")
			.help("Print format: markdown, pretty, csv, tsv")
			.takes_value(true)
			.default_value("pretty"))
		.arg(Arg::with_name("print_keys")
			.short("k")
			.long("print-keys")
			.value_name("KEYS")
			.help("Print the given keys in order (sort by order) (see print.rs)")
			.takes_value(true)
			.default_value("benchmark,config,dataset,cc,revision,commit,speed_mbps,ratio"))
		.arg(Arg::with_name("print_commit")
			.long("print-commit")
			.hidden(true))
		.get_matches();
	if matches.is_present("print_commit") {
		print!("{}", option_env!("ZSTD_COMMIT").unwrap());
		return None;
	}
	let config = Config::load(matches.value_of("config").unwrap());
	let cargo_dir = matches.value_of("cargo").unwrap().into();
	let bin_dir = matches.value_of("bin").map(|x| x.into());
	let benchmark = !matches.is_present("no_benchmark");
	let output_file = matches.value_of("output").unwrap().into();
	let archive_file = matches.value_of("archive").unwrap().into();
	let print = matches.is_present("print");
	let print_format = matches.value_of("print_format").unwrap().into();
	let print_keys = matches.value_of("print_keys").unwrap().split(',').map(|s| s.to_string()).collect();
	let args = BenchArgs {
		config,
		cargo_dir,
		bin_dir,
		benchmark,
		output_file,
		archive_file,
		print,
		print_format,
		print_keys,
	};
	Some(args)
}

fn get_commit<P: AsRef<Path>>(bin: P) -> String {
	let output = Command::new(bin.as_ref())
		.arg("--print-commit")
		.output()
		.expect("print commit to succeed");
	assert_eq!(output.status.success(), true);
	String::from_utf8(output.stdout).unwrap()
}

fn is_symlink(path: &Path) -> bool {
	// Workaround until is_symlink is stabilized
	fs::read_link(path).is_ok()
}

fn copy_binary<P: AsRef<Path>>(bin: P, base_dir: &Path, revision: &str) {
	let bin = bin.as_ref();
	let commit = get_commit(bin);
	let commit_dir = base_dir.join(&commit);
	fs::create_dir_all(&commit_dir).unwrap();
	fs::copy(bin, commit_dir.join(bin.file_name().unwrap())).unwrap();
	let link_dir = base_dir.join(revision);
	if is_symlink(&link_dir) {
		fs::remove_file(&link_dir).unwrap();
	}
	if commit != revision {
		unix_fs::symlink(&commit, &link_dir).unwrap();
	}
}

fn main_process() {
	let args = parse_args();
	if args.is_none() {
		return;
	}
	let args = args.unwrap();
	let bin = "bench";
	std::env::set_var("ZSTD_REPO", args.config.repo());
	if args.benchmark && args.output_file.exists() {
		fs::remove_file(&args.output_file).unwrap();
	}
	if args.benchmark || args.bin_dir.is_some() {
		for commit in args.config.commits() {
			// Hack to get build.rs to rerun
			let success = Command::new("touch")
				.arg("build.rs")
				.status()
				.expect("touch to succeed")
				.success();
			assert_eq!(success, true);
			let success = Command::new("cargo")
				.current_dir(&args.cargo_dir)
				.env("ZSTD_REPO", args.config.repo())
				.env("ZSTD_COMMIT", &commit)
				.arg("build")
				.arg("--release")
				.arg("--bin")
				.arg(bin)
				.status()
				.expect("Cargo to succeed")
				.success();
			assert_eq!(success, true);
			let bin = args.cargo_dir.join("target/release/bench");
			if let Some(bin_dir) = &args.bin_dir {
				copy_binary(&bin, bin_dir, &commit);
			}
			if args.benchmark {
				let success = benchmark_command(&args.config, &bin)
					.status()
					.expect("bench success")
					.success();
				assert_eq!(success, true);
			}
		}
	}
	if args.print {
		args.print_format.print_results(&args.output_file, &args.print_keys);
	}
}

fn load_data_sets(config: &Config) -> Vec<DataSet> {
	config.dataset_configs()
		.iter()
		.map(|ds_config| DataSet::load(ds_config))
		.collect()
}

fn append_results(file: &Path, results: &[BenchmarkResult]) {
	let mut prev = if file.exists() {
		serde_json::from_slice::<Vec<BenchmarkResult>>(&fs::read(&file).unwrap()).unwrap()
	} else {
		Vec::new()
	};
	prev.extend_from_slice(&results);
	let json = serde_json::to_vec(&prev).unwrap();
	fs::write(&file, &json).unwrap();
}

fn run_all_benchmarks(args: &BenchArgs) {
	let bm_factories = get_all_benchmarks();
	let zstd_commit = option_env!("ZSTD_COMMIT").unwrap();
	let zstd_tag = option_env!("ZSTD_TAG");
	let zstd_branch = option_env!("ZSTD_BRANCH");
	println!(
		"Benchmarking {} {:?} {:?}",
		zstd_commit, zstd_tag, zstd_branch
	);
	let data_sets = load_data_sets(&args.config);

	let mut results = Vec::new();
	for benchmark_name in args.config.benchmarks() {
		let bm_configs = args.config.configs_for_benchmark(benchmark_name);
		let bm_factory = bm_factories.get(benchmark_name).unwrap();
		for (config_name, bm_config) in bm_configs {
			let ds_filter = bm_config.get_data_sets();
			let mut bm = bm_factory(bm_config);
			for data_set in &data_sets {
				if let Some(ds_filter) = ds_filter {
					if !ds_filter.contains(data_set.name()) {
						continue;
					}
				}
				println!(
					"{} {:?} {}",
					benchmark_name,
					config_name,
					data_set.name()
				);
				let result = run_benchmark(
					&args.config,
					benchmark_name,
					config_name.as_deref(),
					&mut *bm,
					&data_set,
				);
				results.push(result);
			}
		}
	}

	append_results(&args.output_file, &results);
	append_results(&args.archive_file, &results);
}

fn sub_process() {
	let args = parse_args();
	if args.is_none() {
		return;
	}
	let args = args.unwrap();
	if args.benchmark {
		run_all_benchmarks(&args);
	}
}

fn main() {
	let zstd_commit = option_env!("ZSTD_COMMIT");

	if zstd_commit.is_none() {
		main_process();
	} else {
		sub_process();
	}
}
