extern crate cc;
extern crate fasthash;
extern crate git2;
extern crate serde;
extern crate regex;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Zstd {
	remote: String,
	commit: String,
	out_dir: PathBuf,
	repo_dir: PathBuf,
	commit_id: Option<String>,
	commit_timestamp: Option<i64>,
	commit_tag: Option<String>,
	commit_branch: Option<String>,
	config: Config,
}

impl Zstd {
	fn from_env() -> Option<Self> {
		let remote = env::var("ZSTD_REPO");
		let commit = env::var("ZSTD_COMMIT");
		if remote.is_ok() && commit.is_ok() {
			let remote = remote.unwrap();
			let commit = commit.unwrap();

			let base_dir = env::var("OUT_DIR").unwrap();
			let mut repo_dir = PathBuf::new();
			repo_dir.push(&base_dir);
			repo_dir.push("repo");
			let mut out_dir = PathBuf::new();
			out_dir.push(&base_dir);
			Some(Zstd {
				remote: remote,
				commit: commit,
				out_dir,
				repo_dir,
				commit_id: None,
				commit_timestamp: None,
				commit_tag: None,
				commit_branch: None,
				config: Config::from_env(),
			})
		} else {
			None
		}
	}

	fn print_cargo_rerun_metadata() {
		Config::print_cargo_rerun_metadata();
		println!("cargo:rerun-if-changed=build.rs");
		println!("cargo:rerun-if-changed=src/zstd.c");
		println!("cargo:rerun-if-env-changed=ZSTD_REPO");
		println!("cargo:rerun-if-env-changed=ZSTD_COMMIT");
	}

	fn print_cargo_rustc_metadata(&self) {
		self.config.print_cargo_rustc_metadata();
		println!("cargo:rustc-env=ZSTD_REV={}", self.commit);
		println!(
			"cargo:rustc-env=ZSTD_COMMIT={}",
			self.commit_id.clone().unwrap()
		);
		println!(
			"cargo:rustc-env=ZSTD_COMMIT_TIMESTAMP={}",
			self.commit_timestamp.clone().unwrap()
		);
		if let Some(commit_tag) = &self.commit_tag {
			println!("cargo:rustc-env=ZSTD_TAG={}", commit_tag.clone());
		}
		if let Some(commit_branch) = &self.commit_branch {
			println!("cargo:rustc-env=ZSTD_BRANCH={}", commit_branch.clone());
		}
		println!("cargo:rustc-cfg=zstd");
		println!("cargo:rustc-link-lib=static={}", "zstd_bench");
		println!("cargo:rustc-link-search=native={}", self.out_dir.display());
	}

	fn open_repo(&self) -> Result<git2::Repository, git2::Error> {
		// if self.repo_dir.exists() {
		// 	git2::Repository::open(&self.repo_dir)
		// } else {
		// 	git2::Repository::clone(&self.remote, &self.repo_dir)
		// }
		if self.repo_dir.exists() {
			fs::remove_dir_all(&self.repo_dir).unwrap();
		}
		git2::Repository::clone(&self.remote, &self.repo_dir)
	}

	fn remote_name(&self) -> String {
		let hash = fasthash::xx::hash64(self.remote.as_bytes());
		format!("{:x}", hash)
	}

	fn find_tag(&self, repo: &git2::Repository) -> Option<String> {
		if let Ok(reference) = repo.resolve_reference_from_short_name(&self.commit) {
			if reference.is_tag() {
				reference.name().map(|x| x.to_owned())
			} else {
				None
			}
		} else {
			None
		}
	}

	fn find_branch(&self, repo: &git2::Repository) -> Option<String> {
		if let Ok(reference) = repo.resolve_reference_from_short_name(&self.commit) {
			if reference.is_branch() {
				reference.name().map(|x| x.to_owned())
			} else {
				None
			}
		} else {
			None
		}
	}

