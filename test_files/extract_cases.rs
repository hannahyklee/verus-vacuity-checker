use vstd::prelude::*;

verus! {

spec fn double(x: int) -> int { x * 2 }

fn exec_with_spec(x: u32) -> (r: u32)
    requires x < 100,
    ensures r == x + 1,
{
    x + 1
}

fn exec_no_spec(x: u32) -> u32 {
    x
}

proof fn proof_with_spec(x: int)
    requires x > 0,
    ensures double(x) > x,
{
    assert(double(x) > x);
}

#[verifier::external_body]
fn external(x: u32) -> (r: u32)
    ensures r == x,
{
    x
}

struct S { v: u32 }

impl S {
    fn method(&self) -> (r: u32)
        ensures r == self.v,
    {
        self.v
    }
}

trait T {
    fn sig_only(&self) -> (r: u32)
        ensures r < 10;

    fn with_default(&self) -> (r: u32)
        ensures r == 0,
    {
        0
    }
}

mod inner {
    use super::*;

    pub fn nested() -> (r: u32)
        ensures r == 7,
    {
        let x = if true { 7 } else { 8 };
        x
    }
}

fn main() {}

} // verus!
