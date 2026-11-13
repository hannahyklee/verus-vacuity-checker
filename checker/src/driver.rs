//! End to end: verify the baseline, mutate each candidate, run Verus on each mutant.

use std::path::{Path, PathBuf};

use crate::extract::extract_functions;
use crate::mutate::{diff_for, mutate_source, write_mutants};
use crate::runner::{first_error, run_cargo_verus_with, run_verus, run_verus_in, Outcome, RunResult, VerusConfig};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The stubbed mutant verified: the function's `requires` is contradictory.
    Vacuous,
    /// The stubbed mutant failed verification (Verus could not prove the stub unreachable).
    NotVacuous,
    /// The function's file is not part of the build (inactive `cfg`, undeclared or commented-out
    /// module), so a stub there would "verify" trivially. No mutant was run.
    NotCompiled,
    /// Weak-ensures mode: the mutant returning a default value verified, so the `ensures` accepts
    /// it. A candidate for review, not necessarily a bug.
    Weak,
    /// Weak-ensures mode: the default-returning mutant failed verification (the `ensures` rejects it).
    NotWeak,
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
    /// Mutants decided by the single-function run alone (no full verification needed).
    pub single_function_hits: usize,
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
        return Err(baseline_error(&base));
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
    Ok(Report { functions, single_function_hits: 0 })
}

fn weak_mode() -> bool {
    crate::mutate::mode() == crate::mutate::Mode::DefaultBody
}

fn verdict_of(outcome: Outcome) -> Verdict {
    match outcome {
        Outcome::Verified { .. } if weak_mode() => Verdict::Weak,
        Outcome::Verified { .. } => Verdict::Vacuous,
        Outcome::Failed { errors: Some(n), .. } if n >= 1 && weak_mode() => Verdict::NotWeak,
        Outcome::Failed { errors: Some(n), .. } if n >= 1 => Verdict::NotVacuous,
        Outcome::Failed { .. } => Verdict::Inconclusive("mutant did not compile".into()),
        Outcome::Timeout => Verdict::Inconclusive("timeout".into()),
        Outcome::LaunchError(e) => Verdict::Inconclusive(format!("launch error: {e}")),
    }
}

/// Does the crate build include `file`? Appends a `compile_error!` canary: if the crate still
/// builds, the file is not part of it (inactive `cfg`, undeclared or commented-out `mod`).
/// A run that times out or cannot launch is treated as "compiled" so nothing is hidden.
fn file_is_compiled(config: &VerusConfig, crate_dir: &Path, file: &Path, src: &str, opts: &CrateOptions) -> bool {
    let _guard = RestoreOnDrop { path: file, original: src };
    let canary = format!("{src}\ncompile_error!(\"vacuity-checker canary\");\n");
    if std::fs::write(file, canary).is_err() {
        return true;
    }
    // In root mode `--no-verify` makes this a quick compile check instead of a full verification.
    let mut probe = opts.clone();
    if probe.root.is_some() {
        probe.verus_args.push("--no-verify".to_string());
    }
    let r = run_build(config, crate_dir, &probe);
    !matches!(r.outcome, Outcome::Verified { .. })
}

/// Verify the whole target once: `cargo verus verify`, or `verus` on the configured root file.
fn run_build(config: &VerusConfig, crate_dir: &Path, opts: &CrateOptions) -> RunResult {
    run_build_with(config, crate_dir, opts, &[])
}

/// [`run_build`] with extra arguments for Verus itself (e.g. `--verify-function`).
fn run_build_with(config: &VerusConfig, crate_dir: &Path, opts: &CrateOptions, verus_extra: &[String]) -> RunResult {
    match &opts.root {
        None => {
            let mut args = opts.cargo_args.clone();
            if !verus_extra.is_empty() {
                if !args.iter().any(|a| a == "--") {
                    args.push("--".to_string());
                }
                args.extend(verus_extra.iter().cloned());
            }
            run_cargo_verus_with(config, crate_dir, &args, &opts.env)
        }
        Some(root) => {
            let mut cfg = config.clone();
            cfg.extra_args.extend(opts.verus_args.iter().cloned());
            cfg.extra_args.extend(verus_extra.iter().cloned());
            if !cfg.extra_args.iter().any(|a| a.starts_with("--crate-type")) {
                cfg.extra_args.push("--crate-type=lib".to_string());
            }
            run_verus_in(&cfg, Some(crate_dir), Path::new(root))
        }
    }
}