	fn setup_remote<'a>(
		&self,
		repo: &'a git2::Repository,
	) -> Result<git2::Remote<'a>, git2::Error> {
		for remote in repo.remotes()?.iter().filter_map(|x| x) {
			repo.remote_delete(&remote)?;
		}

		let remote_name = self.remote_name();
		repo.remote(&remote_name, &self.remote)
	}

	fn find_object<'a>(&self, repo: &'a git2::Repository) -> git2::Object<'a> {
		let object = repo.revparse_single(&self.commit);
		if object.is_ok() {
			return object.unwrap();
		}
		let object = repo.revparse_single(&format!("{}/{}", self.remote_name(), &self.commit));
		object.unwrap()
	}

	fn prepare_repo(&mut self) -> Result<(), git2::Error> {
		let repo = self.open_repo()?;
		let mut remote = self.setup_remote(&repo)?;
		let refspecs = remote.fetch_refspecs()?;
		let refspecs: Vec<_> = refspecs.into_iter().map(|x| x.unwrap()).collect();
		remote.fetch(&refspecs, None, None)?;

		let object = self.find_object(&repo);
		let commit = object.peel_to_commit()?;
		self.commit_id = Some(commit.id().to_string());
		self.commit_tag = self.find_tag(&repo);
		self.commit_branch = self.find_branch(&repo);
		self.commit_timestamp = Some(commit.time().seconds());

		self.out_dir.push(&self.commit_id.as_ref().unwrap());

		let mut checkout_builder = git2::build::CheckoutBuilder::new();
		checkout_builder.force();
		repo.checkout_tree(&object, Some(&mut checkout_builder))?;
		repo.set_head_detached(object.id())?;
		assert_eq!(repo.state(), git2::RepositoryState::Clean);
		Ok(())
	}

	fn add_files<P: AsRef<Path>>(&self, build: &mut cc::Build, lib_dir: &Path, sub_dir: P) {
		let mut dir = lib_dir.to_path_buf();
		dir.push(sub_dir);
		for entry in fs::read_dir(&dir).unwrap() {
			let entry = entry.unwrap();
			let path = entry.path();
			let ext = path.extension().unwrap();
			if ext == "c" || ext == "S" {
				build.file(path);
			}
		}
	}

	fn compile(&self) {
		let mut config_path = self.out_dir.clone();
		config_path.push("config.json");

		let prev_config = Config::read(&config_path);
		if let Some(prev_config) = prev_config {
			if self.config == prev_config {
				return;
			}
		}

		let mut lib_dir = self.repo_dir.clone();
		lib_dir.push("lib");
		let mut build = cc::Build::new();
		self.add_files(&mut build, &lib_dir, "common");
		self.add_files(&mut build, &lib_dir, "compress");
		self.add_files(&mut build, &lib_dir, "decompress");
		build.out_dir(&self.out_dir)
			.opt_level(3)
			.flag("-g")
			.include(&lib_dir)
			.file("src/zstd.c")
			.cargo_metadata(false)
			.compile("libzstd_bench.a");
		self.config.write(&config_path);
	}

	fn go(&mut self) {
		self.prepare_repo().unwrap();
		self.compile();
		self.print_cargo_rustc_metadata();
	}
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Config {
	cc: String,
	cc_version: String,
	cflags: String,
	zstd_hash: u64,
}

fn get_version(cc: &str) -> String {
	let output = Command::new(cc)
		.arg("--version")
		.output()
		.unwrap();
	assert_eq!(output.status.success(), true);
	let re = Regex::new(r"\s\d+\.\d+\.\d+\s").unwrap();
	let out = std::str::from_utf8(&output.stdout).unwrap();
	let version = re.find(&out).unwrap();
	version.as_str().to_owned()
}

impl Config {
	fn read(path: &Path) -> Option<Self> {
		fs::read(path)
			.ok()
			.map(|bytes| serde_json::from_slice(&bytes).ok())
			.flatten()
	}

	fn write(&self, path: &Path) {
		let bytes = serde_json::to_vec(&self).unwrap();
		fs::write(path, &bytes).unwrap();
	}

	fn from_env() -> Self {
		let cc = env::var("CC").unwrap_or("cc".to_owned());
		let cc_version = get_version(&cc);
		let cflags = env::var("CFLAGS").unwrap_or(String::new());
		let zstd_hash = fasthash::xx::hash64(&fs::read("src/zstd.c").unwrap());
		Config {
			cc,
			cc_version,
			cflags,
			zstd_hash,
		}
	}

	fn print_cargo_rerun_metadata() {
		println!("cargo:rerun-if-env-changed=CC");
		println!("cargo:rerun-if-env-changed=CFLAGS");
	}

	fn print_cargo_rustc_metadata(&self) {
		println!("cargo:rustc-env=CC={}", self.cc);
		println!("cargo:rustc-env=CC_VERSION={}", self.cc_version);
		println!("cargo:rustc-env=CFLAGS={}", self.cflags);
	}
}

fn main() {
	Zstd::print_cargo_rerun_metadata();

	let zstd = Zstd::from_env();
	if zstd.is_none() {
		return;
	}
	let mut zstd = zstd.unwrap();
	zstd.go();
}
