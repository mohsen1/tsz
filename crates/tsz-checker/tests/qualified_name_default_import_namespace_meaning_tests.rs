//! Regression tests: a qualified type name rooted at an IMPORT ALIAS must ask
//! the alias's *target* for namespace meaning, not skip the question.
//!
//! Structural rule: for `Alias.Member` in type position, tsc resolves `Alias`
//! requesting namespace meaning. A *default* import (`import D from "./m"`)
//! binds the module's `default` export, which contributes only the
//! default-exported declaration's own meaning — never a same-named merge
//! partner's. A default import of a class/interface/function is therefore not a
//! namespace: `import D from "./m"; type X = D.Member` is `TS2702` ("only
//! refers to a type, but is being used as a namespace here"), never `TS2694`
//! ("no exported member"). The fix is scoped to default imports; named and
//! whole-module namespace imports keep their existing resolution. Owner: the
//! qualified-name gate in `state/type_analysis/qualified_names.rs`.
//!
//! Before the fix the gate skipped every import alias unconditionally, so a
//! default-imported class was treated as a namespace anchor and a missing
//! member surfaced `TS2694`. These cases pin the three flipped conformance
//! rows (`decoratorMetadataWithImportDeclarationNameCollision7`,
//! `defaultExportsCannotMerge02`, `defaultExportsCannotMerge03`) and the
//! controls that must NOT move: an `export default <namespace>`/`<enum>` that
//! keeps its namespace meaning, and a whole-module namespace import.
//!
//! Cases vary the exporting declaration (class / interface / merged), the local
//! binding name, and the imported member name to prove the rule is structural
//! and not keyed to any single spelling.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file_with_global_index;

fn opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
}

/// Error codes emitted for the entry file of a multi-file project.
fn codes(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    check_multi_file_with_global_index(files, entry, opts())
        .into_iter()
        .map(|d| d.code)
        .collect()
}

// ───────────────────── default import of a class ─────────────────────

#[test]
fn default_imported_class_used_as_namespace_is_ts2702() {
    // `import D from "./dep"; var a: D.X` — the default export is a class, which
    // has no namespace meaning, so `D.X` is TS2702, not TS2694.
    let codes = codes(
        &[
            ("dep.ts", "export default class D {}\n"),
            ("main.ts", "import D from './dep';\nvar a: D.X;\n"),
        ],
        "main.ts",
    );
    assert!(
        codes.contains(&2702),
        "default-imported class used as a namespace must be TS2702, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2694),
        "must not fall through to the missing-member TS2694 path, got: {codes:?}"
    );
}

#[test]
fn renamed_default_import_of_class_used_as_namespace_is_ts2702() {
    // Vary the local binding name: the class is `Widget`, the import binding is
    // `Gadget`. The rule must not depend on the names matching.
    let codes = codes(
        &[
            ("widget.ts", "export default class Widget {}\n"),
            (
                "use.ts",
                "import Gadget from './widget';\nvar g: Gadget.Missing;\n",
            ),
        ],
        "use.ts",
    );
    assert!(
        codes.contains(&2702),
        "renamed default import of a class used as a namespace must be TS2702, got: {codes:?}"
    );
    assert!(!codes.contains(&2694), "got: {codes:?}");
}

#[test]
fn default_imported_inline_interface_used_as_namespace_is_ts2702() {
    // `export default interface OnlyType { ... }` — a pure type default. Used as
    // a namespace it is TS2702.
    let codes = codes(
        &[
            (
                "dep.ts",
                "export default interface OnlyType { k: number }\n",
            ),
            (
                "main.ts",
                "import OnlyType from './dep';\nvar a: OnlyType.X;\n",
            ),
        ],
        "main.ts",
    );
    assert!(
        codes.contains(&2702),
        "default-imported interface used as a namespace must be TS2702, got: {codes:?}"
    );
    assert!(!codes.contains(&2694), "got: {codes:?}");
}

#[test]
fn default_imported_class_merged_with_namespace_is_ts2702() {
    // The `defaultExportsCannotMerge02` shape: `export default class Decl {}`
    // beside an exported `namespace Decl` — the illegal merge tsc flags TS2652.
    // The default export is still only the class, so `Entity.I` is TS2702 even
    // though the namespace exports `I`.
    let codes = codes(
        &[
            (
                "m1.ts",
                "export default class Decl {}\nexport namespace Decl { export interface I { q: number } }\n",
            ),
            ("m2.ts", "import Entity from './m1';\nvar y: Entity.I;\n"),
        ],
        "m2.ts",
    );
    assert!(
        codes.contains(&2702),
        "a merged default class+namespace used as a namespace must be TS2702, got: {codes:?}"
    );
    assert!(!codes.contains(&2694), "got: {codes:?}");
}

// ───────────────────── controls that must NOT move ─────────────────────

#[test]
fn export_default_namespace_keeps_namespace_meaning() {
    // `export default m` naming a namespace DOES carry namespace meaning, so a
    // present member resolves with no error and a MISSING member is TS2694
    // (never TS2702). This is the direct-namespace-default control.
    let present = codes(
        &[
            (
                "dep.ts",
                "namespace m { export interface foo { a: number } }\nexport default m;\n",
            ),
            ("main.ts", "import D from './dep';\nvar q: D.foo;\n"),
        ],
        "main.ts",
    );
    assert!(
        !present.contains(&2702) && !present.contains(&2694),
        "export default <namespace> member must resolve cleanly, got: {present:?}"
    );

    // A missing member must NOT flip to TS2702 (the regression this fix must
    // avoid). Whether the no-lib multi-file harness also surfaces the TS2694
    // member-lookup diagnostic is asserted end-to-end by the conformance suite;
    // here the load-bearing guard is that the default-imported namespace keeps
    // its namespace meaning.
    let missing = codes(
        &[
            (
                "dep.ts",
                "namespace m { export interface foo { a: number } }\nexport default m;\n",
            ),
            ("main.ts", "import D from './dep';\nvar q: D.bar;\n"),
        ],
        "main.ts",
    );
    assert!(
        !missing.contains(&2702),
        "a missing member of an export-default namespace must not become TS2702, got: {missing:?}"
    );
}

#[test]
fn namespace_import_qualifier_keeps_member_lookup() {
    // The `import * as NS` control: a whole-module namespace import is always a
    // namespace anchor, so a missing member is TS2694, never TS2702.
    let codes = codes(
        &[
            (
                "dep.ts",
                "export interface Thing { a: number }\nexport interface Other { b: number }\n",
            ),
            (
                "main.ts",
                "import * as NS from './dep';\nvar a: NS.Missing;\n",
            ),
        ],
        "main.ts",
    );
    assert!(
        codes.contains(&2694),
        "a namespace-import qualifier must keep the TS2694 member-lookup path, got: {codes:?}"
    );
    assert!(!codes.contains(&2702), "got: {codes:?}");
}

#[test]
fn valid_namespace_member_through_default_namespace_import_is_not_flagged() {
    // Enum default export retains namespace meaning through a default import:
    // `import E from "./e"; type X = E.Member` for an `export default <enum>`
    // is NOT a TS2702 (an enum is a namespace-capable value).
    let codes = codes(
        &[
            ("e.ts", "enum Color { Red, Green }\nexport default Color;\n"),
            ("main.ts", "import C from './e';\nvar bad: C.Nope;\n"),
        ],
        "main.ts",
    );
    // `Nope` is not a member of the enum, so TS2694 (member lookup), never
    // TS2702 (enum has namespace meaning).
    assert!(
        !codes.contains(&2702),
        "an enum default export keeps namespace meaning (no TS2702), got: {codes:?}"
    );
}
