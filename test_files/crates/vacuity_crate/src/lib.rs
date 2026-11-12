// Multi-file crate with known verdicts (see checker/tests/crate.rs).
use vstd::prelude::*;

mod helper;

// Not compiled on this platform: functions in it must not be reported (a stub there "verifies"
// only because the file is never built).
#[cfg(target_os = "windows")]
mod windows_only;
// `src/orphan.rs` is not declared as a module anywhere, so it is never compiled either.

verus! {

pub fn normal(x: u32) -> (r: u32)
    requires x < 100,
    ensures r == x + 1,
{
    x + 1
}

// vacuous
pub fn lib_contradictory(x: u32) -> (r: u32)
    requires x < 5 && x > 10,
    ensures r == x,
{
    x
}

} // verus!
