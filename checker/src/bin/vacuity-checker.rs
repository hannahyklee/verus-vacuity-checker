use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use vacuity_checker::driver::{check_crate, check_file, Verdict};
use vacuity_checker::extract::extract_functions;
use vacuity_checker::mutate::write_mutants;
use vacuity_checker::runner::{run_verus, Outcome, VerusConfig};
use vacuity_checker::report::write_outputs;
use vacuity_checker::config::ProjectConfig;
use vacuity_checker::sweep::{sweep, sweep_config, FileResult, SweepEntry};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("extract") && args.len() == 2 {
        return extract_cmd(&args[1]);
    }
    if args.first().map(String::as_str) == Some("mutate") && args.len() == 3 {
        return match write_mutants(&PathBuf::from(&args[1]), &PathBuf::from(&args[2])) {
            Ok(ms) => {
                for m in ms {
                    println!("{}\t{}", m.function.name, m.path.display());
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::from(1)
            }
        };
    }
    if args.first().map(String::as_str) == Some("check") && args.len() >= 2 {
        return check_cmd(&args[1], &args[2..]);
    }
    if args.first().map(String::as_str) == Some("config-version") && args.len() == 2 {
        // Used by scripts/docker-sweep.sh to pick the image for a project.
        return match ProjectConfig::load(&PathBuf::from(&args[1])) {
            Ok(c) => {
                println!("{}", c.verus.version.unwrap_or_default());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::from(1)
            }
        };
    }
    if args.first().map(String::as_str) == Some("sweep") && args.len() >= 2 {
        return sweep_cmd(&args[1], &args[2..]);
    }
    if args.first().map(String::as_str) != Some("run") || args.len() < 2 {
        eprintln!("usage: vacuity-checker run <file.rs> [--timeout SECS] [--verus PATH]");
        eprintln!("       vacuity-checker extract <file.rs>");
        eprintln!("       vacuity-checker mutate <file.rs> <out_dir>");
        eprintln!("       vacuity-checker sweep <dir> [--config FILE] [--out DIR] [--jobs N] [--timeout SECS] [--verus PATH]");
        eprintln!("       vacuity-checker check <file.rs> [--out DIR] [--timeout SECS] [--verus PATH]");
        return ExitCode::from(2);
    }
    let file = PathBuf::from(&args[1]);
    let mut config = VerusConfig::default();
    let mut it = args[2..].iter();
    while let Some(a) = it.next() {
        match (a.as_str(), it.next()) {
            ("--timeout", Some(v)) => match v.parse() {
                Ok(s) => config.timeout = Duration::from_secs(s),
                Err(_) => return ExitCode::from(2),
            },
            ("--verus", Some(v)) => config.verus_path = PathBuf::from(v),
            _ => {
                eprintln!("bad argument: {a}");
                return ExitCode::from(2);
            }
        }
    }

    let r = run_verus(&config, &file);
    println!("{:?} ({:.1}s)", r.outcome, r.elapsed.as_secs_f32());
    match r.outcome {
        Outcome::Verified { .. } => ExitCode::SUCCESS,
        _ => {
            eprint!("{}{}", r.stdout, r.stderr);
            ExitCode::from(1)
        }
    }
}

fn extract_cmd(path: &str) -> ExitCode {
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(2);
        }
    };
    match extract_functions(&src) {
        Ok(fns) => {
            let json: Vec<_> = fns
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "name": f.name,
                        "kind": format!("{:?}", f.kind).to_lowercase(),
                        "line": f.line,
                        "has_requires": f.has_requires,
                        "has_ensures": f.has_ensures,
                        "external": f.external,
                        "candidate": f.is_candidate(),
                        "body": [f.body.start, f.body.end],
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&json).unwrap());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{path}: {e}");
            ExitCode::from(1)
        }
    }
}

