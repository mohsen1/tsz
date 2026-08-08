//! A parameter binding pattern declares value bindings that stay in scope for
//! the *type* positions of the same signature.
//!
//! Because a signature has no body, the only way to reference one is a `typeof`
//! type query in the signature's own return type. `tsc` resolves such a query
//! to the binding, which has two consequences tsz used to get wrong:
//!
//! * the query must not fall through to global name resolution (`TS2304`);
//! * `TS2842` ("is an unused renaming of") describes an *unused* renaming, so a
//!   `typeof` reference to the renamed binding must silence it.
//!
//! Oracled against `tsc` 7.0.2 with `--strict false --target es2015`.

use tsz_checker::test_utils::{check_source_codes_named, check_source_non_strict_codes};

const TS2304_CANNOT_FIND_NAME: u32 = 2304;
const TS2842_UNUSED_RENAMING: u32 = 2842;

// ---------------------------------------------------------------------------
// TS2842 fires only when the renaming is unused
// ---------------------------------------------------------------------------

#[test]
fn unreferenced_rename_in_function_type_still_reports_ts2842() {
    let codes = check_source_non_strict_codes(
        "type O = { a?: string; b: number };\ntype F = ({ a: renamed }: O) => void;",
    );
    assert!(
        codes.contains(&TS2842_UNUSED_RENAMING),
        "an unreferenced renaming is still TS2842; got {codes:?}"
    );
}

#[test]
fn rename_referenced_by_return_type_query_reports_nothing() {
    let codes = check_source_non_strict_codes(
        "type O = { a?: string; b: number };\ntype F = ({ a: renamed }: O) => typeof renamed;",
    );
    assert!(
        codes.is_empty(),
        "`typeof renamed` uses the renaming, so neither TS2842 nor TS2304 may fire; got {codes:?}"
    );
}

#[test]
fn rename_referenced_from_a_constructor_type_reports_nothing() {
    let codes = check_source_non_strict_codes(
        "type O = { a?: string; b: number };\ntype G = new ({ a: renamed }: O) => typeof renamed;",
    );
    assert!(
        codes.is_empty(),
        "a construct signature scopes its bindings the same way; got {codes:?}"
    );
}

#[test]
fn rename_referenced_from_a_bodyless_function_declaration_reports_nothing() {
    let codes = check_source_non_strict_codes(
        "type O = { a?: string; b: number };\ndeclare function f({ a: renamed }: O): typeof renamed;",
    );
    assert!(
        codes.is_empty(),
        "a bodyless declaration scopes its bindings the same way; got {codes:?}"
    );
}

#[test]
fn a_nested_rename_referenced_by_a_type_query_reports_nothing() {
    let codes = check_source_non_strict_codes(
        "type F = ({ outer: { inner: renamed } }: any) => typeof renamed;",
    );
    assert!(
        codes.is_empty(),
        "the binding is declared by a nested pattern but scoped to the same signature; got {codes:?}"
    );
}

