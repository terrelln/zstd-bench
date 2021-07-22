extern crate serde_json;
use crate::benchmark::BenchmarkResult;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::Path;
use std::fs;

pub enum Format {
	Markdown,
	Pretty,
	PrettyCsv,
	Csv,
	PrettyTsv,
	Tsv,
}

impl From<&str> for Format {
	fn from(other: &str) -> Self {
		match other {
			"markdown" => Format::Markdown,
			"pretty" => Format::Pretty,
			"csv" => Format::PrettyCsv,
			"tsv" => Format::PrettyTsv,
			_ => panic!("unuspported type"),
		}
	}
}

impl Format {
	fn add_line_start(&self, sub_header: bool, line: &mut String) {
		match self {
			Format::Markdown => {
				if sub_header {
					line.push_str("|-")
				} else {
					line.push_str("| ")
				}
			}
			_ => (),
		}
	}

	fn add_line_end(&self, sub_header: bool, line: &mut String) {
		match self {
			Format::Markdown => {
				if sub_header {
					line.push_str("-|")
				} else {
					line.push_str(" |")
				}
			}
			_ => (),
		}
	}

	fn has_sub_header(&self) -> bool {
		match self {
			Format::Markdown | Format::Pretty => true,
			_ => false,
		}
	}

	fn add_sub_header_value(&self, last: bool, len: usize, line: &mut String) {
		match self {
			Format::Markdown | Format::Pretty => {
				line.extend(std::iter::repeat('-').take(len))
			}
			_ => (),
		}
		if !last {
			self.add_sub_header_separater(line);
		}
	}

	fn add_sub_header_separater(&self, line: &mut String) {
		match self {
			Format::Markdown => line.push_str("-|-"),
			Format::Pretty => line.push(' '),
			_ => (),
		}
	}

	fn add_separater(&self, line: &mut String) {
		match self {
			Format::Markdown => line.push_str(" | "),
			Format::Pretty => line.push(' '),
			Format::PrettyCsv => line.push_str(", "),
			Format::Csv => line.push_str(","),
			Format::PrettyTsv | Format::Tsv => line.push('\t'),
		}
	}

	fn add_padding(&self, padding: usize, line: &mut String) {
		match self {
			Format::Markdown
			| Format::Pretty
			| Format::PrettyCsv
			| Format::PrettyTsv => {
				line.extend(std::iter::repeat(' ').take(padding));
			}
			_ => (),
		}
	}

	fn add_value(
		&self,
		last: bool,
		left_pad: bool,
		max_len: usize,
		value: &str,
		line: &mut String,
	) {
		assert_eq!(value.len() <= max_len, true);
		let padding = max_len - value.len();
		if left_pad {
			self.add_padding(padding, line);
		}
		line.push_str(value);
		if !left_pad {
			self.add_padding(padding, line);
		}
		if !last {
			self.add_separater(line);
		}
	}

	pub fn print_rows<S: AsRef<str>>(&self, rows: &[Row], keys: &[S]) {
		if rows.is_empty() {
			return;
		}
		let mut lines = Vec::new();
		let header_offset = if self.has_sub_header() { 2 } else { 1 };
		let nlines = rows.len() + header_offset;
		lines.resize(nlines, String::new());
		for i in 0..nlines {
			let sub_header = i == 1 && self.has_sub_header();
			self.add_line_start(sub_header, &mut lines[i]);
		}
		for (i, key) in keys.iter().enumerate() {
			let last = i == keys.len() - 1;
			let title = rows[0].title(key.as_ref());
			let left_pad = rows[0].values.get(key.as_ref()).unwrap().left_pad();
			let values: Vec<_> = rows.iter().map(|r| r.display(key.as_ref())).collect();
			let max_len = values.iter().map(|s| s.len()).max().unwrap_or(0);
			let max_len = std::cmp::max(title.len(), max_len);

			self.add_value(last, left_pad, max_len, &title, &mut lines[0]);
			if self.has_sub_header() {
				self.add_sub_header_value(last, max_len, &mut lines[1]);
			}
			for (line, value) in values.iter().enumerate() {
				self.add_value(
					last,
					left_pad,
					max_len,
					&value,
					&mut lines[header_offset + line],
				);
			}
		}
		for i in 0..nlines {
			let sub_header = i == 1 && self.has_sub_header();
			self.add_line_end(sub_header, &mut lines[i]);
		}

		for line in &lines {
			println!("{}", line);
		}
	}

