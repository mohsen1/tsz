//! Tests for indexed access `(A & B)[K]` where the intersection member that
//! contributes `K` is an *unevaluated mapped-type alias application* — a user
//! alias `M<…> = { [P in K]: V }` (the lib `Record` is one instance of this
//! shape), possibly behind a wrapper alias.
//!
//! Structural rule: `(A & B)[K]` must resolve `K` against the *evaluated* form
//! of every intersection member, including mapped-type alias applications, the
//! same materialization the `keyof` and assignability paths already perform.
//! Before this fix, such a member was left opaque by the property-existence
//! check (its checker-owned `DefId` could not be expanded by the bare
//! property-access resolver), so the key was falsely reported missing (TS2339)
//! and the indexed access collapsed to a deferred/never type.
//!
//! These tests use a user-defined mapped alias rather than the lib `Record`
//! because the checker unit harness runs without lib definitions; the rule is
//! the same for any mapped-type alias (issue #9648 explicitly notes it is not
//! `Record`-specific). Regression coverage for issue #9648.

use tsz_checker::test_utils::check_source_diagnostics;

fn count(diags: &[tsz_checker::diagnostics::Diagnostic], code: u32) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

fn assert_no_2339(source: &str) {
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2339),
        0,
        "expected no TS2339; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

const REC: &str = "type Rec<K extends string, V> = { [P in K]: V };\n";

/// `({} & Rec<"foo", number>)["foo"]` — empty sibling member.
#[test]
fn mapped_alias_member_empty_sibling_resolves_key() {
    assert_no_2339(&format!(
        "{REC}type A = ({{}} & Rec<\"foo\", number>)[\"foo\"];\nconst a: A = 42;\n"
    ));
}

/// Order independence: `(Rec<"foo", number> & {})["foo"]`.
#[test]
fn mapped_alias_member_first_resolves_key() {
    assert_no_2339(&format!(
        "{REC}type A = (Rec<\"foo\", number> & {{}})[\"foo\"];\nconst a: A = 42;\n"
    ));
}

/// Non-empty sibling literal member: `({ bar: string } & Rec<"foo", number>)["foo"]`.
#[test]
fn mapped_alias_member_nonempty_sibling_resolves_key() {
    assert_no_2339(&format!(
        "{REC}type A = ({{ bar: string }} & Rec<\"foo\", number>)[\"foo\"];\nconst a: A = 42;\n"
    ));
}

/// The contributed key resolves to the correct value type, not `never`/deferred:
/// a wrong assignment to `(... & Rec<"foo", number>)["foo"]` must emit TS2322.
#[test]
fn mapped_alias_member_key_resolves_to_value_type_not_never() {
    let diags = check_source_diagnostics(&format!(
        "{REC}type A = ({{ bar: string }} & Rec<\"foo\", number>)[\"foo\"];\nconst a: A = \"not a number\";\n"
    ));
    assert_eq!(count(&diags, 2339), 0, "no false TS2339");
    assert_eq!(
        count(&diags, 2322),
        1,
        "string assigned to a `number` member must emit TS2322; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Anti-hardcoding (§25): the iteration variable and type-parameter names must
/// not matter. Same shape with `Key`/`Val`/`Q` instead of `K`/`V`/`P`.
#[test]
fn mapped_alias_member_renamed_params_resolves_key() {
    assert_no_2339(
        r#"
type Rec2<Key extends string, Val> = { [Q in Key]: Val };
type A = ({ bar: string } & Rec2<"foo", number>)["foo"];
const a: A = 42;
"#,
    );
}

/// Wrapper alias around the mapped application: `Wrap<Rec<"foo", number>>`.
#[test]
fn wrapper_alias_around_mapped_application_resolves_key() {
    assert_no_2339(&format!(
        "{REC}type Wrap<T> = T;\ntype A = ({{ bar: string }} & Wrap<Rec<\"foo\", number>>)[\"foo\"];\nconst a: A = 42;\n"
    ));
}

/// `keyof` / indexed-access internal consistency: both the key set and the
/// per-key value type must agree on the mapped-alias member.
#[test]
fn keyof_and_indexed_access_agree_on_mapped_alias_member() {
    assert_no_2339(&format!(
        "{REC}type X = {{ bar: string }} & Rec<\"foo\", number>;\ntype V = X[\"foo\"];\nconst v: V = 5;\ntype K = keyof X;\nconst k: K = \"foo\";\n"
    ));
}

/// Sibling literal-prop access must keep working (control).
#[test]
fn sibling_literal_property_access_still_resolves() {
    assert_no_2339(&format!(
        "{REC}type A = ({{ bar: string }} & Rec<\"foo\", number>)[\"bar\"];\nconst a: A = \"a string\";\n"
    ));
}

/// Inline mapped member must keep working (control, never went through the
/// alias-application path).
#[test]
fn inline_mapped_member_still_resolves() {
    assert_no_2339(
        r#"
type A = ({} & { [K in "foo"]: number })["foo"];
const a: A = 42;
"#,
    );
}

/// Negative/fallback: a genuinely absent key must STILL be reported missing —
/// materializing the members must not silently accept unknown keys.
#[test]
fn genuinely_missing_key_still_reports_2339() {
    let diags = check_source_diagnostics(&format!(
        "{REC}type A = ({{ bar: string }} & Rec<\"foo\", number>)[\"baz\"];\ndeclare const a: A;\n"
    ));
    assert!(
        count(&diags, 2339) >= 1,
        "missing key `baz` must still emit TS2339; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}