/// The crate root file: the configured Verus root, else `src/lib.rs` / `src/main.rs`.
fn crate_root_file(crate_dir: &Path, opts: &CrateOptions) -> Option<PathBuf> {
    match &opts.root {
        Some(r) => Some(crate_dir.join(r)),
        None => ["src/lib.rs", "src/main.rs"].iter().map(|p| crate_dir.join(p)).find(|p| p.exists()),
    }
}

/// The Rust module path of `file` given the crate root file, assuming the usual layout
/// (`a/b.rs` or `a/b/mod.rs` is `a::b`). `Some("")` is the root module itself; `None` if the
/// file is outside the root's directory.
fn module_path(root_file: &Path, file: &Path) -> Option<String> {
    if root_file == file {
        return Some(String::new());
    }
    let rel = file.strip_prefix(root_file.parent()?).ok()?;
    let mut parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    let last = parts.pop()?;
    let stem = last.strip_suffix(".rs")?;
    if stem != "mod" {
        parts.push(stem.to_string());
    }
    Some(parts.join("::"))
}

/// Verus arguments that verify only function `f` of `file`, if its module can be named.
/// (`--verify-function` must be paired with `--verify-only-module` or `--verify-root`;
/// `--verify-module` is rejected.)
fn single_function_args(root_file: &Path, file: &Path, f: &crate::extract::FnInfo) -> Option<Vec<String>> {
    let module = module_path(root_file, file)?;
    let mut args = if module.is_empty() { vec!["--verify-root".to_string()] } else { vec!["--verify-only-module".to_string(), module] };
    args.push("--verify-function".to_string());
    args.push(f.name.clone());
    Some(args)
}