fn check_cmd(path: &str, opts: &[String]) -> ExitCode {
    let mut config = VerusConfig::default();
    let mut out: Option<PathBuf> = None;
    let mut it = opts.iter();
    while let Some(a) = it.next() {
        match (a.as_str(), it.next()) {
            ("--out", Some(v)) => out = Some(PathBuf::from(v)),
            ("--timeout", Some(v)) => match v.parse() {
                Ok(s) => config.timeout = Duration::from_secs(s),
                Err(_) => return ExitCode::from(2),
            },
            ("--verus", Some(v)) => config.verus_path = PathBuf::from(v),
            _ => {
                eprintln!("bad argument: {a}");
                return ExitCode::from(2);
            }
        }
    }
    let input = PathBuf::from(path);
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("input");
    let out = out.unwrap_or_else(|| PathBuf::from("results").join(stem));
    // A directory with a Cargo.toml is checked as a crate with `cargo verus verify`.
    let checked = if input.is_dir() { check_crate(&config, &input, &out) } else { check_file(&config, &input, &out) };
    match checked {
        Ok(report) => {
            let rel = input.file_name().map(PathBuf::from).unwrap_or_default();
            let entry = SweepEntry { rel_path: rel, result: FileResult::Checked(report) };
            let root = input.parent().map(PathBuf::from).unwrap_or_default();
            finish(&root, &out, &[entry], &config)
        }
        Err(e) => {
            eprintln!("{path}: {e}");
            ExitCode::from(1)
        }
    }
}

fn sweep_cmd(root: &str, opts: &[String]) -> ExitCode {
    let mut config = VerusConfig::default();
    let mut out = PathBuf::from("results/sweep");
    let mut jobs = 2usize;
    let mut project: Option<ProjectConfig> = None;
    let mut it = opts.iter();
    while let Some(a) = it.next() {
        match (a.as_str(), it.next()) {
            ("--out", Some(v)) => out = PathBuf::from(v),
            ("--config", Some(v)) => match ProjectConfig::load(&PathBuf::from(v)) {
                Ok(c) => project = Some(c),
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::from(2);
                }
            },
            ("--jobs", Some(v)) => match v.parse() {
                Ok(n) => jobs = n,
                Err(_) => return ExitCode::from(2),
            },
            ("--timeout", Some(v)) => match v.parse() {
                Ok(s) => config.timeout = Duration::from_secs(s),
                Err(_) => return ExitCode::from(2),
            },
            ("--verus", Some(v)) => config.verus_path = PathBuf::from(v),
            _ => {
                eprintln!("bad argument: {a}");
                return ExitCode::from(2);
            }
        }
    }
    let root = PathBuf::from(root);
    if let Err(e) = vacuity_checker::outdir::prepare(&out) {
        eprintln!("{e}");
        return ExitCode::from(1);
    }
    if let Some(st) = project.as_ref().and_then(|p| p.stub.as_deref()) {
        if let Err(e) = vacuity_checker::mutate::set_stub(st) {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    }
    if let Some(p) = &project {
        if let Err(e) = vacuity_checker::sweep::run_setup(&root, p) {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    }
    let entries = match &project {
        Some(p) => sweep_config(&config, &root, &out, jobs, p),
        None => sweep(&config, &root, &out, jobs),
    };
    finish(&root, &out, &entries, &config)
}

/// Print vacuous findings and write `report.md` / `summary.json`.
fn finish(root: &std::path::Path, out: &std::path::Path, entries: &[SweepEntry], config: &VerusConfig) -> ExitCode {
    for e in entries {
        if let FileResult::Checked(r) = &e.result {
            for f in r.functions.iter().filter(|f| f.verdict == Verdict::Vacuous) {
                println!("VACUOUS  {}:{}  {}", e.path_of(f).display(), f.line, f.name);
            }
        }
    }
    match write_outputs(root, out, entries, config) {
        Ok(t) => {
            println!(
                "{} target(s): {} checked, {} skipped (baseline failed); {} function(s) mutated, {} vacuous, {} inconclusive, {} in files not compiled",
                t.targets, t.checked, t.skipped, t.functions - t.not_compiled, t.vacuous, t.inconclusive, t.not_compiled
            );
            println!("report: {}", out.join("report.md").display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("could not write report: {e}");
            ExitCode::from(1)
        }
    }
}
