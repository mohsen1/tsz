use super::super::core::*;

/// Node20/NodeNext CJS-of-ESM `"module.exports"` interop, require-style alias
/// shape `import X = require(M)` — `new X()` must check construct signatures
/// on the `"module.exports"` value (not on the synthesized namespace surface).
///
/// Mirrors `esmModuleExports2.ts` (`importer-cts.cts` line 2:
/// `import Foo = require("./exporter.mjs"); new Foo();`).
#[test]
fn test_esm_module_exports_import_equals_require_uses_module_exports_value() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "exporter.mts",
                r#"export default class Foo {}
const oops = "oops";
export { oops as "module.exports" };
"#,
            ),
            (
                "importer.cts",
                r#"import Foo = require("./exporter.mjs");
new Foo();
"#,
            ),
        ],
        "importer.cts",
        CheckerOptions {
            module: tsz_common::ModuleKind::Node20,
            target: tsz_common::common::ScriptTarget::ES2023,
            ..Default::default()
        },
    );
    let ts2351 = diagnostics.iter().filter(|(c, _)| *c == 2351).count();
    assert_eq!(
        ts2351, 1,
        "Expected 1 TS2351 for `new Foo()` where Foo resolves through \
         \"module.exports\" to a non-constructable string. Got: {diagnostics:#?}"
    );
}

/// Same rule with the import-equals binding renamed (`Bar` instead of `Foo`).
/// The fix must not depend on the user-chosen alias name.
#[test]
fn test_esm_module_exports_import_equals_require_with_renamed_alias() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "exporter.mts",
                r#"export default class Foo {}
const value = 42;
export { value as "module.exports" };
"#,
            ),
            (
                "importer.cts",
                r#"import Bar = require("./exporter.mjs");
new Bar();
"#,
            ),
        ],
        "importer.cts",
        CheckerOptions {
            module: tsz_common::ModuleKind::Node20,
            target: tsz_common::common::ScriptTarget::ES2023,
            ..Default::default()
        },
    );
    let ts2351 = diagnostics.iter().filter(|(c, _)| *c == 2351).count();
    assert_eq!(
        ts2351, 1,
        "Expected 1 TS2351 — fix must apply by structural shape, not by alias \
         name. Got: {diagnostics:#?}"
    );
}

/// Node20/NodeNext CJS-of-ESM `"module.exports"` interop, namespace-import
/// alias shape `import * as X from M` — `new X()` must check construct
/// signatures on the `"module.exports"` value (not on the namespace surface).
///
/// Mirrors `esmModuleExports2.ts` (`importer-cts.cts` line 8:
/// `import * as Foo3 from "./exporter.mjs"; new Foo3();`).
#[test]
fn test_esm_module_exports_namespace_import_uses_module_exports_value() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "exporter.mts",
                r#"export default class Foo {}
const oops = "oops";
export { oops as "module.exports" };
"#,
            ),
            (
                "importer.cts",
                r#"import * as Foo3 from "./exporter.mjs";
new Foo3();
"#,
            ),
        ],
        "importer.cts",
        CheckerOptions {
            module: tsz_common::ModuleKind::Node20,
            target: tsz_common::common::ScriptTarget::ES2023,
            ..Default::default()
        },
    );
    let ts2351 = diagnostics.iter().filter(|(c, _)| *c == 2351).count();
    assert_eq!(
        ts2351, 1,
        "Expected 1 TS2351 for `new Foo3()` where Foo3 resolves through \
         \"module.exports\" to a non-constructable string. Got: {diagnostics:#?}"
    );
}

/// Node20/NodeNext CJS-of-ESM `"module.exports"` interop: plain ES6 default
/// import `import Foo2 from M` in a `.cts` file resolves through the
/// `"module.exports"` binding — `new Foo2()` must be TS2351 when the
/// `"module.exports"` value is non-constructable.
///
/// Mirrors `esmModuleExports2.ts` (`importer-cts.cts` line 5:
/// `import Foo2 from "./exporter.mjs"; new Foo2();`).
#[test]
fn test_esm_module_exports_default_import_uses_module_exports_value() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "exporter.mts",
                r#"export default class Foo {}
const oops = "oops";
export { oops as "module.exports" };
"#,
            ),
            (
                "importer.cts",
                r#"import Foo2 from "./exporter.mjs";
new Foo2();
"#,
            ),
        ],
        "importer.cts",
        CheckerOptions {
            module: tsz_common::ModuleKind::Node20,
            target: tsz_common::common::ScriptTarget::ES2023,
            ..Default::default()
        },
    );
    let ts2351 = diagnostics.iter().filter(|(c, _)| *c == 2351).count();
    assert_eq!(
        ts2351, 1,
        "Expected 1 TS2351 for `new Foo2()` where Foo2 is a default import from ESM \
         with non-constructable `module.exports`. Got: {diagnostics:#?}"
    );
}

