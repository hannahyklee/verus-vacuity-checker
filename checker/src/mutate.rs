//! Produce mutated copies of a source file with one function body stubbed out.

use std::path::{Path, PathBuf};

use crate::extract::{extract_functions, FnInfo};

/// The replacement body. Verus requires `unimplemented!()` to be unreachable,
/// so a mutant verifies only if the function's `requires` is contradictory.
pub const STUB: &str = "{ unimplemented!() }";

/// Return `src` with `f`'s body replaced by [`STUB`]. Everything else is byte-identical.
/// The newlines of the old body are kept (after the stub) so line numbers below
/// the function don't shift and errors map back to the original file.
pub fn mutate_source(src: &str, f: &FnInfo) -> String {
    let old = &src[f.body.clone()];
    let newlines = old.matches('\n').count();
    let mut out = String::with_capacity(src.len());
    out.push_str(&src[..f.body.start]);
    out.push_str(STUB);
    out.push_str(&"\n".repeat(newlines));
    out.push_str(&src[f.body.end..]);
    out
}

/// A unified diff (3 lines of context) of the change `mutate_source` makes to `src`.
/// `display_path` is shown in the `---`/`+++` header.
pub fn diff_for(src: &str, f: &FnInfo, display_path: &str) -> String {
    let mutated = mutate_source(src, f);
    let old: Vec<&str> = src.lines().collect();
    let new: Vec<&str> = mutated.lines().collect();
    // The stub keeps the line count, so the two versions pair up line by line.
    let differs = |i: usize| old[i] != new[i];
    let first = (0..old.len()).find(|&i| differs(i)).unwrap_or(0);
    let last = (0..old.len()).rfind(|&i| differs(i)).unwrap_or(0);
    let from = first.saturating_sub(3);
    let to = (last + 3).min(old.len() - 1);
    let n = to - from + 1;

    let mut out = format!("--- a/{display_path}\n+++ b/{display_path}\n");
    out.push_str(&format!("@@ -{0},{n} +{0},{n} @@ fn {1}\n", from + 1, f.name));
    for l in &old[from..first] {
        out.push_str(&format!(" {l}\n"));
    }
    for l in &old[first..=last] {
        out.push_str(&format!("-{l}\n"));
    }
    for l in &new[first..=last] {
        out.push_str(&format!("+{l}\n"));
    }
    for l in &old[last + 1..=to] {
        out.push_str(&format!(" {l}\n"));
    }
    out
}

#[derive(Debug, Clone)]
pub struct Mutant {
    pub function: FnInfo,
    /// The full mutated copy of the file.
    pub path: PathBuf,
    /// Unified diff of the mutation.
    pub diff: PathBuf,
}

fn sanitize(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

/// Write one mutant per candidate function of `input` into `out_dir`
/// (cleared first; see [`crate::outdir::prepare`]), named `<stem>__<index>_<function>.rs`,
/// each with a `.diff` of the change next to it.
pub fn write_mutants(input: &Path, out_dir: &Path) -> Result<Vec<Mutant>, String> {
    let src = std::fs::read_to_string(input).map_err(|e| format!("{}: {e}", input.display()))?;
    let fns = extract_functions(&src)?;
    crate::outdir::prepare(out_dir)?;
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("input");
    let mut out = Vec::new();
    for (i, f) in fns.into_iter().filter(|f| f.is_candidate()).enumerate() {
        let path = out_dir.join(format!("{stem}__{i}_{}.rs", sanitize(&f.name)));
        std::fs::write(&path, mutate_source(&src, &f)).map_err(|e| format!("{}: {e}", path.display()))?;
        let diff = path.with_extension("diff");
        let shown = input.file_name().and_then(|n| n.to_str()).unwrap_or("input");
        std::fs::write(&diff, diff_for(&src, &f, shown)).map_err(|e| format!("{}: {e}", diff.display()))?;
        out.push(Mutant { function: f, path, diff });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_only_the_body() {
        let src = "verus! {\nfn f(x: u32) -> (r: u32)\n    requires x < 5,\n{\n    x\n}\nfn g() {}\n}\n";
        let fns = extract_functions(src).unwrap();
        let m = mutate_source(src, &fns[0]);
        assert!(m.contains("{ unimplemented!() }"));
        assert!(m.contains("fn g() {}"));
        // Same number of lines, still parses, and f is now a stub.
        assert_eq!(m.lines().count(), src.lines().count());
        let after = extract_functions(&m).unwrap();
        assert_eq!(&m[after[0].body.clone()], STUB);
        assert_eq!(after.len(), 2);
    }

    #[test]
    fn diff_shows_removed_body_and_stub() {
        let src = "verus! {\nfn f(x: u32) -> (r: u32)\n    requires x < 5,\n{\n    x\n}\nfn g() {}\n}\n";
        let fns = extract_functions(src).unwrap();
        let d = diff_for(src, &fns[0], "a.rs");
        assert!(d.starts_with("--- a/a.rs\n+++ b/a.rs\n@@ -1,8 +1,8 @@ fn f\n"), "{d}");
        assert!(d.contains("-{\n-    x\n-}\n"), "{d}");
        assert!(d.contains("+{ unimplemented!() }\n"), "{d}");
        assert!(d.contains(" fn g() {}\n"), "{d}");
    }
}
