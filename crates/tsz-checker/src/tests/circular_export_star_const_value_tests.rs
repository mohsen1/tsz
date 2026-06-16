//! Regression: an exported `const` whose home file participates in a circular
//! `export *` re-export chain must resolve to its literal/widened value type in
//! value position (including element-access index-key position), not collapse to
//! `any`.
//!
//! Witnessed by the immer canary (`src/utils/common.ts`): `export const
//! PROTOTYPE = "prototype"` is re-exported through `internal.ts`'s `export *`,
//! and `internal.ts` is imported back by `common.ts`, forming a value-side
//! `export *` cycle. tsz collapsed `PROTOTYPE` to `any` whenever its symbol type
//! was first requested from value-resolution context, producing false `TS7053`
//! (`O[PROTOTYPE]`) and masking real assignability errors. tsc resolves it to
//! `"prototype"` everywhere.
//!
//! The trigger requires ALL of: an exported `const`, a wildcard `export *`
//! re-export cycle back through the const's own file, and resolving the const in
//! value position. Names are varied across cases so the fix follows structure,
//! not identifier text.

use crate::context::CheckerOptions;
use crate::diagnostics::{Diagnostic, diagnostic_codes};
use crate::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

fn check(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    )
}

fn codes(diags: &[Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

fn assert_no_code(diags: &[Diagnostic], code: u32) {
    assert!(
        !codes(diags).contains(&code),
        "did not expect diagnostic {code}, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

fn assert_has_code(diags: &[Diagnostic], code: u32) {
    assert!(
        codes(diags).contains(&code),
        "expected diagnostic {code}, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

/// Core repro: `O[PROTOTYPE]` in the const's own file under the cycle must NOT
/// emit TS7053 (the index key resolves to its literal, not `any`).
#[test]
fn reexported_const_index_key_no_false_ts7053() {
    let diags = check(
        &[
            (
                "./internal.ts",
                r#"export * from "./common";
export const SOMETHING = 1;
"#,
            ),
            (
                "./common.ts",
                r#"import { SOMETHING } from "./internal";
const O = Object;
export const PROTOTYPE = "prototype";
export const CONSTRUCTOR = "constructor";
const objectCtorString = O[PROTOTYPE][CONSTRUCTOR].toString();
export const OTHER = SOMETHING;
"#,
            ),
        ],
        "./common.ts",
    );
    assert_no_code(
        &diags,
        diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN,
    );
    assert_no_code(
        &diags,
        diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_INDEX_EXPRESSION_IS_NOT_OF_TYPE_NUMBE,
    );
}

/// The re-exported const must keep its literal/widened value type in plain value
/// position: `const x = PROTOTYPE; const n: number = x;` is a real TS2322
/// (string not assignable to number), which collapsing to `any` would mask.
#[test]
fn reexported_const_value_type_not_widened_to_any() {
    let diags = check(
        &[
            (
                "./internal.ts",
                r#"export * from "./common";
export const SOMETHING = 1;
"#,
            ),
            (
                "./common.ts",
                r#"import { SOMETHING } from "./internal";
export const PROTOTYPE = "prototype";
const a = PROTOTYPE;
const n: number = a;
export const OTHER = SOMETHING;
"#,
            ),
        ],
        "./common.ts",
    );
    assert_has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
}

/// Renamed binders: the behavior follows structure, not the `PROTOTYPE`/`O`
/// names.
#[test]
fn reexported_const_index_key_renamed_binders() {
    let diags = check(
        &[
            (
                "./barrel.ts",
                r#"export * from "./leaf";
export const seed = 1;
"#,
            ),
            (
                "./leaf.ts",
                r#"import { seed } from "./barrel";
const target = { key: 42 };
export const fieldName = "key";
const value = target[fieldName];
export const echo = seed;
"#,
            ),
        ],
        "./leaf.ts",
    );
    assert_no_code(
        &diags,
        diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN,
    );
    assert_no_code(
        &diags,
        diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_INDEX_EXPRESSION_IS_NOT_OF_TYPE_NUMBE,
    );
}

/// True negative preserved: a genuinely implicit-any index expression still
/// reports TS7053 (here, an untyped parameter used as the index key).
#[test]
fn genuine_implicit_any_index_still_reports_ts7053() {
    let diags = check(
        &[
            (
                "./internal.ts",
                r#"export * from "./common";
export const SOMETHING = 1;
"#,
            ),
            (
                "./common.ts",
                r#"import { SOMETHING } from "./internal";
export const PROTOTYPE = "prototype";
const obj = { prototype: 1 };
export function read(k: any) {
  return obj[k];
}
export const OTHER = SOMETHING;
"#,
            ),
        ],
        "./common.ts",
    );
    assert_has_code(
        &diags,
        diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN,
    );
}
