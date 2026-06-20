//! Cross-module `unique symbol` computed-key identity guards (issue #14126).
//!
//! A `const k = Symbol.for(...)` name-merged with `type k = typeof k` is a
//! merged `TYPE_ALIAS | VALUE` symbol. When such a binding is reached through a
//! re-export chain (`export { k } from "./symbols"`, or `import { k };
//! export { k }`), two things historically broke:
//!
//! 1. The imported binding's VALUE type resolved to the type-alias body
//!    (`typeof k`), which collapses to `error` across the extra hop — so a
//!    computed key `[k]()` emitted a false TS2464 ("A computed property name
//!    must be of type 'string', 'number', 'symbol', or 'any'") and value-side
//!    assignability silently passed (no TS2322 against `symbol`).
//!
//! 2. The `__unique_<id>` member key embedded whichever per-file alias *copy*
//!    resolved at each site. An interface member `[k](): T` keyed in one
//!    importing file's view and a fresh object literal `{ [k]() {} }` keyed in
//!    another file's view embedded different ids for the *same* declaration, so
//!    a literal flowing into a generic-constrained parameter
//!    (`P extends Matcher`) reported a spurious TS2353 / TS2561 excess property
//!    on the very method the constraint requires (ts-pattern's `Matcher`).
//!
//! tsc treats the `const` as one symbol with one identity regardless of import
//! path. These tests pin both the value-meaning resolution and the canonical
//! key identity, varying binder names so the fix stays structural.

use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_multi_file;
use tsz_common::ModuleKind;

fn opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        target: ScriptTarget::ES2020,
        module: ModuleKind::CommonJS,
        no_lib: false,
        ..Default::default()
    }
}

fn codes(diags: &[Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

fn assert_absent(diags: &[Diagnostic], code: u32, context: &str) {
    let hits: Vec<_> = diags
        .iter()
        .filter(|d| d.code == code)
        .map(|d| &d.message_text)
        .collect();
    assert!(
        hits.is_empty(),
        "{context}: expected no TS{code}; got {hits:#?}\nall codes: {:?}",
        codes(diags)
    );
}

// ── Primary repro: re-export + generic constraint (ts-pattern Matcher) ───────

/// A merged `const matcher = Symbol.for(...)` / `type matcher = typeof matcher`
/// re-exported through a barrel, used as a symbol-computed method in both a
/// generic constraint interface and a fresh object literal flowing into that
/// constraint. tsc emits nothing; tsz must agree (no TS2353/TS2561/TS2464).
#[test]
fn reexported_symbol_method_into_generic_constraint_has_no_excess() {
    let files = [
        (
            "symbols.ts",
            r#"
export const matcher = Symbol.for('@x/matcher');
export type matcher = typeof matcher;
"#,
        ),
        (
            "patterns.ts",
            r#"
import { matcher } from './symbols';
export { matcher };
"#,
        ),
        (
            "pattern_type.ts",
            r#"
import { matcher } from './patterns';
type MatchResult = { matched: boolean };
export type MatcherProtocol<input, m> = {
  match: <I>(value: I | input) => MatchResult;
  matcherType?: m;
};
export interface Matcher<input = any, narrowed = any, m = 'default'> {
  [matcher](): MatcherProtocol<input, m>;
}
type Chainable<p> = p & {};
export function chainable<p extends Matcher<any, any, any>>(p: p): Chainable<p> {
  return p as Chainable<p>;
}
"#,
        ),
        (
            "use.ts",
            r#"
import { matcher } from './symbols';
import { chainable } from './pattern_type';
export function optional<input>() {
  return chainable({
    [matcher]() {
      return {
        match: <I>(value: I | input) => ({ matched: true }),
        matcherType: 'optional' as const,
      };
    },
  });
}
"#,
        ),
    ];
    let diags = check_multi_file(&files, "use.ts", opts());
    assert_absent(
        &diags,
        2353,
        "re-exported symbol method into generic constraint",
    );
    assert_absent(
        &diags,
        2561,
        "re-exported symbol method into generic constraint",
    );
    assert_absent(
        &diags,
        2464,
        "re-exported symbol method into generic constraint",
    );
}

/// Renamed binders everywhere — the fix keys on the symbol declaration, not the
/// chosen identifier text.
#[test]
fn reexported_symbol_method_renamed_binders_has_no_excess() {
    let files = [
        (
            "sym.ts",
            r#"
export const tag = Symbol.for('@x/tag');
export type tag = typeof tag;
"#,
        ),
        (
            "barrel.ts",
            r#"
import { tag } from './sym';
export { tag };
"#,
        ),
        (
            "proto.ts",
            r#"
import { tag } from './barrel';
export type Proto<a, b> = { run: <I>(v: I | a) => boolean; kind?: b };
export interface Tagged<a = any, b = 'def'> {
  [tag](): Proto<a, b>;
}
type Wrap<q> = q & {};
export function wrap<q extends Tagged<any, any>>(q: q): Wrap<q> {
  return q as Wrap<q>;
}
"#,
        ),
        (
            "site.ts",
            r#"
import { tag } from './sym';
import { wrap } from './proto';
export function build<a>() {
  return wrap({
    [tag]() {
      return { run: <I>(v: I | a) => true, kind: 'def' as const };
    },
  });
}
"#,
        ),
    ];
    let diags = check_multi_file(&files, "site.ts", opts());
    assert_absent(&diags, 2353, "renamed binders");
    assert_absent(&diags, 2561, "renamed binders");
    assert_absent(&diags, 2464, "renamed binders");
}

// ── Value-meaning resolution ─────────────────────────────────────────────────
//
// The barrel-re-export *value* path (`import { sym } from './barrel'` where
// `barrel` re-exports a merged `const`/`type`, then `[sym]()` / `const x: string
// = sym`) is verified end-to-end against ts-pattern and via the `tsz` CLI on a
// 3-file project: the merged binding keeps its `unique symbol` value type (no
// false TS2464, and `symbol` is still rejected against `string` with TS2322).
// The simplified `check_multi_file` harness does not materialise that
// cross-file re-export value path the same way the driver does (the same
// limitation the sibling `ts2527_unique_symbol_via_reexport_tests` documents),
// so the in-harness value assertion below uses a direct import; the re-export
// *keying* path — the actual #14126 false positive — is covered by the generic
// constraint tests above, which do flow a barrel-reached interface member.

// ── Negative / regression guard ─────────────────────────────────────────────

/// A direct (single-hop) import of the merged binding already worked; it must
/// stay working and keep the same value semantics.
#[test]
fn direct_import_merged_symbol_still_correct() {
    let files = [
        (
            "symbols.ts",
            r#"
export const sym = Symbol.for('@x/sym');
export type sym = typeof sym;
"#,
        ),
        (
            "probe.ts",
            r#"
import { sym } from './symbols';
const bad: string = sym;
const obj = { [sym]() { return 1; } };
"#,
        ),
    ];
    let diags = check_multi_file(&files, "probe.ts", opts());
    assert_absent(&diags, 2464, "direct import merged symbol computed key");
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "direct import must still reject `symbol` assigned to `string`; got {:?}",
        codes(&diags)
    );
}
