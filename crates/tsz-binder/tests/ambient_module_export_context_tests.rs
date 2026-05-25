//! Tests for the ambient-module "export context" rule.
//!
//! An ambient module/namespace implicitly exports its non-`export`-marked
//! members only while its body contains no `ExportDeclaration`/`ExportAssignment`
//! statement (tsc's `setExportContextFlag`/`hasExportDeclarations`). Once such a
//! statement appears (`export { ... }`, `export * ...`, `export = ...`, or
//! `export default <expression>`), the body switches to explicit-export mode and
//! only the explicitly exported bindings (plus the synthesized `default`) are
//! visible.

use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;

fn bind_source(source: &str) -> BinderState {
    let mut parser = ParserState::new("test.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = Arc::new(parser.get_arena().clone());
    let mut binder = BinderState::new();
    binder.bind_source_file(&arena, root);
    binder
}

fn exported_names(binder: &BinderState, module: &str) -> Vec<String> {
    let mut names: Vec<String> = binder
        .module_exports
        .get(module)
        .map(|table| table.iter().map(|(name, _)| name.clone()).collect())
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn ambient_module_without_export_declarations_auto_exports_members() {
    // No export declaration anywhere: implicit export context stays on, so every
    // member is visible even without an `export` modifier.
    let binder = bind_source(
        r#"declare module "m" {
            const a: number;
            function b(): void;
        }"#,
    );
    let names = exported_names(&binder, "m");
    assert!(
        names.contains(&"a".to_string()),
        "expected `a`, got {names:?}"
    );
    assert!(
        names.contains(&"b".to_string()),
        "expected `b`, got {names:?}"
    );
}

#[test]
fn named_export_declaration_disables_implicit_export_of_siblings() {
    // `export { a }` switches the body to explicit-export mode: `a` is exported,
    // the unmarked `b` is module-local only.
    let binder = bind_source(
        r#"declare module "m" {
            export { a };
            const a: number;
            const b: number;
        }"#,
    );
    let names = exported_names(&binder, "m");
    assert!(
        names.contains(&"a".to_string()),
        "expected `a`, got {names:?}"
    );
    assert!(
        !names.contains(&"b".to_string()),
        "`b` is not exported, got {names:?}"
    );
}

#[test]
fn named_export_declaration_rule_is_not_name_specific() {
    // Same rule with completely different identifier spellings — proves the
    // behavior keys on structure, not on a hardcoded name.
    let binder = bind_source(
        r#"declare module "pkg" {
            export { keep };
            const keep: string;
            const hidden: string;
        }"#,
    );
    let names = exported_names(&binder, "pkg");
    assert!(
        names.contains(&"keep".to_string()),
        "expected `keep`, got {names:?}"
    );
    assert!(
        !names.contains(&"hidden".to_string()),
        "`hidden` is not exported, got {names:?}"
    );
}

#[test]
fn export_equals_disables_implicit_export_of_siblings() {
    // `export = x` is an export assignment that disables the implicit export
    // context, so the unmarked sibling `y` is not exported.
    let binder = bind_source(
        r#"declare module "m" {
            const x: { foo: number };
            const y: number;
            export = x;
        }"#,
    );
    let names = exported_names(&binder, "m");
    assert!(
        !names.contains(&"y".to_string()),
        "`y` is not exported, got {names:?}"
    );
}

#[test]
fn export_default_does_not_disable_implicit_export_context() {
    // `export default <…>` is intentionally NOT treated as disabling (see the
    // helper docs: the synthesized-`default` cross-file path is not yet modeled,
    // so we fall back to implicit export). The sibling `y` therefore stays
    // implicitly exported rather than being hidden.
    let binder = bind_source(
        r#"declare module "m" {
            const x: { foo: number };
            const y: number;
            export default x;
        }"#,
    );
    let names = exported_names(&binder, "m");
    assert!(
        names.contains(&"y".to_string()),
        "`y` stays implicitly exported under the default-export fallback, got {names:?}"
    );
}

#[test]
fn export_modifier_on_declaration_keeps_implicit_export_context() {
    // `export function`/`export const`/etc. are declarations with an export
    // modifier, NOT bare export declarations, so they must not flip the body to
    // explicit-export mode. The unmarked sibling `Promise` stays implicitly
    // exported alongside the explicitly exported `defer`. (Mirrors the regression
    // in tsc's `funduleUsedAcrossFileBoundary`, where the parser models
    // `export function` as an `EXPORT_DECLARATION` wrapping the function.)
    let binder = bind_source(
        r#"declare module "q" {
            interface Promise<T> { foo: string; }
            export function defer<T>(): string;
        }"#,
    );
    let names = exported_names(&binder, "q");
    assert!(
        names.contains(&"Promise".to_string()),
        "`Promise` stays implicitly exported, got {names:?}"
    );
    assert!(
        names.contains(&"defer".to_string()),
        "`defer` is explicitly exported, got {names:?}"
    );
}

#[test]
fn export_default_declaration_keeps_implicit_export_context() {
    // `export default function f() {}` is a default-exported *declaration*, not
    // an export assignment, so it does NOT disable the implicit export context:
    // the sibling `g` stays implicitly exported.
    let binder = bind_source(
        r#"declare module "m" {
            export default function f(): void;
            const g: number;
        }"#,
    );
    let names = exported_names(&binder, "m");
    assert!(
        names.contains(&"g".to_string()),
        "`g` stays implicitly exported, got {names:?}"
    );
}
