//! Regression tests for a re-imported namespace used as a qualified-type-name
//! anchor (`Ns.Member`) when a same-named local `type` alias shadows it.
//!
//! Structural rule: tsc resolves the left-hand side of a qualified type name
//! with *namespace* meaning, following the full alias chain. A named/default
//! import that re-imports an `import * as Ns` namespace (forwarded through an
//! `export { Ns }`) therefore stays a valid namespace anchor even when a
//! same-named local `type`/`interface` shadows the binding in type space. tsz
//! previously bailed because the re-exported namespace import classifies as
//! value-only at the flag level, so `namespace_anchor_alias_partner` (which only
//! accepted a `Type` re-export hop) dropped the anchor and emitted a spurious
//! `TS2503` ("Cannot find namespace"). Mined from ts-toolbelt (issue #14225).
//!
//! Binder names vary across the cases so the behavior follows the alias-chain
//! shape, not a spelling.

use super::super::core::*;

/// The minimal #14225 witness: `import * as Ns` re-exported via `export { Ns }`,
/// re-imported and shadowed by a local `type`. `Ns.Member` must anchor through
/// the namespace, with no TS2503.
#[test]
fn reimported_namespace_with_local_type_shadow_anchors_qualified_type() {
    if !lib_files_available() {
        return;
    }
    let files = &[
        ("lib.ts", "export type Intersect<A, B> = A & B\n"),
        ("index.ts", "import * as T from './lib'\nexport { T }\n"),
        (
            "use.ts",
            r#"
import { T } from './index'
type T = [1, 2, 3]
type R = T.Intersect<{ a: 1 }, { b: 2 }>
const r: R = { a: 1, b: 2 }
export { r }
"#,
        ),
    ];
    let diags = compile_named_files_get_diagnostics_with_lib_and_options(files, "use.ts", opts());
    assert!(
        !has_error(&diags, 2503),
        "no TS2503 — re-imported namespace `T` must anchor `T.Intersect` despite the local `type T`. Actual: {diags:#?}"
    );
}

/// Name-agnostic variant: rename every binder. The fix must follow the alias
/// chain by shape, never by the spelling `T`/`Intersect`.
#[test]
fn reimported_namespace_renamed_binders_anchors_qualified_type() {
    if !lib_files_available() {
        return;
    }
    let files = &[
        ("a.ts", "export type Combine<X, Y> = X & Y\n"),
        ("b.ts", "import * as Space from './a'\nexport { Space }\n"),
        (
            "c.ts",
            r#"
import { Space } from './b'
type Space = readonly [0]
type Out = Space.Combine<{ p: 1 }, { q: 2 }>
const out: Out = { p: 1, q: 2 }
export { out }
"#,
        ),
    ];
    let diags = compile_named_files_get_diagnostics_with_lib_and_options(files, "c.ts", opts());
    assert!(
        !has_error(&diags, 2503),
        "no TS2503 — renamed re-imported namespace must still anchor. Actual: {diags:#?}"
    );
}

/// Deep re-export chain: the namespace is forwarded across two modules before
/// being re-imported and shadowed. Each hop must be followed.
#[test]
fn deep_reexport_chain_namespace_with_local_shadow_anchors() {
    if !lib_files_available() {
        return;
    }
    let files = &[
        ("root.ts", "export type Pair<A, B> = [A, B]\n"),
        ("mid1.ts", "import * as NS from './root'\nexport { NS }\n"),
        ("mid2.ts", "export { NS } from './mid1'\n"),
        (
            "leaf.ts",
            r#"
import { NS } from './mid2'
type NS = 0
type R = NS.Pair<1, 2>
const r: R = [1, 2]
export { r }
"#,
        ),
    ];
    let diags = compile_named_files_get_diagnostics_with_lib_and_options(files, "leaf.ts", opts());
    assert!(
        !has_error(&diags, 2503),
        "no TS2503 — a deep re-export chain of a namespace must anchor `NS.Pair`. Actual: {diags:#?}"
    );
}

/// Negative control: a re-imported *value* (`export const`) shadowed by a local
/// `type` is genuinely not a namespace, so `Val.Member` must still diagnose
/// TS2503. The fix must not over-accept value-only re-exports.
#[test]
fn reimported_value_with_local_shadow_still_reports_ts2503() {
    if !lib_files_available() {
        return;
    }
    let files = &[
        ("v.ts", "export const Val = 1\n"),
        ("w.ts", "import { Val } from './v'\nexport { Val }\n"),
        (
            "u.ts",
            r#"
import { Val } from './w'
type Val = [1]
type R = Val.Member
export type { R }
"#,
        ),
    ];
    let diags = compile_named_files_get_diagnostics_with_lib_and_options(files, "u.ts", opts());
    assert!(
        has_error(&diags, 2503),
        "TS2503 expected — a re-imported `const` is not a namespace. Actual: {diags:#?}"
    );
}

fn opts() -> tsz_checker::context::CheckerOptions {
    tsz_checker::context::CheckerOptions {
        strict: true,
        strict_null_checks: true,
        module: tsz_common::common::ModuleKind::ESNext,
        ..Default::default()
    }
}
