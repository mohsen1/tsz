//! Contiguous test shard split out of the parent module to satisfy the
//! source-file line cap.

use super::*;

#[test]
fn ts7022_ts7023_do_not_fire_for_void_expression_return_operand() {
    let diags = check_source_diagnostics(
        r#"
type HowlErrorCallback = (soundId: number, error: unknown) => void;

interface HowlOptions {
  onplayerror?: HowlErrorCallback | undefined;
}

class Howl {
  constructor(public readonly options: HowlOptions) {}
  once(name: "unlock", fn: () => void) {
    console.log(name, fn);
  }
}

const instance = new Howl({
  onplayerror: () => void instance.once("unlock", () => {}),
});
"#,
    );
    let circularity: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7022 | 7023))
        .collect();
    assert!(
        circularity.is_empty(),
        "Expected no TS7022/TS7023 for self-reference under void return expression, got: {:?}",
        diagnostic_ref_summaries(&circularity)
    );
}

#[test]
fn ts7023_no_false_positive_when_property_key_matches_outer_var() {
    // The key in an object literal (also a non-value name position) must not
    // be treated as a lexical reference to a same-named outer variable.
    let diags = check_source_diagnostics(
        r#"
const wrap = (x: number) => ({ wrap: x });
"#,
    );
    let ts7023 = diagnostics_with_code(&diags, 7023);
    assert!(
        ts7023.is_empty(),
        "Expected no TS7023 when an object property key matches the enclosing variable name, got: {:?}",
        diagnostic_messages(&ts7023)
    );
}

#[test]
fn ts2322_no_false_positive_merged_type_alias_and_const_return() {
    // Two name variants guard against name-hardcoding regressions (§25).
    for source in [
        r#"
type Foo = { type: "foo" };
const Foo = {
  make: (): Foo => {
    return { type: "foo" };
  }
};
"#,
        r#"
type MyAlias = { kind: "ok" };
const MyAlias = {
  build: (): MyAlias => {
    return { kind: "ok" };
  }
};
"#,
    ] {
        let diags = check_source_diagnostics(source);
        let ts2322 = diagnostics_with_code(&diags, 2322);
        assert!(
            ts2322.is_empty(),
            "Expected no TS2322 for merged type-alias+const return, got: {:?}",
            diagnostic_messages(&ts2322)
        );
    }
}

#[test]
fn ts2322_real_error_still_reported_for_merged_type_alias_and_const_wrong_return() {
    let diags = check_source_diagnostics(
        r#"
type Status = { code: "ok" };
const Status = {
  make: (): Status => {
    return { code: "wrong" };
  }
};
"#,
    );
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "Expected 1 TS2322 for wrong literal in merged type-alias+const return, got: {:?}",
        diagnostic_messages(&ts2322)
    );
}

#[test]
fn regex_named_group_backreference_inside_character_class_is_not_a_reference() {
    // `\k<h>` inside a character class is a literal escape, not a
    // backreference -- tsc routes it through `characterClassEscape`, which
    // never calls `checkGroupName`, so there is no TS1532 lookup at all
    // (tsc reports a different code, TS1535, not yet implemented in tsz;
    // the false TS1532 from misreading class contents as a reference is the
    // bug this test pins). Oracle-confirmed (typescript@7.0.2): no TS1532.
    let diags = check_source_diagnostics(
        r#"
const regex = /(?<g>x)[\k<h>]/u;
"#,
    );
    let codes = diagnostic_codes(&diags);
    assert!(
        !codes.contains(&1532),
        "Expected no TS1532 for `\\k<...>` inside a character class, got {codes:?}"
    );
}

#[test]
fn regex_named_group_declaration_syntax_inside_character_class_is_not_a_group() {
    // Symmetric case on the declaration side: `(` inside a class is a
    // literal character, never a group open, so this must not be read as
    // declaring a group named `g` (which would then hide the real
    // undeclared-name diagnostic on the `\k<g>` reference below).
    let diags = check_source_diagnostics(
        r#"
const regex = /[(?<g>]\k<g>/u;
"#,
    );
    let codes = diagnostic_codes(&diags);
    assert!(
        codes.contains(&1532),
        "Expected TS1532: `(?<g>` inside a character class must not count as a group declaration, got {codes:?}"
    );
}

#[test]
fn regex_named_group_backreference_outside_class_still_resolves_when_pattern_also_has_a_class() {
    // Regression net: character-class tracking must not suppress a
    // perfectly normal backreference that merely follows a class earlier in
    // the pattern.
    let diags = check_source_diagnostics(
        r#"
const regex = /[a-z](?<g>x)\k<g>/u;
"#,
    );
    let codes = diagnostic_codes(&diags);
    assert!(
        !codes.contains(&1532),
        "Expected no TS1532 for a real backreference after an unrelated character class, got {codes:?}"
    );
}
