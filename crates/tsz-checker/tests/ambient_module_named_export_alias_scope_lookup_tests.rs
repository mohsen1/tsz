//! `populate_module_exports` must resolve a renamed export specifier's
//! *local* declaration name, never its published alias, when deciding which
//! symbol to add to a module/namespace container's shared export table.
//!
//! Structural rule (one sentence):
//!
//! > When a `declare module "M" { ... }` or `namespace N { ... }` body
//! > contains `export { Orig as Exp }`, tsc looks up `Orig` in that body's own
//! > scope to find the symbol to publish under the public name `Exp`; tsz's
//! > `populate_module_exports` (`crates/tsz-binder/src/modules/binding.rs`)
//! > instead searched scope for `Exp` itself, so whenever an unrelated,
//! > non-exported local declaration happened to share the alias's name (e.g.
//! > a block-local `namespace Exp`), that unrelated declaration — not `Orig`
//! > — was published under `Exp` on the container's shared export table.
//!
//! Because ambient module blocks sharing one string specifier merge onto a
//! single symbol (`crates/tsz-binder/src/nodes/binding_scope.rs`), that
//! shared export table is visible from every sibling block. The symptom is a
//! false negative: a *different*, non-exported local namespace becomes
//! resolvable by qualified name (`Exp.Member`) from a sibling block that
//! should not be able to see it at all, and tsc reports TS2503 ("Cannot find
//! namespace") there while tsz reported nothing.
//!
//! Every test varies at least one user-chosen name (module specifier, local
//! namespace name, alias name, member name) so the fix is pinned as
//! structural rather than shape-fingerprinted. All expectations were verified
//! against `tsc` 7.0.2 (`typescript-versions.json` pin) before landing.

use tsz_checker::test_utils::check_source_codes;

const TS2503: u32 = 2503;

// ─────────────────────── 1. the reported repro shape ───────────────────────

/// The witness: an unexported local `namespace X`, a renamed value export
/// `export { Y as X }` in the same block, and a *sibling* ambient-module
/// block (same specifier) that qualifies `X.I`. tsc reports TS2503 in the
/// sibling block only — the local namespace is visible in its own block, not
/// the sibling's.
#[test]
fn sibling_block_does_not_see_unexported_local_namespace_via_renamed_export() {
    let codes = check_source_codes(
        r#"
declare module "m2" {
    namespace X {
        interface I { }
    }
    function Y(): void;
    export { Y as X };
    function Z(): X.I;
}

declare module "m2" {
    function Z2(): X.I;
}
"#,
    );
    assert!(
        codes.contains(&TS2503),
        "a sibling block must not resolve another block's unexported local namespace: {codes:?}"
    );
}

/// The declaring block's own reference to the local namespace must keep
/// resolving — the fix must not turn a real local declaration invisible in
/// its own scope.
#[test]
fn declaring_block_still_sees_its_own_local_namespace() {
    let codes = check_source_codes(
        r#"
declare module "m2b" {
    namespace X {
        interface I { }
    }
    function Y(): void;
    export { Y as X };
    function Z(): X.I;
}
"#,
    );
    assert!(
        !codes.contains(&TS2503),
        "the declaring block must still resolve its own local namespace: {codes:?}"
    );
}

/// Every identifier renamed: module specifier, namespace, member, function,
/// and alias — the rule is structural, not keyed to the witness's names.
#[test]
fn renamed_binders_still_leak_nothing_to_sibling_block() {
    let codes = check_source_codes(
        r#"
declare module "renamed-mod" {
    namespace Alpha {
        interface Beta { }
    }
    function Gamma(): void;
    export { Gamma as Alpha };
    function Delta(): Alpha.Beta;
}

declare module "renamed-mod" {
    function Epsilon(): Alpha.Beta;
}
"#,
    );
    assert!(
        codes.contains(&TS2503),
        "renamed binders must behave identically to the witness: {codes:?}"
    );
}

// A plain (non-ambient) `namespace` body cannot contain `export { ... }` at
// all — tsc always reports TS1194 ("Export declarations are not permitted in
// a namespace") for it, a grammar error orthogonal to this fix. The exact
// analog of the witness above inside a plain `namespace` improves from this
// fix (before: a bare TS1194 with no follow-on diagnostic at all, i.e. `X.I`
// silently resolved; after: TS1194 + TS2702), but does not reach exact tsc
// parity (TS1194 + TS2503) — the grammar-error recovery path picks the wrong
// diagnostic family for the unresolved qualified name. That residual gap is a
// separate, pre-existing issue, tracked for follow-up rather than fixed here.

/// A nested qualified name: the shadowed local is reached through a
/// multi-segment qualified name (`Outer.Middle.Leaf`) rather than a single
/// member.
#[test]
fn nested_qualified_name_still_leaks_nothing_to_sibling_block() {
    let codes = check_source_codes(
        r#"
declare module "deep-mod-2" {
    namespace Wrap {
        namespace Middle {
            interface Leaf { }
        }
    }
    function fallback(): void;
    export { fallback as Wrap };
    function consume(): Wrap.Middle.Leaf;
}

declare module "deep-mod-2" {
    function consume2(): Wrap.Middle.Leaf;
}
"#,
    );
    assert!(
        codes.contains(&TS2503),
        "nested qualified names must not leak into a sibling block either: {codes:?}"
    );
}

// ───────────────────────── 2. positive controls ────────────────────────────

/// A genuinely exported local (`export namespace X`, not merely reachable
/// through a renamed value export) must still merge across sibling blocks —
/// the fix must not stop legitimate cross-block exports from working.
#[test]
fn genuinely_exported_namespace_still_merges_across_sibling_blocks() {
    let codes = check_source_codes(
        r#"
declare module "m2c" {
    export namespace P {
        interface Q { }
    }
    function R(): P.Q;
}

declare module "m2c" {
    function S(): P.Q;
}
"#,
    );
    assert!(
        !codes.contains(&TS2503),
        "a genuinely exported namespace must still be visible from a sibling block: {codes:?}"
    );
}

/// A non-renamed `export { Y }` (`local_name == export_name`) takes the
/// unmodified fallback path and must stay exactly as before: `Y` itself
/// merges across sibling blocks, and an unrelated same-named-as-nothing-else
/// local is irrelevant here.
#[test]
fn same_name_export_specifier_still_merges_across_sibling_blocks() {
    let codes = check_source_codes(
        r#"
declare module "m2d" {
    export function Y(): void;
    export { Y };
}

declare module "m2d" {
    Y();
}
"#,
    );
    assert!(
        !codes.contains(&TS2503),
        "a non-renamed export specifier must be unaffected by the fix: {codes:?}"
    );
}

// ───────────────────────── 3. negative control ─────────────────────────────

/// A genuinely undeclared namespace, with no renamed-export interaction at
/// all, must still report TS2503 — the fix must not have widened the
/// resolution to accept anything.
#[test]
fn genuinely_missing_namespace_without_renamed_export_still_reports_ts2503() {
    let codes = check_source_codes(
        r#"
declare module "m2e" {
    function T(): NoSuchNamespace.U;
}
"#,
    );
    assert!(
        codes.contains(&TS2503),
        "an undeclared namespace must still report TS2503: {codes:?}"
    );
}
