//! Type-member (interface / type-literal) modifier grammar parity with tsc.
//!
//! Positions that `checkGrammarModifiers` (or the parser's own member-shape
//! dispatch) rejects but tsz previously did not, verified against
//! `typescript@7.0.2`:
//!   * `export` / `in` / `out` as a modifier on a type member → TS1070
//!     (`'{0}' modifier cannot appear on a type member.`), anchored at the
//!     modifier. tsz previously mis-parsed `export` to TS1005 and dropped
//!     `in` / `out` silently.
//!   * `readonly` on a method or construct signature → TS1024
//!     (`'readonly' modifier can only appear on a property declaration or index
//!     signature.`), anchored at `readonly`. tsz previously stayed silent.
//!   * A run of one or more `private`/`protected`/`public`/`static`/
//!     `accessor`/`export`/`readonly` modifiers directly followed by a
//!     `get`/`set` accessor → one TS1131 per modifier in the run (each
//!     anchored at its own token), then a clean (modifier-less) accessor. No
//!     modifier at all may precede an accessor signature in an interface or
//!     type literal — unlike a plain method, this fails to parse as any
//!     member at all in tsc, not a semantic TS1024/TS1070. tsz previously
//!     mis-parsed `get`/`set` as the property name (single `readonly`, TS1005)
//!     or reported the uniform semantic TS1070 (every other modifier in the
//!     set, single or stacked).
//!   * `readonly` and a second modifier together on a type member:
//!     `checkGrammarModifiers` checks each leading modifier in SOURCE ORDER and
//!     reports (and stops at) the first one invalid for the member's own kind
//!     — `readonly` before any other illegal modifier (`async`, or any of
//!     `private`/`protected`/`public`/`static`/`accessor`/`override`/
//!     `abstract`/`declare`/`export`/`in`/`out`) on a *method* reports TS1024
//!     only (not tsz's previous bogus TS1005/mis-parse), and on a *property*
//!     reports TS1070 at the second modifier (readonly is legal on a
//!     property, so it is not the offender there). An earlier illegal
//!     modifier (`static`) suppresses a would-be-duplicate TS1070/TS1024 from
//!     a trailing `readonly`/second-modifier (tsz previously double-reported
//!     for `async`; the other nine modifiers previously mis-parsed instead of
//!     reporting at all, since only `async` had this second-modifier
//!     lookahead).
//!   * A second (or later) `readonly` on a type member → TS1030 (`'readonly'
//!     modifier already seen.`), anchored at the repeated `readonly` — a
//!     duplicate-modifier check tsz's type-member parser previously did not
//!     implement at all (the second `readonly` mis-parsed as a failed
//!     property-name lookahead, `NodeIndex::NONE`). Follows the same
//!     source-order-wins rule as the row above: an illegal modifier
//!     encountered before the duplicate `readonly` still wins (`readonly
//!     static readonly x` is TS1070 at `static`), a duplicate `readonly`
//!     encountered first wins over a later illegal modifier (`readonly
//!     readonly static x` is TS1030), and `readonly` on a method reports the
//!     pre-existing TS1024 regardless of how many `readonly`s lead it, since
//!     the method-kind check on the very first `readonly` fires before a
//!     second one is ever inspected.
//!
//! `readonly` on a property / index signature stays legal, and `export` / `in`
//! / `out` used as a member *name* (`export: T`, `export(): void`) stay clean —
//! matching tsc. `import` / `const` / `default` are not type-member modifiers in
//! tsc (they take a parser-recovery path) and are intentionally not covered.
//!
//! The "hard" cascade modifiers `async`/`declare`/`abstract`/`override` before
//! an interface/type-literal accessor also fail to parse in tsc, but with a
//! different recovery shape — one TS1131 per modifier, then the accessor's own
//! tail re-parses as top-level statements (TS1434/TS1005/TS1128) rather than a
//! bare-accessor recovery. That family is now handled and lives, with full
//! fingerprint parity, in `type_member_hard_modifier_accessor_cascade_tests`
//! (see `look_ahead_hard_modifier_run_before_accessor`). `in`/`out` remain on
//! the pre-existing semantic TS1070 path: `in` is a reserved operator whose
//! statement re-parse differs, and both carry variance-position idiosyncrasies.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::position::LineMap;

/// `(code, line, column, message)` fingerprints, 1-based line/column, in the
/// order the parser reported them.
fn fingerprints(source: &str) -> Vec<(u32, u32, u32, String)> {
    let (parser, _root) = parse_source(source);
    let line_map = LineMap::build(source);
    parser
        .get_diagnostics()
        .iter()
        .map(|diag| {
            let pos = line_map.offset_to_position(diag.start, source);
            (
                diag.code,
                pos.line + 1,
                pos.character + 1,
                diag.message.clone(),
            )
        })
        .collect()
}

fn codes(source: &str) -> Vec<u32> {
    let (parser, _root) = parse_source(source);
    parser.get_diagnostics().iter().map(|d| d.code).collect()
}

const TS1070: u32 = diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER;
const TS1071: u32 = diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_AN_INDEX_SIGNATURE;
const TS1024: u32 =
    diagnostic_codes::READONLY_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION_OR_INDEX_SIGNATURE;
