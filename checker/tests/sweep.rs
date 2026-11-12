mod common;

use std::path::PathBuf;
use vacuity_checker::driver::Verdict;
use vacuity_checker::runner::VerusConfig;
use vacuity_checker::sweep::{find_verus_files, sweep, FileResult};

/// A private copy of `test_files/`: sweeps mutate crates in place, and these tests run in parallel.
fn test_files(name: &str) -> PathBuf {
    common::private_copy(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test_files"), name)
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
    let entries = sweep(&VerusConfig::default(), &test_files("sweep_all"), &out, 4);

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

#[test]
fn config_sweep_checks_only_listed_targets() {
    use vacuity_checker::config::ProjectConfig;
    use vacuity_checker::sweep::sweep_config;

    let project = ProjectConfig::from_str(
        "name = \"t\"\n[[target]]\npath = \"crates/vacuity_crate\"\ntimeout_secs = 600\n[[target]]\npath = \"crates/does_not_exist\"\n",
    )
    .unwrap();
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("sweep_config");
    let _ = std::fs::remove_dir_all(&out);
    let entries = sweep_config(&VerusConfig::default(), &test_files("sweep_cfg"), &out, 2, &project);

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].rel_path, PathBuf::from("crates/vacuity_crate"));
    match &entries[0].result {
        FileResult::Checked(r) => {
            let count = |v: Verdict| r.functions.iter().filter(|f| f.verdict == v).count();
            // 2 contradictory, 2 normal, and 2 functions in files that are not compiled.
            assert_eq!((r.functions.len(), count(Verdict::Vacuous), count(Verdict::NotCompiled)), (6, 2, 2));
        }
        other => panic!("{other:?}"),
    }
    // A listed target that is missing is reported, not silently dropped.
    assert!(matches!(entries[1].result, FileResult::Skipped(_)));
}
