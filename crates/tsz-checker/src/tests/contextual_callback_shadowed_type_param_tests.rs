//! Regression tests for issue #15731 — a generic call's contextual callback
//! parameter must not have an *enclosing* signature's same-named type
//! parameter defaulted away.
//!
//! Structural rule: when round-1 inference fixes a callee type parameter to a
//! candidate that still mentions a type parameter of an enclosing signature
//! sharing the same name, `tsc` treats the candidate as fixed — the free
//! parameter belongs to the outer signature. Substitutions in `tsz` are
//! name-keyed, so defaulting the callee's parameter to `unknown` would rewrite
//! the enclosing occurrence too, contextually typing the callback parameter as
//! `Box<unknown>` instead of `Box<T>` and producing a tsz-only TS2322/TS2345.
//!
//! The witness is superjson's `plainer.ts` (`traverse` recursing through
//! `forEach<T>`), but the trigger is purely the name collision: the same code
//! with the callee's parameter renamed was always clean. Every case below is
//! tsc-clean.
use crate::test_utils::check_source_diagnostics;

/// Assert no assignability diagnostics, reporting the offending ones.
fn assert_no_assignability_diagnostics(source: &str, context: &str) {
    let diags = check_source_diagnostics(source);
    let unexpected: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2345)
        .collect();
    assert!(
        unexpected.is_empty(),
        "{context}: expected no TS2322/TS2345, got: {:?}",
        unexpected
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Core repro: callee and caller both spell their type parameter `T`, and the
/// callback parameter must contextually type as `Box<T>`, not `Box<unknown>`.
#[test]
fn colliding_type_param_name_keeps_outer_parameter_in_callback_context() {
    assert_no_assignability_diagnostics(
        r#"
type Box<T> = { item: T };
declare function apply<T>(v: T, run: (v: T) => void): void;

function outer<T>(v: Box<T>) {
    apply(v, (sub) => {
        const probe: Box<T> = sub;
    });
}
"#,
        "colliding type parameter names",
    );
}

/// Renamed binder control. The identical program with the enclosing parameter
/// spelled `Q` was already clean; it must stay clean, proving the fix is about
/// binder scope and not about the spelling `T`.
#[test]
fn renamed_outer_binder_keeps_callback_context() {
    assert_no_assignability_diagnostics(
        r#"
type Box<T> = { item: T };
declare function apply<T>(v: T, run: (v: T) => void): void;

function outer<Q>(v: Box<Q>) {
    apply(v, (sub) => {
        const probe: Box<Q> = sub;
    });
}
"#,
        "renamed outer binder",
    );
}

/// Renamed *callee* binder control — the other half of the matrix.
#[test]
fn renamed_callee_binder_keeps_callback_context() {
    assert_no_assignability_diagnostics(
        r#"
type Box<T> = { item: T };
declare function apply<U>(v: U, run: (v: U) => void): void;

function outer<T>(v: Box<T>) {
    apply(v, (sub) => {
        const probe: Box<T> = sub;
    });
}
"#,
        "renamed callee binder",
    );
}

/// The enclosing parameter carries a constraint while the callee's does not.
/// The candidate is still fixed, so the constraint must not be substituted in
/// place of the outer parameter either.
#[test]
fn constrained_outer_binder_keeps_callback_context() {
    assert_no_assignability_diagnostics(
        r#"
type Box<T> = { item: T };
declare function apply<T>(v: T, run: (v: T) => void): void;

function outer<T extends object>(v: Box<T>) {
    apply(v, (sub) => {
        const probe: Box<T> = sub;
    });
}
"#,
        "constrained outer binder",
    );
}

/// Wrapper/nesting form: the candidate reaches the callback through an index
/// signature rather than directly.
#[test]
fn colliding_type_param_name_through_record_value_context() {
    assert_no_assignability_diagnostics(
        r#"
type Box<T> = { item: T };
type Dict<V> = { [key: string]: V };
declare function forEach<T>(record: Dict<T>, run: (v: T, key: string) => void): void;

function outer<T>(rec: Dict<Box<T>>) {
    forEach(rec, (sub) => {
        const probe: Box<T> = sub;
    });
}
"#,
        "record value context",
    );
}

/// The superjson `plainer.ts` shape: a recursive generic alias reached through
/// an index-signature value, recursing into the same generic function. This is the
/// original canary witness reduced to its inference core.
#[test]
fn recursive_alias_through_record_recurses_without_losing_outer_param() {
    assert_no_assignability_diagnostics(
        r#"
type Dict<V> = { [key: string]: V };
type Tree<T> = InnerNode<T> | Leaf<T>;
type Leaf<T> = [T];
type InnerNode<T> = [T, Dict<Tree<T>>];
type MinimisedTree<T> = Tree<T> | Dict<Tree<T>> | undefined;

declare function forEach<T>(record: Dict<T>, run: (v: T, key: string) => void): void;
declare function isArray(payload: any): payload is any[];

function traverse<T>(tree: MinimisedTree<T>, walker: (v: T, path: string[]) => void): void {
    if (!tree) {
        return;
    }
    if (!isArray(tree)) {
        forEach(tree, (subtree) => traverse(subtree, walker));
        return;
    }
    const [, children] = tree;
    if (children) {
        forEach(children, (child) => {
            traverse(child, walker);
        });
    }
}
"#,
        "recursive alias through record",
    );
}

/// Concrete (non-generic) caller: the candidate has no free type parameter at
/// all, so the ordinary fixing path still applies.
#[test]
fn concrete_outer_argument_still_fixes_callback_context() {
    assert_no_assignability_diagnostics(
        r#"
type Box<T> = { item: T };
declare function apply<T>(v: T, run: (v: T) => void): void;

function outer(v: Box<string>) {
    apply(v, (sub) => {
        const probe: Box<string> = sub;
    });
}
"#,
        "concrete outer argument",
    );
}

/// Negative/fallback control: a callee type parameter with *no* inference
/// candidate must still be defaulted, so the callback parameter reads as
/// `unknown` rather than staying generic. Guards the `new Promise((res) => ...)`
/// behavior that the defaulting path exists for.
#[test]
fn uninferrable_type_param_still_defaults_callback_parameter_to_unknown() {
    let diags = check_source_diagnostics(
        r#"
declare function run<T>(cb: (v: T) => void): void;

run((v) => {
    const probe: string = v;
});
"#,
    );

    assert!(
        diags.iter().any(|d| d.code == 2322),
        "expected TS2322 assigning the defaulted `unknown` callback parameter to `string`, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}
