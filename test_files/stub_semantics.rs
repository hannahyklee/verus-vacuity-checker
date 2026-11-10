// Each body is already `unimplemented!()`, as the mutator would produce.
// Verus requires `unimplemented!()` to be unreachable, so only a function
// with a contradictory `requires` verifies. See tests/stub_semantics.rs.
use vstd::prelude::*;

verus! {

fn good(x: u32) -> (r: u32)
    requires x < 100,
    ensures r == x + 1,
{ unimplemented!() }

fn weak(x: u32) -> (r: u32)
    ensures true,
{ unimplemented!() }

fn contradictory(x: u32) -> (r: u32)
    requires x < 5 && x > 10,
    ensures r == x + 1,
{ unimplemented!() }

fn requires_only(x: u32)
    requires x < 100,
{ unimplemented!() }

fn main() {}

} // verus!
