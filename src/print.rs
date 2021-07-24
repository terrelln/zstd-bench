extern crate itertools;
extern crate serde_json;
use crate::benchmark::BenchmarkResult;
use itertools::Itertools;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub struct Comparison {
	pub key: String,
	pub baseline: String,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Pad {
	Left,
	Right,
	Middle,
}

impl Pad {
	fn padding(&self, padding: usize) -> (usize, usize) {
		match self {
			Pad::Left => (padding, 0),
			Pad::Right => (0, padding),
			Pad::Middle => {
				let left_padding = padding / 2;
				(left_padding, padding - left_padding)
			}
		}
	}
}

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

	fn add_value(&self, last: bool, pad: Pad, max_len: usize, value: &str, line: &mut String) {
		assert_eq!(value.len() <= max_len, true);
		let padding = max_len - value.len();
		let (left_padding, right_padding) = pad.padding(padding);
		self.add_padding(left_padding, line);
		line.push_str(value);
		self.add_padding(right_padding, line);
		if !last {
			self.add_separater(line);
		}
	}

	fn header_lines(&self) -> usize {
		let sub_header_lines = if self.has_sub_header() { 1 } else { 0 };
		1 + sub_header_lines
	}

	fn add_values(
		&self,
		last: bool,
		title: &str,
		pad: Pad,
		values: &[String],
		lines: &mut [String],
	) {
		let max_len = values.iter().map(|s| s.len()).max().unwrap_or(0);
		let max_len = std::cmp::max(title.len(), max_len);
		let header_offset = self.header_lines();
		self.add_value(last, Pad::Middle, max_len, &title, &mut lines[0]);
		if self.has_sub_header() {
			self.add_sub_header_value(last, max_len, &mut lines[1]);
		}
		for (line, value) in values.iter().enumerate() {
			self.add_value(
				last,
				pad,
				max_len,
				&value,
				&mut lines[header_offset + line],
			);
		}
	}

	fn add_rows(&self, last: bool, rows: &[Row], key: &str, lines: &mut [String]) {
		let value0 = rows[0].get(key);
		let is_comparison = value0.is_comparison();
		let title = rows[0].title(key);
		let pad = value0.pad();
		if !is_comparison {
			let values: Vec<_> = rows.iter().map(|r| r.get(key).display()).collect();
			self.add_values(last, &title, pad, &values, lines);
		} else if is_comparison && is_null_comparison(rows, key) {
			println!("null cmp {}", key);
			let values: Vec<_> = rows
				.iter()
				.map(|r| r.get(key).unwrap_comparison()[0].1.display())
				.collect();
			self.add_values(last, &title, pad, &values, lines);
		} else {
			let cmp0 = value0.unwrap_comparison();
			for (i, (c, _)) in cmp0.iter().enumerate() {
				let has_diff =
					i != 0 && rows[0].is_result(key) && cmp0[0].1.is_numeric();
				let last = last && i == cmp0.len() - 1;
				{
					let title = format!("{} ({})", title, c);
					let values: Vec<_> = rows
						.iter()
						.map(|r| r.get(key).unwrap_comparison())
						.map(|c| c[i].1.display())
						.collect();
					self.add_values(
						last && !has_diff,
						&title,
						pad,
						&values,
						lines,
					);
				}
				if has_diff {
					let baseline = &cmp0[0].0;
					let title = format!("{} ({} - {})", title, c, baseline);
					let values: Vec<_> = rows
						.iter()
						.map(|r| r.get(key).unwrap_comparison())
						.map(|c| diff(&c[0].1, &c[i].1))
						.map(|d| format!("{:.1}%", d * 100.0))
						.collect();
					self.add_values(last, &title, pad, &values, lines);
				}
			}
		}
	}

	pub fn print_rows<S: AsRef<str>>(
		&self,
		rows: Vec<Row>,
		keys: &[S],
		cmp: Option<&Comparison>,
	) {
		if rows.is_empty() {
			return;
		}
		let rows = if let Some(cmp) = cmp {
			compare_rows(rows, &keys, cmp)
		} else {
			sort_rows(rows, &keys)
		};
		let mut lines = Vec::new();
		let header_offset = self.header_lines();
		let sub_header_line = header_offset - 1;
		let nlines = rows.len() + header_offset;
		lines.resize(nlines, String::new());
		for i in 0..nlines {
			let sub_header = i == sub_header_line && self.has_sub_header();
			self.add_line_start(sub_header, &mut lines[i]);
		}
		for (i, key) in keys.iter().enumerate() {
			let key = key.as_ref();
			let last = i == keys.len() - 1;
			if let Some(cmp) = cmp {
				if key == cmp.key {
					continue;
				}
			}
			self.add_rows(last, &rows, key, &mut lines);
		}
		for i in 0..nlines {
			let sub_header = i == sub_header_line && self.has_sub_header();
			self.add_line_end(sub_header, &mut lines[i]);
		}

		for line in &lines {
			println!("{}", line);
		}
	}

	pub fn print_results<P: AsRef<Path>, S: AsRef<str>>(
		&self,
		results: P,
		keys: &[S],
		cmp: Option<&Comparison>,
	) {
		let results =
			serde_json::from_slice::<Vec<BenchmarkResult>>(&fs::read(results).unwrap())
				.unwrap();
		let rows = results.into_iter().map(|r| r.into()).collect();
		self.print_rows(rows, &keys, cmp);
	}
}

