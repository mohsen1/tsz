//! Display-budget coverage for `keyof` alias recovery in assignability
//! diagnostics.
//!
//! Structural rule: when assignability diagnostic display tries to recover a
//! user-visible `keyof Name` spelling by scanning unrelated non-generic aliases,
//! tsz must bound that display-only work through `error_reporter::display_budget`
//! before it asks relation/evaluation questions for each candidate. Exhaustion
//! falls back to ordinary type display; it must not change assignability or
//! suppress the diagnostic.

use crate::test_utils::check_source_diagnostics;
use std::fs;
use std::path::Path;

#[test]
fn keyof_alias_recovery_consumes_display_budget_before_relation_probes() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/error_reporter/assignability_keyof_alias_display.rs");
    let source =
        fs::read_to_string(path).expect("failed to read assignability_keyof_alias_display.rs");
    let compact: String = source.chars().filter(|ch| !ch.is_whitespace()).collect();

    assert!(
        compact.contains("display_budget::try_consume_visit()"),
        "`keyof` alias recovery must spend display budget while scanning aliases"
    );
    assert!(
        compact.contains("display_budget::is_exhausted()"),
        "`keyof` alias recovery must stop relation probes after display budget exhaustion"
    );
    assert!(
        !compact.contains("are_mutually_assignable("),
        "`keyof` alias recovery must not run full semantic relation probes while formatting"
    );
}

#[test]
fn named_keyof_alias_display_still_recovers_while_budget_is_available() {
    let diagnostics = check_source_diagnostics(
        r#"
interface TableShape { id: 1; name: 2 }
type TableKeys = keyof TableShape;
declare let key: TableKeys;
key = "missing";
"#,
    );
    let messages: Vec<_> = diagnostics
        .into_iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text)
        .collect();

    assert_eq!(
        messages.len(),
        1,
        "expected exactly one TS2322, got: {messages:?}"
    );
    assert!(
        messages[0].contains("keyof TableShape"),
        "named `keyof` alias display should still recover the operator spelling, got: {}",
        messages[0]
    );
}