#[test]
fn only_the_referenced_rename_of_a_pair_is_exempt() {
    // `used` is referenced, `unused` is not: exactly one TS2842.
    let codes = check_source_non_strict_codes("type F = ({ a: unused, b: used }) => typeof used;");
    assert_eq!(
        codes
            .iter()
            .filter(|&&c| c == TS2842_UNUSED_RENAMING)
            .count(),
        1,
        "the used renaming is exempt and the unused one is not; got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// The binding resolves, so no TS2304
// ---------------------------------------------------------------------------

#[test]
fn shorthand_binding_referenced_by_return_type_query_is_not_unresolved() {
    let codes = check_source_non_strict_codes(
        "type O = { a?: string; b: number };\ntype F = ({ a }: O) => typeof a;",
    );
    assert!(
        !codes.contains(&TS2304_CANNOT_FIND_NAME),
        "`{{ a }}` declares `a`, so `typeof a` resolves; got {codes:?}"
    );
}

#[test]
fn array_pattern_binding_referenced_by_return_type_query_is_not_unresolved() {
    let codes = check_source_non_strict_codes(
        "type F = ([first, second]: [number, string]) => typeof first;",
    );
    assert!(
        !codes.contains(&TS2304_CANNOT_FIND_NAME),
        "an array pattern declares its elements too; got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative cases: the scope must not over-reach
// ---------------------------------------------------------------------------

#[test]
fn a_name_no_parameter_declares_is_still_unresolved() {
    let codes = check_source_non_strict_codes(
        "type O = { a?: string; b: number };\ntype F = ({ a: renamed }: O) => typeof missing;",
    );
    assert!(
        codes.contains(&TS2304_CANNOT_FIND_NAME),
        "`missing` is declared by nothing and must still report TS2304; got {codes:?}"
    );
}

#[test]
fn a_sibling_signatures_binding_does_not_leak() {
    let codes = check_source_non_strict_codes(
        "type A = ({ a: mine }: any) => typeof mine;\ntype B = (x: number) => typeof mine;",
    );
    assert!(
        codes.contains(&TS2304_CANNOT_FIND_NAME),
        "`mine` belongs to A's signature and must not resolve inside B; got {codes:?}"
    );
}

#[test]
fn an_inner_signatures_binding_does_not_leak_outward() {
    // `inner` is declared by the parameter of the *nested* function type, so it
    // is out of scope for the outer signature's own return type.
    let codes = check_source_non_strict_codes(
        "type F = (cb: ({ a: inner }: any) => void) => typeof inner;",
    );
    assert!(
        codes.contains(&TS2304_CANNOT_FIND_NAME),
        "an inner signature's binding is not in the outer signature's scope; got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// The "renamed-from" property may be spelled with any property-name kind.
//
// `tsc` reports TS2842 for a destructuring rename regardless of how the
// property is written — identifier (`a`), string literal (`"a"`), numeric
// literal (`2`), or computed (`["a"]`, `[2]`) — rendering the property name
// verbatim (quotes and brackets preserved). Oracled against
// `renamingDestructuredPropertyInFunctionType` at tsc 7.0.2.
// ---------------------------------------------------------------------------

fn ts2842_messages(source: &str) -> Vec<String> {
    tsz_checker::test_utils::check_source_non_strict(source)
        .into_iter()
        .filter(|d| d.code == TS2842_UNUSED_RENAMING)
        .map(|d| d.message_text)
        .collect()
}

#[test]
fn rename_from_string_literal_property_reports_ts2842() {
    let msgs = ts2842_messages("type F = ({ \"a\": renamed }) => void;");
    assert_eq!(msgs.len(), 1, "a string-literal property rename is flagged");
    assert!(
        msgs[0].contains("unused renaming of '\"a\"'"),
        "the property name renders with its quotes; got {msgs:?}"
    );
}

#[test]
fn rename_from_numeric_property_reports_ts2842() {
    let msgs = ts2842_messages("type F = ({ 2: renamed }) => void;");
    assert_eq!(msgs.len(), 1, "a numeric property rename is flagged");
    assert!(
        msgs[0].contains("unused renaming of '2'"),
        "the numeric property renders verbatim; got {msgs:?}"
    );
}

#[test]
fn rename_from_computed_property_reports_ts2842() {
    let msgs = ts2842_messages("type F = ({ [\"a\"]: renamed }) => void;");
    assert_eq!(msgs.len(), 1, "a computed property rename is flagged");
    assert!(
        msgs[0].contains("unused renaming of '[\"a\"]'"),
        "the computed name renders with its brackets; got {msgs:?}"
    );
}

#[test]
fn rename_from_string_literal_property_in_value_signature_reports_ts2842() {
    // The value-position path (bodyless function declaration) flags the same
    // non-identifier property-name kinds as the type-position path.
    let codes = check_source_non_strict_codes(
        "type O = { a?: string; b: number };\ndeclare function f({ \"a\": renamed }: O): void;",
    );
    assert!(
        codes.contains(&TS2842_UNUSED_RENAMING),
        "a string-literal rename in a bodyless declaration is flagged; got {codes:?}"
    );
}

#[test]
fn non_identifier_rename_referenced_by_type_query_reports_nothing() {
    // The "unused" gate still applies: a `typeof` reference to the renamed
    // binding silences TS2842 even when the property is a string literal.
    let codes = check_source_non_strict_codes("type F = ({ \"a\": renamed }) => typeof renamed;");
    assert!(
        codes.is_empty(),
        "`typeof renamed` uses the renaming, so no TS2842; got {codes:?}"
    );
}

#[test]
fn string_literal_rename_is_binder_name_invariant() {
    // The diagnostic depends on the structural rename, not on the chosen local
    // name — a different binder name produces the same single TS2842.
    for local in ["renamed", "zzz", "alias"] {
        let msgs = ts2842_messages(&format!("type F = ({{ \"a\": {local} }}) => void;"));
        assert_eq!(
            msgs.len(),
            1,
            "renaming `\"a\"` to `{local}` is flagged once; got {msgs:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// TS2842 never fires inside a `.d.ts` file.
//
// A declaration file has no bodies at all, so tsc treats a renamed
// destructured parameter as ordinary declaration syntax rather than a
// possible authoring mistake. Oracled against `tsc` 7.0.2:
// identical `{ a: renamed }: O` source reports TS2842 in a `.ts` file and is
// silent in a `.d.ts` file. Covers both diagnostic-emission sites: the bare
// function-type path (`type F = (...) => void`, `type_node_helpers.rs`'s
// `check_duplicate_parameters_in_type`) and the member/signature path
// (interface methods, `declare function`, a type literal's function-typed
// property; `parameter_checker.rs`'s `check_duplicate_parameters`).
// ---------------------------------------------------------------------------

#[test]
fn bare_function_type_rename_in_declaration_file_reports_nothing() {
    let codes = check_source_codes_named(
        "type O = { a: string; b: number };\ntype F = ({ a: renamed }: O) => void;",
        "lib.d.ts",
    );
    assert!(
        !codes.contains(&TS2842_UNUSED_RENAMING),
        "a `.d.ts` file has no bodies, so tsc never flags a renamed destructured \
         parameter there; got {codes:?}"
    );
}

#[test]
fn interface_method_signature_rename_in_declaration_file_reports_nothing() {
    let codes = check_source_codes_named(
        "type O = { a: string; b: number };\ninterface I { method({ a: renamed }: O): void; }",
        "lib.d.ts",
    );
    assert!(
        !codes.contains(&TS2842_UNUSED_RENAMING),
        "an interface method signature is exempt in a `.d.ts` file too; got {codes:?}"
    );
}

#[test]
fn declare_function_rename_in_declaration_file_reports_nothing() {
    let codes = check_source_codes_named(
        "type O = { a: string; b: number };\ndeclare function f({ a: renamed }: O): void;",
        "lib.d.ts",
    );
    assert!(
        !codes.contains(&TS2842_UNUSED_RENAMING),
        "a `declare function` in a `.d.ts` file is exempt; got {codes:?}"
    );
}

#[test]
fn type_literal_property_function_type_rename_in_declaration_file_reports_nothing() {
    let codes = check_source_codes_named(
        "type O = { a: string; b: number };\ntype T = { f: ({ a: renamed }: O) => void };",
        "lib.d.ts",
    );
    assert!(
        !codes.contains(&TS2842_UNUSED_RENAMING),
        "a function-typed type-literal property is exempt in a `.d.ts` file; got {codes:?}"
    );
}

#[test]
fn non_identifier_property_rename_in_declaration_file_reports_nothing() {
    let codes = check_source_codes_named("type F = ({ \"a\": renamed }) => void;", "lib.d.ts");
    assert!(
        !codes.contains(&TS2842_UNUSED_RENAMING),
        "the declaration-file exemption applies regardless of property-name kind; got {codes:?}"
    );
}

#[test]
fn bare_function_type_rename_in_ordinary_ts_file_still_reports_ts2842() {
    // Adjacent negative case: the exact same source in a `.ts` file (not
    // `.d.ts`) must still report — the exemption is declaration-file-scoped,
    // not a blanket change to the diagnostic.
    let codes = check_source_codes_named(
        "type O = { a: string; b: number };\ntype F = ({ a: renamed }: O) => void;",
        "plain.ts",
    );
    assert!(
        codes.contains(&TS2842_UNUSED_RENAMING),
        "a plain `.ts` file keeps reporting TS2842; got {codes:?}"
    );
}
