//! Cross-file value-position resolution of a name-merged value + type-alias
//! symbol used as a computed property key (#13855).
//!
//! Structural rule: when an exported `const X` (a `unique symbol` binding) is
//! name-merged with `type X = typeof X` and imported into another module,
//! reading `X` in value position — e.g. as the computed key of `{ [X]: v }`
//! or an interface member `{ [X]: T }` — must resolve to the *value* side
//! (the const's `unique symbol` type), exactly as the same declaration does in
//! its own file. tsz previously re-fetched the resolved import target through
//! the local-import-alias pin; per-file binders mint colliding raw `SymbolId`s
//! (no `base_offset`), so the alias shadowed the cross-file target, the merged
//! value+type-alias VALUE side was hidden, and value-position resolution
//! collapsed to the unevaluated `typeof X` type-alias body. Cross-arena that
//! `typeof X` query never reduces to a symbol, so every computed key built
//! from it produced a spurious TS2464 (and the symbol-keyed member atoms
//! disagreed, cascading into TS2322/TS2345/TS2536).
//!
//! Controls below confirm the trigger is specifically the cross-file
//! name-merge: same-file is clean, and cross-file *without* the `type X =
//! typeof X` alias is clean. Binder names are varied so no identifier is
//! load-bearing. Negative cases confirm a genuine value mismatch still
//! surfaces its diagnostic (the fix resolves the key, it does not blanket
//! suppress member checking).

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::check_multi_file_with_global_index;
use tsz_common::common::ModuleKind;

fn check(symbols_src: &str, main_src: &str) -> Vec<Diagnostic> {
    check_multi_file_with_global_index(
        &[("./symbols.ts", symbols_src), ("./main.ts", main_src)],
        "./main.ts",
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            ..CheckerOptions::default()
        },
    )
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<(u32, String)> {
    diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn assert_clean(diagnostics: &[Diagnostic]) {
    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        codes(diagnostics)
    );
}

fn assert_has_code(diagnostics: &[Diagnostic], code: u32) {
    assert!(
        diagnostics.iter().any(|d| d.code == code),
        "expected TS{code}, got: {:?}",
        codes(diagnostics)
    );
}

// ── core repro: object literal + interface + cross-file assignment ────────────

#[test]
fn merged_symbol_object_literal_and_interface_member_resolve() {
    let diags = check(
        r#"
export declare const matcher: unique symbol;
export type matcher = typeof matcher;
"#,
        r#"
import { matcher } from "./symbols";
interface Matcher { [matcher](): number; }
export const make = (): Matcher => ({ [matcher]: () => 1 });
const lit = { [matcher]: () => 1 };
export const m: Matcher = lit;
"#,
    );
    assert_clean(&diags);
}

#[test]
fn merged_symbol_interface_member_element_access_resolves() {
    // Renamed binder: the rule keys off the merge shape, not the name `matcher`.
    let diags = check(
        r#"
export declare const brandKey: unique symbol;
export type brandKey = typeof brandKey;
"#,
        r#"
import { brandKey } from "./symbols";
interface Branded { [brandKey](): string; }
declare const b: Branded;
const s: string = b[brandKey]();
"#,
    );
    assert_clean(&diags);
}

#[test]
fn merged_symbol_object_literal_only_resolves() {
    let diags = check(
        r#"
export declare const wireTag: unique symbol;
export type wireTag = typeof wireTag;
"#,
        r#"
import { wireTag } from "./symbols";
export const lit = { [wireTag]: () => 1 };
"#,
    );
    assert_clean(&diags);
}

#[test]
fn merged_regular_symbol_interface_member_resolves() {
    // #14129: a *regular* `symbol` const (not `unique symbol`) name-merged with
    // `type matcher = typeof matcher`, imported across modules and used as a
    // computed interface key, must resolve to the value's `symbol` type — not
    // the type alias body — so it is a valid computed property name (no TS2464).
    let diags = check(
        r#"
export declare const matcher: symbol;
export type matcher = typeof matcher;
"#,
        r#"
import { matcher } from "./symbols";
interface Matcher { [matcher](): number; }
declare const b: Matcher;
const n: number = b[matcher]();
"#,
    );
    assert_clean(&diags);
}

// NOTE: re-export-chain coverage (#14129) lives in the CLI end-to-end suite
// `crates/tsz-cli/tests/symbol_keyed_member_cross_arena_cli_tests.rs`. The
// in-crate global-index harness builds a single merged binder and does not
// reproduce the driver's per-file cross-arena alias delegation, so a barrel
// re-export hop cannot be exercised here.

// ── controls: trigger is cross-file + name-merge specifically ─────────────────

#[test]
fn same_file_merge_is_clean_control() {
    // Everything in one module: the same-file resolution path already works.
    let diags = crate::test_utils::check_source_diagnostics(
        r#"
declare const matcher: unique symbol;
type matcher = typeof matcher;
interface Matcher { [matcher](): number; }
const lit = { [matcher]: () => 1 };
const m: Matcher = lit;
"#,
    );
    assert_clean(&diags);
}

#[test]
fn cross_file_without_type_alias_is_clean_control() {
    // No `type X = typeof X` merge: the import is a pure value, so value-position
    // resolution never has a type-alias body to collapse to.
    let diags = check(
        r#"
export declare const matcher: unique symbol;
"#,
        r#"
import { matcher } from "./symbols";
interface Matcher { [matcher](): number; }
const lit = { [matcher]: () => 1 };
const m: Matcher = lit;
"#,
    );
    assert_clean(&diags);
}

// ── negative: a genuine value mismatch must still be reported ─────────────────

#[test]
fn merged_symbol_member_value_mismatch_still_errors() {
    // The interface needs `[tag]: number`; the literal supplies a string-valued
    // member. The key resolves, but the member value is wrong, so assignability
    // must still fail (the fix resolves the key, it does not suppress member
    // checking).
    let diags = check(
        r#"
export declare const tag: unique symbol;
export type tag = typeof tag;
"#,
        r#"
import { tag } from "./symbols";
interface Need { [tag]: number; }
const bad: Need = { [tag]: "hello" };
"#,
    );
    // TS2418: the symbol-keyed member resolves, and the contextual member type
    // is enforced — a string value is rejected against the declared `number`.
    assert_has_code(&diags, 2418);
}

#[test]
fn merged_symbol_member_value_mismatch_same_file_baseline() {
    // Same-file baseline for the negative case, to confirm the value-mismatch
    // diagnostic is the same with or without the cross-file merge path.
    let diags = crate::test_utils::check_source_diagnostics(
        r#"
declare const tag: unique symbol;
type tag = typeof tag;
interface Need { [tag]: number; }
const bad: Need = { [tag]: "hello" };
"#,
    );
    assert_has_code(&diags, 2418);
}

// Note: the re-exported (multi-hop) form of this merge —
// `export { X } from "./symbols"` consumed by a third module — cannot be hosted
// by this in-crate harness because per-file binders mint colliding raw
// `SymbolId`s, so the multi-hop alias resolution the fix relies on does not
// reproduce here. That case (#14129) is covered end-to-end in
// `crates/tsz-cli/tests/symbol_keyed_member_cross_arena_cli_tests.rs`.
