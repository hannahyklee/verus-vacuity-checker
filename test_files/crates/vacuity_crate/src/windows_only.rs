use vstd::prelude::*;

verus! {

// Would look vacuous if the file were compiled; it is not.
pub fn windows_contradictory(x: u32) -> (r: u32)
    requires x > 7 && x < 3,
    ensures r == x,
{
    x
}

} // verus!
