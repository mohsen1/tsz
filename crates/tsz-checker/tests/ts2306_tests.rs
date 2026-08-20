//! Tests for TS2306 emission ("File is not a module")
//!
//! These tests verify that importing a file with no imports/exports
//! emits TS2306.

use crate::context::CheckerOptions;
use crate::test_utils::check_multi_file;

#[test]
fn test_ts2306_emitted_for_non_module_import() {
    // Use a binding import (not side-effect) — side-effect imports (`import "x"`)
    // never require the target to be a module, so they don't trigger TS2306.
    let diagnostics = check_multi_file(
        &[("a.ts", "import { x } from './tsx';"), ("tsx.tsx", "")],
        "a.ts",
        CheckerOptions::default(),
    );

    let ts2306_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2306).collect();
    assert!(
        !ts2306_errors.is_empty(),
        "Expected TS2306 error for non-module import, got: {diagnostics:?}"
    );
}
