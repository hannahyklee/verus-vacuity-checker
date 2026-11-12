//! Per-project build configuration (`configs/<project>.toml`).
//!
//! Real projects pin a Verus version and need particular cargo flags, so a project
//! config lists exactly which crates to verify and how:
//!
//! ```toml
//! name = "verified-storage"
//! [verus]
//! version = "0.2026.10.26.d17fbd0"   # Verus release the project needs
//!
//! [[target]]
//! path = "multilog/multilog"          # crate dir, relative to the repo root
//! cargo_args = ["--no-default-features"]
//! timeout_secs = 900                  # per Verus run (optional)
//! env = { LIBCLANG_PATH = "/usr/lib" }
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    pub name: String,
    #[serde(default)]
    pub verus: VerusSpec,
    /// Shell commands run (with `sh -c`, in the repository root) before checking, e.g. to build
    /// a dependency that a target passes via `--extern`. A failing command aborts the sweep.
    /// Body used to stub functions: `unimplemented` (default) or `assert_false`, for Verus
    /// releases that reject `unimplemented!()`.
    pub stub: Option<String>,
    #[serde(default)]
    pub setup: Vec<String>,
    #[serde(default, rename = "target")]
    pub targets: Vec<Target>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerusSpec {
    /// Verus release (e.g. `0.2026.10.26.d17fbd0`). `None` means the image default.
    pub version: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    /// Crate directory (holding `Cargo.toml`, or the Verus root file), relative to the repository root.
    pub path: String,
    /// Verus root file (relative to `path`). When set, the target is verified by running
    /// `verus <verus_args> <root>` in `path` instead of `cargo verus verify`.
    pub root: Option<String>,
    /// Extra arguments for `verus` (only with `root`).
    #[serde(default)]
    pub verus_args: Vec<String>,
    /// Extra arguments for `cargo verus verify`.
    #[serde(default)]
    pub cargo_args: Vec<String>,
    /// Mutate at most this many functions (evenly spread over the crate) to bound the run time.
    pub max_functions: Option<usize>,
    /// Verify only the stubbed function of each mutant (experimental, default false: every mutant
    /// re-verifies the whole crate). Not yet reliable, see the ignored test in tests/crate.rs.
    pub single_function: Option<bool>,
    /// Timeout for each Verus run on this crate.
    pub timeout_secs: Option<u64>,
    /// Extra environment variables for the Verus run.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

impl ProjectConfig {
    pub fn from_str(text: &str) -> Result<Self, String> {
        let c: ProjectConfig = toml::from_str(text).map_err(|e| format!("invalid config: {e}"))?;
        if let Some(st) = &c.stub {
            if !matches!(st.as_str(), "unimplemented" | "assert_false") {
                return Err(format!("unknown stub '{st}' (expected 'unimplemented' or 'assert_false')"));
            }
        }
        for t in &c.targets {
            let p = Path::new(&t.path);
            if p.is_absolute() || p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
                return Err(format!("target path '{}' must be relative and stay inside the repository", t.path));
            }
            if t.root.is_none() && !t.verus_args.is_empty() {
                return Err(format!("target '{}': verus_args needs root", t.path));
            }
            if t.root.is_some() && !t.cargo_args.is_empty() {
                return Err(format!("target '{}': cargo_args cannot be combined with root", t.path));
            }
            if let Some(r) = &t.root {
                let rp = Path::new(r);
                if rp.is_absolute() || rp.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
                    return Err(format!("target '{}': root must be relative and stay inside the target", t.path));
                }
            }
        }
        Ok(c)
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_validates() {
        let c = ProjectConfig::from_str(
            "name = \"x\"\n[verus]\nversion = \"1.2\"\n[[target]]\npath = \"a/b\"\ncargo_args = [\"--no-default-features\"]\ntimeout_secs = 5\nenv = { K = \"v\" }\n[[target]]\npath = \"c\"\n",
        )
        .unwrap();
        assert_eq!(c.verus.version.as_deref(), Some("1.2"));
        assert_eq!(c.targets.len(), 2);
        assert_eq!(c.targets[0].cargo_args, ["--no-default-features"]);
        assert_eq!(c.targets[0].env["K"], "v");
        assert!(c.targets[1].cargo_args.is_empty());
    }

    #[test]
    fn root_targets_and_setup() {
        let c = ProjectConfig::from_str(
            "name=\"x\"\nsetup=[\"make\"]\n[[target]]\npath=\"a\"\nroot=\"src/lib.rs\"\nverus_args=[\"--rlimit\",\"5\"]",
        )
        .unwrap();
        assert_eq!(c.setup, ["make"]);
        assert_eq!(c.targets[0].root.as_deref(), Some("src/lib.rs"));
        assert!(ProjectConfig::from_str("name=\"x\"\n[[target]]\npath=\"a\"\nverus_args=[\"-x\"]").is_err());
        assert!(ProjectConfig::from_str("name=\"x\"\n[[target]]\npath=\"a\"\nroot=\"../l.rs\"").is_err());
    }

    #[test]
    fn rejects_escaping_paths_and_unknown_keys() {
        assert!(ProjectConfig::from_str("name=\"x\"\n[[target]]\npath=\"../x\"").is_err());
        assert!(ProjectConfig::from_str("name=\"x\"\n[[target]]\npath=\"/abs\"").is_err());
        assert!(ProjectConfig::from_str("name=\"x\"\nbogus=1").is_err());
    }
}
