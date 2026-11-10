// Multi-file crate with known verdicts (see checker/tests/crate.rs).
use vstd::prelude::*;

mod helper;

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
