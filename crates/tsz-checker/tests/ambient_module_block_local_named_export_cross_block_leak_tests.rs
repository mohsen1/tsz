//! A block-local (non-exported) namespace declaration must not become
//! visible in a sibling `declare module "M" { ... }` block just because an
//! unrelated `export { Orig as Exp }` specifier in the same block happens to
//! reuse its name as the export alias.
//!
//! Structural rule (one sentence):
//!
//! > When several `declare module "M" { ... }` blocks merge, tsc propagates
//! > only a block's genuine *exports* into the merged module's export table
//! > that sibling blocks see; tsz does the same through
//! > `BinderState::populate_module_exports`
//! > (`crates/tsz-binder/src/modules/binding.rs`), which must resolve an
//! > aliased export specifier's exported symbol by its LOCAL declaration name
//! > (`export { Orig as Exp }` exports the symbol named `Orig`), not by the
//! > exported alias name `Exp`.
//!
//! Before this fix, `populate_module_exports`'s `NAMED_EXPORTS` handling
//! looked the exported symbol up in scope **by the exported alias name**
//! instead of the local declaration name. When a block-local declaration
//! happened to share the alias name (e.g. a plain, non-exported
//! `namespace X` alongside `export { Y as X }` in the same block), the
//! lookup found that unrelated block-local symbol instead of `Y`, exported
//! *it* under the name `X`, and that wrong symbol then leaked into every
//! sibling block through the normal (and otherwise correct) cross-block
//! export-merge path — making the block-local namespace wrongly resolvable
//! from a block that never declared it.
//!
//! Reported witness (oracled against tsc 7.0.2, which reports TS2503 on the
//! second block's `X.I` and nothing else relevant):
//! ```ts
//! declare module "m2" {
//!     namespace X { interface I {} }
//!     function Y();
//!     export { Y as X };
//!     function Z(): X.I;
//! }
//! declare module "m2" {
//!     function Z2(): X.I;
//! }
//! ```
//!
//! Every test varies at least one user-chosen name (module specifier,
//! namespace name, alias name, member name) so the fix is structural rather
//! than shape-fingerprinted.

use tsz_checker::test_utils::check_source_codes;

const TS2503: u32 = 2503;

// ─────────────────── 1. reported repro and its renamings ───────────────────

/// The conformance witness itself: the block-local namespace must stay
/// invisible from the sibling block, even though an aliased export in the
/// declaring block reuses its name.
#[test]
fn block_local_namespace_does_not_leak_across_merged_blocks_via_export_alias() {
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
        "block-local namespace must not leak into a sibling block: {codes:?}"
    );
}

/// Same shape with every binder renamed — the rule is structural, not keyed
/// to the witness's identifiers.
#[test]
fn renamed_binders_block_local_namespace_does_not_leak() {
    let codes = check_source_codes(
        r#"
declare module "mod-alpha" {
    namespace Qq {
        interface Inner { }
    }
    function Helper(): void;
    export { Helper as Qq };
    function Use(): Qq.Inner;
}

declare module "mod-alpha" {
    function UseElsewhere(): Qq.Inner;
}
"#,
    );
    assert!(
        codes.contains(&TS2503),
        "renamed binders must behave identically: {codes:?}"
    );
}

/// A deeper container: the block-local namespace shadowed by the alias is
/// reached through a qualified name.
#[test]
fn nested_qualified_block_local_namespace_does_not_leak() {
    let codes = check_source_codes(
        r#"
declare module "deep-mod" {
    namespace Outer {
        namespace Middle {
            interface Leaf { }
        }
    }
    function fallback(): void;
    export { fallback as Outer };
    function consume(): Outer.Middle.Leaf;
}

declare module "deep-mod" {
    function consumeElsewhere(): Outer.Middle.Leaf;
}
"#,
    );
    assert!(
        codes.contains(&TS2503),
        "nested qualified block-local namespace must not leak: {codes:?}"
    );
}

/// Three merged blocks: the leak must not reach any sibling, not just the
/// immediately following one.
#[test]
fn block_local_namespace_does_not_leak_into_third_merged_block() {
    let codes = check_source_codes(
        r#"
declare module "m9" {
    namespace X {
        interface I { }
    }
    function Y(): void;
    export { Y as X };
}

declare module "m9" {
    function unrelated(): void;
}

declare module "m9" {
    function Z2(): X.I;
}
"#,
    );
    assert!(
        codes.contains(&TS2503),
        "block-local namespace must not leak past an intervening block: {codes:?}"
    );
}

// ───────────────────────── 2. positive/negative controls ──────────────────

/// The declaring block itself must still resolve `X.I` cleanly — the fix
/// must not overcorrect into hiding the local namespace from its own block.
#[test]
fn declaring_block_still_resolves_its_own_local_namespace() {
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
"#,
    );
    assert!(
        !codes.contains(&TS2503),
        "the declaring block must keep resolving its own local namespace: {codes:?}"
    );
}

/// A genuinely (non-aliased) exported namespace must still merge correctly
/// across blocks — the fix must not disturb the already-correct path for
/// direct declarations reached via `node_symbols`.
#[test]
fn genuinely_exported_namespace_still_merges_across_blocks() {
    let codes = check_source_codes(
        r#"
declare module "m11" {
    export namespace X {
        interface I { }
    }
}

declare module "m11" {
    function Z2(): X.I;
}
"#,
    );
    assert!(
        !codes.contains(&TS2503),
        "a genuinely exported namespace must still merge across blocks: {codes:?}"
    );
}

/// When the alias does not collide with any block-local declaration, the
/// aliased export must still correctly export the target VALUE across
/// blocks (the fix must not break the working, non-colliding case).
#[test]
fn non_colliding_aliased_value_export_still_crosses_blocks() {
    let codes = check_source_codes(
        r#"
declare module "m12" {
    function Y(): void;
    export { Y as W };
}

declare module "m12" {
    const w: typeof W;
}
"#,
    );
    assert!(
        !codes.contains(&TS2503),
        "a non-colliding aliased value export must still cross blocks: {codes:?}"
    );
}

/// A block with no `export` statement at all is in tsc's *implicit-export*
/// mode (see the `hasExportDeclarations` doc comment in `binding.rs`): every
/// top-level declaration, including a plain `namespace X`, is treated as
/// exported and does cross into sibling blocks. This is the control that
/// isolates the bug: adding an unrelated `export { ... }` specifier to the
/// same block (as in the other tests above) is what disables implicit
/// export and makes the plain `namespace X` block-local again.
#[test]
fn block_with_no_export_at_all_implicitly_exports_and_crosses_blocks() {
    let codes = check_source_codes(
        r#"
declare module "m13" {
    namespace X {
        interface I { }
    }
}

declare module "m13" {
    function Z2(): X.I;
}
"#,
    );
    assert!(
        !codes.contains(&TS2503),
        "an implicit-export block's namespace must cross into a sibling block: {codes:?}"
    );
}
