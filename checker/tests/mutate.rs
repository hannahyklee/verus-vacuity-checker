use std::path::PathBuf;
use vacuity_checker::extract::extract_functions;
use vacuity_checker::mutate::{write_mutants, STUB};
use vacuity_checker::runner::{run_verus, Outcome, VerusConfig};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test_files").join(name)
}

#[test]
fn mutants_are_valid_and_differ_only_in_one_body() {
    let input = fixture("extract_cases.rs");
    let src = std::fs::read_to_string(&input).unwrap();
    let out_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("mutate_extract_cases");
    let _ = std::fs::remove_dir_all(&out_dir);

    // Baseline must verify, or mutant results mean nothing.
    assert!(run_verus(&VerusConfig::default(), &input).verified());

    let mutants = write_mutants(&input, &out_dir).unwrap();
    let names: Vec<_> = mutants.iter().map(|m| m.function.name.as_str()).collect();
    assert_eq!(names, ["exec_with_spec", "S::method", "T::with_default", "inner::nested"]);

    for m in &mutants {
        let mutated = std::fs::read_to_string(&m.path).unwrap();
        // Outside the replaced body the file is byte-identical.
        let (b, e) = (m.function.body.start, m.function.body.end);
        assert_eq!(&mutated[..b], &src[..b], "{}", m.function.name);
        assert!(mutated[b..].starts_with(STUB));
        assert!(mutated.ends_with(&src[e..]), "{}", m.function.name);
        // Still parses, with the same functions.
        assert_eq!(extract_functions(&mutated).unwrap().len(), 9);

        // Verus accepts the mutant syntactically: it must reach verification and
        // fail there (none of these functions has a contradictory `requires`).
        let r = run_verus(&VerusConfig::default(), &m.path);
        assert!(
            matches!(r.outcome, Outcome::Failed { errors: Some(n), .. } if n >= 1),
            "{}: {:?}\n{}{}", m.function.name, r.outcome, r.stdout, r.stderr
        );
    }
}
