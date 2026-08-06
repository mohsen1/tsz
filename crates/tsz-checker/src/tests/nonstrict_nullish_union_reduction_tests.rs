//! Regression tests for #16580: with `strictNullChecks` off, tsc drops a
//! `null`/`undefined` constituent from *every* union it builds, not just the
//! array-literal element path fixed by #16574.
//!
//! Structural rule: when `strictNullChecks` is off and a union has both a
//! `null`/`undefined` constituent and a non-nullish one, tsc's `addTypeToUnion`
//! never adds the nullable to the member set (it is a subtype of every non-nullish
//! type there), so the union reduces at construction — annotations, aliases,
//! function return types and array-element types alike. The one survivor is an
//! *all-nullish* union (`null | undefined`), which has no non-nullish sibling to
//! absorb into and must stay as-is rather than collapse to `never`.
//!
//! Owner: the solver's union-construction seam
//! (`TypeInterner::reduce_nonstrict_nullish_members`), applied uniformly across
//! every union constructor so a member set keeps one canonical identity per
//! program.
//!
//! Each positive row forces the reduced type into a `TS2322` whose rendered
//! source (or target) type is the observable. Expectations are the tsc renderings
//! pinned in #16580 (`typescript@7.0.2`, `--strict false --strictNullChecks
//! false`). The strict-mode controls prove the change is a no-op under
//! `strictNullChecks`, and the renamed-binder rows keep it structural rather than
//! keyed on any identifier.

use crate::test_utils::{
    check_with_options_code_messages, non_strict_checker_options, strict_checker_options,
};

fn nonstrict(source: &str) -> Vec<(u32, String)> {
    check_with_options_code_messages(source, non_strict_checker_options())
}

fn strict(source: &str) -> Vec<(u32, String)> {
    check_with_options_code_messages(source, strict_checker_options())
}

/// The reduced source type is rendered by assigning to an incompatible target.
fn assert_source_renders(source: &str, rendered_source: &str, target: &str) {
    let messages = nonstrict(source);
    assert_eq!(
        messages,
        vec![(
            2322,
            format!("Type '{rendered_source}' is not assignable to type '{target}'.")
        )],
        "expected source to render `{rendered_source}`: {messages:?}"
    );
}

// --- #16580 positive rows: the nullish constituent is dropped ----------------

/// Row a4 (the issue's repro): a function whose declared return type is
/// `number | null` returns `number` in non-strict mode.
#[test]
fn return_type_drops_null() {
    assert_source_renders(
        "declare function f(): number | null;\nvar probe: string = f();\n",
        "number",
        "string",
    );
}

/// Row a3: a type alias `number | undefined` renders as `number`. This surfaces
/// as a naming difference — once the union reduces there is no alias left to
/// print — and must be fixed at construction, not by expanding the alias in the
/// printer.
#[test]
fn alias_of_number_or_undefined_drops_undefined() {
    assert_source_renders(
        "type T = number | undefined;\ndeclare var a: T;\nvar e: string = a;\n",
        "number",
        "string",
    );
}

/// Row a5: `string | null | undefined` renders as `string` (both nullish members
/// dropped), also collapsing the alias.
#[test]
fn alias_of_string_or_null_or_undefined_drops_both() {
    assert_source_renders(
        "type Zqq = string | null | undefined;\ndeclare var w: Zqq;\nvar e: number = w;\n",
        "string",
        "number",
    );
}

/// Row a1: the element annotation `(number | null)[]` renders as `number[]`.
#[test]
fn array_element_annotation_drops_null() {
    assert_source_renders(
        "declare var a: (number | null)[];\nvar e: string = a;\n",
        "number[]",
        "string",
    );
}

/// The generic-instantiation seam #16593 left as a documented follow-up: a
/// generic interface property typed `T | null` must reduce to `number` after
/// instantiation with `T = number`. The `T | null` union is rebuilt during
/// instantiation (substituting `T`), which bypasses the syntactic union-type-node
/// resolvers #16593 patched but flows through the solver union-construction seam
/// this change owns. Property access renders the instantiated member type.
#[test]
fn generic_property_union_reduces_after_instantiation() {
    assert_source_renders(
        "interface Box<T> { value: T | null; }\ndeclare const b: Box<number>;\nvar e: string = b.value;\n",
        "number",
        "string",
    );
}

// --- Anti-hardcoding: renamed binders reach the same rule --------------------

#[test]
fn renamed_alias_and_function_still_reduce() {
    // A differently-named alias.
    assert_source_renders(
        "type Foo = boolean | null;\ndeclare var q: Foo;\nvar e: number = q;\n",
        "boolean",
        "number",
    );
    // A differently-named function with `undefined`.
    assert_source_renders(
        "declare function gg(): bigint | undefined;\nvar e: string = gg();\n",
        "bigint",
        "string",
    );
}

// --- Negative controls -------------------------------------------------------

/// Row a2: `number | null` with an initializer already renders `number` (agreed
/// before this change); it must still render `number`, not regress to a
/// collapsed-away or widened form.
#[test]
fn declared_number_or_null_still_number() {
    assert_source_renders(
        "var a: number | null = 1;\nvar e: string = a;\n",
        "number",
        "string",
    );
}

/// Row a6: an all-nullish union has no non-nullish sibling to absorb into and
/// must stay as-is — never collapse to `never`. The bare declaration is clean, as
/// tsc reports.
#[test]
fn all_nullish_union_stays_clean() {
    let messages = nonstrict("declare var a: null | undefined;\nvar b: null | undefined = a;\n");
    assert!(
        messages.is_empty(),
        "all-nullish union must remain a valid, clean type (not collapsed): {messages:?}"
    );
}

// --- Strict-mode controls: the change is a no-op under strictNullChecks -------

#[test]
fn strict_mode_keeps_null_in_return_type() {
    let messages = strict("declare function f(): number | null;\nvar probe: string = f();\n");
    assert_eq!(
        messages,
        vec![(
            2322,
            "Type 'number | null' is not assignable to type 'string'.".to_string()
        )],
        "strict mode must keep the null constituent: {messages:?}"
    );
}

#[test]
fn strict_mode_keeps_alias_and_undefined() {
    let messages = strict("type T = number | undefined;\ndeclare var a: T;\nvar e: string = a;\n");
    assert_eq!(
        messages,
        vec![(
            2322,
            "Type 'T' is not assignable to type 'string'.".to_string()
        )],
        "strict mode must keep the alias and its undefined: {messages:?}"
    );
}
