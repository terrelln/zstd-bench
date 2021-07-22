use zstd_bench::print::Format;

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
	Format::Pretty.print_results("results.json", &keys);
}