const TS1131: u32 = diagnostic_codes::PROPERTY_OR_SIGNATURE_EXPECTED;
const TS1030: u32 = diagnostic_codes::MODIFIER_ALREADY_SEEN;

// ---------------------------------------------------------------------------
// TS1070: export / in / out modifier on a type member
// ---------------------------------------------------------------------------

#[test]
fn export_modifier_on_interface_property_reports_ts1070() {
    // `export` starts at column 15 (`interface I { `).
    assert_eq!(
        fingerprints("interface I { export x: number; }"),
        vec![(
            TS1070,
            1,
            15,
            "'export' modifier cannot appear on a type member.".to_string()
        )],
    );
}

#[test]
fn export_modifier_on_interface_method_reports_ts1070() {
    assert_eq!(
        fingerprints("interface I { export m(): void; }"),
        vec![(
            TS1070,
            1,
            15,
            "'export' modifier cannot appear on a type member.".to_string()
        )],
    );
}

#[test]
fn export_modifier_on_type_literal_property_reports_ts1070() {
    // `type T = { ` → `export` at column 12.
    assert_eq!(
        fingerprints("type T = { export x: number; };"),
        vec![(
            TS1070,
            1,
            12,
            "'export' modifier cannot appear on a type member.".to_string()
        )],
    );
}

#[test]
fn in_modifier_on_interface_property_reports_ts1070() {
    assert_eq!(
        fingerprints("interface Foo { in bar: number; }"),
        vec![(
            TS1070,
            1,
            17,
            "'in' modifier cannot appear on a type member.".to_string()
        )],
    );
}

#[test]
fn out_modifier_on_interface_property_reports_ts1070() {
    assert_eq!(
        fingerprints("interface Foo { out bar: number; }"),
        vec![(
            TS1070,
            1,
            17,
            "'out' modifier cannot appear on a type member.".to_string()
        )],
    );
}

#[test]
fn export_modifier_anchors_at_the_keyword_with_leading_whitespace() {
    let source = "interface I {\n    export foo(): void;\n}";
    assert_eq!(
        fingerprints(source),
        vec![(
            TS1070,
            2,
            5,
            "'export' modifier cannot appear on a type member.".to_string()
        )],
    );
}

#[test]
fn export_before_legal_readonly_reports_only_ts1070() {
    // `readonly` after `export` is legal on the property, so only the `export`
    // is rejected — a single diagnostic, no TS1024.
    assert_eq!(
        codes("interface I { export readonly x: number; }"),
        vec![TS1070],
    );
}

// ---------------------------------------------------------------------------
// TS1024: readonly on a method / construct signature
// ---------------------------------------------------------------------------

#[test]
fn readonly_on_interface_method_reports_ts1024() {
    assert_eq!(
        fingerprints("interface I { readonly m(): void; }"),
        vec![(
            TS1024,
            1,
            15,
            "'readonly' modifier can only appear on a property declaration or index signature."
                .to_string()
        )],
    );
}

#[test]
fn readonly_on_interface_generic_method_reports_ts1024() {
    assert_eq!(
        fingerprints("interface I { readonly m<T>(): void; }"),
        vec![(
            TS1024,
            1,
            15,
            "'readonly' modifier can only appear on a property declaration or index signature."
                .to_string()
        )],
    );
}

#[test]
fn readonly_on_construct_signature_reports_ts1024() {
    assert_eq!(
        fingerprints("interface I { readonly new (): void; }"),
        vec![(
            TS1024,
            1,
            15,
            "'readonly' modifier can only appear on a property declaration or index signature."
                .to_string()
        )],
    );
}

#[test]
fn readonly_on_type_literal_method_reports_ts1024() {
    assert_eq!(
        fingerprints("type Shape = { readonly compute(): number; };"),
        vec![(
            TS1024,
            1,
            16,
            "'readonly' modifier can only appear on a property declaration or index signature."
                .to_string()
        )],
    );
}

// ---------------------------------------------------------------------------
// TS1131: readonly directly followed by a get/set accessor
// ---------------------------------------------------------------------------

#[test]
fn readonly_before_get_accessor_reports_ts1131_and_recovers_the_accessor() {
    assert_eq!(
        fingerprints("interface I { readonly get x(): number; }"),
        vec![(TS1131, 1, 15, "Property or signature expected.".to_string())],
    );
}

#[test]
fn readonly_before_set_accessor_reports_ts1131_and_recovers_the_accessor() {
    assert_eq!(
        fingerprints("interface I { readonly set x(v: number); }"),
        vec![(TS1131, 1, 15, "Property or signature expected.".to_string())],
    );
}

#[test]
fn readonly_before_accessor_on_type_literal_reports_ts1131() {
    // `type T = { ` -> `readonly` at column 12.
    assert_eq!(
        fingerprints("type T = { readonly get x(): number; };"),
        vec![(TS1131, 1, 12, "Property or signature expected.".to_string())],
    );
    assert_eq!(
        fingerprints("type T = { readonly set x(v: number); };"),
        vec![(TS1131, 1, 12, "Property or signature expected.".to_string())],
    );
}

#[test]
fn readonly_before_accessor_renamed_binder_and_asi() {
    // Renamed property/container binders, and no explicit semicolons (ASI).
    let source =
        "interface Widget {\n  readonly get value(): string\n  readonly set value(v: string)\n}";
    assert_eq!(
        fingerprints(source),
        vec![
            (TS1131, 2, 3, "Property or signature expected.".to_string()),
            (TS1131, 3, 3, "Property or signature expected.".to_string()),
        ],
    );
}

