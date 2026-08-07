//! Parity pins for #16751: a TS2345 argument-to-parameter TARGET display for a
//! generic parameter constrained to `keyof any` / `keyof unknown` /
//! `keyof never`, or the longhand `string | number | symbol`, must render
//! structurally, not repainted as the coincidentally-shaped lib alias
//! `PropertyKey`.
//!
//! Sibling of #16610/#16748 (assignment-SOURCE display) and #16630 (TS2344
//! constraint display): the same `keyof any` / longhand-union degenerate-key
//! shape resolves eagerly to its fixed key-space union at type-construction
//! time (tsc's `getIndexType`), carries no `aliasSymbol`, and so must never be
//! repainted by tsz's reverse type-to-def lookup finding the lib `PropertyKey`
//! alias for the same structural union. This file covers the TARGET side of a
//! call-argument mismatch (TS2345) specifically — the one row #16748 did not
//! reach, since that fix only covered assignment-source display.
//!
//! Every expectation verified against pinned `typescript@7.0.2`
//! (`--noEmit --strict`).

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
/// `Argument of type 'X' is not assignable to parameter of type 'Y'.` — this
/// returns `Y`.
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

#[test]
fn keyof_any_constraint_renders_structurally() {
    let source = "function f<K extends keyof any>(k: K): K { return k; }\n\
                  f(true);\n";
    assert_eq!(rendered_parameter(source), "string | number | symbol");
}

#[test]
fn keyof_unknown_constraint_renders_structurally() {
    let source = "function f<K extends keyof unknown>(k: K): K { return k; }\n\
                  f(true);\n";
    assert_eq!(rendered_parameter(source), "never");
}

#[test]
fn keyof_never_constraint_renders_structurally() {
    let source = "function f<K extends keyof never>(k: K): K { return k; }\n\
                  f(true);\n";
    assert_eq!(rendered_parameter(source), "string | number | symbol");
}

#[test]
fn longhand_primitive_key_union_constraint_renders_structurally() {
    let source = "function f<K extends string | number | symbol>(k: K): K { return k; }\n\
                  f(true);\n";
    assert_eq!(rendered_parameter(source), "string | number | symbol");
}

/// Renamed binders and a differently-named function, so the row cannot be
/// satisfied by anything keyed on the specific `f`/`K` spelling.
#[test]
fn renamed_binders_keyof_any_constraint_renders_structurally() {
    let source = "function wrap<Q extends keyof any>(q: Q): Q { return q; }\n\
                  wrap(true);\n";
    assert_eq!(rendered_parameter(source), "string | number | symbol");
}

/// Negative control: a constraint written as the lib alias `PropertyKey` must
/// keep rendering the alias name, not the structural union — this row was
/// already correct on `main` and must not regress.
#[test]
fn property_key_alias_constraint_keeps_the_alias_name() {
    let source = "function f<K extends PropertyKey>(k: K): K { return k; }\n\
                  f(true);\n";
    assert_eq!(rendered_parameter(source), "PropertyKey");
}

/// Negative control: a constraint written as a user alias to the same union
/// keeps its own name. Exercises the same code path with a non-lib alias to
/// confirm the fix does not key on `PropertyKey`'s specific `DefId`.
#[test]
fn user_key_union_alias_constraint_keeps_the_alias_name() {
    let source = "type Zed = string | number | symbol;\n\
                  function f<K extends Zed>(k: K): K { return k; }\n\
                  f(true);\n";
    assert_eq!(rendered_parameter(source), "Zed");
}

/// Arrow-function form of the callee: the constraint lookup walks the
/// declaration's `type_parameters`, not a `FunctionDeclaration`-only path.
#[test]
fn arrow_function_keyof_any_constraint_renders_structurally() {
    let source = "const f = <K extends keyof any>(k: K): K => k;\n\
                  f(true);\n";
    assert_eq!(rendered_parameter(source), "string | number | symbol");
}
