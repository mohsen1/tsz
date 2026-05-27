//! Regression coverage for issue #10634: a class imported from another module
//! that is extended with a covariant member return/field referring to the
//! class itself produces a false `TS2416`.
//!
//! Root cause: tsz uses per-file arenas, so the cross-file base class node is
//! not in the consuming file's arena. `base_instance_type_from_expression`
//! falls back to constructor synthesis, whose self-referential members are
//! typed as the *constructor* (Callable) instead of the *instance* type, so the
//! override check `Derived <: Base` compares against the constructor and fails.
//! Single-file (AST path) is correct.
//!
//! Harness note: the self-referential *field* form reproduces in this
//! `check_multi_file` harness; the self-returning *method* form additionally
//! manifests in full project/CLI mode (`tsz --noEmit -p`, `moduleResolution:
//! bundler`) — see #10634 for the CLI repro. The field test below is parked
//! (`#[ignore]`) until the fix lands. The negative test is an active guard
//! against the over-suppression failure mode documented in #10634 (the naive
//! "replace the whole base instance" fix regressed it to clean).

use crate::context::CheckerOptions;
use crate::test_utils::check_multi_file;

fn strict_bundler() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
}

fn ts2416_count(files: &[(&str, &str)]) -> usize {
    check_multi_file(files, "derived.ts", strict_bundler())
        .iter()
        .filter(|d| d.code == 2416)
        .count()
}

#[test]
#[ignore = "blocked on #10634: cross-module self-referential override false TS2416"]
fn cross_module_self_referential_field_no_false_ts2416() {
    // Covariant self-referential field override across modules. tsc: clean.
    let files = &[
        ("base.ts", "export class Base { next!: Base; }\n"),
        (
            "derived.ts",
            "import { Base } from \"./base\";\nexport class Derived extends Base { next!: Derived; }\n",
        ),
    ];
    assert_eq!(
        ts2416_count(files),
        0,
        "covariant self-referential field override should not emit TS2416"
    );
}

#[test]
fn cross_module_incompatible_override_still_errors_ts2416() {
    // Genuinely incompatible override across modules. tsc: TS2416. The fix for
    // #10634 must keep reporting this — the discarded naive fix (replacing the
    // whole base instance) regressed it to clean (false negative).
    let files = &[
        (
            "base.ts",
            "export abstract class Base { abstract val(): string; }\n",
        ),
        (
            "derived.ts",
            "import { Base } from \"./base\";\nexport class Derived extends Base { val(): number { return 1; } }\n",
        ),
    ];
    assert_eq!(
        ts2416_count(files),
        1,
        "incompatible return-type override must still emit TS2416"
    );
}