#[test]
fn readonly_before_accessor_no_return_type_reports_ts1131() {
    assert_eq!(
        fingerprints("interface I { readonly get x() }"),
        vec![(TS1131, 1, 15, "Property or signature expected.".to_string())],
    );
}

#[test]
fn readonly_before_accessor_still_recovers_following_members() {
    // The retry that finds the bare accessor must not swallow later members.
    assert_eq!(
        codes("interface I { readonly get x(): number; y: string; }"),
        vec![TS1131],
    );
}

#[test]
fn readonly_before_method_named_get_stays_ts1024_not_ts1131() {
    // `get` immediately followed by `(` is a method *named* `get`, not an
    // accessor — this is the already-fixed TS1024 case (#16789/#16795), and
    // must not be reclassified as TS1131 by the new accessor lookahead.
    assert_eq!(
        fingerprints("interface I { readonly get(): number; }"),
        vec![(
            TS1024,
            1,
            15,
            "'readonly' modifier can only appear on a property declaration or index signature."
                .to_string()
        )],
    );
}

#[test]
fn readonly_get_as_property_name_stays_clean() {
    // `get` used as an ordinary property name (`get: number`), not the
    // accessor keyword, is legal with `readonly`.
    assert!(codes("interface I { readonly get: number; }").is_empty());
}

#[test]
fn get_accessor_without_readonly_stays_clean() {
    assert!(codes("interface I { get x(): number; set x(v: number); }").is_empty());
}

#[test]
fn readonly_before_accessor_across_a_line_break_is_unaffected() {
    // A line break between `readonly` and `get` takes tsc down a completely
    // different (already-divergent, out-of-scope) ASI path; the new
    // same-line-only lookahead must not fire here.
    let with_break = "interface I {\n  readonly\n  get x(): number\n}";
    let without_break = "interface I {\n  readonly get x(): number\n}";
    assert_ne!(codes(with_break), codes(without_break));
}

// ---------------------------------------------------------------------------
// Negative controls: legal shapes stay clean, member-name uses stay clean
// ---------------------------------------------------------------------------

#[test]
fn readonly_property_and_index_signature_stay_clean() {
    assert!(codes("interface I { readonly x: number; }").is_empty());
    assert!(codes("interface I { readonly [k: string]: number; }").is_empty());
    assert!(codes("type T = { readonly y: string; };").is_empty());
}

#[test]
fn export_in_out_as_member_names_stay_clean() {
    // Followed by `:` → property name; followed by `(` → method name.
    assert!(codes("interface I { export: number; }").is_empty());
    assert!(codes("interface I { export(): void; }").is_empty());
    assert!(codes("interface I { in: number; }").is_empty());
    assert!(codes("interface I { out: number; }").is_empty());
    assert!(codes("interface I { in(): void; }").is_empty());
}

#[test]
fn method_named_readonly_stays_clean() {
    // `readonly` immediately followed by `(` is a method *named* `readonly`,
    // not a modifier — no TS1024.
    assert!(codes("interface I { readonly(): void; }").is_empty());
}

#[test]
fn plain_members_stay_clean() {
    assert!(codes("interface I { m(): void; x: number; }").is_empty());
}

#[test]
fn previously_covered_modifiers_still_report_ts1070() {
    // Guard against a regression in the shared modifier-error branch.
    for modifier in [
        "public",
        "private",
        "protected",
        "static",
        "abstract",
        "declare",
    ] {
        let source = format!("interface I {{ {modifier} x: number; }}");
        assert_eq!(codes(&source), vec![TS1070], "modifier `{modifier}`");
    }
}

// ---------------------------------------------------------------------------
// TS1071: illegal modifier on an index signature
// ---------------------------------------------------------------------------

#[test]
fn export_modifier_on_index_signature_reports_ts1071() {
    // `export` at column 17 (`interface Bag { `).
    assert_eq!(
        fingerprints("interface Bag { export [key: string]: number; }"),
        vec![(
            TS1071,
            1,
            17,
            "'export' modifier cannot appear on an index signature.".to_string(),
        )],
    );
}

// ---------------------------------------------------------------------------
// Multi-modifier runs: `tsc` reports once, anchored on and naming the FIRST
// modifier, then recovers the underlying member/index signature cleanly.
// ---------------------------------------------------------------------------

#[test]
fn two_modifiers_report_once_naming_the_first() {
    // `public static value` — one TS1070 on `public` (column 15), no cascade
    // on the trailing `static`.
    assert_eq!(
        fingerprints("interface I { public static value: number; }"),
        vec![(
            TS1070,
            1,
            15,
            "'public' modifier cannot appear on a type member.".to_string(),
        )],
    );
}

#[test]
fn two_modifiers_reordered_names_the_first() {
    // `static public value` — the first modifier is now `static`.
    assert_eq!(
        fingerprints("interface I { static public value: number; }"),
        vec![(
            TS1070,
            1,
            15,
            "'static' modifier cannot appear on a type member.".to_string(),
        )],
    );
}

