use std::path::PathBuf;
use vacuity_checker::driver::check_file;
use vacuity_checker::report::{totals, write_outputs};
use vacuity_checker::runner::VerusConfig;
use vacuity_checker::sweep::{FileResult, SweepEntry};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test_files")
}

#[test]
fn report_lists_vacuous_functions_with_diffs_and_clears_stale_output() {
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("report_test");
    let _ = std::fs::remove_dir_all(&out);
    let config = VerusConfig::default();

    // A stale file from an earlier run must not survive the next run.
    let target = out.join("vacuity_cases");
    vacuity_checker::outdir::prepare(&out).unwrap();
    vacuity_checker::outdir::prepare(&target).unwrap();
    std::fs::write(target.join("stale.diff"), "old").unwrap();

    let report = check_file(&config, &fixtures().join("vacuity_cases.rs"), &target).unwrap();
    assert!(!target.join("stale.diff").exists());

    let entries = vec![SweepEntry { rel_path: "vacuity_cases.rs".into(), result: FileResult::Checked(report) }];
    let t = write_outputs(&fixtures(), &out, &entries, &config).unwrap();
    assert_eq!((t.functions, t.vacuous, t.inconclusive), (6, 3, 0));
    assert_eq!(t, totals(&entries));

    let md = std::fs::read_to_string(out.join("report.md")).unwrap();
    assert!(md.contains("## Vacuous functions (3)"), "{md}");
    assert!(md.contains("### `contradictory` in `vacuity_cases.rs:18`"), "{md}");
    assert!(md.contains("+{ unimplemented!() }"), "{md}");
    // The signature/spec is shown as unchanged context above the removed body.
    assert!(md.contains("     requires x < 5 && x > 10,"), "{md}");
    assert!(md.contains("-    x\n"), "{md}");

    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(out.join("summary.json")).unwrap()).unwrap();
    let vac: Vec<_> = json.as_array().unwrap().iter().filter(|x| x["verdict"] == "vacuous").collect();
    assert_eq!(vac.len(), 3);
    assert!(vac.iter().all(|x| x["file"] == "vacuity_cases.rs" && PathBuf::from(x["diff"].as_str().unwrap()).exists()));
}
