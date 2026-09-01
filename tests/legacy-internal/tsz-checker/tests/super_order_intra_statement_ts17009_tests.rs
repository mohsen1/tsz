//! TS17009 respects evaluation order *within* a statement.
//!
//! Structural rule (`tsc` `checkThisBeforeSuper` / `isPostSuperFlowNode`):
//! whether `this` in a derived-class constructor is "before" `super()` is
//! decided by the binder's control-flow graph, not by statement position. A
//! `super()` call that definitely executes earlier in the same statement —
//! an earlier object-literal property, array element, call argument, comma
//! operand, template span, or an earlier declarator of the same `let` — makes
//! a later `this` access legal. Conversely, `super(this)` stays an error
//! because arguments evaluate before the call fires, unless an earlier
//! `super()` call already ran.
//!
//! Witness: `conformance/es6/classDeclaration/superCallBeforeThisAccessing8.ts`.
//! Every case below is oracle-confirmed against pinned `typescript@7.0.2`.

use crate::test_utils::check_source_diagnostics;

fn ts17009_count(source: &str) -> usize {
    check_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == 17009)
        .count()
}

fn assert_clean(source: &str) {
    let diags = check_source_diagnostics(source);
    let ts17009: Vec<_> = diags.iter().filter(|d| d.code == 17009).collect();
    assert!(
        ts17009.is_empty(),
        "expected no TS17009, got: {ts17009:?}\nsource:\n{source}"
    );
}

// ── Witness: the conformance fixture's shape ───────────────────────────────

#[test]
fn super_in_earlier_object_property_precedes_this_in_later_property() {
    assert_clean(
        r#"
class Base { constructor(c: unknown) { } }
class D extends Base {
    private _t: number;
    constructor() {
        let x = { k: super(undefined), j: this._t };
    }
}
"#,
    );
}

/// Renamed binders: the rule is structural, not identifier-specific.
#[test]
fn super_in_earlier_object_property_renamed_binders() {
    assert_clean(
        r#"
class Root { constructor(seed: unknown) { } }
class Widget extends Root {
    private payload: string;
    constructor() {
        const bag = { first: super(null), second: this.payload };
    }
}
"#,
    );
}

/// Negative: reversed property order keeps the error.
#[test]
fn this_in_earlier_object_property_than_super_still_errors() {
    let count = ts17009_count(
        r#"
class Base { constructor(c: unknown) { } }
class D extends Base {
    private _t: number;
    constructor() {
        let x = { j: this._t, k: super(undefined) };
    }
}
"#,
    );
    assert_eq!(
        count, 1,
        "expected exactly one TS17009 for this-first order"
    );
}

// ── Array literals, call arguments, comma, template ───────────────────────

#[test]
fn super_in_earlier_array_element_precedes_this() {
    assert_clean(
        r#"
class Base { constructor(c: unknown) { } }
class D extends Base {
    private _t: number;
    constructor() {
        let x = [super(undefined), this._t];
    }
}
"#,
    );
}

#[test]
fn this_in_earlier_array_element_than_super_still_errors() {
    let count = ts17009_count(
        r#"
class Base { constructor(c: unknown) { } }
class D extends Base {
    private _t: number;
    constructor() {
        let x = [this._t, super(undefined)];
    }
}
"#,
    );
    assert_eq!(
        count, 1,
        "expected exactly one TS17009 for this-first array"
    );
}

#[test]
fn super_in_earlier_call_argument_precedes_this() {
    assert_clean(
        r#"
declare function g(a: unknown, b: unknown): void;
class Base { constructor(c: unknown) { } }
class D extends Base {
    private _t: number;
    constructor() {
        g(super(undefined), this._t);
    }
}
"#,
    );
}

#[test]
fn super_in_comma_left_precedes_this_in_right() {
    assert_clean(
        r#"
class Base { constructor(c: unknown) { } }
class D extends Base {
    private _t: number;
    constructor() {
        let x = (super(undefined), this._t);
    }
}
"#,
    );
}

#[test]
fn super_in_earlier_template_span_precedes_this() {
    assert_clean(
        r#"
class Base { constructor(c: unknown) { } }
class D extends Base {
    private _t: number;
    constructor() {
        let x = `${super(undefined)} ${this._t}`;
    }
}
"#,
    );
}

#[test]
fn super_in_logical_left_precedes_this_in_right() {
    assert_clean(
        r#"
class Base { constructor(c: unknown) { } }
class D extends Base {
    private _t: number;
    constructor() {
        let x = super(undefined) && this._t;
    }
}
"#,
    );
}

/// Negative: a `super()` call behind `&&` is conditional and does not count
/// for a later statement.
#[test]
fn conditional_super_behind_logical_and_still_errors() {
    let count = ts17009_count(
        r#"
declare const c: boolean;
class Base { constructor(c: unknown) { } }
class D extends Base {
    private _t: number;
    constructor() {
        let x = c && super(undefined);
        let y = this._t;
    }
}
"#,
    );
    assert_eq!(count, 1, "conditional super must not legitimize this");
}

