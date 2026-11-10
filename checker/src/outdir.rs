//! Output directories that the checker owns and may safely clear.

use std::path::Path;

const MARKER: &str = ".vacuity-checker";

/// Make `dir` an empty output directory.
///
/// An existing directory is only cleared if it was created by this tool (it holds a
/// marker file) or is already empty, so a mistaken `--out .` cannot delete anything.
pub fn prepare(dir: &Path) -> Result<(), String> {
    let err = |e: std::io::Error| format!("{}: {e}", dir.display());
    if dir.exists() {
        let owned = dir.join(MARKER).exists();
        let empty = std::fs::read_dir(dir).map_err(err)?.next().is_none();
        if !owned && !empty {
            return Err(format!(
                "refusing to clear {}: not empty and not created by vacuity-checker (choose another --out)",
                dir.display()
            ));
        }
        std::fs::remove_dir_all(dir).map_err(err)?;
    }
    std::fs::create_dir_all(dir).map_err(err)?;
    std::fs::write(dir.join(MARKER), "output directory of vacuity-checker; safe to delete\n").map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clears_owned_and_refuses_foreign() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/outdir_test");
        let _ = std::fs::remove_dir_all(&base);
        let owned = base.join("owned");
        prepare(&owned).unwrap();
        std::fs::write(owned.join("stale.txt"), "x").unwrap();
        prepare(&owned).unwrap();
        assert!(!owned.join("stale.txt").exists());

        let foreign = base.join("foreign");
        std::fs::create_dir_all(&foreign).unwrap();
        std::fs::write(foreign.join("precious.txt"), "x").unwrap();
        assert!(prepare(&foreign).is_err());
        assert!(foreign.join("precious.txt").exists());
    }
}
