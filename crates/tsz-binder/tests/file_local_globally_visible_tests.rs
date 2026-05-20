//! Tests for `BinderState::file_local_is_globally_visible`.
//!
//! Structural rule: a `file_locals` entry is visible to other files only when
//! the binder's source file is **not** an external module, or the entry came
//! from a `declare global { ... }` block (tracked in `global_augmentations`).
//! Without this filter, module-local declarations (type aliases, classes,
//! interfaces, functions in a file with an `import` or `export`) leak into
//! sibling files' name resolution and silently make `export type Foo = ...`
//! from one module resolvable as `Foo` in another module without an import.

use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;

fn bind_source(file_name: &str, source: &str) -> BinderState {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = Arc::new(parser.get_arena().clone());
    let mut binder = BinderState::new();
    binder.bind_source_file(&arena, root);
    binder
}

#[test]
fn module_local_type_alias_is_not_globally_visible() {
    let binder = bind_source(
        "Helpers.ts",
        "export type StringKeyOf<TObj> = Extract<string, keyof TObj>;\n",
    );
    assert!(binder.is_external_module());
    assert!(binder.file_locals.has("StringKeyOf"));
    assert!(
        !binder.file_local_is_globally_visible("StringKeyOf"),
        "module-local `export type` must not surface through the cross-file index"
    );
}

#[test]
fn module_local_type_alias_with_different_name_is_not_globally_visible() {
    // Same shape as above with a different name choice — the filter is on
    // is_external_module + global_augmentations, never on the spelling.
    let binder = bind_source(
        "Renamed.ts",
        "export type Lookup<K> = K extends string ? K : never;\n",
    );
    assert!(binder.is_external_module());
    assert!(binder.file_locals.has("Lookup"));
    assert!(!binder.file_local_is_globally_visible("Lookup"));
}

#[test]
fn module_local_class_is_not_globally_visible() {
    let binder = bind_source("Klass.ts", "export class Widget { value: number = 0; }\n");
    assert!(binder.is_external_module());
    assert!(binder.file_locals.has("Widget"));
    assert!(!binder.file_local_is_globally_visible("Widget"));
}

#[test]
fn module_local_function_is_not_globally_visible() {
    let binder = bind_source("Fn.ts", "export function helper(): void {}\n");
    assert!(binder.is_external_module());
    assert!(binder.file_locals.has("helper"));
    assert!(!binder.file_local_is_globally_visible("helper"));
}

#[test]
fn module_local_interface_is_not_globally_visible() {
    let binder = bind_source(
        "Iface.ts",
        "export interface Shape { kind: \"circle\" | \"square\" }\n",
    );
    assert!(binder.is_external_module());
    assert!(binder.file_locals.has("Shape"));
    assert!(!binder.file_local_is_globally_visible("Shape"));
}

#[test]
fn script_file_top_level_type_alias_is_globally_visible() {
    // No top-level import/export — this is a script, so its top-level
    // declarations share the global scope and must surface cross-file.
    let binder = bind_source(
        "globals.ts",
        "type GlobalAlias = { tag: \"global\" };\nfunction globalHelper() {}\n",
    );
    assert!(!binder.is_external_module());
    assert!(binder.file_locals.has("GlobalAlias"));
    assert!(binder.file_local_is_globally_visible("GlobalAlias"));
    assert!(binder.file_locals.has("globalHelper"));
    assert!(binder.file_local_is_globally_visible("globalHelper"));
}

#[test]
fn declare_global_namespace_in_external_module_is_globally_visible() {
    // External module file with `declare global { namespace MyNs { ... } }`:
    // `MyNs` is added to `file_locals` via the global-augmentation hoist path
    // *and* recorded in `global_augmentations`, so it must remain
    // cross-file visible while the sibling module-local `type Local = ...`
    // alias must not.
    let binder = bind_source(
        "Aug.ts",
        "export {};\n\
         type Local = number;\n\
         declare global {\n  namespace MyNs { type Inner = string }\n}\n",
    );
    assert!(binder.is_external_module());
    assert!(binder.file_local_is_globally_visible("MyNs"));
    assert!(!binder.file_local_is_globally_visible("Local"));
}

#[test]
fn declare_global_var_in_external_module_is_globally_visible() {
    let binder = bind_source(
        "Var.ts",
        "export {};\nconst moduleLocal = 1;\ndeclare global {\n  const myGlobal: number;\n}\n",
    );
    assert!(binder.is_external_module());
    assert!(binder.file_local_is_globally_visible("myGlobal"));
    assert!(!binder.file_local_is_globally_visible("moduleLocal"));
}

#[test]
fn declare_global_type_alias_in_external_module_is_globally_visible() {
    let binder = bind_source(
        "T.ts",
        "export {};\ntype ModuleLocal = number;\n\
         declare global {\n  type MyGlobal = { kind: \"global\" };\n}\n",
    );
    assert!(binder.is_external_module());
    assert!(
        binder.global_augmentations.contains_key("MyGlobal"),
        "declare global type alias must be recorded in global_augmentations: keys={:?}",
        binder.global_augmentations.keys().collect::<Vec<_>>()
    );
    assert!(binder.file_local_is_globally_visible("MyGlobal"));
    assert!(!binder.file_local_is_globally_visible("ModuleLocal"));
}

#[test]
fn declare_global_interface_in_external_module_is_globally_visible() {
    let binder = bind_source(
        "I.ts",
        "export {};\ninterface ModuleLocalIface { x: number }\n\
         declare global {\n  interface GlobalIface { y: number }\n}\n",
    );
    assert!(binder.is_external_module());
    assert!(binder.file_local_is_globally_visible("GlobalIface"));
    assert!(!binder.file_local_is_globally_visible("ModuleLocalIface"));
}

#[test]
fn unknown_name_is_not_globally_visible_in_external_module() {
    // Defensive: a name that doesn't exist anywhere should never claim
    // global visibility regardless of binder state.
    let binder = bind_source("M.ts", "export const x = 1;\n");
    assert!(binder.is_external_module());
    assert!(!binder.file_local_is_globally_visible("NeverDeclared"));
}

#[test]
fn unknown_name_in_script_returns_true() {
    // In a script, the predicate is independent of presence — every entry
    // in file_locals shares the global scope, and asking about a non-existent
    // name in a non-module file returns `true` because the predicate's role
    // is to filter *populated* entries at index-build time.
    let binder = bind_source("globals.ts", "const x = 1;\n");
    assert!(!binder.is_external_module());
    // Caller passes only names that are in file_locals; for parity with the
    // ProgramContext-index loop the predicate must say "yes" for any name
    // when the file is a script.
    assert!(binder.file_local_is_globally_visible("anything"));
}
