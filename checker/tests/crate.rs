mod common;

use std::path::PathBuf;
use vacuity_checker::driver::{check_crate, Verdict};
use vacuity_checker::runner::VerusConfig;

fn crate_dir() -> PathBuf {
    let original = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test_files/crates/vacuity_crate");
    common::private_copy(&original, "crate_test")
}

#[test]
fn crate_verdicts_across_files_and_files_are_restored() {
    let dir = crate_dir();
    let before: Vec<_> = ["src/lib.rs", "src/helper.rs"]
        .iter().map(|f| std::fs::read_to_string(dir.join(f)).unwrap()).collect();
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("crate_vacuity");
    let _ = std::fs::remove_dir_all(&out);

    let report = check_crate(&VerusConfig { timeout: std::time::Duration::from_secs(600), ..Default::default() }, &dir, &out).unwrap();

    use Verdict::*;
    let got: Vec<_> = report.functions.iter()
        .map(|f| (f.file.to_str().unwrap().to_string(), f.name.clone(), f.verdict.clone())).collect();
    // Functions in files that are not built are reported as such, never as vacuous.
    for name in ["windows_contradictory", "orphan_contradictory"] {
        let f = report.functions.iter().find(|f| f.name == name).unwrap();
        assert_eq!(f.verdict, NotCompiled, "{name}");
    }
    let got: Vec<_> = got.into_iter().filter(|(_, _, v)| *v != NotCompiled).collect();
    let mut want = vec![
        ("src/helper.rs".to_string(), "helper_contradictory".to_string(), Vacuous),
        ("src/helper.rs".to_string(), "helper_normal".to_string(), NotVacuous),
        ("src/lib.rs".to_string(), "lib_contradictory".to_string(), Vacuous),
        ("src/lib.rs".to_string(), "normal".to_string(), NotVacuous),
    ];
    let mut got = got;
    got.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    want.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    assert_eq!(got, want);

    // Crate mutants are applied in place, so only diffs are saved (no full copies).
    for f in report.functions.iter().filter(|f| f.verdict != NotCompiled) {
        assert!(f.mutant.is_none());
        let diff = std::fs::read_to_string(&f.diff).unwrap();
        assert!(diff.contains("+{ unimplemented!() }"), "{diff}");
        assert!(diff.contains(&format!("+++ b/{}", f.file.display())), "{diff}");
    }
    assert!(std::fs::read_dir(&out).unwrap().flatten().all(|e| e.path().extension().is_none_or(|x| x != "rs")));

    // Source files must be exactly as they were.
    let after: Vec<_> = ["src/lib.rs", "src/helper.rs"]
        .iter().map(|f| std::fs::read_to_string(dir.join(f)).unwrap()).collect();
    assert_eq!(before, after);
}

#[test]
fn concurrent_run_on_the_same_crate_is_refused() {
    let dir = common::private_copy(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test_files/crates/vacuity_crate"),
        "lock_test",
    );
    let before = std::fs::read_to_string(dir.join("src/lib.rs")).unwrap();
    std::fs::write(dir.join(".vacuity-checker.lock"), "").unwrap(); // another run holds it
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("lock_test_out");
    let err = check_crate(&Default::default(), &dir, &out).unwrap_err();
    assert!(err.contains("locked by another run"), "{err}");
    // The refused run must not touch the sources or remove the other run's lock.
    assert_eq!(before, std::fs::read_to_string(dir.join("src/lib.rs")).unwrap());
    assert!(dir.join(".vacuity-checker.lock").exists());
}

#[test]
fn max_functions_caps_and_spreads_the_sample() {
    use vacuity_checker::driver::{check_crate_with, CrateOptions};
    let original = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test_files/crates/vacuity_crate");
    let dir = common::private_copy(&original, "crate_capped_test");
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("crate_capped");
    let _ = std::fs::remove_dir_all(&out);
    let config = VerusConfig { timeout: std::time::Duration::from_secs(600), ..Default::default() };
    let opts = CrateOptions { max_functions: Some(3), ..Default::default() };
    let report = check_crate_with(&config, &dir, &out, &opts).unwrap();
    // Functions in files that are not built are reported but do not count toward the cap.
    let mutated = report.functions.iter().filter(|f| f.verdict != Verdict::NotCompiled).count();
    assert_eq!(mutated, 3, "{:?}", report.functions.iter().map(|f| (&f.name, &f.verdict)).collect::<Vec<_>>());
}

#[test]
fn root_mode_cap_counts_only_compiled_functions() {
    use vacuity_checker::driver::{check_crate_with, CrateOptions};
    let original = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test_files/crates/vacuity_crate");
    let dir = common::private_copy(&original, "crate_root_mode_test");
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("crate_root_mode");
    let _ = std::fs::remove_dir_all(&out);
    let config = VerusConfig { timeout: std::time::Duration::from_secs(600), ..Default::default() };
    let opts = CrateOptions { root: Some("src/lib.rs".into()), max_functions: Some(2), ..Default::default() };
    let report = check_crate_with(&config, &dir, &out, &opts).unwrap();
    let names = |v: &Verdict| report.functions.iter().filter(|f| &f.verdict == v).map(|f| f.name.clone()).collect::<Vec<_>>();
    let not_compiled = names(&Verdict::NotCompiled);
    assert!(not_compiled.contains(&"windows_contradictory".to_string()), "{not_compiled:?}");
    assert!(not_compiled.contains(&"orphan_contradictory".to_string()), "{not_compiled:?}");
    let mutated = report.functions.len() - not_compiled.len();
    assert_eq!(mutated, 2, "{:?}", report.functions.iter().map(|f| (&f.name, &f.verdict)).collect::<Vec<_>>());
}

// TODO: the fast path is not used yet (`fast path not used` assertion): in cargo mode the
// single-function run is not decided (cargo-verus arguments after `--` need checking), and on
// tla-rs a mutant hung Verus at 0% CPU. Verdicts stay correct because of the full-run fallback.
#[test]
#[ignore = "single-function optimization is unfinished"]
fn single_function_mode_gives_the_same_verdicts_as_whole_crate() {
    use vacuity_checker::driver::{check_crate_with, CrateOptions};
    let original = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test_files/crates/vacuity_crate");
    let config = VerusConfig { timeout: std::time::Duration::from_secs(600), ..Default::default() };
    for (label, root) in [("cargo", None), ("root", Some("src/lib.rs".to_string()))] {
        let mut results = Vec::new();
        for single in [false, true] {
            let mut hits = 0;
            let dir = common::private_copy(&original, &format!("crate_single_{label}_{single}"));
            let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("crate_single_{label}_{single}"));
            let _ = std::fs::remove_dir_all(&out);
            let opts = CrateOptions { root: root.clone(), single_function: single, ..Default::default() };
            let report = check_crate_with(&config, &dir, &out, &opts).unwrap();
            hits = report.single_function_hits;
            let mut v: Vec<_> = report.functions.iter().map(|f| (f.file.clone(), f.name.clone(), f.verdict.clone())).collect();
            v.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
            let not_vacuous = v.iter().filter(|(_, _, x)| *x == Verdict::NotVacuous).count();
            // Every not-vacuous mutant must be decided by the fast single-function run, not the
            // fallback (which would give the same verdicts and hide a broken fast path).
            assert_eq!(hits, if single { not_vacuous } else { 0 }, "{label} single={single}: fast path not used");
            results.push(v);
        }
        assert_eq!(results[0], results[1], "{label}: single-function verdicts differ from whole-crate");
        assert!(results[1].iter().any(|(_, _, v)| *v == Verdict::Vacuous), "{label}: expected a vacuous function");
    }
}