#[derive(PartialEq, PartialOrd, Clone)]
enum Value {
	String(String),
	Integer(u64),
	Float(f64),
	Comparison(Vec<(String, Value)>),
	None,
}

impl Value {
	fn pad(&self) -> Pad {
		match &self {
			Value::Integer(_) | Value::Float(_) => Pad::Left,
			_ => Pad::Right,
		}
	}

	fn is_comparison(&self) -> bool {
		matches!(&self, &Value::Comparison(_))
	}

	fn is_numeric(&self) -> bool {
		matches!(&self, &Value::Integer(_) | &Value::Float(_))
	}

	fn unwrap_comparison(&self) -> &Vec<(String, Value)> {
		if let Value::Comparison(c) = self {
			return c;
		}
		panic!("Not comparison");
	}

	fn unwrap_str(&self) -> &str {
		if let Value::String(s) = self {
			return s;
		}
		panic!("Not string");
	}

	fn display(&self) -> String {
		match &self {
			&Value::String(s) => s.clone(),
			&Value::Integer(i) => i.to_string(),
			&Value::Float(f) => format!("{:.2}", f),
			&Value::None => "N/A".to_string(),
			&Value::Comparison(_) => panic!("Can't display comparison"),
		}
	}
}

fn diff(x: &Value, y: &Value) -> f64 {
	if let Value::Integer(x) = x {
		if let Value::Integer(y) = y {
			return (*y as f64 - *x as f64) / (*x as f64);
		}
	}
	if let Value::Float(x) = x {
		if let Value::Float(y) = y {
			return (y - x) / x;
		}
	}
	panic!("Can't diff non-numeric values or values of differing types");
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

#[derive(Clone)]
pub struct Row {
	values: HashMap<String, Value>,
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

	fn get(&self, key: &str) -> &Value {
		self.values.get(key).unwrap()
	}

	fn is_result(&self, key: &str) -> bool {
		if key == "ratio" {
			true
		} else if key.ends_with("bytes") {
			true
		} else if key.starts_with("duration_ns") {
			true
		} else if key.starts_with("speed_mbps") {
			true
		} else {
			false
		}
	}

	pub fn keys(&self) -> impl Iterator<Item = &String> {
		self.values.keys()
	}
}

fn sort_rows<S: AsRef<str>>(mut rows: Vec<Row>, order: &[S]) -> Vec<Row> {
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

fn compare_rows<S: AsRef<str>>(rows: Vec<Row>, keys: &[S], cmp: &Comparison) -> Vec<Row> {
	assert_eq!(keys.iter().any(|k| k.as_ref() == cmp.key), true);
	let prefix: Vec<_> = keys
		.iter()
		.map(|k| k.as_ref())
		.take_while(|k| k.as_ref() != cmp.key)
		.collect();
	let suffix: Vec<_> = keys
		.iter()
		.map(|k| k.as_ref())
		.skip(prefix.len() + 1)
		.collect();

	let rows = sort_rows(rows, &keys);
	let mut out = Vec::new();
	for (_, cmp_iter) in &rows
		.iter()
		.group_by(|row| prefix.iter().map(|k| row.get(k)).collect::<Vec<_>>())
	{
		let cmp_rows: Vec<_> = cmp_iter.collect();
		assert_ne!(cmp_rows.len(), 0);
		let mut out_row = cmp_rows[0].clone();
		for key in &suffix {
			let mut cmp_value = Vec::new();
			for r in &cmp_rows {
				let c = r.get(&cmp.key).unwrap_str();
				let v = r.get(key);
				cmp_value.push((c.to_string(), v.clone()));
			}
			let old = out_row
				.values
				.insert(key.to_string(), Value::Comparison(cmp_value));
			assert_eq!(old.is_some(), true);
		}
		out.push(out_row);
	}
	out
}

fn is_null_comparison(rows: &[Row], key: &str) -> bool {
	rows.iter()
		.map(|r| r.get(key).unwrap_comparison())
		.all(|c| {
			let v0 = &c[0].1;
			c[1..].iter().all(|v| &v.1 == v0)
		})
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

		let uncompressed_bytes = result.uncompressed_bytes;
		let speed_mbps = |duration| 1000. * (uncompressed_bytes as f64) / (duration as f64);

		let ratio = (uncompressed_bytes as f64) / (result.compressed_bytes as f64);

		values.insert("ratio", ratio.into());
		values.insert("speed_mbps", speed_mbps(result.duration_ns.mean).into());
		values.insert("speed_mbps_min", speed_mbps(result.duration_ns.max).into());
		values.insert("speed_mbps_max", speed_mbps(result.duration_ns.min).into());
		values.insert(
			"speed_mbps_median",
			speed_mbps(result.duration_ns.median).into(),
		);
		// TODO: This is probably wrong...
		values.insert(
			"speed_mbps_stddev",
			speed_mbps(result.duration_ns.std_dev).into(),
		);

		let mut titles = HashMap::new();
		titles.insert("speed_mbps", "Speed MB/s");
		titles.insert("cc", "Compiler");
		titles.insert("cc_version", "Compiler Version");
		titles.insert("cflags", "Compiler Flags");

		let values = values
			.into_iter()
			.map(|(k, v)| (k.to_string(), v))
			.collect();

		Row { values, titles }
	}
}