#[test]
fn export_leading_a_modifier_run_names_export() {
    let source = "interface I { export static value: number; }";
    assert_eq!(codes(source), vec![TS1070]);
    assert!(fingerprints(source)[0].3.starts_with("'export'"));
}

#[test]
fn three_modifiers_report_once_and_keep_the_readonly_member() {
    // `public static readonly x` — one TS1070 on `public`; the trailing
    // `readonly x` still parses as a member (no cascade).
    assert_eq!(
        codes("interface I { public static readonly x: number; }"),
        vec![TS1070],
    );
}

#[test]
fn modifier_run_before_index_signature_is_a_single_ts1071() {
    assert_eq!(
        fingerprints("interface I { public static [key: string]: number; }"),
        vec![(
            TS1071,
            1,
            15,
            "'public' modifier cannot appear on an index signature.".to_string(),
        )],
    );
}

#[test]
fn declare_export_type_literal_run_names_declare() {
    let source = "type T = { declare export field: number; };";
    assert_eq!(codes(source), vec![TS1070]);
    assert!(fingerprints(source)[0].3.starts_with("'declare'"));
}

#[test]
fn illegal_modifier_member_does_not_swallow_following_members() {
    // Exactly one diagnostic; the following `y` member is not lost to a cascade.
    assert_eq!(
        codes("interface I { export x: number; y: string; }"),
        vec![TS1070],
    );
}

#[test]
fn multi_modifier_run_is_not_keyed_to_a_binder_name() {
    // Structural, not tied to any identifier spelling.
    for src in [
        "interface Alpha { static public beta: number; }",
        "type Gamma = { public static delta: string; };",
    ] {
        assert_eq!(codes(src), vec![TS1070], "structural rule: {src}");
    }
}

// ---------------------------------------------------------------------------
// Any "clean" modifier (not just `readonly`) directly before an accessor:
// one TS1131, anchored at the modifier, then a clean accessor retry.
// ---------------------------------------------------------------------------

#[test]
fn clean_modifiers_before_accessor_report_ts1131_not_ts1070() {
    for modifier in [
        "private",
        "protected",
        "public",
        "static",
        "accessor",
        "export",
    ] {
        let source = format!("interface I {{ {modifier} get x(): number; }}");
        assert_eq!(codes(&source), vec![TS1131], "modifier `{modifier}`");

        let source = format!("interface I {{ {modifier} set x(v: number); }}");
        assert_eq!(codes(&source), vec![TS1131], "modifier `{modifier}` (set)");
    }
}

#[test]
fn clean_modifier_before_accessor_on_type_literal_reports_ts1131() {
    assert_eq!(codes("type T = { static get x(): number; };"), vec![TS1131],);
}

#[test]
fn clean_modifier_before_accessor_recovers_a_working_accessor() {
    // Exactly one diagnostic; the accessor itself is not lost to a cascade,
    // and a following member still parses.
    assert_eq!(
        codes("interface I { static get x(): number; y: string; }"),
        vec![TS1131],
    );
}

#[test]
fn stacked_clean_modifiers_before_accessor_report_one_ts1131_each() {
    // tsc reports a SEPARATE TS1131 per modifier in the run — unlike the
    // TS1070 family, this one does not collapse to a single diagnostic naming
    // only the first modifier.
    assert_eq!(
        fingerprints("interface I { public static get x(): number; }"),
        vec![
            (TS1131, 1, 15, "Property or signature expected.".to_string(),),
            (TS1131, 1, 22, "Property or signature expected.".to_string(),),
        ],
    );
}

#[test]
fn three_stacked_clean_modifiers_before_accessor_report_three_ts1131() {
    assert_eq!(
        codes("interface I { export static readonly get x(): number; }"),
        vec![TS1131, TS1131, TS1131],
    );
}

#[test]
fn readonly_mixed_into_a_clean_modifier_run_before_accessor_still_reports_each() {
    // `readonly` participates in the run like any other clean modifier when
    // it is not the sole/first modifier — distinct from the dedicated
    // single-`readonly` case, but going through the same generalized path.
    assert_eq!(
        codes("interface I { static readonly get x(): number; }"),
        vec![TS1131, TS1131],
    );
    assert_eq!(
        codes("interface I { readonly static get x(): number; }"),
        vec![TS1131, TS1131],
    );
}

#[test]
fn clean_modifier_run_is_not_keyed_to_a_binder_name() {
    // Structural, not tied to any identifier spelling (renamed binders + a
    // stacked run, in one pass).
    assert_eq!(
        codes("interface Alpha { static get beta(): number; }"),
        vec![TS1131],
    );
    assert_eq!(
        codes("type Gamma = { public static get delta(): number; };"),
        vec![TS1131, TS1131],
    );
}

// ---------------------------------------------------------------------------
// Hard-cascade modifiers (`async`/`declare`/`abstract`/`override`) before an
// accessor now derail into tsc's abandon-body statement re-parse cascade,
// covered in full by `type_member_hard_modifier_accessor_cascade_tests`. Only
// `in`/`out` remain on the pre-existing semantic TS1070 path (see the module
// doc comment); guarded here so they are not accidentally swept into the hard
// set.
// ---------------------------------------------------------------------------

