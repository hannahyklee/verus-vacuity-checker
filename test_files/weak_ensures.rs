use vstd::prelude::*;
verus! {
fn strong(x: u32) -> (r: u32) requires x < 100, ensures r == x + 1 { x + 1 }
fn weak(x: u32) -> (r: u32) requires x < 100, ensures r >= 0 { x + 1 }
fn weak_bool(x: u32) -> (r: bool) ensures r == true || r == false { x > 3 }
fn is_zero(x: u32) -> (r: bool) ensures r == (x == 0) { x == 0 }
fn zero() -> (r: u32) ensures r == 0 { 0 }
fn unit(v: &mut Vec<u32>) ensures final(v)@.len() >= 0 { v.push(1); }
fn unit_strong(v: &mut Vec<u32>) requires old(v)@.len() < 100, ensures final(v)@.len() == old(v)@.len() + 1 { v.push(1); }
fn pair(x: u32) -> (r: (u32, bool)) ensures r.0 == x { (x, true) }
struct S { a: u32 }
fn mk(x: u32) -> (r: S) ensures r.a == x { S { a: x } }
}
fn main() {}
