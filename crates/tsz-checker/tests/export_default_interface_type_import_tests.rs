//! Regression tests: a default-imported inline `export default interface`
//! must resolve to the interface type in the importing file.
//!
//! Structural rule: the binder models `export default interface A { ... }` as a
//! type-only `"default"` ALIAS symbol whose declaration is the inline
//! `INTERFACE_DECLARATION` node, with no `value_declaration`. The same node also
//! declares a local interface symbol in scope. When the alias has no
//! `import_module`, no `value_declaration`, and one of its declarations is an
//! inline interface/type-alias node, the checker resolves to that local type
//! symbol's type (owner: `compute_type_of_symbol` alias path in the checker).
//! Before the fix, the alias fell through to the generic alias-`any` path, so a
//! default-imported interface annotation widened to `any` — silently dropping
//! both definite-assignment (TS2454) and assignability (TS2322) diagnostics in
//! the consuming file. The TS2454 facet is covered end-to-end by the
//! `exportDefaultInterface` conformance fixture; these unit tests cover the
//! type-resolution facet (TS2322) that the no-lib multi-file harness supports.
//!
//! Cases vary the interface name, the imported binding name, and the import
//! shape (`import`, `import type`) to prove the rule is structural and not keyed
//! to any single spelling.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file_with_global_index;

fn opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
}

/// Return the error codes emitted for the entry file of a multi-file project.
fn codes(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    check_multi_file_with_global_index(files, entry, opts())
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn default_imported_inline_interface_resolves_type_for_ts2322() {
    // The imported interface type must be live for assignability: assigning its
    // `number` property to a `string` is TS2322. If the annotation widened to
    // `any` (the bug), no TS2322 would fire.
    let codes = codes(
        &[
            ("a.ts", "export default interface A { value: number; }\n"),
            (
                "b.ts",
                "import A from './a';\nvar a: A;\nconst bad: string = a.value;\n",
            ),
        ],
        "b.ts",
    );
    assert!(
        codes.contains(&2322),
        "default-imported inline interface property type must be live (TS2322), got: {codes:?}"
    );
}

#[test]
fn renamed_default_import_of_inline_interface_resolves_type() {
    // Vary both binder names: the interface is `Widget`, the import binding is
    // `Gadget`. The rule must not depend on the names matching.
    let codes = codes(
        &[
            (
                "widget.ts",
                "export default interface Widget { size: number; }\n",
            ),
            (
                "use.ts",
                "import Gadget from './widget';\ndeclare const g: Gadget;\nconst bad: string = g.size;\n",
            ),
        ],
        "use.ts",
    );
    assert!(
        codes.contains(&2322),
        "renamed default import of inline interface must resolve type (TS2322), got: {codes:?}"
    );
}

#[test]
fn import_type_default_of_inline_interface_resolves_type() {
    // `import type` default of an inline interface must also resolve the type.
    let codes = codes(
        &[
            ("m.ts", "export default interface Conf { level: number; }\n"),
            (
                "u.ts",
                "import type C from './m';\ndeclare const c: C;\nconst bad: string = c.level;\n",
            ),
        ],
        "u.ts",
    );
    assert!(
        codes.contains(&2322),
        "import type default of inline interface must resolve type (TS2322), got: {codes:?}"
    );
}

#[test]
fn default_imported_inline_interface_is_not_widened_to_any() {
    // Negative facet: a property that does NOT exist on the imported interface
    // must surface TS2339. If the annotation widened to `any` (the bug), the
    // missing-property access would be silently accepted.
    let codes = codes(
        &[
            (
                "model.ts",
                "export default interface Model { name: string; }\n",
            ),
            (
                "view.ts",
                "import Model from './model';\ndeclare const m: Model;\nconst missing = m.doesNotExist;\n",
            ),
        ],
        "view.ts",
    );
    assert!(
        codes.contains(&2339),
        "default-imported inline interface must not widen to `any` (expect TS2339), got: {codes:?}"
    );
}
