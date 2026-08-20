//! Tests for TS2305 emission ("Module has no exported member")
//!
//! These tests verify that named imports report TS2305 when the resolved
//! module does not export the requested symbol.

use crate::context::CheckerOptions;
use crate::test_utils::check_multi_file;

#[test]
fn test_ts2305_emitted_for_missing_export_in_resolved_module() {
    let diagnostics = check_multi_file(
        &[
            ("a.ts", "import { missing } from \"./foo\";"),
            ("foo.ts", "export const base = 1;"),
        ],
        "a.ts",
        CheckerOptions::default(),
    );

    let ts2305_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2305).collect();
    assert!(
        !ts2305_errors.is_empty(),
        "Expected TS2305 error for missing export, got: {diagnostics:?}"
    );
}
