//! Per-repository findings: `report.md` (for people) and `summary.json` (for tools).

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

use crate::driver::Verdict;
use crate::runner::VerusConfig;
use crate::sweep::{FileResult, SweepEntry};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub targets: usize,
    pub checked: usize,
    pub skipped: usize,
    pub functions: usize,
    pub vacuous: usize,
    pub inconclusive: usize,
}

pub fn totals(entries: &[SweepEntry]) -> Totals {
    let mut t = Totals { targets: entries.len(), ..Default::default() };
    for e in entries {
        match &e.result {
            FileResult::Skipped(_) => t.skipped += 1,
            FileResult::Checked(r) => {
                t.checked += 1;
                t.functions += r.functions.len();
                for f in &r.functions {
                    match f.verdict {
                        Verdict::Vacuous => t.vacuous += 1,
                        Verdict::Inconclusive(_) => t.inconclusive += 1,
                        Verdict::NotVacuous => {}
                    }
                }
            }
        }
    }
    t
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").arg("-C").arg(root).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// `verus.version` from the `version.json` next to the verus binary.
fn verus_version(config: &VerusConfig) -> Option<String> {
    let text = std::fs::read_to_string(config.verus_path.parent()?.join("version.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v["verus"]["version"].as_str().map(str::to_string)
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").chars().take(120).collect()
}

/// Render the Markdown report for one checked repository (or directory).
pub fn render_markdown(root: &Path, entries: &[SweepEntry], config: &VerusConfig) -> String {
    let t = totals(entries);
    let mut m = String::new();
    // Inside a container the real path is not meaningful; the caller can supply a label.
    let label = std::env::var("VACUITY_ROOT_LABEL").unwrap_or_else(|_| root.display().to_string());
    let _ = writeln!(m, "# Vacuity report: `{label}`\n");
    if let Some(remote) = git(root, &["remote", "get-url", "origin"]) {
        let _ = writeln!(m, "- Repository: {remote}");
    }
    if let Some(commit) = git(root, &["rev-parse", "HEAD"]) {
        let _ = writeln!(m, "- Commit: `{commit}`");
    }
    let _ = writeln!(m, "- Verus: {}", verus_version(config).unwrap_or_else(|| "unknown".into()));
    let _ = writeln!(m, "- Stub: `unimplemented!()` (a mutant verifies only if the function's `requires` is unsatisfiable)\n");

    let _ = writeln!(m, "## Summary\n");
    let _ = writeln!(m, "| | |\n|---|---|");
    let _ = writeln!(m, "| Targets (crates / standalone files) | {} |", t.targets);
    let _ = writeln!(m, "| Checked | {} |", t.checked);
    let _ = writeln!(m, "| Skipped (baseline did not verify) | {} |", t.skipped);
    let _ = writeln!(m, "| Functions mutated | {} |", t.functions);
    let _ = writeln!(m, "| **Vacuous** | **{}** |", t.vacuous);
    let _ = writeln!(m, "| Not vacuous | {} |", t.functions - t.vacuous - t.inconclusive);
    let _ = writeln!(m, "| Inconclusive | {} |\n", t.inconclusive);

    let _ = writeln!(m, "## Vacuous functions ({})\n", t.vacuous);
    if t.vacuous == 0 {
        let _ = writeln!(m, "None found.\n");
    }
    for e in entries {
        let FileResult::Checked(r) = &e.result else { continue };
        for f in r.functions.iter().filter(|f| f.verdict == Verdict::Vacuous) {
            let _ = writeln!(m, "### `{}` in `{}:{}`\n", f.name, e.path_of(f).display(), f.line);
            let _ = writeln!(
                m,
                "With its body replaced by `unimplemented!()` the file still verifies, so the \
                 precondition can never hold. Mutation:\n"
            );
            let diff = std::fs::read_to_string(&f.diff).unwrap_or_else(|_| "(diff unavailable)\n".into());
            let _ = writeln!(m, "```diff\n{}```\n", diff);
        }
    }

    if t.inconclusive > 0 {
        let _ = writeln!(m, "## Inconclusive ({})\n", t.inconclusive);
        let _ = writeln!(m, "| Function | Location | Reason |\n|---|---|---|");
        for e in entries {
            let FileResult::Checked(r) = &e.result else { continue };
            for f in &r.functions {
                if let Verdict::Inconclusive(why) = &f.verdict {
                    let _ = writeln!(m, "| `{}` | `{}:{}` | {why} |", f.name, e.path_of(f).display(), f.line);
                }
            }
        }
        m.push('\n');
    }

    if t.skipped > 0 {
        let _ = writeln!(m, "## Skipped targets ({})\n", t.skipped);
        let _ = writeln!(m, "| Target | Reason |\n|---|---|");
        for e in entries {
            if let FileResult::Skipped(why) = &e.result {
                let _ = writeln!(m, "| `{}` | {} |", e.rel_path.display(), first_line(why).replace('|', "\\|"));
            }
        }
        m.push('\n');
    }

    let _ = writeln!(m, "## Per target\n");
    let _ = writeln!(m, "| Target | Functions mutated | Vacuous |\n|---|---|---|");
    for e in entries {
        if let FileResult::Checked(r) = &e.result {
            let v = r.functions.iter().filter(|f| f.verdict == Verdict::Vacuous).count();
            let _ = writeln!(m, "| `{}` | {} | {v} |", e.rel_path.display(), r.functions.len());
        }
    }
    m
}

/// Machine-readable results, one object per function (or per skipped target).
pub fn summary_json(entries: &[SweepEntry]) -> serde_json::Value {
    let mut v = Vec::new();
    for e in entries {
        match &e.result {
            FileResult::Checked(r) => {
                for f in &r.functions {
                    let verdict = match &f.verdict {
                        Verdict::Vacuous => "vacuous".to_string(),
                        Verdict::NotVacuous => "not_vacuous".to_string(),
                        Verdict::Inconclusive(w) => format!("inconclusive: {w}"),
                    };
                    v.push(serde_json::json!({
                        "file": e.path_of(f), "function": f.name, "line": f.line,
                        "verdict": verdict, "diff": f.diff, "mutant": f.mutant,
                    }));
                }
            }
            FileResult::Skipped(why) => {
                v.push(serde_json::json!({ "file": e.rel_path, "skipped": first_line(why) }));
            }
        }
    }
    serde_json::Value::Array(v)
}

/// Write `report.md` and `summary.json` into `out`.
pub fn write_outputs(
    root: &Path,
    out: &Path,
    entries: &[SweepEntry],
    config: &VerusConfig,
) -> std::io::Result<Totals> {
    std::fs::create_dir_all(out)?;
    std::fs::write(out.join("report.md"), render_markdown(root, entries, config))?;
    std::fs::write(out.join("summary.json"), serde_json::to_string_pretty(&summary_json(entries)).unwrap())?;
    Ok(totals(entries))
}
