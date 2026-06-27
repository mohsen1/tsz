//! Contiguous test shard split out of the parent module to satisfy the
//! source-file line cap. Covers TS2820 ("Did you mean") spelling suggestions
//! for assignment targets that only reduce to a string-literal union after
//! evaluation (type aliases, distributive conditionals, template-`infer`
//! captures) and therefore route through the display-types diagnostic emitter
//! rather than the union-aware `render_failure_reason` path.

use super::*;

#[test]
fn ts2820_template_infer_distributed_union_target() {
    // A distributive conditional that captures literals via a template-`infer`
    // pattern produces the literal union `"alpha" | "beta"`. A near-miss source
    // literal must surface the TS2820 spelling suggestion, not a bare TS2322.
    let source = r#"
type Strip<S> = S extends `prefix_${infer E}` ? E : never;
type X = Strip<"prefix_alpha" | "prefix_beta">;
const b: X = "alpa";
"#;
    let all = get_all_diagnostics(source);
    let msgs2820 = ts2820_messages(source);
    assert_eq!(msgs2820.len(), 1, "expected one TS2820, got: {all:#?}");
    assert!(
        msgs2820[0].contains("Did you mean '\"alpha\"'"),
        "expected suggestion of \"alpha\", got: {}",
        msgs2820[0]
    );
    assert!(
        !all.iter().any(|(code, _)| *code == 2322),
        "TS2322 must be upgraded to TS2820, got: {all:#?}"
    );
}

#[test]
fn ts2820_template_infer_distributed_union_target_renamed_binders() {
    // Same structural rule, different binder names + trailing-text capture:
    // the fix is structural, so renaming the alias/param and the pattern must
    // not change the outcome.
    let source = r#"
type Tail<Word> = Word extends `${infer Head}Btn` ? Head : never;
type Names = Tail<"saveBtn" | "loadBtn">;
const n: Names = "sav";
"#;
    let msgs2820 = ts2820_messages(source);
    assert_eq!(msgs2820.len(), 1, "expected one TS2820, got: {msgs2820:#?}");
    assert!(
        msgs2820[0].contains("Did you mean '\"save\"'"),
        "expected suggestion of \"save\", got: {}",
        msgs2820[0]
    );
}

#[test]
fn ts2820_plain_alias_string_literal_union_target() {
    // Broadened path: a plain (non-conditional) string-literal union behind a
    // type alias also routes through the display-types emitter and must surface
    // the suggestion.
    let source = r#"
type Color = "scarlet" | "emerald" | "cobalt";
const c: Color = "emrald";
"#;
    let msgs2820 = ts2820_messages(source);
    assert_eq!(msgs2820.len(), 1, "expected one TS2820, got: {msgs2820:#?}");
    assert!(
        msgs2820[0].contains("Did you mean '\"emerald\"'"),
        "expected suggestion of \"emerald\", got: {}",
        msgs2820[0]
    );
}

#[test]
fn ts2820_template_infer_union_far_miss_keeps_ts2322() {
    // Control: a source literal that is NOT a near miss of any member must keep
    // the bare TS2322 (no spurious suggestion).
    let source = r#"
type Strip<S> = S extends `prefix_${infer E}` ? E : never;
type X = Strip<"prefix_alpha" | "prefix_beta">;
const b: X = "zzzzzzzz";
"#;
    let all = get_all_diagnostics(source);
    assert!(
        all.iter().any(|(code, _)| *code == 2322),
        "expected bare TS2322, got: {all:#?}"
    );
    assert!(
        !all.iter().any(|(code, _)| *code == 2820),
        "must not emit a spurious TS2820, got: {all:#?}"
    );
}
