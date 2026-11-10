use vstd::prelude::*;

verus! {

// vacuous, in a submodule
pub fn helper_contradictory(x: u32) -> (r: u32)
    requires x > 7 && x < 3,
    ensures r == x + 2,
{
    x
}

// not vacuous
pub fn helper_normal(x: u32) -> (r: u32)
    requires x < 10,
    ensures r == x,
{
    x
}

} // verus!
