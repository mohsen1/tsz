//! Tests for contextual-type propagation through `expr!` to type-argument
//! inference of overloaded generic methods.
//!
//! Regression: tsz emitted a false TS2740 for
//!     let r: `SVGRectElement` = document.querySelector('.svg-rectangle')!;
//! because the contextual-return-type substitution computed for the matched
//! overload (`querySelector<E extends Element = Element>(s: string): E | null`)
//! was not applied to the final return type when other overloads in the same
//! group failed to bind their type parameter from the argument. The re-resolve
//! step in `overload_resolution.rs` would default `E` to `Element` and discard
//! the inferred binding `E = SVGRectElement` already computed via the
//! contextual return type.

#[test]
fn nonnull_assertion_propagates_contextual_type_through_overloaded_generic_call() {
    let source = r#"
interface MySVGMap { "rect": SVGRectElement; }
interface Foo {
  qs<K extends keyof MySVGMap>(s: K): MySVGMap[K] | null;
  qs<E extends Element = Element>(s: string): E | null;
}
declare const f: Foo;
let r: SVGRectElement = f.qs('.bad-name')!;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2740: Vec<_> = diagnostics.iter().filter(|d| d.code == 2740).collect();
    assert!(
        ts2740.is_empty(),
        "expected no TS2740 — contextual `SVGRectElement` should infer E for the generic overload, got: {:?}",
        diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn nonnull_assertion_keys_off_structural_overload_shape_not_identifier_names() {
    // The fix is shape-based on the matched overload's signature; it should
    // work whichever names the user picks for the type parameter, the
    // method, or the tag-name map.
    for (tparam, method, mapname) in [
        ("E", "qs", "MySVGMap"),
        ("T", "find", "Names"),
        ("Result", "select", "_M"),
    ] {
        let source = format!(
            r#"
interface SVGRectElement extends Element {{ requiredExtensions: any; }}
interface {mapname} {{ "rect": SVGRectElement; }}
interface Foo {{
  {method}<K extends keyof {mapname}>(s: K): {mapname}[K] | null;
  {method}<{tparam} extends Element = Element>(s: string): {tparam} | null;
}}
declare const f: Foo;
let r: SVGRectElement = f.{method}('.bad-name')!;
"#
        );
        let diagnostics = tsz_checker::test_utils::check_source_diagnostics(&source);
        let ts2740: Vec<_> = diagnostics.iter().filter(|d| d.code == 2740).collect();
        assert!(
            ts2740.is_empty(),
            "names {tparam}/{method}/{mapname}: expected no TS2740, got: {:?}",
            diagnostics
                .iter()
                .map(|d| format!("TS{}: {}", d.code, d.message_text))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn nonnull_assertion_does_not_invent_type_when_no_contextual() {
    // Without a contextual return type, the generic overload's E defaults
    // to its bound (Element). This must not regress.
    let source = r#"
interface MySVGMap { "rect": SVGRectElement; }
interface Foo {
  qs<K extends keyof MySVGMap>(s: K): MySVGMap[K] | null;
  qs<E extends Element = Element>(s: string): E | null;
}
declare const f: Foo;
let r: Element = f.qs('.bad-name')!;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2740: Vec<_> = diagnostics.iter().filter(|d| d.code == 2740).collect();
    assert!(
        ts2740.is_empty(),
        "Element-targeted assignment should still type-check, got: {:?}",
        diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}
