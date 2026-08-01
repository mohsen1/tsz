//! Renamed export specifiers inside a namespace/module container must not
//! create an in-module lexical binding.
//!
//! Structural rule (one sentence):
//!
//! > When a container body — `declare module "M" { ... }` or a `namespace N
//! > { ... }` — contains `export { Orig as Exp }`, tsc records `Exp` on that
//! > container's export table only and never as an in-module lexical/type
//! > binding, so a same-named local declaration of `Exp` keeps resolving
//! > locally; tsz does the same through the binder's export-specifier path
//! > (`crates/tsz-binder/src/modules/import_export.rs`).
//!
//! Before this fix the binder seeded `Exp` into the current scope and
//! `file_locals` whenever the specifier appeared inside a container, which
//! clobbered a same-named local. In the witness below the clobbered local is
//! `namespace X`, so `X.I` stopped resolving as a namespace and produced a
//! spurious TS2503 ("Cannot find namespace 'X'") in an ambient module and a
//! spurious TS2702 ("'X' only refers to a type, but is being used as a
//! namespace here") in a plain namespace. tsc reports neither.
//!
//! The same-name (`export { X }`) and top-level-module forms were already
//! correct — `seed_module_export`'s own doc comment describes exactly this
//! clobbering hazard for the top-level case; the container case simply
//! bypassed it.
//!
//! Reported witness:
//! `TypeScript/tests/cases/compiler/exportSpecifierAndExportedMemberDeclaration.ts`,
//! whose only conformance delta was the extra TS2503.
//!
//! Every test varies at least one user-chosen name (module specifier,
//! namespace name, alias name, member name) so the fix is structural rather
//! than shape-fingerprinted. All expectations below were pinned against
//! `tsc` 7.0.2 before the fix was written.

use tsz_checker::test_utils::check_source_codes;

const TS2503: u32 = 2503;
const TS2702: u32 = 2702;

// ─────────────────── 1. reported repro and its renamings ───────────────────

/// The conformance witness itself: a renamed export whose exported name
/// collides with a local `namespace` must leave the namespace resolvable,
/// both in the declaring block and in a merged sibling block.
#[test]
fn ambient_module_renamed_export_keeps_local_namespace_resolvable() {
    let codes = check_source_codes(
        r#"
declare module "m2" {
    export namespace X {
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
        !codes.contains(&TS2503),
        "renamed export must not shadow the local namespace: {codes:?}"
    );
}

/// Same shape with every binder renamed — the rule is structural, not keyed
/// to the witness's identifiers.
#[test]
fn renamed_binders_keep_local_namespace_resolvable() {
    let codes = check_source_codes(
        r#"
declare module "mod-alpha" {
    export namespace Qq {
        interface Inner { }
    }
    function Helper(): void;
    export { Helper as Qq };
    function Use(): Qq.Inner;
}
"#,
    );
    assert!(
        !codes.contains(&TS2503),
        "renamed binders must behave identically: {codes:?}"
    );
}

/// Declaration order must not matter.
///
/// Worth knowing: this shape is clean on the **CLI** even without the fix
/// (the namespace, bound after the specifier, overwrites the clobbered scope
/// slot back) but still fails in this **harness** without it. The two
/// surfaces disagree on binding order for the same source, so a CLI-only
/// check would have called this case healthy. Pinning it here keeps the
/// order-independence real rather than order-accidental.
#[test]
fn specifier_before_namespace_keeps_local_namespace_resolvable() {
    let codes = check_source_codes(
        r#"
declare module "m3" {
    function Y(): void;
    export { Y as X };
    export namespace X {
        interface I { }
    }
    function Z(): X.I;
}
"#,
    );
    assert!(
        !codes.contains(&TS2503),
        "specifier-before-namespace must resolve: {codes:?}"
    );
}

/// A deeper container: the local shadowed by the specifier is a nested
/// namespace reached through a qualified name.
#[test]
fn renamed_export_keeps_nested_qualified_namespace_resolvable() {
    let codes = check_source_codes(
        r#"
declare module "deep-mod" {
    export namespace Outer {
        namespace Middle {
            interface Leaf { }
        }
    }
    function fallback(): void;
    export { fallback as Outer };
    function consume(): Outer.Middle.Leaf;
}
"#,
    );
    assert!(
        !codes.contains(&TS2503),
        "nested qualified namespace must stay resolvable: {codes:?}"
    );
}

// ──────────────── 2. the plain-namespace surface (TS2702) ─────────────────

/// The same binder path drives a plain `namespace` body, where the symptom
/// of the clobber is TS2702 rather than TS2503. The body is still a grammar
/// error (TS1194, "Export declarations are not permitted in a namespace"),
/// which tsz already reports and tsc agrees on — but tsc reports *only*
/// that, so the extra TS2702 was a second false positive from one cause.
#[test]
fn namespace_body_renamed_export_does_not_add_ts2702() {
    let codes = check_source_codes(
        r#"
namespace Outer {
    export namespace X {
        export interface I { }
    }
    function Y() {}
    export { Y as X };
    export function Z(): X.I { return null as any; }
}
"#,
    );
    assert!(
        !codes.contains(&TS2702),
        "namespace body must not report a spurious TS2702: {codes:?}"
    );
}

// ───────────────────────── 3. negative controls ───────────────────────────

/// A genuinely undeclared namespace must still report TS2503. Without this,
/// the fix could have been "stop reporting TS2503 here" rather than "resolve
/// the local correctly".
#[test]
fn genuinely_missing_namespace_still_reports_ts2503() {
    let codes = check_source_codes(
        r#"
declare module "m5" {
    function Z(): Missing.I;
}
"#,
    );
    assert!(
        codes.contains(&TS2503),
        "an undeclared namespace must still report TS2503: {codes:?}"
    );
}

/// A missing namespace reached through a renamed export's *exported* name
/// from inside the same container is still missing: the alias lives on the
/// container's export table, not in the body's lexical scope, so referring
/// to it locally as a namespace must not start resolving.
#[test]
fn renamed_export_name_is_not_itself_a_local_namespace() {
    let codes = check_source_codes(
        r#"
declare module "m10" {
    namespace Src {
        interface I { }
    }
    export { Src as Pub };
    function Z(): Pub.I;
}
"#,
    );
    assert!(
        codes.contains(&TS2503),
        "the exported alias name must not become a local namespace: {codes:?}"
    );
}

/// Non-renamed `export { Y }` inside a container is untouched by the fix and
/// must stay clean.
#[test]
fn same_name_export_specifier_stays_clean() {
    let codes = check_source_codes(
        r#"
declare module "m7" {
    namespace X {
        interface I { }
    }
    function Y(): void;
    export { Y };
    function Z(): X.I;
}
"#,
    );
    assert!(
        !codes.contains(&TS2503),
        "same-name export specifier must stay clean: {codes:?}"
    );
}

/// The top-level module form already routed through `seed_module_export`;
/// pinned here so a future change to that branch cannot silently regress the
/// surface this fix aligns the container case with.
#[test]
fn top_level_renamed_export_keeps_local_namespace_resolvable() {
    let codes = check_source_codes(
        r#"
export namespace X {
    export interface I { }
}
function Y() {}
export { Y as X };
export function Z(): X.I { return null as any; }
"#,
    );
    assert!(
        !codes.contains(&TS2503),
        "top-level renamed export must stay clean: {codes:?}"
    );
}
