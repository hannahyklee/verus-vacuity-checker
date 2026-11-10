use std::path::PathBuf;
use vacuity_checker::driver::Verdict;
use vacuity_checker::runner::VerusConfig;
use vacuity_checker::sweep::{find_verus_files, sweep, FileResult};

fn test_files() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test_files")
}

#[test]
fn finds_only_files_with_verus_blocks() {
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("sweep_find");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join("sub")).unwrap();
    std::fs::create_dir_all(tmp.join("target")).unwrap();
    std::fs::write(tmp.join("a.rs"), "verus! { }").unwrap();
    std::fs::write(tmp.join("sub/b.rs"), "verus! { }").unwrap();
    std::fs::write(tmp.join("plain.rs"), "fn main() {}").unwrap();
    std::fs::write(tmp.join("target/c.rs"), "verus! { }").unwrap();
    std::fs::write(tmp.join("notes.txt"), "verus!").unwrap();
    let found: Vec<_> = find_verus_files(&tmp, None)
        .iter().map(|p| p.strip_prefix(&tmp).unwrap().to_string_lossy().into_owned()).collect();
    assert_eq!(found, ["a.rs", "sub/b.rs"]);
}

#[test]
fn sweeps_test_files() {
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("sweep_test_files");
    let _ = std::fs::remove_dir_all(&out);
    let entries = sweep(&VerusConfig::default(), &test_files(), &out, 4);

    let by_name = |n: &str| entries.iter().find(|e| e.rel_path.to_str() == Some(n)).unwrap();
    // Baseline failures are skipped, not mutated.
    for n in ["getting_started_failure.rs", "stub_semantics.rs"] {
        assert!(matches!(by_name(n).result, FileResult::Skipped(_)), "{n}");
    }
    // Files with no candidates are checked with zero mutants.
    match &by_name("getting_started.rs").result {
        FileResult::Checked(r) => assert!(r.functions.is_empty()),
        other => panic!("{other:?}"),
    }
    // Known vacuous functions are found.
    match &by_name("vacuity_cases.rs").result {
        FileResult::Checked(r) => {
            let mut v: Vec<_> = r.functions.iter().filter(|f| f.verdict == Verdict::Vacuous).map(|f| f.name.as_str()).collect();
            v.sort();
            assert_eq!(v, ["S::method_contradictory", "contradictory", "hidden_contradiction"]);
        }
        other => panic!("{other:?}"),
    }
    // Results are in a deterministic (sorted) order.
    let names: Vec<_> = entries.iter().map(|e| e.rel_path.clone()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
}
