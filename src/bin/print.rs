use zstd_bench::print::{Format, Comparison};

fn main() {
	let keys = [
		"cc",
		"benchmark",
		"config",
		"dataset",
		"revision",
		"commit",
		"speed_mbps",
		"ratio",
	];
	let cmp = Comparison { key: "revision".to_string(), baseline: "dev".to_string() };
	Format::Pretty.print_results("results.json", &keys, Some(&cmp));
}
