//! Run Verus on a single file and classify the result.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct VerusConfig {
    /// Path to the `verus` binary.
    pub verus_path: PathBuf,
    /// Kill Verus after this long.
    pub timeout: Duration,
    /// Extra arguments passed before the file path.
    pub extra_args: Vec<String>,
}

impl Default for VerusConfig {
    /// `$VERUS_PATH` if set, otherwise `~/src/verus/verus`.
    fn default() -> Self {
        let verus_path = std::env::var_os("VERUS_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join("src/verus/verus"));
        VerusConfig {
            verus_path,
            timeout: Duration::from_secs(120),
            extra_args: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Verus reported zero errors.
    Verified { verified: u32 },
    /// Verus reported errors, or failed before verification (parse/type error).
    /// `errors` is `None` when no summary line was printed.
    Failed { verified: Option<u32>, errors: Option<u32> },
    /// Verus was killed after exceeding the timeout.
    Timeout,
    /// Verus could not be launched at all.
    LaunchError(String),
}

#[derive(Debug, Clone)]
pub struct RunResult {
    pub outcome: Outcome,
    pub stdout: String,
    pub stderr: String,
    pub elapsed: Duration,
}

impl RunResult {
    pub fn verified(&self) -> bool {
        matches!(self.outcome, Outcome::Verified { .. })
    }
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

/// PATH with `~/.cargo/bin` prepended: Verus needs `rustup` to be discoverable.
fn path_with_cargo() -> std::ffi::OsString {
    let mut paths = vec![home_dir().join(".cargo/bin")];
    if let Some(p) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&p));
    }
    std::env::join_paths(paths).unwrap_or_default()
}

/// Parse `verification results:: N verified, M errors` (ANSI codes stripped).
pub fn parse_summary(output: &str) -> Option<(u32, u32)> {
    let clean = strip_ansi(output);
    let line = clean.lines().find(|l| l.contains("verification results"))?;
    let after = line.split("::").nth(1)?;
    let mut nums = after
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<u32>());
    let verified = nums.next()?.ok()?;
    let errors = nums.next()?.ok()?;
    Some((verified, errors))
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn drain<R: Read + Send + 'static>(mut r: R) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = r.read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).into_owned()
    })
}

/// Run Verus on `file` and classify the result.
pub fn run_verus(config: &VerusConfig, file: &Path) -> RunResult {
    let start = Instant::now();
    let launch_err = |msg: String| RunResult {
        outcome: Outcome::LaunchError(msg),
        stdout: String::new(),
        stderr: String::new(),
        elapsed: start.elapsed(),
    };

    let mut child = match Command::new(&config.verus_path)
        .args(&config.extra_args)
        .arg(file)
        .env("PATH", path_with_cargo())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return launch_err(format!("{}: {e}", config.verus_path.display()));
        }
    };

    // Drain pipes on threads so a chatty child can't block on a full pipe.
    let out_h = drain(child.stdout.take().unwrap());
    let err_h = drain(child.stderr.take().unwrap());

    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if start.elapsed() >= config.timeout => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(e) => return launch_err(format!("wait failed: {e}")),
        }
    };

    let stdout = out_h.join().unwrap_or_default();
    let stderr = err_h.join().unwrap_or_default();
    let elapsed = start.elapsed();

    let outcome = match status {
        None => Outcome::Timeout,
        Some(status) => {
            // Verus prints the summary to stdout; look in both to be safe.
            let summary = parse_summary(&stdout).or_else(|| parse_summary(&stderr));
            match summary {
                Some((v, 0)) if status.success() => Outcome::Verified { verified: v },
                Some((v, e)) => Outcome::Failed { verified: Some(v), errors: Some(e) },
                None => Outcome::Failed { verified: None, errors: None },
            }
        }
    };
    RunResult { outcome, stdout, stderr, elapsed }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_summary() {
        assert_eq!(parse_summary("verification results:: 3 verified, 0 errors\n"), Some((3, 0)));
        assert_eq!(
            parse_summary("\x1b[1mverification results\x1b[0m:: 1 verified, 2 errors"),
            Some((1, 2))
        );
        assert_eq!(parse_summary("error: something else"), None);
    }
}
