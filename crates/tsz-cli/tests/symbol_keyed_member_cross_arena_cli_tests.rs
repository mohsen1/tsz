//! Cross-file `unique symbol` const that is **name-merged with a
//! `type X = typeof X` alias** and imported into another module (#13855).
//!
//! Structural rule: a `const X = Symbol()` / `const X = Symbol.for(...)`
//! declaration has the `unique symbol` value identity `typeof X`. When that
//! const is name-merged with `type X = typeof X` and imported, the importing
//! module must still see `X` as `unique symbol` in value position, so every
//! computed-property member key path — interface/type-literal member,
//! object-literal member, and element access — keys on the same binding
//! identity (`__unique_<sym>`). Before the fix the imported value+type-alias
//! merge routed through the "merged interface+value" identifier path, whose
//! cross-file value-declaration resolution returned the general `symbol` type
//! instead of `unique symbol` (diverging from the same-file inference). The
//! wide `symbol` then keyed the interface member as a binding-identity member
//! while the object literal degraded to a `[k: symbol]` index signature, so a
//! legitimate `{ [X]: v }` object failed to satisfy `interface I { [X]: T }`
//! (false TS2322/TS2345/TS2464).
//!
//! The defect is cross-file + name-merge only: the same-file form and the
//! cross-file form *without* the `type X = typeof X` alias are both clean, and
//! the in-crate `check_multi_file_with_libs` harness cannot host it (it expands
//! lib aliases inline and does not reproduce the driver's cross-arena value
//! delegation), so this is a real multi-module driver test.

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

