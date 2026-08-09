//! TS2540 on `export const` members accessed through a whole-module import
//! binding (`import m = require('...')`).
//!
//! Structural rule: `import m = require('./mod')` binds `m` to the target
//! module namespace, whose `export const` members are readonly — the same rule
//! as a same-file `namespace M { export const x = 0 }`, where `M.x = 1` is
//! TS2540. Writing a const member through the import binding (`m.x = 1`,
//! `m.x += 1`, `m.x++`, `++m.x`, `m["x"] = 1`) must therefore report TS2540 and
//! suppress the TS2322 type-mismatch that otherwise fires against the const's
//! narrowed literal type. `export let`/`var` members stay writable.
//!
//! Owner: `is_namespace_const_property_inner` in the checker's readonly path,
//! extended to follow the alias to its target module and resolve the export
//! cross-file (the alias itself is not a MODULE symbol and, for a plain
//! named-export target, does not resolve to the member through
//! `resolve_alias_symbol`).
//!
//! Adjacent cases covered:
//! - property write, compound assignment, increment, and element access
//! - `let` member stays writable (negative)
//! - renamed alias binder (structural, not name-keyed)
//! - named value import `obj.p` is NOT a module export even when the module
//!   also exports a top-level `p` (negative — no false positive)
//! - same-file `namespace M` const still readonly (regression guard)
//! - `import * as ns` namespace import keeps every member readonly, const or
//!   not (regression guard)

use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn readonly_diags(files: &[(&str, &str)]) -> Vec<(String, u32, String)> {
    tsz_checker::test_utils::check_all_multi_file_with_global_index(
        files,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    // TS2540 (readonly named property), TS2542 (readonly index signature),
    // TS2322 (type mismatch — should be suppressed alongside TS2540).
    .filter(|diag| matches!(diag.code, 2540 | 2542 | 2322))
    .map(|diag| (diag.file, diag.code, diag.message_text))
    .collect()
}

const MOD_A: &str = "export const x = 0;\nexport let y = 0;\n";

#[test]
fn import_equals_const_member_write_is_readonly() {
    let diags = readonly_diags(&[
        ("a.ts", MOD_A),
        ("b.ts", "import m = require('./a');\nm.x = 1;\n"),
    ]);
    // Exactly one TS2540 on `m.x`, and no TS2322 (suppressed by the readonly
    // diagnostic).
    let ts2540: Vec<_> = diags.iter().filter(|(_, c, _)| *c == 2540).collect();
    assert_eq!(
        diags.iter().filter(|(_, c, _)| *c == 2322).count(),
        0,
        "TS2322 must be suppressed alongside TS2540: {diags:?}"
    );
    assert_eq!(ts2540.len(), 1, "expected one TS2540: {diags:?}");
    assert!(
        ts2540[0].2.contains('x'),
        "message should name 'x': {diags:?}"
    );
}

#[test]
fn import_equals_const_member_all_write_forms_are_readonly() {
    // Property write, compound assignment, prefix/postfix increment, element
    // access — every write form of the const member is TS2540, matching tsc.
    let diags = readonly_diags(&[
        ("a.ts", MOD_A),
        (
            "b.ts",
            "import m = require('./a');\n\
             m.x = 1;\n\
             m.x += 2;\n\
             m.x++;\n\
             ++m.x;\n\
             m[\"x\"] = 0;\n",
        ),
    ]);
    assert_eq!(
        diags.iter().filter(|(_, c, _)| *c == 2540).count(),
        5,
        "all five const write forms should report TS2540: {diags:?}"
    );
    assert_eq!(
        diags.iter().filter(|(_, c, _)| *c == 2322).count(),
        0,
        "no TS2322 should survive alongside the readonly diagnostics: {diags:?}"
    );
}

#[test]
fn import_equals_let_member_stays_writable() {
    // `export let y` is not const, so writing it through the binding is allowed
    // (no TS2540). The assignment is type-compatible, so no TS2322 either.
    let diags = readonly_diags(&[
        ("a.ts", MOD_A),
        ("b.ts", "import m = require('./a');\nm.y = 5;\nm.y++;\n"),
    ]);
    assert!(diags.is_empty(), "let member must stay writable: {diags:?}");
}

#[test]
fn import_equals_readonly_is_structural_not_name_keyed() {
    // A different alias binder name still reports TS2540 — the rule is
    // structural (const export through a module binding), not tied to the name.
    let diags = readonly_diags(&[
        ("a.ts", MOD_A),
        (
            "consumer.ts",
            "import renamedBinding = require('./a');\nrenamedBinding.x = 1;\n",
        ),
    ]);
    assert_eq!(
        diags.iter().filter(|(_, c, _)| *c == 2540).count(),
        1,
        "renamed alias must still report TS2540: {diags:?}"
    );
}

#[test]
fn named_value_import_property_is_not_a_module_const() {
    // `import { obj }` binds a single value. `obj.p` accesses the object's own
    // (mutable) property `p`, NOT the module-level `export const p` — so no
    // TS2540 false positive, even though the module also exports `p`.
    let diags = readonly_diags(&[
        (
            "mod.ts",
            "export const obj = { p: 0 };\nexport const p = 99;\n",
        ),
        ("use.ts", "import { obj } from './mod';\nobj.p = 1;\n"),
    ]);
    assert_eq!(
        diags.iter().filter(|(_, c, _)| *c == 2540).count(),
        0,
        "object property write must not be treated as a module const: {diags:?}"
    );
}

#[test]
fn same_file_namespace_const_still_readonly() {
    // Regression guard: the pre-existing same-file namespace path is unchanged.
    let diags = readonly_diags(&[(
        "n.ts",
        "namespace M { export const x = 0; export let y = 0; }\nM.x = 1;\nM.y = 2;\n",
    )]);
    assert_eq!(
        diags.iter().filter(|(_, c, _)| *c == 2540).count(),
        1,
        "same-file namespace const write should still report exactly one TS2540: {diags:?}"
    );
}

#[test]
fn namespace_import_keeps_all_members_readonly() {
    // Regression guard: `import * as ns` treats every member as readonly (an ES
    // namespace object is immutable), const or not — both `ns.x` and `ns.y` are
    // TS2540, matching tsc. The const path must not narrow this to const-only.
    let diags = readonly_diags(&[
        ("a.ts", MOD_A),
        (
            "star.ts",
            "import * as ns from './a';\nns.x = 1;\nns.y = 2;\n",
        ),
    ]);
    assert_eq!(
        diags.iter().filter(|(_, c, _)| *c == 2540).count(),
        2,
        "namespace import should keep both members readonly: {diags:?}"
    );
}
