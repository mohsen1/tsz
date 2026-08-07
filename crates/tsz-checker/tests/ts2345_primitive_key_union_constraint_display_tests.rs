//! Parity pins for #16751: the TS2345 target display for a call argument
//! checked against a generic parameter whose constraint is the canonical
//! primitive key union `string | number | symbol`.
//!
//! The structural rule is the one every other type-display surface obeys — the
//! spelling written at the site decides — and it is the same rule the TS2344
//! explicit-type-argument path already enforces
//! (`ts2344_primitive_key_union_alias_constraint_display_tests`). A constraint
//! written as `keyof any` or the longhand `string | number | symbol` renders
//! structurally; a constraint written as an alias (`PropertyKey`, a user
//! `type Zed = …`, or a chain to either) renders that alias name.
//!
//! tsz interns one `TypeId` for every spelling of the key union and, through
//! the reverse type-to-def lookup, repaints it with whatever coincidentally
//! shaped alias is in scope (the lib `PropertyKey`). That reverse lookup already
//! recovers the correct name for a constraint written *as* an alias, so the fix
//! only intercepts the two structural spellings — read from the callee's
//! type-parameter constraint node in the AST, because at the type level every
//! spelling has collapsed to the same interned union.
//!
//! Every expectation here was verified against pinned `typescript@7.0.2`
//! (`--strict`). Each fixture calls a generic function with an argument that
//! fails the constraint (`boolean`/`true` against a key-space bound), producing
//! exactly one TS2345.

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_default_lib_files};

/// The single TS2345 message a fixture produces.
fn ts2345_message(source: &str) -> String {
    let diagnostics = check_source_with_libs_code_messages(
        source,
        "case.ts",
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
        &load_default_lib_files(),
    );
    let mut matches: Vec<&(u32, String)> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2345)
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one TS2345 for this fixture, got {diagnostics:?}"
    );
    matches.remove(0).1.clone()
}

/// The rendered parameter type from a TS2345 message
/// `Argument of type 'A' is not assignable to parameter of type 'P'.` — this
/// returns `P`.
fn rendered_parameter(source: &str) -> String {
    let message = ts2345_message(source);
    let marker = "is not assignable to parameter of type '";
    let start = message
        .find(marker)
        .unwrap_or_else(|| panic!("unexpected TS2345 shape: {message}"))
        + marker.len();
    let rest = &message[start..];
    let end = rest
        .rfind('\'')
        .unwrap_or_else(|| panic!("unexpected TS2345 shape: {message}"));
    rest[..end].to_string()
}

// ---------------------------------------------------------------------------
// Structural spellings render by their members (the rows the fix repairs).
// ---------------------------------------------------------------------------

/// `keyof any` resolves to the key union and is displayed structurally, never
/// as the coincidentally-shaped `PropertyKey` alias.
#[test]
fn keyof_any_constraint_renders_structurally() {
    let source = "function f<K extends keyof any>(k: K): K { return k; }\nf(true);\n";
    assert_eq!(rendered_parameter(source), "string | number | symbol");
}

/// The longhand union carries no alias to preserve, so it renders structurally.
#[test]
fn longhand_key_union_constraint_renders_structurally() {
    let source =
        "function f<K extends string | number | symbol>(k: K): K { return k; }\nf(true);\n";
    assert_eq!(rendered_parameter(source), "string | number | symbol");
}

/// Renamed binders and a reordered longhand union, so the row cannot be
/// satisfied by anything keyed on a specific name or the canonical member order.
#[test]
fn renamed_binders_reordered_longhand_renders_structurally() {
    let source =
        "function gg<Q extends symbol | string | number>(q: Q): Q { return q; }\ngg(false);\n";
    assert_eq!(rendered_parameter(source), "string | number | symbol");
}

/// A `keyof any` constraint on a value whose declared type is a function type
/// (`declare const f: <K extends keyof any>(…) => …`) is recovered from the
/// variable declaration's type annotation.
#[test]
fn const_function_type_keyof_any_constraint_renders_structurally() {
    let source = "declare const f: <K extends keyof any>(k: K) => K;\nf(true);\n";
    assert_eq!(rendered_parameter(source), "string | number | symbol");
}

// ---------------------------------------------------------------------------
// Alias spellings keep their name (negative controls the fix must not repaint).
// ---------------------------------------------------------------------------

/// The lib alias `PropertyKey` renders as its name — the control that must not
/// regress to structural.
#[test]
fn lib_property_key_alias_constraint_renders_the_alias() {
    let source = "function f<K extends PropertyKey>(k: K): K { return k; }\nf(true);\n";
    assert_eq!(rendered_parameter(source), "PropertyKey");
}

/// A user alias for the same union renders its own name (`Zed`), not the lib
/// `PropertyKey` and not the structural union.
#[test]
fn user_key_union_alias_constraint_renders_the_alias() {
    let source = "type Zed = string | number | symbol;\n\
                  function f<K extends Zed>(k: K): K { return k; }\n\
                  f(true);\n";
    assert_eq!(rendered_parameter(source), "Zed");
}

/// A pure alias-to-alias chain resolves to the underlying alias (`A`), the one
/// whose body is directly the key union — not the head written at the site.
#[test]
fn alias_chain_to_key_union_constraint_renders_the_underlying_alias() {
    let source = "type A = string | number | symbol;\n\
                  type B = A;\n\
                  function f<K extends B>(k: K): K { return k; }\n\
                  f(true);\n";
    assert_eq!(rendered_parameter(source), "A");
}

// ---------------------------------------------------------------------------
// Non-key-union constraints are unaffected (shape/altitude controls).
// ---------------------------------------------------------------------------

/// A two-member primitive union constraint is not the canonical key union, so
/// it never took the alias-repaint path and continues to render its members.
#[test]
fn two_member_primitive_union_constraint_renders_its_members() {
    let source = "function f<K extends string | number>(k: K): K { return k; }\nf(true);\n";
    assert_eq!(rendered_parameter(source), "string | number");
}

/// An object constraint renders structurally through the ordinary path,
/// untouched by the key-union recovery.
#[test]
fn object_constraint_renders_structurally() {
    let source = "function f<K extends { a: number }>(k: K): K { return k; }\nf(true);\n";
    assert_eq!(rendered_parameter(source), "{ a: number; }");
}

/// A bare primitive constraint renders as the primitive, untouched.
#[test]
fn string_primitive_constraint_renders_the_primitive() {
    let source = "function f<K extends string>(k: K): K { return k; }\nf(true);\n";
    assert_eq!(rendered_parameter(source), "string");
}
