//! Regression tests for #16252 — TS2300 fires on every declaration of a
//! duplicated interface / type-literal member, whatever spelling each name used.
//!
//! `tsc` decides duplication from the *member key* a declaration produces, then
//! reports `Duplicate identifier` at every declaration in the group. tsz gated
//! TS2300 behind "every name in the group was written syntactically", so a group
//! containing any computed name — `["abc"]`, `[c0]`, `[E.A]`, `[s]` — reported
//! only its consequences (TS2717 / TS2687) and never the duplication itself.
//! Well-known symbol members were skipped a step earlier still, losing all three
//! diagnostics.
//!
//! The converse defect lived in the same scan: a computed name that resolves to
//! *no* member key fell back to the expression's source text, so `[k]` where
//! `k: string` invented a key and two such declarations reported a duplicate
//! `tsc` does not.
//!
//! Every binder name below is distinct from every other so nothing can key on an
//! identifier string, and each shape appears in both an `interface` and a `type`
//! literal because the two containers run different scans.
//!
//! Expectations pinned against `typescript@7.0.2`
//! (`--noEmit --strict --lib es2022 --target es2022`).

use crate::context::CheckerOptions;
use crate::test_utils::{
    check_source_strict_messages, check_source_with_libs_code_messages, load_default_lib_files,
};

const DUPLICATE_IDENTIFIER: u32 = 2300;
const IDENTICAL_MODIFIERS: u32 = 2687;
const SUBSEQUENT_PROPERTY: u32 = 2717;

