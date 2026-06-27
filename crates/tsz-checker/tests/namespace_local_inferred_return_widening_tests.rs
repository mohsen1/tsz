//! Regression tests for issue #14773.
//!
//! tsc widens an unannotated function's inferred return literal
//! (`getReturnTypeFromBody` → `getWidenedType`) regardless of where the
//! function is declared. A non-exported function declared inside a `namespace`
//! that is reached only through a sibling call (`return privFn()`) is widened
//! exactly like a top-level or exported function.
//!
//! tsz previously left such a callee's signature un-widened (`(): 1` instead of
//! `(): number`) because the callee's `infer_return_type_from_body` was first
//! triggered *during* the caller's return-expression typing, which sets the
//! ambient `preserve_literal_types` flag — and that leaked flag suppressed the
//! callee's own literal widening. The flag is an outer-expression concern; a
//! named declaration's signature is context-independent, so its widening must
//! not inherit it. Binder names are varied so the rule is structural, not
//! name-keyed.

use tsz_checker::test_utils::check_source_code_messages as diagnostics;

fn ts2322(source: &str) -> Vec<String> {
    diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, msg)| msg)
        .collect()
}

/// The reported witness: a non-exported namespace sibling returning a `const`
/// literal, reached only through `pubFn`, must infer `number`.
#[test]
fn non_exported_namespace_sibling_const_return_widens() {
    let source = r#"
namespace M {
  const priv = 1;
  function privFn() { return priv; }
  export function pubFn() { return privFn(); }
}
const ok: number = M.pubFn();
"#;
    assert!(
        ts2322(source).is_empty(),
        "non-exported namespace sibling chain must widen `1` to `number`, got: {:?}",
        ts2322(source)
    );
}

/// And the inferred type really is the widened `number` — assigning to the
/// narrower literal `0` must report `number`, exactly as tsc does (it previously
/// reported `1`).
#[test]
fn non_exported_namespace_sibling_reports_widened_source() {
    let source = r#"
namespace Box {
  const seed = 1;
  function make() { return seed; }
  export function take() { return make(); }
}
const bad: 0 = Box.take();
"#;
    let msgs = ts2322(source);
    assert_eq!(msgs.len(), 1, "expected exactly one TS2322, got: {msgs:?}");
    assert!(
        msgs[0].contains("Type 'number'"),
        "inferred return must widen to `number`, got: {}",
        msgs[0]
    );
}

/// A fresh literal return (`return 1`) through the same non-exported sibling
/// chain widens too.
#[test]
fn non_exported_namespace_sibling_fresh_literal_widens() {
    let source = r#"
namespace Ns {
  function helper() { return 7; }
  export function entry() { return helper(); }
}
const ok: number = Ns.entry();
"#;
    assert!(
        ts2322(source).is_empty(),
        "non-exported namespace sibling returning a fresh literal must widen, got: {:?}",
        ts2322(source)
    );
}

/// String and boolean literals follow the same rule.
#[test]
fn non_exported_namespace_sibling_string_and_boolean_widen() {
    let string_src = r#"
namespace S {
  function inner() { return "tag"; }
  export function outer() { return inner(); }
}
const ok: string = S.outer();
"#;
    assert!(
        ts2322(string_src).is_empty(),
        "string-literal sibling chain must widen to `string`, got: {:?}",
        ts2322(string_src)
    );

    let bool_src = r#"
namespace B {
  function inner() { return true; }
  export function outer() { return inner(); }
}
const ok: boolean = B.outer();
"#;
    assert!(
        ts2322(bool_src).is_empty(),
        "boolean-literal sibling chain must widen to `boolean`, got: {:?}",
        ts2322(bool_src)
    );
}

/// Multi-hop non-exported chains and nested namespaces widen as well.
#[test]
fn nested_namespace_multi_hop_widens() {
    let source = r#"
namespace Outer {
  namespace Inner {
    function deep() { return 5; }
    export function mid() { return deep(); }
  }
  export function top() { return Inner.mid(); }
}
const ok: number = Outer.top();
"#;
    assert!(
        ts2322(source).is_empty(),
        "nested namespace multi-hop chain must widen, got: {:?}",
        ts2322(source)
    );
}

/// Over-widening guard: an explicit `as const` return through the same chain
/// must still be preserved (assignable to the narrow literal, not widened).
#[test]
fn const_assertion_through_namespace_sibling_is_preserved() {
    let source = r#"
namespace M {
  function privFn() { return 1 as const; }
  export function pubFn() { return privFn(); }
}
const pinned: 1 = M.pubFn();
"#;
    assert!(
        ts2322(source).is_empty(),
        "`as const` literal must be preserved through the sibling chain, got: {:?}",
        ts2322(source)
    );
}

/// Over-widening guard: an explicit literal return-type annotation on the
/// non-exported sibling is preserved.
#[test]
fn explicit_literal_annotation_through_namespace_sibling_is_preserved() {
    let source = r#"
namespace M {
  function privFn(): 1 { return 1; }
  export function pubFn() { return privFn(); }
}
const pinned: 1 = M.pubFn();
"#;
    assert!(
        ts2322(source).is_empty(),
        "explicit literal annotation must be preserved through the sibling chain, got: {:?}",
        ts2322(source)
    );
}

/// Over-widening guard: two distinct fresh literals through the chain stay a
/// literal union (the #14530 invariant must survive the widening-context reset).
#[test]
fn distinct_literal_union_through_namespace_sibling_is_preserved() {
    let source = r#"
namespace M {
  function classify(n: number) { if (n > 0) return "a"; return "b"; }
  export function pub(n: number) { return classify(n); }
}
const u: "a" | "b" = M.pub(0);
"#;
    assert!(
        ts2322(source).is_empty(),
        "distinct-literal union must be preserved through the sibling chain, got: {:?}",
        ts2322(source)
    );
}