/// Node20/NodeNext CJS-of-ESM `"module.exports"` interop: CJS require binding
/// `const Foo = require(M)` in a `.cjs` file resolves through the
/// `"module.exports"` binding — `new Foo()` must be TS2351 when the
/// `"module.exports"` value is non-constructable.
///
/// Mirrors `esmModuleExports2.ts` (`importer-cjs.cjs` line 2:
/// `new Foo()`).
#[test]
fn test_esm_module_exports_cjs_require_binding_uses_module_exports_value() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "exporter.mts",
                r#"export default class Foo {}
const oops = "oops";
export { oops as "module.exports" };
"#,
            ),
            (
                "importer.cjs",
                r#"const Foo = require("./exporter.mjs");
new Foo();
"#,
            ),
        ],
        "importer.cjs",
        CheckerOptions {
            module: tsz_common::ModuleKind::Node20,
            target: tsz_common::common::ScriptTarget::ES2023,
            check_js: true,
            allow_js: true,
            ..Default::default()
        },
    );
    let ts2351 = diagnostics.iter().filter(|(c, _)| *c == 2351).count();
    assert_eq!(
        ts2351, 1,
        "Expected 1 TS2351 for `new Foo()` in CJS file where Foo is require-bound \
         to ESM with non-constructable `module.exports`. Got: {diagnostics:#?}"
    );
}

/// Same as the CJS require case but with a renamed binding (`Bar` instead of
/// `Foo`) — proves the rule is structural, not name-dependent.
#[test]
fn test_esm_module_exports_cjs_require_binding_with_renamed_alias() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "exporter.mts",
                r#"export default class Foo {}
const val = 42;
export { val as "module.exports" };
"#,
            ),
            (
                "importer.cjs",
                r#"const Bar = require("./exporter.mjs");
new Bar();
"#,
            ),
        ],
        "importer.cjs",
        CheckerOptions {
            module: tsz_common::ModuleKind::Node20,
            target: tsz_common::common::ScriptTarget::ES2023,
            check_js: true,
            allow_js: true,
            ..Default::default()
        },
    );
    let ts2351 = diagnostics.iter().filter(|(c, _)| *c == 2351).count();
    assert_eq!(
        ts2351, 1,
        "Expected 1 TS2351 — fix must apply by structural shape, not by binding \
         name. Got: {diagnostics:#?}"
    );
}

/// Node20/NodeNext CJS-of-ESM `"module.exports"` interop applies to both
/// `Node20` and `NodeNext` module modes. Mirrors the same shape under
/// `NodeNext` to prove the rule is keyed on the module class, not on the
/// exact `Node20` value.
#[test]
fn test_esm_module_exports_namespace_import_uses_module_exports_under_nodenext() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "exporter.mts",
                r#"export default class Foo {}
const oops = "oops";
export { oops as "module.exports" };
"#,
            ),
            (
                "importer.cts",
                r#"import * as Foo3 from "./exporter.mjs";
new Foo3();
"#,
            ),
        ],
        "importer.cts",
        CheckerOptions {
            module: tsz_common::ModuleKind::NodeNext,
            target: tsz_common::common::ScriptTarget::ES2023,
            ..Default::default()
        },
    );
    let ts2351 = diagnostics.iter().filter(|(c, _)| *c == 2351).count();
    assert_eq!(
        ts2351, 1,
        "Expected 1 TS2351 under NodeNext too. Got: {diagnostics:#?}"
    );
}

/// Bisect test: narrows which subset of 3 forms triggers the 0-diagnostics bug.
#[test]
fn test_esm_module_exports2_bisect_combinations() {
    use super::super::core::compile_named_project_get_diagnostics_with_options;
    let exporter = (
        "exporter.mts",
        "export default class Foo {}\nconst oops = \"oops\";\nexport { oops as \"module.exports\" };\n",
    );
    let opts = tsz_checker::CheckerOptions {
        module: tsz_common::ModuleKind::Node20,
        target: tsz_common::common::ScriptTarget::ES2022,
        check_js: true,
        allow_js: true,
        ..Default::default()
    };
    let f1f2 = compile_named_project_get_diagnostics_with_options(
        &[
            exporter,
            (
                "importer-cts.cts",
                "import Foo = require(\"./exporter.mjs\");\nnew Foo();\n\nimport Foo2 from \"./exporter.mjs\";\nnew Foo2();\n",
            ),
        ],
        opts.clone(),
    );
    let f1f3 = compile_named_project_get_diagnostics_with_options(
        &[
            exporter,
            (
                "importer-cts.cts",
                "import Foo = require(\"./exporter.mjs\");\nnew Foo();\n\nimport * as Foo3 from \"./exporter.mjs\";\nnew Foo3();\n",
            ),
        ],
        opts,
    );
    let f1f2_ts2351 = f1f2.iter().filter(|(c, _)| *c == 2351).count();
    let f1f3_ts2351 = f1f3.iter().filter(|(c, _)| *c == 2351).count();
    assert_eq!(f1f2_ts2351, 2, "F1+F2: expected 2 TS2351, all: {f1f2:#?}");
    assert_eq!(f1f3_ts2351, 2, "F1+F3: expected 2 TS2351, all: {f1f3:#?}");
}

