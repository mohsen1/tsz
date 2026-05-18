//! Regression tests for issue #6052: false positive TS2300 when re-exporting an
//! interface that is also module-augmented.
//!
//! Structural rule: `export { X [as Y] } from "mod"` (re-export with a `from`
//! clause) does not create a local declaration for X — it forwards the source
//! module's symbol. When the same file also declares `declare module "mod" {
//! interface X { ... } }`, the augmentation merges into the source; the
//! re-export then forwards the merged result. This must never trigger TS2300.

use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn check_files(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    tsz_checker::test_utils::check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::ES2020,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

const SOURCE: &str = "export interface User { id: number; name: string; }\n";

/// Primary repro from issue #6052: re-export with rename + module augmentation
/// in the same file must produce no errors.
#[test]
fn reexport_with_rename_and_augmentation_no_ts2300() {
    let test = r#"
import type { User } from "./source";

export { User as MyUser } from "./source";

declare module "./source" {
  interface User {
    email?: string;
  }
}

const u: User = { id: 1, name: "Test", email: "test@test.com" };
"#;
    let codes = check_files(&[("source.ts", SOURCE), ("test.ts", test)], "test.ts");
    assert!(
        !codes.contains(&2300),
        "TS2300 must not fire on re-export with rename + augmentation; got: {codes:?}"
    );
}

/// Variant: same-name re-export (no alias) — the exported name is identical to
/// the imported name. The `from` clause still makes it a passthrough re-export.
#[test]
fn reexport_same_name_and_augmentation_no_ts2300() {
    let test = r#"
export { User } from "./source";

declare module "./source" {
  interface User {
    email?: string;
  }
}
"#;
    let codes = check_files(&[("source.ts", SOURCE), ("test.ts", test)], "test.ts");
    assert!(
        !codes.contains(&2300),
        "TS2300 must not fire on same-name re-export + augmentation; got: {codes:?}"
    );
}

/// Variant: type-only re-export. `export type { User } from "./source"` is also
/// a passthrough re-export and must not conflict with the augmentation.
#[test]
fn type_reexport_and_augmentation_no_ts2300() {
    let test = r#"
export type { User } from "./source";

declare module "./source" {
  interface User {
    email?: string;
  }
}
"#;
    let codes = check_files(&[("source.ts", SOURCE), ("test.ts", test)], "test.ts");
    assert!(
        !codes.contains(&2300),
        "TS2300 must not fire on type-only re-export + augmentation; got: {codes:?}"
    );
}

/// Variant: different alias names prove the fix is structural, not keyed on any
/// specific identifier spelling.
#[test]
fn reexport_with_alternate_alias_and_augmentation_no_ts2300() {
    let test = r#"
export { User as PublicUser } from "./source";
export { User as ExportedUser } from "./source";

declare module "./source" {
  interface User {
    role?: string;
  }
}
"#;
    let codes = check_files(&[("source.ts", SOURCE), ("test.ts", test)], "test.ts");
    assert!(
        !codes.contains(&2300),
        "TS2300 must not fire with alternate alias names; got: {codes:?}"
    );
}

/// Negative case: a genuine local duplicate class declaration must still report
/// TS2300 (the fix must not suppress real conflicts).
#[test]
fn genuine_duplicate_class_still_ts2300() {
    let test = "class Foo {}\nclass Foo {}\nexport {};\n";
    let codes = check_files(&[("test.ts", test)], "test.ts");
    assert!(
        codes.contains(&2300),
        "TS2300 must still fire for genuine duplicate class declarations; got: {codes:?}"
    );
}