fn codes(source: &str) -> Vec<u32> {
    let mut found: Vec<u32> = check_source_strict_messages(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect();
    found.sort_unstable();
    found
}

fn count_of(source: &str, code: u32) -> usize {
    codes(source).into_iter().filter(|c| *c == code).count()
}

fn messages_for(source: &str, code: u32) -> Vec<String> {
    check_source_strict_messages(source)
        .into_iter()
        .filter(|(c, _)| *c == code)
        .map(|(_, message)| message)
        .collect()
}

/// Both declarations of a duplicated member carry TS2300 — not just the
/// redeclaration. A fix that emitted one diagnostic would pass a bare
/// "is TS2300 present" assertion, so the count is the assertion.
fn assert_duplicate_on_both(source: &str, expected_name: &str) {
    let messages = messages_for(source, DUPLICATE_IDENTIFIER);
    assert_eq!(
        messages.len(),
        2,
        "expected TS2300 on both declarations for source:\n{source}\ngot {messages:?}"
    );
    for message in &messages {
        assert!(
            message.contains(expected_name),
            "TS2300 should name the member {expected_name}; got {message:?}"
        );
    }
}

fn assert_no_duplicate(source: &str) {
    assert_eq!(
        count_of(source, DUPLICATE_IDENTIFIER),
        0,
        "expected no TS2300 for source:\n{source}\ngot {:?}",
        messages_for(source, DUPLICATE_IDENTIFIER)
    );
}

// ---------------------------------------------------------------------------
// A computed literal name duplicates a plainly written one
// ---------------------------------------------------------------------------

#[test]
fn interface_identifier_and_computed_string_are_duplicates() {
    assert_duplicate_on_both(
        "export {};\ninterface Alpha { zeta: number; [\"zeta\"]: string; }\n",
        "'zeta'",
    );
}

#[test]
fn type_literal_identifier_and_computed_string_are_duplicates() {
    assert_duplicate_on_both(
        "export {};\ntype Beta = { kappa: number; [\"kappa\"]: string };\n",
        "'kappa'",
    );
}

#[test]
fn interface_two_computed_strings_are_duplicates() {
    assert_duplicate_on_both(
        "export {};\ninterface Gamma { [\"omega\"]: number; [\"omega\"]: string; }\n",
        "'[\"omega\"]'",
    );
}

#[test]
fn type_literal_two_computed_strings_are_duplicates() {
    assert_duplicate_on_both(
        "export {};\ntype Delta = { [\"psi\"]: number; [\"psi\"]: string };\n",
        "'[\"psi\"]'",
    );
}

// ---------------------------------------------------------------------------
// Names that only a resolved expression can produce
// ---------------------------------------------------------------------------

#[test]
fn interface_const_reference_pair_are_duplicates() {
    assert_duplicate_on_both(
        "export {};\ndeclare const theta: \"lambda\";\ninterface Epsilon { [theta]: number; [theta]: string; }\n",
        "'[theta]'",
    );
}

#[test]
fn interface_const_references_with_different_spellings_but_one_key_are_duplicates() {
    // `"1"` and `1` are two spellings of the member key `1`. This is the exact
    // shape a stale comment in the checker claimed `tsc` reports as TS2717 only;
    // the oracle reports TS2300 on both declarations as well.
    assert_duplicate_on_both(
        "export {};\ndeclare const iota: \"1\";\ndeclare const nu: 1;\ninterface Zeta { [iota]: number; [nu]: string; }\n",
        "'[iota]'",
    );
}

#[test]
fn interface_enum_member_pair_are_duplicates() {
    assert_duplicate_on_both(
        "export {};\nenum Rho { Sigma = \"tau\" }\ninterface Eta { [Rho.Sigma]: number; [Rho.Sigma]: string; }\n",
        "'[Rho.Sigma]'",
    );
}

#[test]
fn interface_unique_symbol_pair_are_duplicates() {
    assert_duplicate_on_both(
        "export {};\ndeclare const upsilon: unique symbol;\ninterface Chi { [upsilon]: number; [upsilon]: string; }\n",
        "'[upsilon]'",
    );
}

// ---------------------------------------------------------------------------
// Well-known symbols — the whole group used to be skipped, losing TS2300,
// TS2717 and TS2687 together. These need the real lib files: `Symbol.iterator`
// only resolves to a member key when `SymbolConstructor` is in scope, and a
// well-known-symbol name whose expression types as ERROR is deliberately
// discarded rather than turned into a phantom member.
// ---------------------------------------------------------------------------

fn lib_messages(source: &str) -> Vec<(u32, String)> {
    let libs = load_default_lib_files();
    check_source_with_libs_code_messages(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
        &libs,
    )
}

fn lib_messages_for(source: &str, code: u32) -> Vec<String> {
    lib_messages(source)
        .into_iter()
        .filter(|(c, _)| *c == code)
        .map(|(_, message)| message)
        .collect()
}

#[test]
fn interface_well_known_symbol_pair_are_duplicates() {
    let source =
        "export {};\ninterface Phi { [Symbol.iterator]: number; [Symbol.iterator]: string; }\n";
    let duplicates = lib_messages_for(source, DUPLICATE_IDENTIFIER);
    assert_eq!(
        duplicates.len(),
        2,
        "expected TS2300 on both declarations; got {duplicates:?}"
    );
    for message in &duplicates {
        assert!(
            message.contains("'[Symbol.iterator]'"),
            "TS2300 should name the member '[Symbol.iterator]'; got {message:?}"
        );
    }
    assert_eq!(
        lib_messages_for(source, SUBSEQUENT_PROPERTY).len(),
        1,
        "the redeclaration should still carry TS2717"
    );
}

#[test]
fn interface_well_known_symbol_modifier_disagreement_reports_both_codes() {
    let source = "export {};\ninterface Omicron { readonly [Symbol.hasInstance]: number; [Symbol.hasInstance]: number; }\n";
    assert_eq!(
        lib_messages_for(source, DUPLICATE_IDENTIFIER).len(),
        2,
        "expected TS2300 on both declarations"
    );
    let modifiers = lib_messages_for(source, IDENTICAL_MODIFIERS);
    assert_eq!(modifiers.len(), 2, "TS2687 should flag both declarations");
    for message in &modifiers {
        assert!(
            message.contains("'[Symbol.hasInstance]'"),
            "TS2687 should name the member '[Symbol.hasInstance]'; got {message:?}"
        );
    }
}

#[test]
fn distinct_well_known_symbols_are_not_duplicates_with_libs() {
    let source = "export {};\ninterface Psi { [Symbol.iterator]: number; [Symbol.asyncIterator]: string; [Symbol.hasInstance]: boolean; }\n";
    assert!(
        lib_messages_for(source, DUPLICATE_IDENTIFIER).is_empty(),
        "distinct well-known symbols name distinct members"
    );
}

// ---------------------------------------------------------------------------
// Renderer asymmetry: TS2300 names the member by the FIRST declaration's
// spelling, TS2717 by the subsequent one's
// ---------------------------------------------------------------------------

#[test]
fn duplicate_identifier_uses_the_first_declarations_spelling() {
    // `["mu"]` is written first, so both TS2300s say `["mu"]` even though the
    // second declaration spells the same member `mu`.
    assert_duplicate_on_both(
        "export {};\ninterface Pi { [\"mu\"]: number; mu: string; }\n",
        "'[\"mu\"]'",
    );
}

#[test]
fn subsequent_property_uses_the_redeclarations_spelling() {
    let messages = messages_for(
        "export {};\ninterface Xi { [\"xi\"]: number; xi: string; }\n",
        SUBSEQUENT_PROPERTY,
    );
    assert_eq!(messages.len(), 1, "got {messages:?}");
    assert!(
        messages[0].contains("'xi'") && !messages[0].contains("'[\"xi\"]'"),
        "TS2717 should name the member by the redeclaration's spelling; got {:?}",
        messages[0]
    );
}

#[test]
fn identical_modifiers_uses_the_first_declarations_spelling() {
    let messages = messages_for(
        "export {};\ninterface Nu { readonly [\"rho\"]: number; rho: number; }\n",
        IDENTICAL_MODIFIERS,
    );
    assert_eq!(messages.len(), 2, "got {messages:?}");
    for message in &messages {
        assert!(
            message.contains("'[\"rho\"]'"),
            "TS2687 should name the member by the first declaration's spelling; got {message:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Three declarations: every one of them is flagged, once
// ---------------------------------------------------------------------------

#[test]
fn three_declarations_each_get_exactly_one_duplicate_identifier() {
    let source =
        "export {};\ninterface Tau { [\"beta\"]: number; beta: string; [\"beta\"]: boolean; }\n";
    assert_eq!(
        count_of(source, DUPLICATE_IDENTIFIER),
        3,
        "each declaration in the group is flagged exactly once"
    );
    assert_eq!(
        count_of(source, SUBSEQUENT_PROPERTY),
        2,
        "TS2717 is reported on the two redeclarations only"
    );
}

// ---------------------------------------------------------------------------
// Negative controls — distinct members must stay clean. Each of these resolves
// through the same computed-name machinery as a positive row above, so a fix
// that grouped by source text rather than by member key would trip them.
// ---------------------------------------------------------------------------

#[test]
fn distinct_well_known_symbols_are_not_duplicates() {
    assert_no_duplicate(
        "export {};\ninterface Psi { [Symbol.iterator]: number; [Symbol.asyncIterator]: string; [Symbol.hasInstance]: boolean; }\n",
    );
}

#[test]
fn distinct_unique_symbols_are_not_duplicates() {
    assert_no_duplicate(
        "export {};\ndeclare const alphaSym: unique symbol;\ndeclare const betaSym: unique symbol;\ninterface Omega { [alphaSym]: number; [betaSym]: string; }\n",
    );
}

#[test]
fn distinct_const_references_are_not_duplicates() {
    assert_no_duplicate(
        "export {};\ndeclare const first: \"one\";\ndeclare const second: \"two\";\ninterface Kappa { [first]: number; [second]: string; }\n",
    );
}

#[test]
fn distinct_enum_members_are_not_duplicates() {
    assert_no_duplicate(
        "export {};\nenum Colour { Red = \"red\", Blue = \"blue\" }\ninterface Lambda { [Colour.Red]: number; [Colour.Blue]: string; }\n",
    );
}

#[test]
fn method_overloads_are_not_duplicates() {
    assert_no_duplicate("export {};\ninterface Mu { sigma(): void; sigma(x: number): void; }\n");
}

// ---------------------------------------------------------------------------
// A computed name that names no member. `[widening]` where `widening: string`
// is not late-bindable, so it declares nothing and cannot duplicate anything —
// including a second declaration spelled identically. tsz used to fall back to
// the expression's source text here and report a duplicate `tsc` does not.
// ---------------------------------------------------------------------------

#[test]
fn interface_non_literal_computed_names_are_never_duplicates() {
    let source = "export {};\ndeclare const widening: string;\ninterface Upsilon { [widening]: number; [widening]: string; }\n";
    assert_no_duplicate(source);
    assert_eq!(
        count_of(source, SUBSEQUENT_PROPERTY),
        0,
        "a name that declares no member cannot be a subsequent declaration of one"
    );
}

#[test]
fn type_literal_non_literal_computed_names_are_never_duplicates() {
    assert_no_duplicate(
        "export {};\ndeclare const spreading: string;\ntype Iota = { [spreading]: number; [spreading]: string };\n",
    );
}

#[test]
fn a_non_literal_computed_name_does_not_collide_with_its_own_identifier() {
    // The syntactic fallback keyed `[gamma]` under `gamma`, which collided with
    // a sibling member literally named `gamma`.
    assert_no_duplicate(
        "export {};\ndeclare const gamma: string;\ninterface Theta { gamma: number; [gamma]: string; }\n",
    );
}

// ---------------------------------------------------------------------------
// Declaration merging across two blocks is a different rule: `tsc` reports
// TS2717 there and no TS2300. Un-gating the single-block scan must not leak
// into it.
// ---------------------------------------------------------------------------

#[test]
fn members_merged_across_two_interface_blocks_are_not_duplicate_identifiers() {
    let source = "export {};\ninterface Delta { [\"epsilon\"]: number; }\ninterface Delta { [\"epsilon\"]: string; }\n";
    assert_no_duplicate(source);
    assert_eq!(
        count_of(source, SUBSEQUENT_PROPERTY),
        1,
        "the merged redeclaration still carries TS2717"
    );
}
