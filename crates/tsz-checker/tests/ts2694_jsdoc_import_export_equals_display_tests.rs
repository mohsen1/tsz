//! TS2694 namespace text for a JSDoc `import(…)` / `typeof import(…)`
//! reference to a module whose export surface is an export assignment — the
//! JSDoc follow-up to #17208 (#17212 fixed the TS-syntax walks and left the
//! JSDoc walks as a tracked follow-up).
//!
//! Structural rule (oracle: `typescript@7.0.2`), mirroring the TS-syntax
//! `import_type_namespace_name` naming so every TS2694 walk agrees:
//! - Named `export = <target>` (const/class/function/namespace, incl. an
//!   aliased `const t = …; export = t`): the target symbol's own name, no
//!   module path, no `.export=` suffix.
//! - Anonymous target (`module.exports = { … }`): `"<resolved path>".export=`.
//! - A module without an export assignment: `"<resolved path>"`, no suffix —
//!   the JSDoc `typeof import` walk previously appended `.export=` here
//!   unconditionally, which was wrong.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

fn ts2694(files: &[(&str, &str)], entry: &str) -> Vec<String> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            allow_js: true,
            check_js: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .filter(|d| d.code == 2694)
    .map(|d| d.message_text)
    .collect()
}

// ---- JSDoc bare `import(...)` walk ----

#[test]
fn jsdoc_import_named_const_target_names_target() {
    let msgs = ts2694(
        &[
            (
                "pkg/index.ts",
                "const shape = { edge: 1 };\nexport = shape;\n",
            ),
            (
                "main.js",
                "/**\n * @param {import('./pkg').Absent} a\n */\nfunction f(a) { return a; }\n",
            ),
        ],
        "main.js",
    );
    assert_eq!(
        msgs,
        vec!["Namespace 'shape' has no exported member 'Absent'.".to_string()]
    );
}

#[test]
fn jsdoc_import_named_class_target_names_target() {
    let msgs = ts2694(
        &[
            (
                "pkg/index.ts",
                "class Widget { w = 1; }\nexport = Widget;\n",
            ),
            (
                "main.js",
                "/**\n * @param {import('./pkg').Absent} a\n */\nfunction f(a) { return a; }\n",
            ),
        ],
        "main.js",
    );
    assert_eq!(
        msgs,
        vec!["Namespace 'Widget' has no exported member 'Absent'.".to_string()]
    );
}

#[test]
fn jsdoc_import_anonymous_target_keeps_export_equals() {
    let msgs = ts2694(
        &[
            ("jmod.js", "module.exports = { edge: 1 };\n"),
            (
                "main.js",
                "/**\n * @param {import('./jmod').Absent} a\n */\nfunction f(a) { return a; }\n",
            ),
        ],
        "main.js",
    );
    assert_eq!(
        msgs,
        vec!["Namespace '\"jmod\".export=' has no exported member 'Absent'.".to_string()]
    );
}

// ---- JSDoc `typeof import(...)` walk ----

#[test]
fn jsdoc_typeof_import_anonymous_target_keeps_export_equals() {
    let msgs = ts2694(
        &[
            ("jmod.js", "module.exports = { edge: 1 };\n"),
            (
                "main.js",
                "/**\n * @param {typeof import('./jmod').Absent} a\n */\nfunction f(a) { return a; }\n",
            ),
        ],
        "main.js",
    );
    assert_eq!(
        msgs,
        vec!["Namespace '\"jmod\".export=' has no exported member 'Absent'.".to_string()]
    );
}

/// The key follow-up fix: a plain named-export CommonJS module (`exports.FOO`,
/// no export assignment) must NOT get a `.export=` suffix — the JSDoc `typeof
/// import` walk previously appended it unconditionally.
#[test]
fn jsdoc_typeof_import_plain_named_export_module_omits_export_equals() {
    let msgs = ts2694(
        &[
            ("sub/index.js", "exports.FOO = \"foo\";\n"),
            (
                "main.js",
                "/**\n * @param {typeof import('./sub').Missing} a\n */\nfunction f(a) { return a; }\n",
            ),
        ],
        "main.js",
    );
    assert_eq!(
        msgs,
        vec!["Namespace '\"sub/index\"' has no exported member 'Missing'.".to_string()]
    );
}

/// Multi-segment JSDoc `import("./m").Bar.Q` under a named `export = ns`
/// namespace: the qualified-chain walk roots at the export= target `ns`, then
/// the traversed segment `Bar`, with no module path and no `.export=`.
#[test]
fn jsdoc_import_multi_segment_under_named_namespace_target() {
    let msgs = ts2694(
        &[
            (
                "mod.d.ts",
                "declare namespace ns {\n  namespace Bar {\n    function method(): void;\n  }\n}\nexport = ns;\n",
            ),
            (
                "main.js",
                "/**\n * @param {import('./mod').Bar.Q} a\n */\nfunction f(a) { return a; }\n",
            ),
        ],
        "main.js",
    );
    assert_eq!(
        msgs,
        vec!["Namespace 'ns.Bar' has no exported member 'Q'.".to_string()]
    );
}

// ---- Binder-name independence ----

#[test]
fn jsdoc_import_renamed_binders_follow_the_target_name() {
    let msgs = ts2694(
        &[
            (
                "pkg/index.ts",
                "class SomethingEntirelyDifferent { z = 1; }\nexport = SomethingEntirelyDifferent;\n",
            ),
            (
                "main.js",
                "/**\n * @param {import('./pkg').Absent} a\n */\nfunction f(a) { return a; }\n",
            ),
        ],
        "main.js",
    );
    assert_eq!(
        msgs,
        vec!["Namespace 'SomethingEntirelyDifferent' has no exported member 'Absent'.".to_string()]
    );
}
