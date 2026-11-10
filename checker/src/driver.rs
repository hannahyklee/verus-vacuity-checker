//! End to end: verify the baseline, mutate each candidate, run Verus on each mutant.

use std::path::{Path, PathBuf};

use crate::mutate::write_mutants;
use crate::runner::{run_verus, Outcome, VerusConfig};

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
    pub name: String,
    pub line: usize,
    pub verdict: Verdict,
    pub mutant: PathBuf,
}

#[derive(Debug)]
pub struct Report {
    pub functions: Vec<FnReport>,
}

/// Check one file. `Err` means the baseline did not verify (or the file could not
/// be read/parsed), in which case mutant results would be meaningless.
pub fn check_file(config: &VerusConfig, input: &Path, out_dir: &Path) -> Result<Report, String> {
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
        let verdict = match r.outcome {
            Outcome::Verified { .. } => Verdict::Vacuous,
            Outcome::Failed { errors: Some(n), .. } if n >= 1 => Verdict::NotVacuous,
            Outcome::Failed { .. } => Verdict::Inconclusive("mutant did not compile".into()),
            Outcome::Timeout => Verdict::Inconclusive("timeout".into()),
            Outcome::LaunchError(e) => Verdict::Inconclusive(format!("launch error: {e}")),
        };
        functions.push(FnReport {
            name: m.function.name,
            line: m.function.line,
            verdict,
            mutant: m.path,
        });
    }
    Ok(Report { functions })
}
