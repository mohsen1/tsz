//! Parity pins for #16630: a TS2344 constraint written as a **non-generic
//! alias** whose body is the canonical primitive key union
//! (`string | number | symbol`) renders as the alias name, not structurally.
//!
//! The structural rule is the same one every other type-display surface obeys:
//! the spelling written at the site decides. A constraint written as an alias
//! (`K extends PropertyKey`, `K extends Zed`) renders that alias name; a
//! constraint written longhand (`K extends string | number | symbol`) renders
//! structurally. `tsc` keeps the two apart because `getUnionType` keys its
//! cache on the member list *plus* the alias identity, so the aliased and the
//! longhand spelling are two distinct `Type` objects.
//!
//! tsz interns one `TypeId` for both spellings. On the type-alias-application
//! surface the constraint validator resolves the constraint's `Lazy(DefId)`
//! wrapper to the shared canonical union before building the diagnostic (the
//! assignability check needs the concrete union), and the key-union display
//! path then force-expands that union structurally — which was correct for the
//! longhand spelling but wrong for the aliased one, since a user alias such as
//! `Zed` could no longer be told apart from a longhand union. The fix recovers
//! the written alias name from the *unresolved* constraint, so a longhand union
//! (which carries no `Lazy` wrapper) keeps its structural rendering.
//!
//! Every expectation here was verified against the pinned oracle
//! (`typescript@7.0.2`, `--noEmit --strict --lib es2024 --target es2022`).
//! One fixture per row: a generic type alias whose parameter carries the
//! constraint under test, applied to `boolean`, which fails every row's
//! constraint and produces exactly one TS2344.

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_default_lib_files};

/// The single TS2344 message a fixture produces.
fn ts2344_message(source: &str) -> String {
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
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one TS2344 for this fixture, got {diagnostics:?}"
    );
    matches.remove(0).1.clone()
}

/// The rendered constraint from a TS2344 message
/// `Type 'X' does not satisfy the constraint 'C'.` — this returns `C`.
fn rendered_constraint(source: &str) -> String {
    let message = ts2344_message(source);
    let marker = "does not satisfy the constraint '";
    let start = message
        .find(marker)
        .unwrap_or_else(|| panic!("unexpected TS2344 shape: {message}"))
        + marker.len();
    let rest = &message[start..];
    let end = rest
        .rfind('\'')
        .unwrap_or_else(|| panic!("unexpected TS2344 shape: {message}"));
    rest[..end].to_string()
}

// ---------------------------------------------------------------------------
// Aliased spellings render the alias name.
// ---------------------------------------------------------------------------

/// The lib alias `PropertyKey` (whose body is `string | number | symbol`).
#[test]
fn lib_property_key_alias_constraint_renders_the_alias() {
    let source = "type G<K extends PropertyKey> = K;\n\
                  type Bad = G<boolean>;\n";
    assert_eq!(rendered_constraint(source), "PropertyKey");
}

/// A user alias for the same union renders its own name (`Zed`), not the lib
/// `PropertyKey` and not the structural union. This is the row the fix repairs.
#[test]
fn user_key_union_alias_constraint_renders_the_alias() {
    let source = "type Zed = string | number | symbol;\n\
                  type G<K extends Zed> = K;\n\
                  type Bad = G<boolean>;\n";
    assert_eq!(rendered_constraint(source), "Zed");
}

/// Renamed binders and a differently-named alias, so the row cannot be
/// satisfied by anything keyed on the specific `Zed`/`PropertyKey` spelling.
#[test]
fn renamed_binders_user_key_union_alias_constraint_renders_the_alias() {
    let source = "type Whatever = symbol | string | number;\n\
                  type H<Q extends Whatever> = Q;\n\
                  type Bad = H<boolean>;\n";
    assert_eq!(rendered_constraint(source), "Whatever");
}

/// An alias chain to the key union (`type A = …; type B = A`) still resolves to
/// the *written* alias `B`, since `B` is the spelling at the constraint site.
#[test]
fn alias_chain_to_key_union_constraint_renders_the_written_alias() {
    let source = "type A = string | number | symbol;\n\
                  type B = A;\n\
                  type G<K extends B> = K;\n\
                  type Bad = G<boolean>;\n";
    assert_eq!(rendered_constraint(source), "B");
}

// ---------------------------------------------------------------------------
// Longhand and operator spellings stay structural (negative controls).
// ---------------------------------------------------------------------------

/// A constraint written longhand carries no alias to preserve, so it renders
/// structurally — the negative control that the fix must not repaint.
#[test]
fn longhand_key_union_constraint_renders_structurally() {
    let source = "type G<K extends string | number | symbol> = K;\n\
                  type Bad = G<boolean>;\n";
    assert_eq!(rendered_constraint(source), "string | number | symbol");
}

/// `keyof any` resolves to the key union in `tsc` and is displayed
/// structurally. It reaches display through a different mechanism than the
/// alias path and must be left untouched.
#[test]
fn keyof_any_constraint_renders_structurally() {
    let source = "type G<K extends keyof any> = K;\n\
                  type Bad = G<boolean>;\n";
    assert_eq!(rendered_constraint(source), "string | number | symbol");
}

// ---------------------------------------------------------------------------
// Non-key-union aliases are unaffected (shape and altitude controls).
// ---------------------------------------------------------------------------

/// A two-member primitive union alias (`string | number`) already rendered its
/// name and continues to; it is not the canonical key union, so it never took
/// the force-expand path.
#[test]
fn two_member_union_alias_constraint_still_renders_the_alias() {
    let source = "type Pair = string | number;\n\
                  type G<K extends Pair> = K;\n\
                  type Bad = G<boolean>;\n";
    assert_eq!(rendered_constraint(source), "Pair");
}

/// An object alias constraint is unaffected — it renders its name through the
/// ordinary display path, not the key-union path.
#[test]
fn object_alias_constraint_still_renders_the_alias() {
    let source = "type Foo = { a: number };\n\
                  type G<K extends Foo> = K;\n\
                  type Bad = G<number>;\n";
    assert_eq!(rendered_constraint(source), "Foo");
}

/// A primitive alias (`type S = string`) renders as its underlying primitive
/// (`string`), never the alias name — `tsc` strips the alias for a body that
/// resolves to a shared primitive singleton. The key-union recovery must not
/// widen to cover this.
#[test]
fn primitive_alias_constraint_renders_the_underlying_primitive() {
    let source = "type S = string;\n\
                  type G<K extends S> = K;\n\
                  type Bad = G<number>;\n";
    assert_eq!(rendered_constraint(source), "string");
}
