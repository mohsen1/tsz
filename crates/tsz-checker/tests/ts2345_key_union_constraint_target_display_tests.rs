//! Parity pins for #16751: a `TS2345` whose reported parameter type is a
//! type-parameter constraint canonicalized to the primitive key union
//! (`string | number | symbol`).
//!
//! When a generic call cannot infer a valid type argument, `tsc` clamps the
//! parameter to the type parameter's constraint and reports the argument
//! against it. A constraint written longhand (`string | number | symbol`) or as
//! `keyof any` / `keyof never` carries no `aliasSymbol`, so `tsc` renders it
//! structurally; a constraint written as `PropertyKey` keeps that alias.
//!
//! tsz interns one union `TypeId` for every spelling and it carries
//! `PropertyKey`'s registered alias, so the general target-display path
//! repainted all four spellings as `PropertyKey`. The fix recovers the *written*
//! constraint clause from the callee's type-parameter declaration and expands
//! the union structurally only for the longhand / `keyof`-degenerate spellings,
//! leaving the `PropertyKey` control (and a user alias such as `Zed`) untouched.
//! It is the target-side sibling of #16630/#16663 (TS2344 constraint display)
//! and #16748 (TS2322 `keyof any` source display).
//!
//! Every expectation was verified against the pinned oracle
//! (`typescript@7.0.2`, `--noEmit --strict`), matching the reduction #16748
//! established for `keyof any` on the source side.

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_default_lib_files};

/// The rendered parameter type from the single TS2345 message a fixture
/// produces — `... not assignable to parameter of type 'P'.` returns `P`.
fn ts2345_parameter_display(source: &str) -> String {
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
    let message = &matches.remove(0).1;
    let marker = "not assignable to parameter of type '";
    let start = message
        .find(marker)
        .map(|i| i + marker.len())
        .unwrap_or_else(|| panic!("no parameter clause in {message:?}"));
    let rest = &message[start..];
    let end = rest.find('\'').unwrap_or(rest.len());
    rest[..end].to_string()
}

#[test]
fn keyof_any_constraint_renders_structurally() {
    let source = "function f<K extends keyof any>(k: K): K { return k; }\nf(true);\n";
    assert_eq!(ts2345_parameter_display(source), "string | number | symbol");
}

#[test]
fn longhand_key_union_constraint_renders_structurally() {
    let source =
        "function f<K extends string | number | symbol>(k: K): K { return k; }\nf(true);\n";
    assert_eq!(ts2345_parameter_display(source), "string | number | symbol");
}

#[test]
fn keyof_never_constraint_renders_structurally() {
    // `keyof never` reduces to the same universal key set as `keyof any`.
    let source = "function f<K extends keyof never>(k: K): K { return k; }\nf(true);\n";
    assert_eq!(ts2345_parameter_display(source), "string | number | symbol");
}

#[test]
fn property_key_constraint_keeps_its_alias() {
    // Control: a constraint written as the `PropertyKey` alias keeps that name.
    let source = "function f<K extends PropertyKey>(k: K): K { return k; }\nf(true);\n";
    assert_eq!(ts2345_parameter_display(source), "PropertyKey");
}

#[test]
fn renamed_binder_with_reordered_union_renders_structurally() {
    // Binder-name and member-order variation: the recovery is keyed on the
    // written clause, not on the type-parameter spelling `K`.
    let source = "function pick<Zz extends symbol | string | number>(z: Zz): Zz { return z; }\npick(true);\n";
    assert_eq!(ts2345_parameter_display(source), "string | number | symbol");
}

#[test]
fn second_type_parameter_constraint_renders_structurally() {
    // The failing argument's parameter maps to the second type parameter; the
    // recovery matches the parameter slot to its type parameter by name.
    let source = "function f<A, K extends keyof any>(a: A, k: K): K { return k; }\nf(1, true);\n";
    assert_eq!(ts2345_parameter_display(source), "string | number | symbol");
}

#[test]
fn arrow_function_const_callee_renders_structurally() {
    // A callee bound to a `const` arrow: the recovery follows the variable
    // declaration's initializer to the arrow's type parameters.
    let source = "const f = <K extends keyof any>(k: K): K => k;\nf(true);\n";
    assert_eq!(ts2345_parameter_display(source), "string | number | symbol");
}

#[test]
fn function_expression_const_callee_renders_structurally() {
    let source = "const f = function <K extends string | number | symbol>(k: K): K { return k; };\nf(true);\n";
    assert_eq!(ts2345_parameter_display(source), "string | number | symbol");
}

#[test]
fn user_alias_key_union_constraint_keeps_its_alias_name() {
    // A user alias whose body is the key union renders its own name, not
    // `PropertyKey` and not the structural expansion — the spelling decides.
    let source = "type Zed = string | number | symbol;\n\
                  function f<K extends Zed>(k: K): K { return k; }\nf(true);\n";
    assert_eq!(ts2345_parameter_display(source), "Zed");
}
