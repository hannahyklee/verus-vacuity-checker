//! Documents how Verus treats an `unimplemented!()` body: it must be provably
//! unreachable, so only a contradictory `requires` lets the stub verify.

use std::path::PathBuf;
use vacuity_checker::runner::{run_verus, Outcome, VerusConfig};

#[test]
fn only_contradictory_requires_verifies_with_stub() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test_files/stub_semantics.rs");
    let src = std::fs::read_to_string(&path).unwrap();
    let r = run_verus(&VerusConfig::default(), &path);

    // `contradictory` and `main` verify; the other three functions fail.
    assert_eq!(r.outcome, Outcome::Failed { verified: Some(2), errors: Some(3) }, "{}{}", r.stdout, r.stderr);

    // The failing stubs are exactly those in good/weak/requires_only.
    let mut failed: Vec<String> = Vec::new();
    for l in r.stderr.lines().chain(r.stdout.lines()) {
        if let Some(rest) = l.trim().strip_prefix("--> ") {
            let line: usize = rest.rsplitn(3, ':').nth(1).unwrap().parse().unwrap();
            failed.push(src.lines().nth(line - 1).unwrap().to_string());
            // Find the enclosing fn name by scanning upward.
            let name = src.lines().take(line).collect::<Vec<_>>().iter().rev()
                .find_map(|s| s.strip_prefix("fn ").map(|t| t.split('(').next().unwrap().to_string()))
                .unwrap();
            *failed.last_mut().unwrap() = name;
        }
    }
    failed.sort();
    assert_eq!(failed, ["good", "requires_only", "weak"]);
}

#[test]
fn assert_false_stub_has_the_same_semantics() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test_files/stub_semantics_assert.rs");
    let r = run_verus(&VerusConfig::default(), &path);
    assert_eq!(r.outcome, Outcome::Failed { verified: Some(2), errors: Some(3) }, "{}{}", r.stdout, r.stderr);
}
