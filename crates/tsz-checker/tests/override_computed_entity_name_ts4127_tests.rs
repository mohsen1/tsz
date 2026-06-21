//! Regression tests for TS4127 on `override` members with computed names that
//! are entity-name property-access chains.
//!
//! tsc's `isLateBindableName` treats a computed name whose expression is an
//! entity-name expression (`a.b.c`) as a *static* (non-dynamic) name when the
//! expression's type is usable as a property name — a `unique symbol` or a
//! string/number literal. Such a member may carry `override`. tsz previously
//! recognized only bare-identifier and `Symbol.<name>` forms, so a deeper chain
//! such as `globalThis.Symbol.hasInstance` (seen in the wild in `runtypes`) was
//! treated as dynamic, producing a spurious TS4127.
//!
//! The structural rule under test: a computed name `[a.b.c]` is non-dynamic iff
//! `a.b.c` is an entity-name expression whose type is usable as a property name,
//! independent of the chain depth or the binder names involved. A widened type
//! (plain `string`) on the same entity-name shape stays dynamic.
//!
//! Note: a literal-typed property access is used as the positive witness here
//! because it is lib-free and the test harness preserves literal property types
//! through member access. The `unique symbol` analogue (`globalThis.Symbol.*`)
//! is verified at the full-pipeline/CLI level; the unit harness's lib does not
//! model `globalThis.Symbol.hasInstance` as a `unique symbol`, and a separate
//! gap widens `unique symbol` read off an object property to `symbol`.

use tsz_checker::test_utils::check_source_code_messages as compile_and_get_diagnostics;

fn ts4127s(source: &str) -> Vec<(u32, String)> {
    compile_and_get_diagnostics(source)
        .into_iter()
        .filter(|(c, _)| *c == 4127)
        .collect()
}

/// A literal-typed entity-name property access (`Lit.k` where `Lit.k: "fixed"`)
/// is late-bindable, so the override must not draw TS4127. This is the case the
/// fix flips: before, the property-access expression fell through to the dynamic
/// catch-all; after, the entity-name + usable-type check accepts it.
#[test]
fn override_literal_typed_entity_name_is_not_dynamic() {
    let ts4127 = ts4127s(
        r#"
declare const Lit: { readonly k: "fixed" };
class Base { [Lit.k](): void {} }
class Sub extends Base { override [Lit.k](): void {} }
"#,
    );
    assert!(
        ts4127.is_empty(),
        "entity-name literal-typed computed override must not be dynamic; got TS4127: {ts4127:#?}"
    );
}

/// Proves the rule is not binder-name-specific: a differently named chain with a
/// numeric-literal-typed leaf is likewise late-bindable.
#[test]
fn override_numeric_literal_entity_name_is_not_dynamic() {
    let ts4127 = ts4127s(
        r#"
declare const Cfg: { readonly idx: 7 };
class Base { [Cfg.idx](): void {} }
class Sub extends Base { override [Cfg.idx](): void {} }
"#,
    );
    assert!(
        ts4127.is_empty(),
        "entity-name numeric-literal computed override must not be dynamic; got TS4127: {ts4127:#?}"
    );
}

/// Negative control: the same entity-name shape with a *widened* `string` type
/// is genuinely dynamic, so `override` must still draw TS4127 — matching tsc.
/// Guards the fix against over-broadening to all entity-name accesses.
#[test]
fn override_widened_string_entity_name_is_dynamic() {
    let ts4127 = ts4127s(
        r#"
declare const W: { readonly n: string };
class Base { [W.n](): void {} }
class Sub extends Base { override [W.n](): void {} }
"#,
    );
    assert!(
        !ts4127.is_empty(),
        "widened-string computed override must stay dynamic (TS4127); got none"
    );
}
