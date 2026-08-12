//! #17253: #17251 wired TS1155 (`'{0}' declarations must be initialized.`)
//! from the parser but classified it as `is_real_syntax_error` /
//! `is_structural_parse_error` in `check_utils.rs`. Those two lists drive
//! `checker.ctx.has_real_syntax_errors`, which broadly suppresses a file's
//! other checker diagnostics the way tsc suppresses cascading errors after a
//! genuine structural parse failure — but TS1155 is a checker-style grammar
//! check over a syntactically valid AST (tsc's `checkGrammarVariableDeclaration`
//! runs from the checker, not the parser), so it must never trigger that
//! suppression. Misclassified, it silently deleted the very companion
//! diagnostics (TS2588, TS7005, TS18046 …) tsc reports alongside it in the
//! same file — 11 conformance rows broke from a single, targeted fix landing.
//!
//! Fixing the classification (moving TS1155 into `is_parser_grammar_code`,
//! mirroring TS1313's identical fix) uncovered a second, independent,
//! pre-existing bug: the checker also has its own TS1155 emission
//! (`variable_checking/core.rs`, predates #17251, kept alive only for the
//! checker-only unit-test harness in `using_declaration_implicit_any_tests.rs`
//! that never sees parser diagnostics) which was accidentally silenced by the
//! very misclassification bug above (its own `!has_real_syntax_errors` gate
//! was always false). Un-suppressing the classification without also
//! deduping would have traded a missing-sibling regression for a duplicate
//! TS1155. `checker_diagnostics.rs` now drops the checker's copy wherever a
//! parser TS1155 already covers the same position — the identical shape as
//! the pre-existing TS2499 double-emission fix in the same file.
//!
//! Expectations are oracle-pinned against `typescript@7.0.2`
//! (`--noEmit --strict --target es2022 --lib es2022`).

use crate::args::CliArgs;
use clap::Parser;
use tsz_checker::diagnostics::Diagnostic;

/// Compile a single-file `source` with `--strict` and return its diagnostics.
fn compile_source(source: &str) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("main.ts"), source).expect("write repro file");

    let argv = vec![
        "tsz",
        "--ignoreConfig",
        "--noEmit",
        "--strict",
        "--target",
        "es2022",
        "--lib",
        "es2022",
        "main.ts",
    ];
    let args = CliArgs::try_parse_from(argv).expect("parse args");
    crate::driver::compile(&args, dir.path())
        .expect("compile should succeed")
        .diagnostics
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    let mut codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

fn count_code(diagnostics: &[Diagnostic], code: u32) -> usize {
    diagnostics.iter().filter(|d| d.code == code).count()
}

/// `constDeclarations-errors.ts`-shaped witness: an uninitialized `const`
/// later reassigned. tsc reports TS1155 exactly once, plus the TS7005
/// declaration-site implicit-any and the TS2588 constant-assignment error —
/// neither companion may be dropped, and TS1155 itself must not double-fire
/// now that its own checker-side self-suppression gate is no longer wrongly
/// tripped.
#[test]
fn uninitialized_const_reassigned_keeps_ts1155_once_and_both_companions() {
    let diags = compile_source("const x;\nx = 1;\n");
    assert_eq!(
        count_code(&diags, 1155),
        1,
        "TS1155 must fire exactly once (parser + checker both emit it internally); got {:?}",
        codes(&diags)
    );
    assert_eq!(count_code(&diags, 7005), 1, "got {:?}", codes(&diags));
    assert_eq!(count_code(&diags, 2588), 1, "got {:?}", codes(&diags));
}

/// `for-of2.ts`-shaped witness: a C-style `for` header's uninitialized
/// `const`. tsc keeps TS1155 alongside the implicit-any and the later
/// `unknown`-typed-value error from the loop body — the exact family #17253
/// found dropped.
#[test]
fn c_style_for_header_uninitialized_const_keeps_companions() {
    let diags = compile_source("for (const x; ; ) {\n  const y: unknown = 1 as any;\n  y();\n}\n");
    assert_eq!(count_code(&diags, 1155), 1, "got {:?}", codes(&diags));
    assert_eq!(count_code(&diags, 7005), 1, "got {:?}", codes(&diags));
    assert_eq!(count_code(&diags, 18046), 1, "got {:?}", codes(&diags));
}

/// Mirror-image half of the same membership gap: TS1155 must itself be
/// suppressed when the file carries an unrelated real syntax error, matching
/// every other member of `is_parser_grammar_code`. Oracle: `let a: = 1;`
/// alone reports TS1110 only.
#[test]
fn uninitialized_const_suppressed_alongside_real_syntax_error() {
    let diags = compile_source("let a: = 1;\nconst x;\n");
    assert_eq!(
        count_code(&diags, 1155),
        0,
        "TS1155 must be suppressed alongside a real syntax error; got {:?}",
        codes(&diags)
    );
    assert!(codes(&diags).contains(&1110), "got {:?}", codes(&diags));
}

/// A plain uninitialized `const` alone still reports TS1155 (Direction A) —
/// the suppression fix must not silence the diagnostic entirely.
#[test]
fn uninitialized_const_alone_reports_ts1155_once() {
    let diags = compile_source("const x;\n");
    assert_eq!(count_code(&diags, 1155), 1, "got {:?}", codes(&diags));
}
