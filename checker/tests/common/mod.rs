//! Shared helpers for integration tests.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// Copy `src` (skipping `target/`) to a fresh directory under the cargo test tmp dir.
/// Tests that mutate crates in place must work on a private copy: tests run in parallel,
/// and two runs on one directory would corrupt each other.
pub fn private_copy(src: &Path, name: &str) -> PathBuf {
    let dst = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("copies").join(name);
    let _ = std::fs::remove_dir_all(&dst);
    copy_dir(src, &dst);
    dst
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap().flatten() {
        let (from, to) = (e.path(), dst.join(e.file_name()));
        if e.file_type().unwrap().is_dir() {
            if e.file_name() != "target" {
                copy_dir(&from, &to);
            }
        } else {
            std::fs::copy(&from, &to).unwrap();
        }
    }
}
