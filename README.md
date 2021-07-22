# Zstd Bench

To get started:

```
cd /path/to/zstd-bench

# Use the example config
cp examples/config.toml .
# Edit the config:
#   * Set the right commits (if you want)
#   * Set the repo to point at your zstd repo
#   * Set the dataset files to point to the right locations

# Build the benchmark
cargo build
# See the benchmark help options
./target/debug/bench -h

# Run the benchmark with the default options and print the results
# Results will be saved to results.json and printed to screen
# Results are also archived to archive.json (TODO: Improve this system)
./target/debug/bench --print

# Re-print the results in markdown format with only the specified keys.
# See src/print.rs for the available keys.
# Results are printed by the sort order of the keys.
./target/debug/bench --no-benchmark --print --print-format markdown \
	--print-keys benchmark,config,revision,speed_mbps,ratio

# Don't benchmark, instead copy the binaries for each commit into bin/
./target/debug/bench --no-benchmark --bin bin/

# See the directory structure
ls -l bin/

# Run the dev binary using examples/compress.toml and record it
perf record --call-graph=lbr -- ./bin/dev/bench --config examples/compress_config.toml
```
