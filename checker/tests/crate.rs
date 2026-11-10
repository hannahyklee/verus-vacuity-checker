use std::path::PathBuf;
use vacuity_checker::driver::{check_crate, Verdict};
use vacuity_checker::runner::VerusConfig;

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test_files/crates/vacuity_crate")
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
    for f in &report.functions {
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
