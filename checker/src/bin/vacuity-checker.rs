use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use vacuity_checker::driver::{check_file, Verdict};
use vacuity_checker::extract::extract_functions;
use vacuity_checker::mutate::write_mutants;
use vacuity_checker::runner::{run_verus, Outcome, VerusConfig};

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
    if args.first().map(String::as_str) != Some("run") || args.len() < 2 {
        eprintln!("usage: vacuity-checker run <file.rs> [--timeout SECS] [--verus PATH]");
        eprintln!("       vacuity-checker extract <file.rs>");
        eprintln!("       vacuity-checker mutate <file.rs> <out_dir>");
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
    match check_file(&config, &input, &out) {
        Ok(report) => {
            let mut vacuous = 0;
            for f in &report.functions {
                let (tag, note) = match &f.verdict {
                    Verdict::Vacuous => {
                        vacuous += 1;
                        ("VACUOUS", String::new())
                    }
                    Verdict::NotVacuous => ("ok", String::new()),
                    Verdict::Inconclusive(why) => ("?", format!("  ({why})")),
                };
                println!("{tag:8} {}:{}  {}{note}", path, f.line, f.name);
            }
            println!("{} function(s) checked, {vacuous} vacuous; mutants in {}", report.functions.len(), out.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{path}: {e}");
            ExitCode::from(1)
        }
    }
}
