//! Integration tests; require a working Verus install (see VERUS_PATH).

use std::path::PathBuf;
use std::time::Duration;
use vacuity_checker::runner::{run_verus, Outcome, VerusConfig};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test_files").join(name)
}

#[test]
fn verifies_passing_file() {
    let r = run_verus(&VerusConfig::default(), &fixture("getting_started.rs"));
    assert!(matches!(r.outcome, Outcome::Verified { .. }), "{:?}\n{}{}", r.outcome, r.stdout, r.stderr);
}

#[test]
fn reports_failing_file() {
    let r = run_verus(&VerusConfig::default(), &fixture("getting_started_failure.rs"));
    assert!(matches!(r.outcome, Outcome::Failed { errors: Some(1), .. }), "{:?}", r.outcome);
}

#[test]
fn reports_launch_error() {
    let cfg = VerusConfig { verus_path: "/nonexistent/verus".into(), ..Default::default() };
    let r = run_verus(&cfg, &fixture("getting_started.rs"));
    assert!(matches!(r.outcome, Outcome::LaunchError(_)));
}

#[test]
fn times_out() {
    let cfg = VerusConfig { timeout: Duration::from_millis(1), ..Default::default() };
    let r = run_verus(&cfg, &fixture("getting_started.rs"));
    assert_eq!(r.outcome, Outcome::Timeout);
}
