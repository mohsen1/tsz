//! Regression coverage for #14797: an interface method (or function-valued
//! property) that returns polymorphic `this`, called on a value typed as a
//! generic type parameter `T extends I`, must resolve to `this`/`T` — not to the
//! interface constraint `I`.
//!
//! Structural rule: when a member is resolved through a *type parameter*
//! receiver, the member's polymorphic `this` must stay `this` so the checker can
//! rebind it to the type parameter. The solver's bare-`TypeParameter` property
//! path already skips `this` binding for concrete-object constraints (the class
//! case), but an *interface* constraint is a `Lazy(DefId)` semantic ref, so the
//! first-pass noop-resolver lookup is degenerate and the checker's env-eval
//! retry previously re-resolved the member on the evaluated interface object
//! *with* `this` binding, collapsing `clone(): this` to `(): INode`. That drew a
//! false `TS2322` on `return n.clone();`. `tsc` keeps the result `T`.
//!
//! The fix is name-agnostic (it keys on the structural `Lazy` constraint, not on
//! identifiers), so every case below varies its binders.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn strict_options() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    }
}

fn ts2322(source: &str) -> usize {
    check_source(source, "test.ts", strict_options())
        .into_iter()
        .filter(|d| d.code == 2322)
        .count()
}

/// The exact repro from #14797: `n.clone(): this` for `n: T extends INode`.
#[test]
fn interface_this_return_through_generic_param_is_clean() {
    let source = r#"
interface INode { clone(): this; }
function cloneI<T extends INode>(n: T): T { return n.clone(); }
"#;
    assert_eq!(
        ts2322(source),
        0,
        "`n.clone()` for `n: T extends INode` must return `T`, not `INode`"
    );
}

/// Same rule, fully renamed binders — the fix is structural, not name-keyed.
#[test]
fn interface_this_return_renamed_binders() {
    let source = r#"
interface Shape { duplicate(): this; }
function copy<Element extends Shape>(item: Element): Element {
    return item.duplicate();
}
"#;
    assert_eq!(ts2322(source), 0, "renamed binders must behave identically");
}

/// Chained interface `this`-returning calls keep `this` bound to the param.
#[test]
fn interface_this_return_chained_calls() {
    let source = r#"
interface Cursor { next(): this; prev(): this; }
function step<C extends Cursor>(c: C): C {
    return c.next().prev();
}
"#;
    assert_eq!(ts2322(source), 0, "chained `this`-returns must stay `C`");
}

/// An interface function-valued *property* returning `this` behaves the same.
#[test]
fn interface_this_property_through_generic_param_is_clean() {
    let source = r#"
interface Linked { self: () => this; }
function identity<L extends Linked>(node: L): L {
    return node.self();
}
"#;
    assert_eq!(
        ts2322(source),
        0,
        "`node.self()` for a `() => this` property must return `L`"
    );
}

/// Class control (parity target): the class path already preserves `T`.
#[test]
fn class_this_return_through_generic_param_is_clean() {
    let source = r#"
class CNode { clone(): this { return this; } }
function cloneC<T extends CNode>(n: T): T { return n.clone(); }
"#;
    assert_eq!(
        ts2322(source),
        0,
        "class `this`-return must stay `T` (control)"
    );
}

/// Direct (non-generic) interface receiver: `clone()` on an `INode` value is
/// `INode`, and returning it as `INode` must stay clean.
#[test]
fn direct_interface_receiver_this_return_is_interface() {
    let source = r#"
interface INode { clone(): this; }
function dup(n: INode): INode { return n.clone(); }
"#;
    assert_eq!(
        ts2322(source),
        0,
        "direct `INode` receiver `clone()` is assignable to `INode`"
    );
}

/// Negative control: returning an unrelated interface value as `T` must still
/// error — the fix must not blanket-silence TS2322 on generic-param returns.
#[test]
fn unrelated_interface_value_returned_as_param_still_errors() {
    let source = r#"
interface INode { clone(): this; tag: string; }
function bad<T extends INode>(n: T, other: INode): T {
    return other;
}
"#;
    assert_eq!(
        ts2322(source),
        1,
        "a bare `INode` (not `this`/`T`) returned as `T` must still draw TS2322"
    );
}
