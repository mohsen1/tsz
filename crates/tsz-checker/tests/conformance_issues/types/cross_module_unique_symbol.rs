//! Cross-module `unique symbol` identity for the value+type-alias merge idiom.
//!
//! A module commonly exports a `unique symbol` as both a value and a same-named
//! type alias:
//!
//! ```ts
//! export const k = Symbol.for("k");
//! export type k = typeof k;
//! ```
//!
//! Both meanings denote the single `unique symbol` identity of `k`. tsc keeps
//! that identity when the binding is imported (directly, through a re-export
//! chain, or via a namespace) and used in value position.
//!
//! tsz previously resolved the merged symbol through the generic type-alias
//! body path: lowering `typeof k` asked for the value of `k`, which re-entered
//! the symbol while it was on the resolution stack, collapsed to the error
//! type, and cached it. Every later value read of the merged symbol then saw
//! `any`, so `const s: string = k` was silently accepted (false negative) and
//! computed-key / indexed accesses keyed on `k` lost their `unique symbol`
//! identity. The regression below pins the value-position type back to a real
//! `unique symbol` (assigning it to `string` must report `TS2322`).

use super::super::core::*;

fn ts2322_count(diags: &[(u32, String)]) -> usize {
    diags.iter().filter(|(c, _)| *c == 2322).count()
}

/// Re-export chain: `a.ts` declares the merged value+type-alias, `b.ts`
/// re-exports it, `c.ts` imports the re-exported binding and uses it in value
/// position. The value must keep its `unique symbol` identity (not collapse to
/// `any`), so assigning it to `string` reports `TS2322`.
#[test]
fn reexported_value_type_alias_unique_symbol_keeps_value_identity() {
    let files = &[
        (
            "a.ts",
            r#"
export const tag = Symbol.for("tag");
export type tag = typeof tag;
"#,
        ),
        (
            "b.ts",
            r#"
import { tag } from "./a";
export { tag };
"#,
        ),
        (
            "c.ts",
            r#"
import { tag } from "./b";
const s: string = tag;
"#,
        ),
    ];
    let diagnostics = compile_named_files_get_diagnostics_with_lib_and_options(
        files,
        "c.ts",
        tsz_checker::context::CheckerOptions {
            strict: true,
            strict_null_checks: true,
            module: tsz_common::common::ModuleKind::ESNext,
            ..Default::default()
        },
    );
    assert_eq!(
        ts2322_count(&diagnostics),
        1,
        "re-exported unique-symbol value must remain a symbol (TS2322 on string assignment), \
         not collapse to `any`: {diagnostics:?}"
    );
}

/// Direct cross-file import of the merged value+type-alias binding. The
/// renamed binder (`brand`, not `tag`) guards against any name-based shortcut.
#[test]
fn imported_value_type_alias_unique_symbol_keeps_value_identity() {
    let files = &[
        (
            "sym.ts",
            r#"
export const brand = Symbol.for("brand");
export type brand = typeof brand;
"#,
        ),
        (
            "use.ts",
            r#"
import { brand } from "./sym";
const s: string = brand;
"#,
        ),
    ];
    let diagnostics = compile_named_files_get_diagnostics_with_lib_and_options(
        files,
        "use.ts",
        tsz_checker::context::CheckerOptions {
            strict: true,
            strict_null_checks: true,
            module: tsz_common::common::ModuleKind::ESNext,
            ..Default::default()
        },
    );
    assert_eq!(
        ts2322_count(&diagnostics),
        1,
        "imported unique-symbol value must remain a symbol (TS2322 on string assignment): \
         {diagnostics:?}"
    );
}

/// The `typeof`-side type alias resolved cross-file must also denote the
/// `unique symbol` (not `any`): a value of that aliased type is not assignable
/// to `string`.
#[test]
fn cross_file_typeof_type_alias_resolves_to_unique_symbol() {
    let files = &[
        (
            "sym.ts",
            r#"
export const handle = Symbol.for("handle");
export type handle = typeof handle;
"#,
        ),
        (
            "use.ts",
            r#"
import * as syms from "./sym";
type H = syms.handle;
declare const h: H;
const s: string = h;
"#,
        ),
    ];
    let diagnostics = compile_named_files_get_diagnostics_with_lib_and_options(
        files,
        "use.ts",
        tsz_checker::context::CheckerOptions {
            strict: true,
            strict_null_checks: true,
            module: tsz_common::common::ModuleKind::ESNext,
            ..Default::default()
        },
    );
    assert_eq!(
        ts2322_count(&diagnostics),
        1,
        "cross-file `typeof`-merged type alias must resolve to `unique symbol`, \
         not `any`: {diagnostics:?}"
    );
}