/// Isolated test: 3 import forms in one .cts file, no preceding sub-cases.
/// This rules out thread-local state contamination from d1/d2/d3 sub-cases.
#[test]
fn test_esm_module_exports2_three_forms_isolated() {
    use super::super::core::compile_named_project_get_diagnostics_with_options;
    let exporter = (
        "exporter.mts",
        "export default class Foo {}\nconst oops = \"oops\";\nexport { oops as \"module.exports\" };\n",
    );
    let opts = tsz_checker::CheckerOptions {
        module: tsz_common::ModuleKind::Node20,
        target: tsz_common::common::ScriptTarget::ES2022,
        check_js: true,
        allow_js: true,
        ..Default::default()
    };
    // Form 1 alone to confirm it still works in isolation
    let d_f1_alone = compile_named_project_get_diagnostics_with_options(
        &[
            exporter,
            (
                "importer-cts.cts",
                "import Foo = require(\"./exporter.mjs\");\nnew Foo();\n",
            ),
        ],
        opts.clone(),
    );
    assert_eq!(
        d_f1_alone.iter().filter(|(c, _)| *c == 2351).count(),
        1,
        "form 1 alone: expected 1 TS2351. All diags: {d_f1_alone:#?}"
    );
    // Forms 2+3 only (default + namespace), no import=require
    let d_f2_f3 = compile_named_project_get_diagnostics_with_options(
        &[
            exporter,
            (
                "importer-cts.cts",
                "import Foo2 from \"./exporter.mjs\";\nnew Foo2();\n\nimport * as Foo3 from \"./exporter.mjs\";\nnew Foo3();\n",
            ),
        ],
        opts.clone(),
    );
    assert_eq!(
        d_f2_f3.iter().filter(|(c, _)| *c == 2351).count(),
        2,
        "forms 2+3: expected 2 TS2351. All diags: {d_f2_f3:#?}"
    );
    // All 3 forms
    let d = compile_named_project_get_diagnostics_with_options(
        &[
            exporter,
            (
                "importer-cts.cts",
                "import Foo = require(\"./exporter.mjs\");\nnew Foo();\n\nimport Foo2 from \"./exporter.mjs\";\nnew Foo2();\n\nimport * as Foo3 from \"./exporter.mjs\";\nnew Foo3();\n",
            ),
        ],
        opts,
    );
    assert_eq!(
        d.iter().filter(|(c, _)| *c == 2351).count(),
        3,
        "3-forms isolated: expected 3 TS2351. All diags: {d:#?}"
    );
}