#[test]
fn hard_cascade_modifiers_before_accessor_now_cascade_not_ts1070() {
    // `declare`/`abstract`/`override` are hard modifiers: one TS1131, then the
    // accessor tail re-parses as statements — TS1434 (unexpected keyword),
    // TS1005 (`;` expected), TS1128 (declaration or statement expected). Full
    // fingerprint parity lives in the sibling cascade suite.
    const TS1434: u32 = diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER;
    const TS1005: u32 = diagnostic_codes::EXPECTED;
    const TS1128: u32 = diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED;
    for modifier in ["declare", "abstract", "override"] {
        let source = format!("interface I {{ {modifier} get x(): number; }}");
        assert_eq!(
            codes(&source),
            vec![TS1131, TS1434, TS1005, TS1128],
            "modifier `{modifier}`",
        );
    }
}

#[test]
fn in_out_before_accessor_on_generic_interface_still_report_ts1070() {
    // `in` is a reserved operator (different statement re-parse) and both
    // `in`/`out` carry variance-position idiosyncrasies, so they are excluded
    // from the hard set and keep the pre-existing semantic TS1070.
    for modifier in ["in", "out"] {
        let source = format!("interface I<T> {{ {modifier} get x(): number; }}");
        assert_eq!(codes(&source), vec![TS1070], "modifier `{modifier}`");
    }
}

#[test]
fn hard_modifier_poisons_a_run_with_a_leading_clean_modifier() {
    // A hard modifier anywhere in the run keeps the WHOLE run off the
    // clean-only bare-accessor recovery, even when a clean modifier (`static`)
    // leads it — matching tsc, which reports one TS1131 per modifier
    // (`static` and `declare`) then re-parses the accessor tail as statements.
    const TS1434: u32 = diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER;
    const TS1005: u32 = diagnostic_codes::EXPECTED;
    const TS1128: u32 = diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED;
    assert_eq!(
        codes("interface I { static declare get x(): number; }"),
        vec![TS1131, TS1131, TS1434, TS1005, TS1128],
    );
}

#[test]
fn line_break_before_accessor_is_unaffected_for_any_clean_modifier() {
    // A line break between the last modifier and `get`/`set` takes tsc down a
    // different (out-of-scope) ASI path; the lookahead must not fire.
    let with_break = "interface I {\n  static\n  get x(): number\n}";
    let without_break = "interface I {\n  static get x(): number\n}";
    assert_ne!(codes(with_break), codes(without_break));
}

#[test]
fn clean_modifier_used_as_accessor_own_name_stays_ts1070() {
    // `static get(): number` — a method literally *named* `get`, not an
    // accessor — is unaffected by the new lookahead and keeps the pre-existing
    // semantic TS1070.
    assert_eq!(codes("interface I { static get(): number; }"), vec![TS1070]);
    assert_eq!(codes("interface I { static get: number; }"), vec![TS1070]);
}

// ---------------------------------------------------------------------------
// `readonly` + `async` on a type member: tsc's `checkGrammarModifiers` checks
// each leading modifier in SOURCE ORDER and reports (and stops at) the first
// one that is invalid for the member's own kind. `readonly` is invalid only
// on a method/construct signature (TS1024); `async` is invalid on any type
// member (TS1070). Verified against `typescript@7.0.2`.
// ---------------------------------------------------------------------------

#[test]
fn readonly_before_async_property_reports_ts1070_at_async() {
    // `readonly` is legal on a property, so `async` — invalid on any type
    // member — is the first (and only) offending modifier.
    assert_eq!(
        fingerprints("interface I { readonly async x: number; }"),
        vec![(
            TS1070,
            1,
            24,
            "'async' modifier cannot appear on a type member.".to_string()
        )],
    );
}

#[test]
fn readonly_before_async_method_reports_ts1024_at_readonly() {
    // `readonly` is checked first in source order and is invalid on a
    // method — tsc reports TS1024 there and never reaches `async`.
    assert_eq!(
        fingerprints("interface I { readonly async x(): number; }"),
        vec![(
            TS1024,
            1,
            15,
            "'readonly' modifier can only appear on a property declaration or index signature."
                .to_string()
        )],
    );
}

#[test]
fn readonly_before_async_property_on_type_literal_reports_ts1070() {
    assert_eq!(
        fingerprints("type T = { readonly async x: number };"),
        vec![(
            TS1070,
            1,
            21,
            "'async' modifier cannot appear on a type member.".to_string()
        )],
    );
}

#[test]
fn readonly_before_async_method_on_type_literal_reports_ts1024() {
    assert_eq!(
        codes("type T = { readonly async x(): number };"),
        vec![TS1024]
    );
}

#[test]
fn readonly_before_async_method_keeps_following_member() {
    // Exactly one diagnostic; the following `y` member is not lost.
    assert_eq!(
        codes("interface I { readonly async x(): number; y: string; }"),
        vec![TS1024],
    );
}

#[test]
fn readonly_before_async_before_method_named_get_reports_ts1024() {
    // `get` here is an ordinary method name, not an accessor — disambiguated
    // from the unrelated `readonly get()` accessor-lookahead path.
    assert_eq!(
        codes("interface I { readonly async get(): number; }"),
        vec![TS1024],
    );
}

