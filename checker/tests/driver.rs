use std::path::PathBuf;
use vacuity_checker::driver::{check_file, Verdict};
use vacuity_checker::runner::VerusConfig;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test_files").join(name)
}

#[test]
fn vacuity_cases_have_expected_verdicts() {
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("driver_vacuity_cases");
    let _ = std::fs::remove_dir_all(&out);
    let report = check_file(&VerusConfig::default(), &fixture("vacuity_cases.rs"), &out).unwrap();

    use Verdict::*;
    let expected = [
        ("normal", NotVacuous),
        ("contradictory", Vacuous),
        ("hidden_contradiction", Vacuous),
        ("ensures_only", NotVacuous),
        // `no_spec` is not a candidate, so it must not appear at all.
        ("S::method_contradictory", Vacuous),
        ("S::method_normal", NotVacuous),
    ];
    let got: Vec<_> = report.functions.iter().map(|f| (f.name.as_str(), f.verdict.clone())).collect();
    let want: Vec<_> = expected.iter().map(|(n, v)| (*n, v.clone())).collect();
    assert_eq!(got, want);
}

#[test]
fn failing_baseline_is_an_error() {
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("driver_failure");
    let r = check_file(&VerusConfig::default(), &fixture("getting_started_failure.rs"), &out);
    assert!(r.unwrap_err().contains("baseline does not verify"));
}
