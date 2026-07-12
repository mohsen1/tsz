//! Negative controls for the consumed-`.d.ts` TS2344 delegate gate.
//!
//! `validate_type_reference_type_arguments` is skipped when the checker's
//! diagnostics are discarded — i.e. inside a transient cross-arena delegate
//! (`delegate_for_arena`) that lowers a consumed lib alias body. That skip
//! removes a super-linear re-validation blowup on faithful React libs while
//! staying diagnostic-neutral (the delegate's diagnostics were dropped at
//! `push_diagnostic` anyway). These tests pin the behaviour the skip must NOT
//! change: TS2344 still fires for a constraint violation written in a checked
//! file, whether that file is a user `.ts` referencing a consumed-lib generic
//! or a `.d.ts` that is itself the checked program root.

use std::sync::Arc;

use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source_with_libs_code_messages;

/// A consumed lib that declares a generic alias with a `string` constraint.
/// Loaded as a `LibFile` (a consumed `.d.ts`), so lowering its `Box` body runs
/// in a diagnostics-discarding delegate — exactly the context the fix gates.
fn constrained_generic_lib() -> Vec<Arc<LibFile>> {
    vec![Arc::new(LibFile::from_source(
        "constrained.d.ts".to_string(),
        "type Box<T extends string> = T;\n".to_string(),
    ))]
}

fn ts2344_count(diags: &[(u32, String)]) -> usize {
    diags.iter().filter(|(code, _)| *code == 2344).count()
}

#[test]
fn user_reference_to_consumed_lib_generic_reports_ts2344() {
    // The violation is written in the checked USER file; it is validated by the
    // primary (non-discarding) checker, so the delegate gate must not suppress
    // it. `number` does not satisfy `Box`'s `string` constraint.
    let diags = check_source_with_libs_code_messages(
        "type Bad = Box<number>;\n",
        "file.ts",
        CheckerOptions::default(),
        &constrained_generic_lib(),
    );
    assert_eq!(
        ts2344_count(&diags),
        1,
        "user reference to a consumed-lib generic with a bad type arg must still report TS2344, got: {diags:?}"
    );
}

#[test]
fn user_reference_to_consumed_lib_generic_satisfying_arg_is_clean() {
    // Positive control: a satisfying argument must not report TS2344, ensuring
    // the assertion above measures a real constraint check rather than an
    // unconditional error.
    let diags = check_source_with_libs_code_messages(
        "type Ok = Box<\"literal\">;\n",
        "file.ts",
        CheckerOptions::default(),
        &constrained_generic_lib(),
    );
    assert_eq!(
        ts2344_count(&diags),
        0,
        "a satisfying type argument to a consumed-lib generic must not report TS2344, got: {diags:?}"
    );
}

#[test]
fn checked_declaration_file_root_reports_ts2344() {
    // A `.d.ts` that is the checked program root runs in the primary checker
    // (diagnostics NOT discarded), so the delegate gate must not suppress its
    // own constraint violations. This distinguishes "consumed `.d.ts`" (gated)
    // from "checked `.d.ts` root" (still validated).
    let diags = check_source_with_libs_code_messages(
        "type Box<T extends string> = T;\ntype Bad = Box<number>;\n",
        "root.d.ts",
        CheckerOptions::default(),
        &[],
    );
    assert_eq!(
        ts2344_count(&diags),
        1,
        "a constraint violation in a checked .d.ts root must still report TS2344, got: {diags:?}"
    );
}
