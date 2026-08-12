//! Tests for TS1155 ("'{0}' declarations must be initialized."), emitted as a
//! grammar check while parsing variable declarations
//! (`report_const_or_using_uninitialized`).
//!
//! Structural rule (mirrors tsc's `checkGrammarVariableDeclaration`): a
//! declarator whose declaration list is `const`, `using`, or `await using`
//! (tsc's `isVarConstLike`) and which has no initializer reports TS1155,
//! anchored at the binding **name** — tsc underlines only the name node, never a
//! trailing type annotation. `let` / `var` may be uninitialized and are exempt.
//! The check sits in the same `else if` arm that follows the TS1182 destructuring
//! check, so the two are mutually exclusive: a binding pattern reports TS1182 and
//! never TS1155. Ambient declarations (`declare const x;`, whole-file `.d.ts`)
//! and `catch` bindings are exempt, matching the sibling TS1182 gate. A
//! `for...in` / `for...of` head is exempt (the iterable supplies the value); a
//! C-style `for (const x; ; )` header is not, so each uninitialized const-like
//! declarator there is flagged.
//!
//! The keyword substituted into `{0}` is reconstructed from the declaration-list
//! flags: `await using` sets the `Const` and `Using` bits, `using` only `Using`,
//! `const` only `Const`.
//!
//! Tests vary the binder name (`x`, `value`, `count`, …) to prove the behavior is
//! structural and not keyed to a specific identifier spelling.

use crate::parser::test_fixture::parse_source;

const TS1155: u32 = 1155;
const TS1182: u32 = 1182;

/// All `(code, start_offset, message)` triples the parser reports for `source`.
fn diags(source: &str) -> Vec<(u32, u32, String)> {
    let (parser, _root) = parse_source(source);
    parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.start, d.message.clone()))
        .collect()
}

/// The `(start_offset, message)` pairs at which TS1155 was reported.
fn ts1155(source: &str) -> Vec<(u32, String)> {
    diags(source)
        .into_iter()
        .filter(|(code, _, _)| *code == TS1155)
        .map(|(_, start, message)| (start, message))
        .collect()
}

/// Just the byte offsets at which TS1155 was reported.
fn ts1155_starts(source: &str) -> Vec<u32> {
    ts1155(source).into_iter().map(|(start, _)| start).collect()
}

fn has_code(source: &str, code: u32) -> bool {
    diags(source).iter().any(|(c, _, _)| *c == code)
}

// ---------------------------------------------------------------------------
// Plain-statement path: the base positive cases.
// ---------------------------------------------------------------------------

#[test]
fn const_identifier_without_initializer_reports_ts1155_at_name() {
    // `const a;` — the `a` sits at byte offset 6 (`const ` is six chars).
    let hits = ts1155("const a;");
    assert_eq!(hits.len(), 1, "exactly one TS1155, got {hits:?}");
    assert_eq!(hits[0].0, 6, "anchored at the name `a`");
    assert_eq!(hits[0].1, "'const' declarations must be initialized.");
}

#[test]
fn const_spans_only_the_name_not_the_type_annotation() {
    // `const y: number;` — tsc underlines only `y` (width 1), not `: number`.
    let (parser, _root) = parse_source("const y: number;");
    let hit = parser
        .get_diagnostics()
        .iter()
        .find(|d| d.code == TS1155)
        .expect("TS1155 present");
    assert_eq!(hit.start, 6, "starts at `y`");
    assert_eq!(hit.length, 1, "spans only the one-char name, not the type");
}

#[test]
fn const_reports_are_structural_across_binder_names() {
    // Longer names widen the anchor to the name's own width and never key on the
    // spelling.
    for (source, name, offset, width) in [
        ("const x;", "x", 6u32, 1u32),
        ("const value;", "value", 6, 5),
        ("const longIdentifier;", "longIdentifier", 6, 14),
    ] {
        let (parser, _root) = parse_source(source);
        let hit = parser
            .get_diagnostics()
            .iter()
            .find(|d| d.code == TS1155)
            .unwrap_or_else(|| panic!("TS1155 expected for `{source}` (name {name})"));
        assert_eq!(hit.start, offset, "anchor for {source}");
        assert_eq!(hit.length, width, "width for {source}");
    }
}