#[test]
fn async_before_readonly_property_reports_ts1070_at_async() {
    // `async` comes first in source order: always invalid, reported and
    // stopped on immediately — `readonly` (which would be legal on this
    // property) is never separately evaluated.
    assert_eq!(
        fingerprints("interface I { async readonly x: number; }"),
        vec![(
            TS1070,
            1,
            15,
            "'async' modifier cannot appear on a type member.".to_string()
        )],
    );
}

#[test]
fn async_before_readonly_method_reports_ts1070_only_not_ts1024_too() {
    // Regression guard: `async` fires first and tsc's single-diagnostic-per-
    // member rule means the trailing `readonly`-on-method check must not
    // also fire TS1024 for the same member.
    assert_eq!(
        codes("interface I { async readonly x(): number; }"),
        vec![TS1070],
    );
}

#[test]
fn static_before_async_reports_ts1070_once_not_twice() {
    // Regression guard: an earlier illegal modifier (`static`) already
    // reports TS1070; the trailing `async` must still be consumed (so the
    // member parses cleanly) but must not report its own second TS1070.
    assert_eq!(
        codes("interface I { static async x(): number; }"),
        vec![TS1070],
    );
}

#[test]
fn static_readonly_async_reports_ts1070_at_static_only() {
    // Three-modifier run: `static` (first, illegal) wins; neither the
    // trailing `readonly` (TS1024) nor `async` (TS1070) separately report.
    assert_eq!(
        fingerprints("interface I { static readonly async x(): number; }"),
        vec![(
            TS1070,
            1,
            15,
            "'static' modifier cannot appear on a type member.".to_string()
        )],
    );
}

#[test]
fn static_async_readonly_reports_ts1070_at_static_only() {
    assert_eq!(
        codes("interface I { static async readonly x(): number; }"),
        vec![TS1070],
    );
}

#[test]
fn export_before_readonly_async_reports_ts1070_at_export_only() {
    assert_eq!(
        codes("interface I { export readonly async x(): number; }"),
        vec![TS1070],
    );
}

#[test]
fn readonly_async_used_as_property_name_stays_clean() {
    // `async` immediately followed by `:` is the property's own name, not a
    // modifier — `readonly async: number` is an ordinary readonly property.
    assert_eq!(
        codes("interface I { readonly async: number; }"),
        Vec::<u32>::new()
    );
}

#[test]
fn readonly_async_used_as_method_name_reports_ts1024_unrelated_to_async_fix() {
    // `async` immediately followed by `(` is the method's own name here;
    // `readonly` on this (async-named) method is the pre-existing,
    // unrelated TS1024 rule.
    assert_eq!(
        codes("interface I { readonly async(): number; }"),
        vec![TS1024]
    );
}

#[test]
fn readonly_async_optional_property_reports_ts1070_at_async() {
    assert_eq!(
        codes("interface I { readonly async x?: number; }"),
        vec![TS1070],
    );
}

// ---------------------------------------------------------------------------
// `readonly` before a second modifier OTHER than `async`: the same
// source-order / member-kind rule generalizes to all eleven illegal
// type-member modifiers, oracle-verified (`typescript@7.0.2`) across the full
// set. Previously only `async` had the second-modifier lookahead, so every
// other modifier here (`static`/`public`/`private`/`protected`/`accessor`/
// `override`/`abstract`/`declare`/`export`/`in`/`out`) mis-parsed the second
// modifier as the property/method name instead of reporting at all.
// ---------------------------------------------------------------------------

const READONLY_SECOND_MODIFIERS: [&str; 11] = [
    "static",
    "public",
    "private",
    "protected",
    "accessor",
    "override",
    "abstract",
    "declare",
    "export",
    "in",
    "out",
];

#[test]
fn readonly_before_second_modifier_method_reports_ts1024_at_readonly() {
    // Method kind: `readonly` is illegal on a method/construct signature and
    // is first in source order, so it wins over every second modifier —
    // uniformly TS1024 at column 15, regardless of which modifier follows.
    for modifier in READONLY_SECOND_MODIFIERS {
        let source = format!("interface I {{ readonly {modifier} m(): number; }}");
        assert_eq!(
            fingerprints(&source),
            vec![(
                TS1024,
                1,
                15,
                "'readonly' modifier can only appear on a property declaration or index signature."
                    .to_string()
            )],
            "source: {source}",
        );
    }
}

#[test]
fn readonly_before_second_modifier_property_reports_ts1070_at_second_modifier() {
    // Property kind: `readonly` is legal here, so the second modifier is the
    // first (and only) offender — anchored at its own position (column 24),
    // not at `readonly`.
    for modifier in READONLY_SECOND_MODIFIERS {
        let source = format!("interface I {{ readonly {modifier} p: number; }}");
        assert_eq!(
            fingerprints(&source),
            vec![(
                TS1070,
                1,
                24,
                format!("'{modifier}' modifier cannot appear on a type member.")
            )],
            "source: {source}",
        );
    }
}

#[test]
fn readonly_before_second_modifier_on_type_literal_reports_ts1024() {
    assert_eq!(
        codes("type T = { readonly static m(): number };"),
        vec![TS1024],
    );
}

#[test]
fn readonly_before_second_modifier_keeps_following_member() {
    // Exactly one diagnostic; the following `y` member is not lost, matching
    // the existing `readonly_before_async_method_keeps_following_member`
    // guard for the other ten modifiers.
    assert_eq!(
        codes("interface I { readonly static m(): number; y: string; }"),
        vec![TS1024],
    );
}