/// Verdict for the mutant currently written to `file`. With `single_function` this first verifies
/// just the stubbed function. A failure with errors is final: the unmodified crate verified, so the
/// stub caused it. Anything else (verified, nothing selected, could not launch) is re-checked with
/// a full verification, so a function that the name filter missed can never be reported vacuous.
/// The flag is true when the single-function run alone decided it.
fn verify_mutant(config: &VerusConfig, crate_dir: &Path, file: &Path, f: &crate::extract::FnInfo, opts: &CrateOptions) -> (Verdict, bool) {
    if opts.single_function {
        if let Some(args) = crate_root_file(crate_dir, opts).and_then(|root| single_function_args(&root, file, f)) {
            let r = run_build_with(config, crate_dir, opts, &args);
            if let Outcome::Failed { errors: Some(n), .. } = r.outcome {
                if n >= 1 {
                    return (if weak_mode() { Verdict::NotWeak } else { Verdict::NotVacuous }, true);
                }
            }
        }
    }
    (verdict_of(run_build(config, crate_dir, opts).outcome), false)
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

/// Exclusive lock on a crate while its files are being mutated in place. Two runs on the same
/// crate would each capture the other's mutated file as the "original" and could leave a stub behind.
struct CrateLock(PathBuf);

impl CrateLock {
    fn acquire(crate_dir: &Path) -> Result<Self, String> {
        let path = crate_dir.join(".vacuity-checker.lock");
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => Ok(CrateLock(path)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(format!(
                "{} is locked by another run (delete {} if that run crashed; then check `git diff` for leftover stubs)",
                crate_dir.display(),
                path.display()
            )),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }
}

impl Drop for CrateLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
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
    check_crate_with(config, crate_dir, out_dir, &CrateOptions::default())
}

/// Per-crate settings from a project config.
#[derive(Debug, Clone, Default)]
pub struct CrateOptions {
    /// Extra arguments for `cargo verus verify`.
    pub cargo_args: Vec<String>,
    /// Verus root file (relative to the crate dir); when set, `verus` is run directly on it
    /// with `verus_args` instead of `cargo verus verify`.
    pub root: Option<String>,
    pub verus_args: Vec<String>,
    /// Mutate at most this many functions, spread evenly over the crate (`None`: all).
    pub max_functions: Option<usize>,
    /// Verify only the stubbed function (`--verify-function`) instead of the whole crate.
    /// Off by default here; project configs turn it on unless they opt out.
    pub single_function: bool,
    pub env: Vec<(String, String)>,
}

/// [`check_crate`] with extra cargo arguments and environment.
pub fn check_crate_with(
    config: &VerusConfig,
    crate_dir: &Path,
    out_dir: &Path,
    opts: &CrateOptions,
) -> Result<Report, String> {
    let crate_dir = crate_dir
        .canonicalize()
        .map_err(|e| format!("{}: {e}", crate_dir.display()))?;
    let _lock = CrateLock::acquire(&crate_dir)?;
    let base: RunResult = run_build(config, &crate_dir, opts);
    if !base.verified() {
        return Err(baseline_error(&base));
    }
    crate::outdir::prepare(out_dir)?;

    // Pass 1: find the candidates of every file and whether the build includes the file.
    struct Item {
        file: PathBuf,
        rel: PathBuf,
        src: String,
        candidates: Vec<crate::extract::FnInfo>,
        compiled: bool,
    }
    let mut items = Vec::new();
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
        let candidates: Vec<_> = fns.into_iter().filter(|f| f.is_candidate()).collect();
        if candidates.is_empty() {
            continue;
        }
        let compiled = file_is_compiled(config, &crate_dir, &file, &src, opts);
        items.push(Item { file, rel, src, candidates, compiled });
    }

    // The cap applies to functions in compiled files only, spread evenly over them.
    let total: usize = items.iter().filter(|i| i.compiled).map(|i| i.candidates.len()).sum();
    let mut seen = 0usize;
    let to_do = opts.max_functions.map_or(total, |c| c.min(total));
    let mut done = 0usize;
    let mut single_function_hits = 0usize;
    let mut functions = Vec::new();
    for Item { file, rel, src, candidates, compiled } in items {
        for (i, f) in candidates.into_iter().enumerate() {
            if compiled {
                if let Some(cap) = opts.max_functions {
                    let g = seen;
                    seen += 1;
                    if (g + 1) * cap / total.max(1) <= g * cap / total.max(1) {
                        continue;
                    }
                }
            }
            let mutated = mutate_source(&src, &f);
            let tag = rel.with_extension("").to_string_lossy().replace(['/', '\\'], "__");
            let name: String = f.name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
            let diff = out_dir.join(format!("{tag}__{i}_{name}.diff"));
            let _ = std::fs::write(&diff, diff_for(&src, &f, &rel.to_string_lossy()));

            if !compiled {
                functions.push(FnReport { file: rel.clone(), name: f.name, line: f.line, verdict: Verdict::NotCompiled, mutant: None, diff });
                continue;
            }
            let verdict = {
                let _guard = RestoreOnDrop { path: &file, original: &src };
                std::fs::write(&file, &mutated).map_err(|e| format!("{}: {e}", file.display()))?;
                let (v, hit) = verify_mutant(config, &crate_dir, &file, &f, opts);
                single_function_hits += hit as usize;
                v
            };
            done += 1;
            eprintln!("[{done}/{to_do}] {}:{} {} -> {:?}", rel.display(), f.line, f.name, verdict);
            functions.push(FnReport { file: rel.clone(), name: f.name, line: f.line, verdict, mutant: None, diff });
        }
    }
    Ok(Report { functions, single_function_hits })
}

/// One-line reason first (what the report shows), then the full output.
fn baseline_error(base: &RunResult) -> String {
    let why = match &base.outcome {
        Outcome::Timeout => "timeout".to_string(),
        Outcome::LaunchError(e) => format!("launch error: {e}"),
        o => first_error(&base.stdout, &base.stderr).unwrap_or_else(|| format!("{o:?}")),
    };
    format!("baseline does not verify: {why}\n{:?}\n{}{}", base.outcome, base.stdout, base.stderr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_paths() {
        let root = Path::new("/c/src/lib.rs");
        assert_eq!(module_path(root, root), Some(String::new()));
        assert_eq!(module_path(root, Path::new("/c/src/helper.rs")).as_deref(), Some("helper"));
        assert_eq!(module_path(root, Path::new("/c/src/a/b.rs")).as_deref(), Some("a::b"));
        assert_eq!(module_path(root, Path::new("/c/src/a/mod.rs")).as_deref(), Some("a"));
        assert_eq!(module_path(root, Path::new("/elsewhere/x.rs")), None);
        // Root file in the crate directory itself.
        assert_eq!(module_path(Path::new("/c/lib.rs"), Path::new("/c/x/y.rs")).as_deref(), Some("x::y"));
    }
}
