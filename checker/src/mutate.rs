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

#[derive(Debug, Clone)]
pub struct Mutant {
    pub function: FnInfo,
    pub path: PathBuf,
}

fn sanitize(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

/// Write one mutant per candidate function of `input` into `out_dir`
/// (created if needed), named `<stem>__<index>_<function>.rs`.
pub fn write_mutants(input: &Path, out_dir: &Path) -> Result<Vec<Mutant>, String> {
    let src = std::fs::read_to_string(input).map_err(|e| format!("{}: {e}", input.display()))?;
    let fns = extract_functions(&src)?;
    std::fs::create_dir_all(out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("input");
    let mut out = Vec::new();
    for (i, f) in fns.into_iter().filter(|f| f.is_candidate()).enumerate() {
        let path = out_dir.join(format!("{stem}__{i}_{}.rs", sanitize(&f.name)));
        std::fs::write(&path, mutate_source(&src, &f)).map_err(|e| format!("{}: {e}", path.display()))?;
        out.push(Mutant { function: f, path });
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
}