#[test]
fn readonly_second_modifier_used_as_property_name_stays_clean() {
    // `static` immediately followed by `:` is the property's own name, not a
    // modifier — mirrors `readonly_async_used_as_property_name_stays_clean`.
    assert_eq!(
        codes("interface I { readonly static: number; }"),
        Vec::<u32>::new()
    );
}

// ---------------------------------------------------------------------------
// Longer modifier chains after `readonly` (3+ leading modifiers): tsc still
// reports a single diagnostic naming only the first offender, and every
// modifier past it must still be consumed so the member parses cleanly.
// Regression coverage for a reviewer-flagged residual on #16827: before this,
// `readonly async static x` only consumed the first trailing modifier and
// left the second unconsumed, mis-parsing it as the name (bogus TS1005).
// ---------------------------------------------------------------------------

#[test]
fn readonly_three_modifier_chain_method_reports_ts1024_at_readonly() {
    assert_eq!(
        fingerprints("interface D { readonly async static m(): void; }"),
        vec![(
            TS1024,
            1,
            15,
            "'readonly' modifier can only appear on a property declaration or index signature."
                .to_string()
        )],
    );
}

#[test]
fn readonly_three_modifier_chain_property_reports_ts1070_at_first_offender() {
    assert_eq!(
        fingerprints("interface D { readonly async static p: number; }"),
        vec![(
            TS1070,
            1,
            24,
            "'async' modifier cannot appear on a type member.".to_string()
        )],
    );
}

#[test]
fn readonly_three_modifier_chain_property_reordered_names_the_first() {
    // Same three modifiers, `static` before `async`: the offender is
    // whichever comes first in source order after `readonly`.
    assert_eq!(
        fingerprints("interface D { readonly static async p: number; }"),
        vec![(
            TS1070,
            1,
            24,
            "'static' modifier cannot appear on a type member.".to_string()
        )],
    );
}

#[test]
fn readonly_three_modifier_chain_without_async_reports_ts1070_once() {
    assert_eq!(
        codes("interface D { readonly static public p: number; }"),
        vec![TS1070],
    );
}

#[test]
fn readonly_four_modifier_chain_method_reports_ts1024_once() {
    assert_eq!(
        codes("interface D { readonly static public accessor override m(): void; }"),
        vec![TS1024],
    );
}

#[test]
fn readonly_three_modifier_chain_keeps_following_member() {
    assert_eq!(
        codes("interface D { readonly async static p: number; y: string; }"),
        vec![TS1070],
    );
}

#[test]
fn readonly_second_modifier_used_as_method_name_reports_ts1024() {
    // `static` immediately followed by `(` is the method's own name here;
    // `readonly` on this (static-named) method is the pre-existing,
    // unrelated TS1024 rule — mirrors the `async`-as-name control.
    assert_eq!(
        codes("interface I { readonly static(): number; }"),
        vec![TS1024]
    );
}

// ---------------------------------------------------------------------------
// `async` before a second illegal modifier (`static`/`public`/`private`/
// `protected`/`accessor`/`override`/`abstract`/`declare`/`export`/`in`/`out`):
// unlike `readonly`, `async` is illegal on a type member regardless of the
// member's own kind (property or method), so `checkGrammarModifiers` reports
// TS1070 at `async` itself in every case, oracle-verified (`typescript@7.0.2`)
// across the full set. Previously the second modifier mis-parsed as the
// property/method name instead of being consumed (bogus TS1005), since
// `parse_async_type_member_restriction` only consumed the single leading
// `async` token — the same class of bug #16827 fixed for `readonly`-first.
// ---------------------------------------------------------------------------

const ASYNC_SECOND_MODIFIERS: [&str; 11] = [
    "static",
    "public",
    "private",
    "protected",
    "accessor",
    "override",
    "abstract",
    "declare",
    "export",
    "in",
    "out",
];

#[test]
fn async_before_second_modifier_method_reports_ts1070_at_async() {
    for modifier in ASYNC_SECOND_MODIFIERS {
        let source = format!("interface F {{ async {modifier} m(): void; }}");
        assert_eq!(
            fingerprints(&source),
            vec![(
                TS1070,
                1,
                15,
                "'async' modifier cannot appear on a type member.".to_string()
            )],
            "source: {source}",
        );
    }
}

#[test]
fn async_before_second_modifier_property_reports_ts1070_at_async() {
    for modifier in ASYNC_SECOND_MODIFIERS {
        let source = format!("interface I {{ async {modifier} p: number; }}");
        assert_eq!(
            fingerprints(&source),
            vec![(
                TS1070,
                1,
                15,
                "'async' modifier cannot appear on a type member.".to_string()
            )],
            "source: {source}",
        );
    }
}

#[test]
fn async_before_second_modifier_keeps_following_member() {
    // Exactly one diagnostic; the following `y` member is not lost.
    assert_eq!(
        codes("interface I { async static p: number; y: string; }"),
        vec![TS1070],
    );
}

#[test]
fn async_three_modifier_chain_reports_ts1070_once_at_async() {
    assert_eq!(
        fingerprints("interface I { async static public m(): void; }"),
        vec![(
            TS1070,
            1,
            15,
            "'async' modifier cannot appear on a type member.".to_string()
        )],
    );
}