#[test]
fn using_declaration_uses_the_using_keyword() {
    let hits = ts1155("using u;");
    assert_eq!(hits.len(), 1, "one TS1155, got {hits:?}");
    assert_eq!(hits[0].1, "'using' declarations must be initialized.");
}

#[test]
fn await_using_declaration_uses_the_await_using_keyword() {
    // `await using` in a module body. Only the TS1155 keyword text is asserted;
    // top-level-await grammar noise on other codes is irrelevant here.
    let hits = ts1155("await using resource;");
    assert!(
        hits.iter()
            .any(|(_, m)| m == "'await using' declarations must be initialized."),
        "expected an `await using` TS1155, got {hits:?}",
    );
}

#[test]
fn multi_declarator_flags_only_the_uninitialized_const_binders() {
    // `const a, b = 1, c;` — TS1155 for `a` (offset 6) and `c` (offset 16), never
    // the initialized `b`.
    let starts = ts1155_starts("const a, b = 1, c;");
    assert_eq!(starts, vec![6, 16], "only `a` and `c`, got {starts:?}");
}

// ---------------------------------------------------------------------------
// Exemptions: `let` / `var`, initialized, ambient, destructuring.
// ---------------------------------------------------------------------------

#[test]
fn let_and_var_without_initializer_are_exempt() {
    assert!(ts1155_starts("let a;").is_empty(), "let is exempt");
    assert!(ts1155_starts("var b;").is_empty(), "var is exempt");
}

#[test]
fn initialized_const_is_clean() {
    assert!(ts1155_starts("const a = 1;").is_empty());
    assert!(ts1155_starts("using u = getResource();").is_empty());
}

#[test]
fn ambient_declare_const_is_exempt() {
    // `declare const x;` is legally uninitialized — tsc reports no TS1155.
    assert!(
        ts1155_starts("declare const x;").is_empty(),
        "ambient declaration exempt",
    );
}

#[test]
fn const_destructuring_reports_ts1182_not_ts1155() {
    // A binding pattern without an initializer is TS1182; TS1155 must not also
    // fire (tsc returns before the `isVarConstLike` arm).
    assert!(
        !has_code("const { a } = obj;", TS1155),
        "initialized destructuring is clean",
    );
    let uninitialized = "const { a };";
    assert!(
        has_code(uninitialized, TS1182),
        "destructuring-without-initializer reports TS1182",
    );
    assert!(
        !has_code(uninitialized, TS1155),
        "and never additionally reports TS1155",
    );
}

// ---------------------------------------------------------------------------
// `for` headers: C-style flagged, `for...in` / `for...of` exempt.
// ---------------------------------------------------------------------------

#[test]
fn c_style_for_const_without_initializer_reports_ts1155() {
    // `for (const x; ; ) {}` — `x` at offset 11.
    let starts = ts1155_starts("for (const x; ; ) {}");
    assert_eq!(
        starts,
        vec![11],
        "C-style for flags the const, got {starts:?}"
    );
}

#[test]
fn c_style_for_multi_declarator_flags_each_uninitialized_const() {
    // `for (const x, y; ; ) {}` — `x` at 11, `y` at 14.
    let starts = ts1155_starts("for (const x, y; ; ) {}");
    assert_eq!(starts, vec![11, 14], "both const binders, got {starts:?}");
}

#[test]
fn c_style_for_let_and_initialized_const_are_clean() {
    assert!(ts1155_starts("for (let x; ; ) {}").is_empty(), "let exempt");
    assert!(
        ts1155_starts("for (const x = 0; ; ) {}").is_empty(),
        "initialized const clean",
    );
}

#[test]
fn for_of_and_for_in_const_heads_are_exempt() {
    // The iterable supplies the value, so tsc reports no TS1155 here.
    assert!(
        ts1155_starts("for (const item of items) {}").is_empty(),
        "for...of exempt",
    );
    assert!(
        ts1155_starts("for (const key in record) {}").is_empty(),
        "for...in exempt",
    );
}

#[test]
fn for_of_const_head_is_exempt_across_binder_names() {
    for name in ["item", "x", "value", "entry"] {
        let source = format!("for (const {name} of source) {{}}");
        assert!(
            ts1155_starts(&source).is_empty(),
            "for...of exempt for binder `{name}`",
        );
    }
}