	pub fn print_results<P: AsRef<Path>, S: AsRef<str>>(&self, results: P, keys: &[S]) {
		let results = serde_json::from_slice::<Vec<BenchmarkResult>>(&fs::read(results).unwrap()).unwrap();
		let rows: Vec<Row> = results.into_iter().map(|r| r.into()).collect();
		let rows = sort_rows(rows, &keys);
		self.print_rows(&rows, &keys);
	}
}

#[derive(PartialEq, PartialOrd)]
enum Value {
	String(String),
	Integer(u64),
	Float(f64),
	None,
}

impl Value {
	fn left_pad(&self) -> bool {
		match &self {
			Value::Integer(_) | Value::Float(_) => true,
			_ => false,
		}
	}
}

impl From<String> for Value {
	fn from(s: String) -> Self {
		Value::String(s)
	}
}

impl From<u64> for Value {
	fn from(i: u64) -> Self {
		Value::Integer(i)
	}
}

impl From<f64> for Value {
	fn from(f: f64) -> Self {
		Value::Float(f)
	}
}

impl<T: Into<Value>> From<Option<T>> for Value {
	fn from(o: Option<T>) -> Self {
		match o {
			Some(x) => x.into(),
			None => Value::None,
		}
	}
}

pub struct Row {
	values: HashMap<&'static str, Value>,
	titles: HashMap<&'static str, &'static str>,
}

impl Row {
	fn title(&self, key: &str) -> String {
		if let Some(title) = self.titles.get(key) {
			return title.to_string();
		}
		let mut title = String::new();
		let mut prev_space = true;
		for chr in key.chars() {
			if chr == '_' {
				prev_space = true;
				title.push(' ');
			} else if prev_space {
				chr.to_uppercase().for_each(|c| title.push(c));
				prev_space = false;
			} else {
				title.push(chr);
			}
		}
		title
	}

	fn display(&self, key: &str) -> String {
		match &self.values.get(key).unwrap() {
			&Value::String(s) => s.clone(),
			&Value::Integer(i) => i.to_string(),
			&Value::Float(f) => format!("{:.2}", f),
			&Value::None => "N/A".to_string(),
		}
	}
}

pub fn sort_rows<S: AsRef<str>>(mut rows: Vec<Row>, order: &[S]) -> Vec<Row> {
	rows.sort_by(|lhs, rhs| {
		for key in order {
			let x = lhs.values.get(key.as_ref()).unwrap();
			let y = rhs.values.get(key.as_ref()).unwrap();
			let ord = x.partial_cmp(y);
			if let Some(ord) = ord {
				if ord != Ordering::Equal {
					return ord;
				}
			}
		}
		Ordering::Equal
	});
	rows
}

impl From<BenchmarkResult> for Row {
	fn from(result: BenchmarkResult) -> Self {
		let mut values: HashMap<_, Value> = HashMap::new();
		values.insert("commit", result.zstd_commit[..10].to_string().into());
		values.insert("revision", result.zstd_revision.into());
		values.insert("tag", result.zstd_tag.into());
		values.insert("branch", result.zstd_branch.into());
		values.insert(
			"commit_timestamp",
			(result.zstd_commit_timestamp as u64).into(),
		);
		values.insert("cc", result.cc.into());
		values.insert("cc_version", result.cc_version.into());
		values.insert("cflags", result.cflags.into());
		values.insert("command_prefix", result.command_prefix.join(" ").into());
		values.insert("benchmark", result.benchmark_name.into());
		values.insert("config", result.config_name.into());
		values.insert("dataset", result.data_set.into());
		values.insert("iters_per_run", result.iters_per_run.into());
		values.insert("runs", result.runs.into());
		values.insert("uncompressed_bytes", result.uncompressed_bytes.into());
		values.insert("compressed_bytes", result.compressed_bytes.into());
		values.insert("duration_ns", result.duration_ns.mean.into());
		values.insert("duration_ns_min", result.duration_ns.min.into());
		values.insert("duration_ns_max", result.duration_ns.max.into());
		values.insert("duration_ns_median", result.duration_ns.median.into());
		values.insert("duration_ns_stddev", result.duration_ns.std_dev.into());

		let speed_mbps = 1000. * (result.uncompressed_bytes as f64)
			/ (result.duration_ns.mean as f64);
		let ratio = (result.uncompressed_bytes as f64) / (result.compressed_bytes as f64);

		values.insert("ratio", ratio.into());
		values.insert("speed_mbps", speed_mbps.into());

		let mut titles = HashMap::new();
		titles.insert("speed_mbps", "Speed MB/s");
		titles.insert("cc", "Compiler");
		titles.insert("cc_version", "Compiler Version");
		titles.insert("cflags", "Compiler Flags");

		Row { values, titles }
	}
}