// ── super(this) and double-super ───────────────────────────────────────────

/// Arguments evaluate before the call fires: `super(this)` is an error.
#[test]
fn this_as_super_argument_still_errors() {
    let count = ts17009_count(
        r#"
class Base { constructor(c: unknown) { } }
class D extends Base {
    constructor() {
        super(this);
    }
}
"#,
    );
    assert_eq!(count, 1, "super(this) must keep its TS17009");
}

/// A prior completed `super()` call legitimizes `this` even inside a second
/// super call's arguments.
#[test]
fn this_in_second_super_call_arguments_after_first_super_is_clean() {
    assert_clean(
        r#"
class Base { constructor(c: unknown) { } }
class D extends Base {
    constructor() {
        super(1);
        super(this);
    }
}
"#,
    );
}

// ── Variable declarators ───────────────────────────────────────────────────

#[test]
fn super_in_earlier_declarator_precedes_this_in_later_declarator() {
    assert_clean(
        r#"
class Base { constructor(c: unknown) { } }
class D extends Base {
    private _t: number;
    constructor() {
        let a = super(undefined), b = this._t;
    }
}
"#,
    );
}

#[test]
fn this_in_earlier_declarator_than_super_still_errors() {
    let count = ts17009_count(
        r#"
class Base { constructor(c: unknown) { } }
class D extends Base {
    private _t: number;
    constructor() {
        let b = this._t, a = super(undefined);
    }
}
"#,
    );
    assert_eq!(count, 1, "expected one TS17009 for this-first declarator");
}

// ── Conditional expression branches ────────────────────────────────────────

/// Each branch runs after the condition; a branch-local `super()` precedes a
/// branch-local `this` in comma sequence.
#[test]
fn super_then_this_within_each_conditional_branch_is_clean() {
    assert_clean(
        r#"
declare const c: boolean;
class Base { constructor(c: unknown) { } }
class D extends Base {
    private _t: number;
    constructor() {
        let x = c ? (super(undefined), this._t) : (super(0), this._t);
    }
}
"#,
    );
}

// ── Statement forms: return/throw, loops ───────────────────────────────────

#[test]
fn super_before_this_inside_return_expression_is_clean() {
    assert_clean(
        r#"
class Base { constructor(c: unknown) { } }
class D extends Base {
    private _t: number;
    constructor() {
        return void (super(undefined), this._t);
    }
}
"#,
    );
}

#[test]
fn super_before_this_inside_throw_expression_is_clean() {
    assert_clean(
        r#"
class Base { constructor(c: unknown) { } }
class D extends Base {
    private _t: number;
    constructor() {
        throw [super(undefined), this._t];
    }
}
"#,
    );
}

/// First-iteration flow: a `super()` statement earlier in the loop body
/// precedes a later `this` in the same body.
#[test]
fn super_statement_before_this_in_while_body_is_clean() {
    assert_clean(
        r#"
declare const c: boolean;
class Base { constructor(c: unknown) { } }
class D extends Base {
    private _t: number;
    constructor() {
        while (c) {
            super(undefined);
            let y = this._t;
        }
    }
}
"#,
    );
}

/// A do-while body executes at least once, so a `super()` inside it counts
/// for statements after the loop.
#[test]
fn super_in_do_while_body_precedes_this_after_loop() {
    assert_clean(
        r#"
declare const c: boolean;
class Base { constructor(c: unknown) { } }
class D extends Base {
    private _t: number;
    constructor() {
        do { super(undefined); } while (c);
        let y = this._t;
    }
}
"#,
    );
}

/// A for-of iterated expression evaluates exactly once, before the body.
#[test]
fn super_in_for_of_iterated_expression_precedes_this_in_body() {
    assert_clean(
        r#"
class Base { constructor(c: unknown) { } }
class D extends Base {
    private _t: number;
    constructor() {
        for (const v of [super(undefined), 1]) { let y = this._t; }
    }
}
"#,
    );
}

/// Negative: a while body may never run; `super()` inside it does not count
/// for statements after the loop.
#[test]
fn super_only_in_while_body_does_not_reach_after_loop() {
    let count = ts17009_count(
        r#"
declare const c: boolean;
class Base { constructor(c: unknown) { } }
class D extends Base {
    private _t: number;
    constructor() {
        while (c) { super(undefined); }
        let y = this._t;
    }
}
"#,
    );
    assert_eq!(count, 1, "while-body super must not count after the loop");
}

/// Nested containers: `super()` deep in one subtree precedes `this` in a later
/// sibling subtree.
#[test]
fn super_in_nested_earlier_subtree_precedes_this_in_later_subtree() {
    assert_clean(
        r#"
class Base { constructor(c: unknown) { } }
class D extends Base {
    private _t: number;
    constructor() {
        let x = { a: { b: super(undefined) }, c: [this._t] };
    }
}
"#,
    );
}
