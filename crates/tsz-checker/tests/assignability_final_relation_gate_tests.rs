//! Cache-honesty pins for the checker-final assignability funnel
//! (issue #13243 step 4).
//!
//! The post-relation true-override gates (keyof literal membership and
//! namespace property mismatch here) now run on the relation cache-miss path
//! and the combined verdict is cached. These tests pin that a repeated query
//! of the same (source, target) pair — a cache hit served without re-running
//! any gate — still reports the gate's rejection, and that the gates fire on
//! structure rather than on specific binder names.

use tsz_checker::test_utils::check_source_code_messages as diagnostics;

fn ts2322_count(diags: &[(u32, String)]) -> usize {
    diags.iter().filter(|(code, _)| *code == 2322).count()
}

#[test]
fn keyof_literal_gate_rejects_and_repeated_pair_stays_rejected() {
    // Two identical (string-literal, keyof T) relation queries: the second
    // is served from the checker-final cache and must stay rejected.
    let diags = diagnostics(
        r#"
type Catalog = { sku: string; price: number };
const first: keyof Catalog = "missing";
const second: keyof Catalog = "missing";
const okay: keyof Catalog = "sku";
"#,
    );
    assert_eq!(
        ts2322_count(&diags),
        2,
        "both keyof-literal mismatches must be rejected (cache hit included). Got: {diags:#?}"
    );
}

#[test]
fn keyof_literal_gate_fires_with_renamed_binders() {
    let diags = diagnostics(
        r#"
type Zwiebel = { wurzel: boolean; schale: string };
const erste: keyof Zwiebel = "kern";
const zweite: keyof Zwiebel = "schale";
"#,
    );
    assert_eq!(
        ts2322_count(&diags),
        1,
        "renamed-binder keyof mismatch must still be rejected exactly once. Got: {diags:#?}"
    );
}

#[test]
fn namespace_property_mismatch_gate_rejects_and_repeated_pair_stays_rejected() {
    let diags = diagnostics(
        r#"
namespace Registry {
    export const limit: string = "ten";
}
const a: { limit: number } = Registry;
const b: { limit: number } = Registry;
const c: { limit: string } = Registry;
"#,
    );
    assert_eq!(
        ts2322_count(&diags),
        2,
        "namespace property mismatches must be rejected on first and repeated (cached) queries. Got: {diags:#?}"
    );
}

#[test]
fn namespace_property_mismatch_gate_fires_with_renamed_binders() {
    let diags = diagnostics(
        r#"
namespace Khazana {
    export const seema: boolean = true;
}
const napa: { seema: string } = Khazana;
const thika: { seema: boolean } = Khazana;
"#,
    );
    assert_eq!(
        ts2322_count(&diags),
        1,
        "renamed-binder namespace mismatch must still be rejected exactly once. Got: {diags:#?}"
    );
}

#[test]
fn keyof_literal_gate_leaves_unresolvable_keyof_to_the_relation() {
    // Generic keyof targets have no concrete key set; the gate must not
    // reject, leaving the verdict to the solver relation.
    let diags = diagnostics(
        r#"
function pick<T>(key: keyof T): keyof T {
    return key;
}
"#,
    );
    assert_eq!(
        ts2322_count(&diags),
        0,
        "unresolvable keyof targets must not be force-rejected. Got: {diags:#?}"
    );
}