#[test]
fn async_second_modifier_reordered_still_names_async() {
    // `async` is always the offender regardless of what follows it in
    // source order — unlike the `readonly`-first family, there is no
    // "legal on a property" carve-out for `async`.
    assert_eq!(
        codes("interface I { async public static p: number; }"),
        vec![TS1070],
    );
}

#[test]
fn async_second_modifier_used_as_property_name_still_reports_ts1070_at_async() {
    // `static` immediately followed by `:` is the property's own name, not a
    // modifier — but `async` itself is still illegal here and still reports.
    assert_eq!(
        fingerprints("interface I { async static: number; }"),
        vec![(
            TS1070,
            1,
            15,
            "'async' modifier cannot appear on a type member.".to_string()
        )],
    );
}

// A second (or later) `readonly` on a type member — TS1030 ("'readonly'
// modifier already seen."), not tsc's checkGrammarModifiers duplicate check,
// which tsz's parser previously did not implement at all for type members:
// the second `readonly` mis-parsed as a failed property-name lookahead
// (`NodeIndex::NONE`) instead of reporting. `checkGrammarModifiers` walks
// modifiers in SOURCE ORDER and stops at the first violation, so a duplicate
// `readonly` only wins when nothing else in the run fires first — an earlier
// illegal modifier (`static`) still wins over a trailing duplicate `readonly`
// (mirroring the existing `readonly`-then-illegal-modifier precedence), and
// `readonly` on a method reports the pre-existing TS1024 regardless of how
// many `readonly`s lead it, since the method-kind check on the very first
// `readonly` fires before a second one is ever inspected.

#[test]
fn duplicate_readonly_on_interface_property_reports_ts1030() {
    assert_eq!(
        fingerprints("interface I { readonly readonly x: number; }"),
        vec![(
            TS1030,
            1,
            24,
            "'readonly' modifier already seen.".to_string()
        )],
    );
}

#[test]
fn duplicate_readonly_on_type_literal_property_reports_ts1030() {
    assert_eq!(
        fingerprints("type T = { readonly readonly x: number; };"),
        vec![(
            TS1030,
            1,
            21,
            "'readonly' modifier already seen.".to_string()
        )],
    );
}

#[test]
fn triple_readonly_on_property_reports_ts1030_once_at_the_second() {
    // A third `readonly` is silently swallowed, not separately reported —
    // `checkGrammarModifiers` stops at the first violation.
    assert_eq!(
        fingerprints("interface I { readonly readonly readonly x: number; }"),
        vec![(
            TS1030,
            1,
            24,
            "'readonly' modifier already seen.".to_string()
        )],
    );
}

#[test]
fn duplicate_readonly_on_index_signature_reports_ts1030() {
    // An index signature can never be a method, so `readonly`'s own TS1024
    // never competes here — the duplicate always gets to report. (Index
    // signatures return early in the parser, a separate code path from the
    // property/method branch that the other rows in this matrix exercise.)
    assert_eq!(
        fingerprints("interface I { readonly readonly [k: string]: number; }"),
        vec![(
            TS1030,
            1,
            24,
            "'readonly' modifier already seen.".to_string()
        )],
    );
}

#[test]
fn duplicate_readonly_on_method_reports_ts1024_not_ts1030() {
    // The method-kind check on the FIRST `readonly` fires immediately, before
    // the parser ever inspects the second one for a duplicate — anchored at
    // the first `readonly`, matching the existing single-`readonly` TS1024
    // row exactly.
    assert_eq!(
        fingerprints("interface I { readonly readonly m(): void; }"),
        vec![(
            TS1024,
            1,
            15,
            "'readonly' modifier can only appear on a property declaration or index signature."
                .to_string()
        )],
    );
}

#[test]
fn duplicate_readonly_then_illegal_modifier_reports_ts1030_and_swallows_the_rest() {
    // The duplicate `readonly` is encountered before `static` in source
    // order, so it wins; `static` is consumed silently rather than mis-parsed
    // as the property name.
    assert_eq!(
        fingerprints("interface I { readonly readonly static x: number; }"),
        vec![(
            TS1030,
            1,
            24,
            "'readonly' modifier already seen.".to_string()
        )],
    );
}

#[test]
fn illegal_modifier_then_duplicate_readonly_reports_ts1070_and_swallows_the_readonly() {
    // `static` is encountered before the second `readonly` in source order,
    // so it wins here — the reverse ordering from the row above. The trailing
    // duplicate `readonly` must still be consumed so `x` parses as the name,
    // not mis-parsed as a failed name lookahead.
    assert_eq!(
        fingerprints("interface I { readonly static readonly x: number; }"),
        vec![(
            TS1070,
            1,
            24,
            "'static' modifier cannot appear on a type member.".to_string()
        )],
    );
}

#[test]
fn duplicate_readonly_then_async_reports_ts1030_and_swallows_async() {
    assert_eq!(
        fingerprints("interface I { readonly readonly async x: number; }"),
        vec![(
            TS1030,
            1,
            24,
            "'readonly' modifier already seen.".to_string()
        )],
    );
}

#[test]
fn duplicate_readonly_does_not_swallow_the_following_member() {
    assert_eq!(
        codes("interface I { readonly readonly x: number; y: string; }"),
        vec![TS1030],
    );
}