/// Full conformance mirror for `esmModuleExports2.ts`: all four import forms in
/// a `.cts` file importing an ESM module with a non-constructable `"module.exports"`
/// must each emit TS2351.
///
/// This tests the exact file/import-form combination the conformance suite uses,
/// using `compile_named_project_get_diagnostics_with_options` so that all files
/// are checked together — matching how the CLI conformance runner behaves.
#[test]
fn test_esm_module_exports2_all_forms_emit_ts2351() {
    use super::super::core::compile_named_project_get_diagnostics_with_options;
    let exporter = (
        "exporter.mts",
        "export default class Foo {}\nconst oops = \"oops\";\nexport { oops as \"module.exports\" };\n",
    );
    let opts = CheckerOptions {
        module: tsz_common::ModuleKind::Node20,
        target: tsz_common::common::ScriptTarget::ES2022,
        check_js: true,
        allow_js: true,
        ..Default::default()
    };
    // Form 1: import Foo = require() in .cts
    let d1 = compile_named_project_get_diagnostics_with_options(
        &[
            exporter,
            (
                "importer-cts.cts",
                "import Foo = require(\"./exporter.mjs\");\nnew Foo();\n",
            ),
        ],
        opts.clone(),
    );
    assert_eq!(
        d1.iter().filter(|(c, _)| *c == 2351).count(),
        1,
        "Form 1 (import=require in .cts): expected 1 TS2351. Got: {d1:#?}"
    );
    // Form 2: import Foo2 from in .cts
    let d2 = compile_named_project_get_diagnostics_with_options(
        &[
            exporter,
            (
                "importer-cts.cts",
                "import Foo2 from \"./exporter.mjs\";\nnew Foo2();\n",
            ),
        ],
        opts.clone(),
    );
    assert_eq!(
        d2.iter().filter(|(c, _)| *c == 2351).count(),
        1,
        "Form 2 (default import in .cts): expected 1 TS2351. Got: {d2:#?}"
    );
    // Form 3: import * as Foo3 from in .cts
    let d3 = compile_named_project_get_diagnostics_with_options(
        &[
            exporter,
            (
                "importer-cts.cts",
                "import * as Foo3 from \"./exporter.mjs\";\nnew Foo3();\n",
            ),
        ],
        opts.clone(),
    );
    assert_eq!(
        d3.iter().filter(|(c, _)| *c == 2351).count(),
        1,
        "Form 3 (namespace import in .cts): expected 1 TS2351. Got: {d3:#?}"
    );
    // Form 4: const Foo = require() in .cjs
    let d4 = compile_named_project_get_diagnostics_with_options(
        &[
            exporter,
            (
                "importer-cjs.cjs",
                "const Foo = require(\"./exporter.mjs\");\nnew Foo();\n",
            ),
        ],
        opts.clone(),
    );
    assert_eq!(
        d4.iter().filter(|(c, _)| *c == 2351).count(),
        1,
        "Form 4 (require() in .cjs): expected 1 TS2351. Got: {d4:#?}"
    );

    // forms 1+2 only (no form 3): should get 2 TS2351
    let d_f1_f2 = compile_named_project_get_diagnostics_with_options(
        &[
            exporter,
            (
                "importer-cts.cts",
                "import Foo = require(\"./exporter.mjs\");\nnew Foo();\n\nimport Foo2 from \"./exporter.mjs\";\nnew Foo2();\n",
            ),
        ],
        opts.clone(),
    );
    assert_eq!(
        d_f1_f2.iter().filter(|(c, _)| *c == 2351).count(),
        2,
        "forms 1+2 only: expected 2 TS2351. Got: {d_f1_f2:#?}"
    );
    // forms 1+3 only (no form 2): should get 2 TS2351
    let d_f1_f3 = compile_named_project_get_diagnostics_with_options(
        &[
            exporter,
            (
                "importer-cts.cts",
                "import Foo = require(\"./exporter.mjs\");\nnew Foo();\n\nimport * as Foo3 from \"./exporter.mjs\";\nnew Foo3();\n",
            ),
        ],
        opts.clone(),
    );
    assert_eq!(
        d_f1_f3.iter().filter(|(c, _)| *c == 2351).count(),
        2,
        "forms 1+3 only: expected 2 TS2351. Got: {d_f1_f3:#?}"
    );
    // cts-only (without cjs file): should get 3 TS2351
    let d_cts_only = compile_named_project_get_diagnostics_with_options(
        &[
            exporter,
            (
                "importer-cts.cts",
                "import Foo = require(\"./exporter.mjs\");\nnew Foo();\n\nimport Foo2 from \"./exporter.mjs\";\nnew Foo2();\n\nimport * as Foo3 from \"./exporter.mjs\";\nnew Foo3();\n",
            ),
        ],
        opts.clone(),
    );
    assert_eq!(
        d_cts_only.iter().filter(|(c, _)| *c == 2351).count(),
        3,
        "cts-only (3 forms): expected 3 TS2351. Got: {d_cts_only:#?}"
    );
    // cjs-only (without cts file): should get 1 TS2351
    let d_cjs_only = compile_named_project_get_diagnostics_with_options(
        &[
            exporter,
            (
                "importer-cjs.cjs",
                "const Foo = require(\"./exporter.mjs\");\nnew Foo();\n",
            ),
        ],
        opts.clone(),
    );
    assert_eq!(
        d_cjs_only.iter().filter(|(c, _)| *c == 2351).count(),
        1,
        "cjs-only: expected 1 TS2351. Got: {d_cjs_only:#?}"
    );

    // All forms combined — mirrors the actual conformance test structure exactly
    let d_all = compile_named_project_get_diagnostics_with_options(
        &[
            exporter,
            (
                "importer-cts.cts",
                "import Foo = require(\"./exporter.mjs\");\nnew Foo();\n\nimport Foo2 from \"./exporter.mjs\";\nnew Foo2();\n\nimport * as Foo3 from \"./exporter.mjs\";\nnew Foo3();\n",
            ),
            (
                "importer-cjs.cjs",
                "const Foo = require(\"./exporter.mjs\");\nnew Foo();\n",
            ),
        ],
        opts,
    );
    assert_eq!(
        d_all.iter().filter(|(c, _)| *c == 2351).count(),
        4,
        "All forms combined: expected 4 TS2351. Got: {d_all:#?}"
    );
}
