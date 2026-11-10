// Hand-written cases with known verdicts; see tests/driver.rs for the table.
// Every body is real and the file verifies unmutated.
use vstd::prelude::*;

verus! {

spec fn is_big(x: u32) -> bool { x > 100 }

// not vacuous: ordinary precondition
fn normal(x: u32) -> (r: u32)
    requires x < 100,
    ensures r == x + 1,
{
    x + 1
}

// vacuous: directly contradictory precondition
fn contradictory(x: u32) -> (r: u32)
    requires x < 5 && x > 10,
    ensures r == x + 1,
{
    x
}

// vacuous: the contradiction only appears after unfolding the spec fn
fn hidden_contradiction(x: u32) -> (r: u32)
    requires is_big(x) && x < 50,
    ensures r == x,
{
    x
}

// not vacuous: no precondition, only an ensures (the stub cannot verify)
fn ensures_only(x: u32) -> (r: u32)
    ensures r == x,
{
    x
}

// not a candidate: no spec at all
fn no_spec(x: u32) -> u32 {
    x
}

struct S { v: u32 }

impl S {
    // vacuous: contradictory precondition on a method
    fn method_contradictory(&self) -> (r: u32)
        requires self.v < 3 && self.v > 7,
        ensures r == self.v,
    {
        self.v
    }

    // not vacuous
    fn method_normal(&self) -> (r: u32)
        requires self.v < 3,
        ensures r == self.v,
    {
        self.v
    }
}

fn main() {}

} // verus!
