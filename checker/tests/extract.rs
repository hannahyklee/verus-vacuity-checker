use std::path::PathBuf;
use vacuity_checker::extract::{extract_functions, FnKind};

fn fixture(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test_files").join(name);
    std::fs::read_to_string(p).unwrap()
}

#[test]
fn extract_cases() {
    let src = fixture("extract_cases.rs");
    let fns = extract_functions(&src).unwrap();
    let names: Vec<_> = fns.iter().map(|f| f.name.as_str()).collect();
    // `T::sig_only` has no body, so it must not appear.
    assert_eq!(
        names,
        ["double", "exec_with_spec", "exec_no_spec", "proof_with_spec", "external",
         "S::method", "T::with_default", "inner::nested", "main"]
    );
    let candidates: Vec<_> = fns.iter().filter(|f| f.is_candidate()).map(|f| f.name.as_str()).collect();
    assert_eq!(
        candidates,
        ["exec_with_spec", "S::method", "T::with_default", "inner::nested"]
    );
    assert_eq!(fns[0].kind, FnKind::Spec);
    assert_eq!(fns[3].kind, FnKind::Proof);
    assert!(fns[4].external);
    // Spans cover exactly the braced body.
    for f in &fns {
        let body = &src[f.body.clone()];
        assert!(body.starts_with('{') && body.ends_with('}'), "{}: {body:?}", f.name);
    }
}

#[test]
fn getting_started_has_no_candidates() {
    let fns = extract_functions(&fixture("getting_started.rs")).unwrap();
    assert_eq!(fns.len(), 2);
    assert!(fns.iter().all(|f| !f.is_candidate()));
}
