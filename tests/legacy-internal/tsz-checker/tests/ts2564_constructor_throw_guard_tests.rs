//! Definite-assignment (TS2564, `strictPropertyInitialization`) parity for
//! constructors that `throw`.
//!
//! Structural rule: a property is definitely assigned at the end of a
//! constructor iff it is assigned on every path that *reaches the end* (the
//! normal fall-through plus any early `return`). A `throw` path never reaches
//! the end, so it contributes no completion path: an early-throw guard
//! (`if (bad) throw;`) must NOT cancel an assignment made on the normal path
//! that follows it, and a constructor whose every path throws satisfies
//! definite assignment vacuously for all properties.
//!
//! tsz previously modelled `throw` identically to `return` (both folded their
//! pre-exit assigned-set into `exits`, which was intersected with the normal
//! path), so `if (bad) throw; this.x = v;` produced a false TS2564, and
//! always-throwing constructors with no/conditional assignment also did. The
//! fix lives in `flow/flow_analysis/core.rs`: `throw` yields `(None, None)`,
//! and a `(None, None)` reduction (no completion path at all) is vacuously
//! all-assigned.

use crate::test_utils::check_source_strict_codes as check_strict;

const TS2564: u32 = 2564;

fn count_2564(source: &str) -> usize {
    check_strict(source)
        .iter()
        .filter(|&&c| c == TS2564)
        .count()
}

#[test]
fn early_throw_guard_does_not_cancel_later_normal_assignment() {
    // The witnessing pattern (ts-morph Node.ts): a guard clause throws, then
    // the property is assigned on the normal path. tsc is clean.
    assert_eq!(
        count_2564(
            r#"
class Widget {
  readonly size: number;
  constructor(n: number) {
    if (n < 0) { throw new Error("bad"); }
    this.size = n;
  }
}
"#,
        ),
        0,
        "guard-throw followed by normal-path assignment must not report TS2564",
    );
}

#[test]
fn early_throw_guard_without_block_braces() {
    // Same rule, brace-less `if (cond) throw;` form; differently-named binder.
    assert_eq!(
        count_2564(
            r#"
class Gadget {
  readonly weight: number;
  constructor(w: number) {
    if (w < 0) throw new Error("bad");
    this.weight = w;
  }
}
"#,
        ),
        0,
    );
}

#[test]
fn constructor_that_always_throws_is_vacuously_assigned() {
    // Every path throws -> the end is unreachable -> all properties vacuously
    // definitely-assigned. Covers both no-assignment and conditional-assignment.
    assert_eq!(
        count_2564(
            r#"
class NeverBuilt {
  readonly handle: number;
  constructor() { throw new Error("unsupported"); }
}
class AlsoNeverBuilt {
  readonly token: number;
  constructor(p: boolean) {
    if (p) { this.token = 1; }
    throw new Error("unsupported");
  }
}
"#,
        ),
        0,
        "an always-throwing constructor must not report TS2564 (vacuous)",
    );
}

#[test]
fn throw_in_else_branch_keeps_then_assignment_definite() {
    // The only completing path assigns the property; the other branch throws.
    assert_eq!(
        count_2564(
            r#"
class Conn {
  readonly port: number;
  constructor(ok: boolean, n: number) {
    if (ok) { this.port = n; } else { throw new Error("closed"); }
  }
}
"#,
        ),
        0,
    );
}

#[test]
fn derived_class_super_then_throw_guard_then_assign() {
    assert_eq!(
        count_2564(
            r#"
class Animal { constructor(_n: number) {} }
class Dog extends Animal {
  readonly age: number;
  constructor(v: number) {
    super(v);
    if (v < 0) throw new Error("bad");
    this.age = v;
  }
}
"#,
        ),
        0,
    );
}

#[test]
fn early_return_guard_still_reports_when_normal_path_unassigned() {
    // `return` IS a completion path (unlike `throw`): the early-return path
    // reaches the end without assigning, so the property is not definitely
    // assigned. tsc reports TS2564 here; the fix must preserve that.
    assert_eq!(
        count_2564(
            r#"
class Maybe {
  readonly value: number;
  constructor(n: number) {
    if (n < 0) { return; }
    this.value = n;
  }
}
"#,
        ),
        1,
        "early-return guard leaves the property unassigned on a completing path",
    );
}

#[test]
fn assignment_only_before_throw_still_reports_on_normal_path() {
    // The property is assigned only inside the throwing branch; the normal
    // (fall-through) completion never assigns it, so TS2564 still fires.
    assert_eq!(
        count_2564(
            r#"
class Partial {
  readonly slot: number;
  constructor(n: number) {
    if (n < 0) { this.slot = n; throw new Error("bad"); }
  }
}
"#,
        ),
        1,
    );
}

#[test]
fn never_assigned_property_still_reports() {
    // Baseline: no throw involved, property never assigned -> TS2564.
    assert_eq!(
        count_2564(
            r#"
class Empty {
  readonly field: number;
  constructor(n: number) {
    const unused = n;
  }
}
"#,
        ),
        1,
    );
}
