//! End to end: verify the baseline, mutate each candidate, run Verus on each mutant.

use std::path::{Path, PathBuf};

use crate::extract::extract_functions;
use crate::mutate::{diff_for, mutate_source, write_mutants};
use crate::runner::{run_cargo_verus, run_verus, Outcome, RunResult, VerusConfig};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The stubbed mutant verified: the function's `requires` is contradictory.
    Vacuous,
    /// The stubbed mutant failed verification (Verus could not prove the stub unreachable).
    NotVacuous,
    /// The mutant did not compile, timed out, or Verus could not be launched.
    Inconclusive(String),
}

#[derive(Debug, Clone)]
pub struct FnReport {
    /// Source file relative to the crate root; empty for a single-file check.
    pub file: PathBuf,
    pub name: String,
    pub line: usize,
    pub verdict: Verdict,
    /// The full mutated file (single-file checks only; crate mutants are applied in place).
    pub mutant: Option<PathBuf>,
    /// Unified diff of the mutation.
    pub diff: PathBuf,
}

#[derive(Debug)]
pub struct Report {
    pub functions: Vec<FnReport>,
}

/// Check one file. `Err` means the baseline did not verify (or the file could not
/// be read/parsed), in which case mutant results would be meaningless.
pub fn check_file(config: &VerusConfig, input: &Path, out_dir: &Path) -> Result<Report, String> {
    // Library-style files (no `fn main`) need `--crate-type=lib` or rustc errors out.
    let src = std::fs::read_to_string(input).map_err(|e| format!("{}: {e}", input.display()))?;
    let mut config = config.clone();
    if !src.contains("fn main") && !config.extra_args.iter().any(|a| a.starts_with("--crate-type")) {
        config.extra_args.push("--crate-type=lib".to_string());
    }
    let config = &config;
    let base = run_verus(config, input);
    if !base.verified() {
        return Err(format!(
            "baseline does not verify: {:?}\n{}{}",
            base.outcome, base.stdout, base.stderr
        ));
    }
    let mut functions = Vec::new();
    for m in write_mutants(input, out_dir)? {
        let r = run_verus(config, &m.path);
        let verdict = verdict_of(r.outcome);
        functions.push(FnReport {
            file: PathBuf::new(),
            name: m.function.name,
            line: m.function.line,
            verdict,
            mutant: Some(m.path),
            diff: m.diff,
        });
    }
    Ok(Report { functions })
}

fn verdict_of(outcome: Outcome) -> Verdict {
    match outcome {
        Outcome::Verified { .. } => Verdict::Vacuous,
        Outcome::Failed { errors: Some(n), .. } if n >= 1 => Verdict::NotVacuous,
        Outcome::Failed { .. } => Verdict::Inconclusive("mutant did not compile".into()),
        Outcome::Timeout => Verdict::Inconclusive("timeout".into()),
        Outcome::LaunchError(e) => Verdict::Inconclusive(format!("launch error: {e}")),
    }
}

/// Restores a file's original contents when dropped, even on panic.
struct RestoreOnDrop<'a> {
    path: &'a Path,
    original: &'a str,
}

impl Drop for RestoreOnDrop<'_> {
    fn drop(&mut self) {
        let _ = std::fs::write(self.path, self.original);
    }
}

const CRATE_SKIP_DIRS: &[&str] = &["target", "target-verus", ".git"];

/// All `.rs` files under `dir` (skipping build dirs and nested crates), sorted.
fn crate_sources(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let (path, Ok(ft)) = (e.path(), e.file_type()) else { continue };
            if ft.is_dir() {
                let name = e.file_name();
                let nested_crate = path != dir && path.join("Cargo.toml").exists();
                if !CRATE_SKIP_DIRS.iter().any(|s| name == *s) && !nested_crate {
                    stack.push(path);
                }
            } else if ft.is_file() && path.extension().is_some_and(|x| x == "rs") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Check a Cargo crate with `cargo verus verify`.
///
/// Each mutant is written over the real source file (so module paths and relative
/// path dependencies keep working), verified, and the file is restored afterwards.
/// A diff of every mutant is saved under `out_dir` for inspection. The crate must
/// not be modified by anyone else while this runs.
pub fn check_crate(config: &VerusConfig, crate_dir: &Path, out_dir: &Path) -> Result<Report, String> {
    let crate_dir = crate_dir
        .canonicalize()
        .map_err(|e| format!("{}: {e}", crate_dir.display()))?;
    let base: RunResult = run_cargo_verus(config, &crate_dir);
    if !base.verified() {
        return Err(format!(
            "baseline does not verify: {:?}\n{}{}",
            base.outcome, base.stdout, base.stderr
        ));
    }
    crate::outdir::prepare(out_dir)?;

    let mut functions = Vec::new();
    for file in crate_sources(&crate_dir) {
        let Ok(src) = std::fs::read_to_string(&file) else { continue };
        if !src.contains("verus!") {
            continue;
        }
        let rel = file.strip_prefix(&crate_dir).unwrap_or(&file).to_path_buf();
        let fns = match extract_functions(&src) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("warning: {}: {e}", rel.display());
                continue;
            }
        };
        for (i, f) in fns.into_iter().filter(|f| f.is_candidate()).enumerate() {
            let mutated = mutate_source(&src, &f);
            let tag = rel.with_extension("").to_string_lossy().replace(['/', '\\'], "__");
            let name: String = f.name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
            let diff = out_dir.join(format!("{tag}__{i}_{name}.diff"));
            let _ = std::fs::write(&diff, diff_for(&src, &f, &rel.to_string_lossy()));

            let verdict = {
                let _guard = RestoreOnDrop { path: &file, original: &src };
                std::fs::write(&file, &mutated).map_err(|e| format!("{}: {e}", file.display()))?;
                verdict_of(run_cargo_verus(config, &crate_dir).outcome)
            };
            functions.push(FnReport { file: rel.clone(), name: f.name, line: f.line, verdict, mutant: None, diff });
        }
    }
    Ok(Report { functions })
}
