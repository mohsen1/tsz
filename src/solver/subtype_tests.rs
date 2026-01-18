```rust
//! Unit tests for subtyping.

use crate::ty::*;
use crate::solver::subtype::SubtypeCx;

fn mk_tcx() -> TyCtxt<'static> {
    TyCtxt::new()
}

#[test]
fn test_simple() {
    let tcx = mk_tcx();
    let cx = SubtypeCx::new(&tcx);

    let t_int = Ty::int();
    let t_bot = Ty::bot();

    assert!(cx.is_subtype(t_bot, t_int));
    assert!(!cx.is_subtype(t_int, t_bot));
    assert!(cx.is_subtype(t_int, t_int));
}

#[test]
fn test_arrow() {
    let tcx = mk_tcx();
    let cx = SubtypeCx::new(&tcx);

    let t_int = Ty::int();
    let t_bot = Ty::bot();
    let t_top = Ty::top();

    // (Bot -> Int) <: (Top -> Int)
    // Input: Top <: Bot (True, Top is supertype of Bot? Wait. LHS input is Bot. RHS input is Top.
    // Input check: RHS input <= LHS input -> Top <= Bot -> False?
    // Wait, contravariance:
    // f1 <: f2 iff f2.input
