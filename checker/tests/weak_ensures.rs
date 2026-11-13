//! Weak-ensures mode (kept in its own test binary: the mutation mode is process-wide).
use std::path::PathBuf;
use vacuity_checker::driver::{check_file, Verdict};
use vacuity_checker::runner::VerusConfig;

#[test]
fn default_body_mutants_flag_only_postconditions_that_accept_the_default() {
    vacuity_checker::mutate::set_mode("default_body").unwrap();
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test_files/weak_ensures.rs");
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("weak_ensures");
    let _ = std::fs::remove_dir_all(&out);
    let report = check_file(&VerusConfig::default(), &file, &out).unwrap();
    let got: Vec<(&str, Verdict)> = report.functions.iter().map(|f| (f.name.as_str(), f.verdict.clone())).collect();
    use Verdict::*;
    // `zero` (body is already the default) and `mk` (user type: no default) are not candidates.
    assert_eq!(
        got,
        [
            ("strong", NotWeak),
            ("weak", Weak),
            ("weak_bool", Weak),
            ("is_zero", NotWeak),
            ("unit", Weak),
            ("unit_strong", NotWeak),
            ("pair", NotWeak),
        ]
    );
    let weak = report.functions.iter().find(|f| f.name == "weak").unwrap();
    let diff = std::fs::read_to_string(&weak.diff).unwrap();
    assert!(diff.contains("+fn weak") && diff.contains("{ 0 }"), "{diff}");
}
