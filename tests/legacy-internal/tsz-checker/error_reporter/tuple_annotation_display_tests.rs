//! Regression tests for canonical display of structural tuple / function type
//! annotations in assignability diagnostics.
//!
//! tsc always renders a written tuple or function/constructor type annotation
//! through `typeToString` (`[number, string]`, `(a: number) => void`) rather
//! than preserving the author's source spelling. tsz's declared-annotation
//! source-text fallback previously leaked the written form — including
//! non-canonical whitespace such as `[number,string]` or `[number,   string]`.
//! The fix routes these structural annotations back through the canonical
//! structural formatter (see
//! `annotation_is_canonicalized_structural_type`), while type-reference /
//! alias annotations keep their as-written name.

use crate::test_utils::check_source_diagnostics;

fn ts2322_message(source: &str) -> String {
    let diagnostics = check_source_diagnostics(source);
    diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| {
            panic!(
                "expected a TS2322 diagnostic, got codes: {:?}",
                diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
            )
        })
        .message_text
        .clone()
}

#[test]
fn tuple_annotation_renders_canonical_comma_spacing() {
    // Source omits the space after the comma; tsc canonicalizes to `, `.
    let message = ts2322_message("const t: [number,string] = [1, 's']; const a: number[] = t;");
    assert!(
        message.contains("Type '[number, string]'"),
        "tuple annotation should render canonically as `[number, string]`, got: {message}"
    );
    assert!(
        !message.contains("[number,string]"),
        "tuple annotation must not leak the compact source spelling, got: {message}"
    );
}

#[test]
fn tuple_annotation_collapses_non_canonical_whitespace() {
    // Extra interior whitespace must not survive into the rendered type.
    let message = ts2322_message("const t: [number,   string] = [1, 's']; const a: number[] = t;");
    assert!(
        message.contains("Type '[number, string]'"),
        "tuple annotation should collapse extra whitespace to `[number, string]`, got: {message}"
    );
}

#[test]
fn named_tuple_annotation_renders_canonical_spacing() {
    let message = ts2322_message("const t: [a:number,b:string] = [1, 's']; const a: number[] = t;");
    assert!(
        message.contains("Type '[a: number, b: string]'"),
        "named tuple annotation should render as `[a: number, b: string]`, got: {message}"
    );
}

#[test]
fn function_type_annotation_renders_canonical_spacing() {
    let message = ts2322_message(
        "const f: (a:number,b:string)=>void = (() => {}) as any; const g: number = f;",
    );
    assert!(
        message.contains("Type '(a: number, b: string) => void'"),
        "function-type annotation should render as `(a: number, b: string) => void`, got: {message}"
    );
}

#[test]
fn constructor_type_annotation_renders_canonical_spacing() {
    let message = ts2322_message(
        "const c: new(a:number,b:string)=>object = (class {}) as any; const g: number = c;",
    );
    assert!(
        message.contains("Type 'new (a: number, b: string) => object'"),
        "constructor-type annotation should render as `new (a: number, b: string) => object`, \
         got: {message}"
    );
}

#[test]
fn inline_function_source_with_coincidental_alias_expands() {
    // #17119: `g` is annotated with an INLINE function type, not with `Fn`.
    // tsc renders the expanded signature for the source, never a coincidental
    // structurally-identical alias name.
    let message = ts2322_message(
        "type Fn = () => string; declare const g: () => string; \
         type Want = () => number; const bad: Want = g;",
    );
    assert!(
        message.contains("Type '() => string'"),
        "inline function-type source must expand, not show the coincidental alias: {message}"
    );
    assert!(
        !message.contains("'Fn'"),
        "must not substitute the coincidental alias `Fn`: {message}"
    );
}

#[test]
fn inline_tuple_source_with_coincidental_alias_expands() {
    // A tuple annotation is the same family: `t`'s inline `[number, string]`
    // has no `aliasSymbol`, so it must expand rather than render as `Pair`.
    let message = ts2322_message(
        "type Pair = [number, string]; declare const t: [number, string]; \
         const a: number[] = t;",
    );
    assert!(
        message.contains("Type '[number, string]'"),
        "inline tuple source must expand, not show the coincidental alias: {message}"
    );
    assert!(
        !message.contains("'Pair'"),
        "must not substitute the coincidental alias `Pair`: {message}"
    );
}

#[test]
fn inline_tuple_target_with_coincidental_alias_expands() {
    // The target mirror: `t`'s inline tuple annotation has no `aliasSymbol`, so
    // the target renders expanded, not as `Pair`.
    let message = ts2322_message(
        "type Pair = [number, string]; declare const s: [string, number]; \
         const t: [number, string] = s;",
    );
    assert!(
        message.contains("type '[number, string]'"),
        "inline tuple target must expand, not show the coincidental alias: {message}"
    );
    assert!(
        !message.contains("'Pair'"),
        "must not substitute the coincidental alias `Pair` on the target: {message}"
    );
}

#[test]
fn tuple_type_alias_annotation_keeps_its_name() {
    // A *reference* to a tuple alias must still display the alias name, not the
    // expanded structural form — the fix is scoped to inline structural
    // annotations only.
    let message = ts2322_message(
        "type Pair = [number, string]; const t: Pair = [1, 's']; const a: number[] = t;",
    );
    assert!(
        message.contains("Type 'Pair'"),
        "a tuple *alias* reference must keep its name `Pair`, got: {message}"
    );
}
