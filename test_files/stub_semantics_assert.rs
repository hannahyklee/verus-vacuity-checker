// Same as stub_semantics.rs, with the `assert_false` stub (for Verus releases without unimplemented!()).
// Verus requires `unimplemented!()` to be unreachable, so only a function
// with a contradictory `requires` verifies. See tests/stub_semantics.rs.
use vstd::prelude::*;

verus! {

fn good(x: u32) -> (r: u32)
    requires x < 100,
    ensures r == x + 1,
{ vstd::pervasive::unreached() }

fn weak(x: u32) -> (r: u32)
    ensures true,
{ vstd::pervasive::unreached() }

fn contradictory(x: u32) -> (r: u32)
    requires x < 5 && x > 10,
    ensures r == x + 1,
{ vstd::pervasive::unreached() }

fn requires_only(x: u32)
    requires x < 100,
{ vstd::pervasive::unreached() }

fn main() {}

} // verus!
