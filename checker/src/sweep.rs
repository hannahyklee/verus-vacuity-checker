//! Run the checker over every Verus file under a directory.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::driver::{check_crate, check_file, Report};
use crate::runner::VerusConfig;

#[derive(Debug)]
pub enum FileResult {
    Checked(Report),
    /// The unmutated file did not verify (or could not be read/parsed): skipped.
    Skipped(String),
}

#[derive(Debug)]
pub struct SweepEntry {
    /// Path relative to the swept root.
    pub rel_path: PathBuf,
    pub result: FileResult,
}

impl SweepEntry {
    /// Path of the function's source file relative to the swept root.
    pub fn path_of(&self, f: &crate::driver::FnReport) -> PathBuf {
        if f.file.as_os_str().is_empty() { self.rel_path.clone() } else { self.rel_path.join(&f.file) }
    }
}

const SKIP_DIRS: &[&str] = &["target", ".git", "node_modules"];

/// What a sweep found under a root directory.
#[derive(Debug, Default)]
pub struct Projects {
    /// Directories holding a `Cargo.toml` with a `[package]` section; checked as a unit.
    pub crates: Vec<PathBuf>,
    /// Standalone `.rs` files containing a `verus!` block (outside any crate), sorted.
    pub files: Vec<PathBuf>,
}

fn is_package_manifest(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("Cargo.toml")).is_ok_and(|s| s.contains("[package]"))
}

/// Find crates and standalone Verus files under `root`.
/// Skips build/VCS directories and `skip_dir` (typically the output dir).
pub fn find_projects(root: &Path, skip_dir: Option<&Path>) -> Projects {
    let skip_dir = skip_dir.and_then(|d| d.canonicalize().ok());
    let mut out = Projects::default();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if is_package_manifest(&dir) {
            out.crates.push(dir);
            continue; // the crate's own files are checked via the crate
        }
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let path = entry.path();
            // `file_type` does not follow symlinks, so symlink loops are not walked.
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() {
                let name = entry.file_name();
                let skipped = SKIP_DIRS.iter().any(|s| name == *s)
                    || skip_dir.as_ref().is_some_and(|s| path.canonicalize().ok().as_ref() == Some(s));
                if !skipped {
                    stack.push(path);
                }
            } else if ft.is_file()
                && path.extension().is_some_and(|e| e == "rs")
                && std::fs::read_to_string(&path).is_ok_and(|s| s.contains("verus!"))
            {
                out.files.push(path);
            }
        }
    }
    out.crates.sort();
    out.files.sort();
    out
}

/// Standalone `.rs` files under `root` that contain a `verus!` block (not inside a crate).
pub fn find_verus_files(root: &Path, skip_dir: Option<&Path>) -> Vec<PathBuf> {
    find_projects(root, skip_dir).files
}

enum Job {
    Crate(PathBuf),
    File(PathBuf),
}

/// Check every Verus file under `root` with `jobs` files in flight at once.
/// Mutants go to `out_dir/<relative path with separators replaced>/`.
pub fn sweep(config: &VerusConfig, root: &Path, out_dir: &Path, jobs: usize) -> Vec<SweepEntry> {
    let found = find_projects(root, Some(out_dir));
    let jobs_list: Vec<Job> = found
        .crates
        .into_iter()
        .map(Job::Crate)
        .chain(found.files.into_iter().map(Job::File))
        .collect();
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<(usize, SweepEntry)>> = Mutex::new(Vec::new());

    std::thread::scope(|s| {
        for _ in 0..jobs.max(1) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(job) = jobs_list.get(i) else { break };
                let path = match job {
                    Job::Crate(p) | Job::File(p) => p,
                };
                let rel = path.strip_prefix(root).unwrap_or(path).to_path_buf();
                let sub = rel.with_extension("").to_string_lossy().replace(['/', '\\'], "__");
                let sub = if sub.is_empty() { "crate".to_string() } else { sub };
                let checked = match job {
                    Job::Crate(p) => check_crate(config, p, &out_dir.join(&sub)),
                    Job::File(p) => check_file(config, p, &out_dir.join(&sub)),
                };
                let result = match checked {
                    Ok(r) => FileResult::Checked(r),
                    Err(e) => FileResult::Skipped(e),
                };
                results.lock().unwrap().push((i, SweepEntry { rel_path: rel, result }));
            });
        }
    });

    let mut v = results.into_inner().unwrap();
    v.sort_by_key(|(i, _)| *i);
    v.into_iter().map(|(_, e)| e).collect()
}