/// Diagnostics that this family produces as false positives: assignability
/// mismatches (TS2322/TS2345/TS2353), the invalid-computed-key error (TS2464)
/// that the degraded value type triggers, and the possibly-`undefined`-callee
/// error (TS2722) raised when the unresolved key fails to register the member
/// so its access falls back to a `… | undefined` index lookup.
fn family_false_positives(diagnostics: &[Diagnostic]) -> Vec<(u32, String)> {
    diagnostics
        .iter()
        .filter(|d| matches!(d.code, 2322 | 2345 | 2353 | 2464 | 2536 | 2722))
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// Assert the repro compiles clean in both root-file orders (consumer-first is
/// the cross-file regression direction).
fn assert_clean_both_orders(files: &[(&str, &str)]) {
    let names: Vec<&str> = files.iter().map(|(name, _)| *name).collect();
    let forward = family_false_positives(&compile_in_order(files, &names));
    assert!(
        forward.is_empty(),
        "expected no symbol-keyed-member false positives in forward order {names:?}, got: {forward:?}"
    );
    let reversed: Vec<&str> = names.iter().rev().copied().collect();
    let backward = family_false_positives(&compile_in_order(files, &reversed));
    assert!(
        backward.is_empty(),
        "expected no symbol-keyed-member false positives in reversed order {reversed:?}, got: {backward:?}"
    );
}

/// The issue's minimal repro: `Symbol.for` + `type X = typeof X`, imported,
/// keyed by object literals (contextual and bare) against an interface member.
#[test]
fn cross_file_symbol_for_name_merged_object_literal_satisfies_interface() {
    assert_clean_both_orders(&[
        (
            "symbols.ts",
            r#"
export const matcher = Symbol.for('@demo/matcher');
export type matcher = typeof matcher;
"#,
        ),
        (
            "pattern.ts",
            r#"
import { matcher } from './symbols';
export interface Matcher { [matcher](): number; }
export const make = (): Matcher => ({ [matcher]: () => 1 });
const lit = { [matcher]: () => 1 };
export const m: Matcher = lit;
"#,
        ),
    ]);
}

/// `Symbol()` (not `Symbol.for`) initializer + name-merge, renamed binder —
/// the rule follows the binding identity, not the `matcher` identifier text.
#[test]
fn cross_file_symbol_call_name_merged_object_literal_satisfies_interface_renamed() {
    assert_clean_both_orders(&[
        (
            "syms.ts",
            r#"
export const tag = Symbol();
export type tag = typeof tag;
"#,
        ),
        (
            "use.ts",
            r#"
import { tag } from './syms';
export interface Tagged { [tag](): string; }
export const build = (): Tagged => ({ [tag]: () => "x" });
const obj = { [tag]: () => "x" };
export const t: Tagged = obj;
"#,
        ),
    ]);
}

/// Element access and indexed-access type position on a cross-file
/// name-merged symbol must agree with the interface member key.
#[test]
fn cross_file_symbol_name_merged_element_and_indexed_access_agree() {
    assert_clean_both_orders(&[
        (
            "k.ts",
            r#"
export const key = Symbol.for('@demo/key');
export type key = typeof key;
"#,
        ),
        (
            "m.ts",
            r#"
import { key } from './k';
export interface Box { [key]: number; }
const b: Box = { [key]: 1 };
export const v: number = b[key];
export type V = Box[key];
export const v2: number = (null as any as V);
"#,
        ),
    ]);
}

/// Re-export chain (#14129): the merged value+type symbol reaches the consumer
/// through an intermediate `export { X } from "./symbols"` module. Value-position
/// resolution must follow the re-export to the const's VALUE side; otherwise the
/// re-export collapses to the unevaluated `typeof X` type-alias body and the
/// computed key spuriously reports TS2464.
#[test]
fn cross_file_reexported_name_merged_object_literal_satisfies_interface() {
    assert_clean_both_orders(&[
        (
            "symbols.ts",
            r#"
export const matcher = Symbol.for('@demo/matcher');
export type matcher = typeof matcher;
"#,
        ),
        (
            "reexport.ts",
            r#"
export { matcher } from './symbols';
"#,
        ),
        (
            "pattern.ts",
            r#"
import { matcher } from './reexport';
export interface Matcher { [matcher](): number; }
export const make = (): Matcher => ({ [matcher]: () => 1 });
const lit = { [matcher]: () => 1 };
export const m: Matcher = lit;
"#,
        ),
    ]);
}

/// #14129 (ts-pattern witness): the merged value+type-alias symbol is consumed
/// through a **re-export barrel** (`import { matcher }; export { matcher }`),
/// and the interface keyed by `[matcher]` imports it from the barrel — a
/// two-hop alias chain. The chain must still reach the value-side `symbol`, so
/// `[matcher]` is a valid computed key (no false TS2464).
#[test]
fn cross_file_symbol_name_merged_through_named_reexport_barrel() {
    assert_clean_both_orders(&[
        (
            "symbols.ts",
            r#"
export const matcher = Symbol.for('@demo/matcher');
export type matcher = typeof matcher;
"#,
        ),
        (
            "barrel.ts",
            r#"
import { matcher } from './symbols';
export { matcher };
"#,
        ),
        (
            "pattern.ts",
            r#"
import { matcher } from './barrel';
export interface Matcher { [matcher](): number; }
const lit = { [matcher]: () => 1 };
export const m: Matcher = lit;
export declare const b: Matcher;
export const n: number = b[matcher]();
"#,
        ),
    ]);
}

/// `export { X } from "./m"` re-export-specifier barrel form (renamed binder),
/// plus a third hop, to exercise multi-hop chain following.
#[test]
fn cross_file_symbol_name_merged_through_reexport_from_chain() {
    assert_clean_both_orders(&[
        (
            "syms.ts",
            r#"
export const wireTag = Symbol.for('@demo/wire');
export type wireTag = typeof wireTag;
"#,
        ),
        (
            "mid.ts",
            r#"
export { wireTag } from './syms';
"#,
        ),
        (
            "barrel.ts",
            r#"
export { wireTag } from './mid';
"#,
        ),
        (
            "use.ts",
            r#"
import { wireTag } from './barrel';
export interface Wire { [wireTag](): string; }
const obj = { [wireTag]: () => "x" };
export const w: Wire = obj;
"#,
        ),
    ]);
}

/// `export * from "./m"` wildcard barrel: the single-hop named lookup misses
/// the star, so the chain falls back to the binder's full re-export resolver
/// and must still reach the value side.
#[test]
fn cross_file_symbol_name_merged_through_star_reexport_barrel() {
    assert_clean_both_orders(&[
        (
            "k.ts",
            r#"
export const brandKey = Symbol.for('@demo/brand');
export type brandKey = typeof brandKey;
"#,
        ),
        (
            "barrel.ts",
            r#"
export * from './k';
"#,
        ),
        (
            "m.ts",
            r#"
import { brandKey } from './barrel';
export interface Branded { [brandKey]: number; }
const b: Branded = { [brandKey]: 1 };
export const v: number = b[brandKey];
"#,
        ),
    ]);
}

/// Negative control across a re-export barrel: the key resolves, but a wrong
/// value against the declared member type must still be rejected.
#[test]
fn cross_file_symbol_name_merged_through_barrel_wrong_value_still_errors() {
    let files = &[
        (
            "s.ts",
            r#"
export const m = Symbol.for('@demo/m');
export type m = typeof m;
"#,
        ),
        (
            "barrel.ts",
            r#"
import { m } from './s';
export { m };
"#,
        ),
        (
            "p.ts",
            r#"
import { m } from './barrel';
export interface I { [m](): number; }
export const bad: I = { [m]: 123 };
"#,
        ),
    ];
    let names: Vec<&str> = files.iter().map(|(name, _)| *name).collect();
    let diags = compile_in_order(files, &names);
    assert!(
        diags
            .iter()
            .any(|d| matches!(d.code, 2322 | 2345 | 2418 | 2353)),
        "expected an assignability error for the wrong computed-property value through a barrel, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// #14130 (ts-pattern witness): the merged value+type-alias symbol is consumed
/// through a **namespace import** (`import * as symbols`) and keyed as
/// `[symbols.matcher]` — a *property-access* computed name rather than a bare
/// identifier. The const value here is a plain **string literal**
/// (`'@ts-pattern/matcher'`), not `Symbol.for(...)`, so the key is a
/// string-literal late-bound name. Resolving `symbols.matcher` in value
/// position must surface the const's literal VALUE, not the unevaluated
/// `typeof matcher` type-alias body; otherwise the interface member key fails
/// (false TS2464), the member never registers, and invoking the result of the
/// element access reports a spurious TS2722.
#[test]
fn cross_file_namespace_import_string_literal_merge_computed_key() {
    assert_clean_both_orders(&[
        (
            "symbols.ts",
            r#"
export const matcher = '@ts-pattern/matcher';
export type matcher = typeof matcher;
"#,
        ),
        (
            "helpers.ts",
            r#"
import * as symbols from './symbols';
export interface Matchable {
  [symbols.matcher](): { match: (v: unknown) => boolean };
}
declare function isMatchable(x: unknown): x is Matchable;
export function matchPattern(pattern: unknown, value: unknown) {
  if (isMatchable(pattern)) {
    return pattern[symbols.matcher]().match(value);
  }
}
"#,
        ),
    ]);
}

/// Renamed-binder variant of the namespace-import case: the rule follows the
/// merged binding's value side, not the `matcher` identifier text or the
/// `symbols` namespace alias name.
#[test]
fn cross_file_namespace_import_string_literal_merge_computed_key_renamed() {
    assert_clean_both_orders(&[
        (
            "keys.ts",
            r#"
export const wireTag = '@demo/wire-tag';
export type wireTag = typeof wireTag;
export const seq = 7;
export type seq = typeof seq;
"#,
        ),
        (
            "use.ts",
            r#"
import * as ns from './keys';
export interface Wire {
  [ns.wireTag](): string;
  [ns.seq](): number;
}
declare const w: Wire;
export const a: string = w[ns.wireTag]();
export const b: number = w[ns.seq]();
const lit = { [ns.wireTag]: () => "x", [ns.seq]: () => 1 };
export const ok: Wire = lit;
"#,
        ),
    ]);
}

/// Negative control for the namespace-import form: the key resolves correctly,
/// so a wrong value against the declared member type must still be rejected —
/// the fix surfaces the value side without silencing real diagnostics.
#[test]
fn cross_file_namespace_import_string_literal_merge_wrong_value_still_errors() {
    let files = &[
        (
            "symbols.ts",
            r#"
export const matcher = '@ts-pattern/matcher';
export type matcher = typeof matcher;
"#,
        ),
        (
            "helpers.ts",
            r#"
import * as symbols from './symbols';
export interface Matchable { [symbols.matcher](): number; }
export const bad: Matchable = { [symbols.matcher]: 123 };
"#,
        ),
    ];
    let names: Vec<&str> = files.iter().map(|(name, _)| *name).collect();
    let diags = compile_in_order(files, &names);
    assert!(
        diags
            .iter()
            .any(|d| matches!(d.code, 2322 | 2345 | 2418 | 2353)),
        "expected an assignability error for the wrong computed-property value through a namespace import, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Negative control: a genuinely wrong computed-property value must still be
/// rejected, proving the fix preserves real diagnostics rather than silencing
/// the member key. (`123` is not assignable to the method type.)
#[test]
fn cross_file_symbol_name_merged_wrong_value_still_errors() {
    let files = &[
        (
            "s.ts",
            r#"
export const m = Symbol.for('@demo/m');
export type m = typeof m;
"#,
        ),
        (
            "p.ts",
            r#"
import { m } from './s';
export interface I { [m](): number; }
export const bad: I = { [m]: 123 };
"#,
        ),
    ];
    let names: Vec<&str> = files.iter().map(|(name, _)| *name).collect();
    let diags = compile_in_order(files, &names);
    assert!(
        diags
            .iter()
            .any(|d| matches!(d.code, 2322 | 2345 | 2418 | 2353)),
        "expected an assignability error for the wrong computed-property value, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}
