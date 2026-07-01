//! Cross-module generic-method type-parameter constraint resolution (refs #15256).
//!
//! Structural rule: when a class exposes a generic method whose type-parameter
//! constraint references a type alias declared in a DIFFERENT module than the
//! class, and the class is consumed from a THIRD module, the real multi-module
//! driver must resolve that constraint to the alias, not degrade it to `Error`.
//!
//! Before the fix, materializing the imported class's instance type in the
//! consuming file rebuilt the method signature in a transient cross-arena child
//! checker that failed to resolve the third-module alias and committed `Error`
//! as the constraint. An `Error` constraint is trivially "satisfied", so generic
//! inference widened a literal argument to the constraint base (`string`),
//! producing a spurious `TS2322`/`TS2345` cascade (the kysely `bareC(...)`
//! family) where `tsc` preserves the literal.
//!
//! This must be a real multi-module driver test (`crate::driver::compile`): the
//! in-crate checker harness resolves every file through a single context and
//! does not exercise the cross-arena class delegation that hosts the bug.
//!
//! The matrix varies the alias shape (bare primitive alias, generic
//! `keyof DB & string` alias) and the binder names so the fix follows structure,
//! not identifier text, and checks each project in both root-file orders so the
//! result cannot depend on which file the driver happens to check first. The
//! negative case pins that a genuinely incompatible argument still fails the
//! restored constraint, so the recovery does not silently drop enforcement.

use crate::args::CliArgs;
use clap::Parser;
use tsz_checker::diagnostics::Diagnostic;

/// Compile `files` (written into one temp dir) with the given root-file order.
fn compile_in_order(files: &[(&str, &str)], root_order: &[&str]) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    for (name, contents) in files {
        std::fs::write(dir.path().join(name), contents).expect("write repro file");
    }

    let mut argv: Vec<&str> = vec![
        "tsz",
        "--ignoreConfig",
        "--noEmit",
        "--strict",
        "--target",
        "es2022",
        "--module",
        "esnext",
        "--moduleResolution",
        "bundler",
        "--lib",
        "es2022",
    ];
    argv.extend_from_slice(root_order);

    let args = CliArgs::try_parse_from(argv).expect("parse args");
    crate::driver::compile(&args, dir.path())
        .expect("compile should succeed")
        .diagnostics
}

fn codes(diags: &[Diagnostic]) -> Vec<(u32, String)> {
    diags
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// Positive: the literal argument survives the cross-module constraint, so the
/// call raises no argument error (no TS2345) and its `'lit'`-typed result assigns
/// cleanly (no TS2322). Checked in both root-file orders.
fn assert_literal_preserved(files: &[(&str, &str)], roots: &[&str], label: &str) {
    for order in [roots.to_vec(), roots.iter().rev().copied().collect()] {
        let diags = compile_in_order(files, &order);
        let offending: Vec<_> = diags
            .iter()
            .filter(|d| d.code == 2322 || d.code == 2345)
            .collect();
        assert!(
            offending.is_empty(),
            "[{label}] (root order {order:?}) expected the literal argument to survive the \
             cross-module constraint (no TS2322/TS2345), got: {:?}",
            codes(&diags)
        );
    }
}

#[test]
fn bare_primitive_alias_constraint_preserves_literal() {
    // `type AnyTable = string` in a third module; the class method constrains
    // `TE extends AnyTable`. `db.bareC('sys.tables')` must infer `'sys.tables'`.
    assert_literal_preserved(
        &[
            ("types.ts", "export type AnyTable = string;\n"),
            (
                "kysely.ts",
                "import type { AnyTable } from './types';\n\
                 export declare class QC { bareC<TE extends AnyTable>(from: TE): TE; }\n",
            ),
            (
                "use.ts",
                "import type { QC } from './kysely';\n\
                 declare const db: QC;\n\
                 const a = db.bareC('sys.tables');\n\
                 const check: 'sys.tables' = a;\n",
            ),
        ],
        &["types.ts", "kysely.ts", "use.ts"],
        "bare-primitive-alias",
    );
}

#[test]
fn generic_keyof_alias_constraint_preserves_literal() {
    // The real kysely shape: `type AnyTable<DB> = keyof DB & string`.
    assert_literal_preserved(
        &[
            (
                "types.ts",
                "export type AnyTable<DB> = keyof DB & string;\n",
            ),
            (
                "kysely.ts",
                "import type { AnyTable } from './types';\n\
                 export declare class QC<DB> { bareC<TE extends AnyTable<DB>>(from: TE): TE; }\n",
            ),
            (
                "use.ts",
                "import type { QC } from './kysely';\n\
                 interface DB1 { 'sys.tables': { name: string } }\n\
                 declare const db: QC<DB1>;\n\
                 const a = db.bareC('sys.tables');\n\
                 const check: 'sys.tables' = a;\n",
            ),
        ],
        &["types.ts", "kysely.ts", "use.ts"],
        "generic-keyof-alias",
    );
}

#[test]
fn renamed_binders_preserve_literal() {
    // Anti-hardcoding: identical structure, different identifiers everywhere.
    assert_literal_preserved(
        &[
            ("names.ts", "export type Col = string;\n"),
            (
                "builder.ts",
                "import type { Col } from './names';\n\
                 export declare class Query { pick<K extends Col>(k: K): K; }\n",
            ),
            (
                "app.ts",
                "import type { Query } from './builder';\n\
                 declare const q: Query;\n\
                 const v = q.pick('id');\n\
                 const keep: 'id' = v;\n",
            ),
        ],
        &["names.ts", "builder.ts", "app.ts"],
        "renamed-binders",
    );
}

#[test]
fn incompatible_argument_still_fails_constraint() {
    // Negative: the recovery restores the REAL constraint, so a genuinely
    // incompatible argument still fails the `TE extends string` check.
    let files = &[
        ("types.ts", "export type AnyTable = string;\n"),
        (
            "kysely.ts",
            "import type { AnyTable } from './types';\n\
             export declare class QC { bareC<TE extends AnyTable>(from: TE): TE; }\n",
        ),
        (
            "use.ts",
            "import type { QC } from './kysely';\n\
             declare const db: QC;\n\
             const bad = db.bareC(123);\n",
        ),
    ];
    let diags = compile_in_order(files, &["types.ts", "kysely.ts", "use.ts"]);
    assert!(
        diags.iter().any(|d| d.code == 2345),
        "expected TS2345 (number not assignable to the string constraint), got: {:?}",
        codes(&diags)
    );
}
