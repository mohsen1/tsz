//! Control-flow termination through a never-returning *imported* function.
//!
//! Structural rule: when a function's last statement is a call to a function
//! declared `: never`, tsc treats the call as terminating, marks the endpoint
//! unreachable, and suppresses TS2366 ("Function lacks ending return
//! statement…"). This must hold when the never-returning callee is reached
//! through an import alias — directly (`import { die }`) or transitively through
//! an `export *` barrel re-export. tsz previously read the import specifier's
//! (annotation-less) declaration, so the `: never` was invisible across the
//! module boundary; the fix inspects the alias's resolved function-type return
//! (`reachability_checker.rs` `symbol_explicitly_returns_never`).

use crate::context::CheckerOptions;
use crate::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

fn ts2366_count(files: &[(&str, &str)], entry: &str) -> usize {
    let opts = CheckerOptions {
        strict: true,
        module: ModuleKind::ESNext,
        ..CheckerOptions::default()
    };
    check_multi_file(files, entry, opts)
        .iter()
        .filter(|d| d.code == 2366)
        .count()
}

const DIE: &str = "export function die(code: number): never { throw new Error(String(code)); }\n";
const CONSUMER: &str =
    "export function values(obj: unknown): string[] { if (obj) { return [\"a\"]; } die(6); }\n";

#[test]
fn direct_imported_never_returning_call_terminates_control_flow() {
    let files = &[
        ("errors.ts", DIE),
        (
            "main.ts",
            &format!("import {{ die }} from \"./errors\";\n{CONSUMER}"),
        ),
    ];
    assert_eq!(
        ts2366_count(files, "main.ts"),
        0,
        "a direct-imported never-returning call must terminate control flow (no TS2366)"
    );
}

#[test]
fn never_returning_through_export_star_barrel_terminates() {
    let files = &[
        ("errors.ts", DIE),
        ("internal.ts", "export * from \"./errors\";\n"),
        (
            "main.ts",
            &format!("import {{ die }} from \"./internal\";\n{CONSUMER}"),
        ),
    ];
    assert_eq!(
        ts2366_count(files, "main.ts"),
        0,
        "a never-returning call reached through an export* barrel must terminate control flow"
    );
}

#[test]
fn non_never_imported_call_still_reports_ts2366() {
    // Negative guard: an imported function whose return type is NOT `never` must
    // still leave the endpoint reachable and report TS2366.
    let files = &[
        (
            "errors.ts",
            "export function notNever(code: number): string { return String(code); }\n",
        ),
        (
            "main.ts",
            "import { notNever } from \"./errors\";\nexport function values(obj: unknown): string[] { if (obj) { return [\"a\"]; } notNever(6); }\n",
        ),
    ];
    assert_eq!(
        ts2366_count(files, "main.ts"),
        1,
        "a non-never imported call must still report TS2366"
    );
}
