//! Regression tests for the `import.meta` module-compatibility diagnostic
//! (refs #12261).
//!
//! `tsc` reports two *distinct* diagnostics for an unsupported `import.meta`:
//!
//! * **TS1343** — "only allowed when the '--module' option is 'es2020',
//!   'es2022', 'esnext', 'system', 'node16', 'node18', 'node20', or
//!   'nodenext'" — for every module kind below `ES2020` that is not `System`
//!   and not a Node resolution mode (CommonJS, AMD, UMD, ES2015). The
//!   meta-property is unavailable for the whole module mode, independent of
//!   the file.
//! * **TS1470** — "not allowed in files which will build into CommonJS
//!   output" — only under Node16/Node18/Node20/NodeNext, and only for files
//!   that resolve to CommonJS format (`.cts`/`.cjs` or a CJS-implied `.ts`).
//!
//! Earlier tsz emitted TS1470 for *all* sub-ES2020 module kinds, so plain
//! `--module commonjs` / `es2015` produced the wrong code. The structural rule
//! keys on the effective module kind (and, for Node modes, the file format),
//! never on the spelling of the `import.meta` access, so the property name is
//! varied across cases.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_multi_file_with_libs, check_source, load_compiled_lib_files, load_lib_files,
};
use tsz_common::common::{ModuleKind, ScriptTarget};

const TS1343: u32 = 1343;
const TS1470: u32 = 1470;

fn codes(source: &str, file_name: &str, module: ModuleKind) -> Vec<u32> {
    let options = CheckerOptions {
        module,
        ..CheckerOptions::default()
    };
    check_source(source, file_name, options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn commonjs_module_emits_ts1343_not_ts1470() {
    let codes = codes(
        "export const u = import.meta.url;",
        "a.ts",
        ModuleKind::CommonJS,
    );
    assert!(
        codes.contains(&TS1343),
        "commonjs should report TS1343 for import.meta; got {codes:?}"
    );
    assert!(
        !codes.contains(&TS1470),
        "commonjs must not report the Node-only TS1470; got {codes:?}"
    );
}

#[test]
fn es2015_module_emits_ts1343() {
    // Different access spelling to confirm the rule is module-kind driven.
    let codes = codes(
        "export const here = import.meta.dirname;",
        "mod.ts",
        ModuleKind::ES2015,
    );
    assert!(
        codes.contains(&TS1343),
        "es2015 should report TS1343 for import.meta; got {codes:?}"
    );
    assert!(
        !codes.contains(&TS1470),
        "es2015 must not report TS1470; got {codes:?}"
    );
}

#[test]
fn amd_module_emits_ts1343() {
    let codes = codes(
        "export const r = import.meta.resolve;",
        "amd-file.ts",
        ModuleKind::AMD,
    );
    assert!(
        codes.contains(&TS1343),
        "amd should report TS1343 for import.meta; got {codes:?}"
    );
    assert!(
        !codes.contains(&TS1470),
        "amd must not report TS1470; got {codes:?}"
    );
}

#[test]
fn es2020_module_supports_import_meta() {
    let codes = codes(
        "export const u = import.meta.url;",
        "a.ts",
        ModuleKind::ES2020,
    );
    assert!(
        !codes.contains(&TS1343) && !codes.contains(&TS1470),
        "es2020 supports import.meta natively; expected no module diagnostic, got {codes:?}"
    );
}

#[test]
fn esnext_module_supports_import_meta() {
    let codes = codes(
        "export const u = import.meta.url;",
        "a.ts",
        ModuleKind::ESNext,
    );
    assert!(
        !codes.contains(&TS1343) && !codes.contains(&TS1470),
        "esnext supports import.meta natively; expected no module diagnostic, got {codes:?}"
    );
}

#[test]
fn system_module_supports_import_meta() {
    let codes = codes(
        "export const u = import.meta.url;",
        "a.ts",
        ModuleKind::System,
    );
    assert!(
        !codes.contains(&TS1343) && !codes.contains(&TS1470),
        "system supports import.meta natively; expected no module diagnostic, got {codes:?}"
    );
}

#[test]
fn node16_commonjs_file_emits_ts1470_not_ts1343() {
    // A `.cts` file under node16 builds into CommonJS output -> TS1470.
    let codes = codes(
        "export const u = import.meta.url;",
        "a.cts",
        ModuleKind::Node16,
    );
    assert!(
        codes.contains(&TS1470),
        "node16 .cts (CommonJS output) should report TS1470; got {codes:?}"
    );
    assert!(
        !codes.contains(&TS1343),
        "node16 must not fall back to the module-wide TS1343; got {codes:?}"
    );
}

#[test]
fn node16_esm_file_supports_import_meta() {
    // A `.mts` file under node16 is an ES module -> import.meta is allowed.
    let codes = codes(
        "export const u = import.meta.url;",
        "a.mts",
        ModuleKind::Node16,
    );
    assert!(
        !codes.contains(&TS1470) && !codes.contains(&TS1343),
        "node16 .mts (ES module) supports import.meta; expected no module diagnostic, got {codes:?}"
    );
}

#[test]
fn default_module_none_emits_no_import_meta_diagnostic() {
    // `ModuleKind::None` is the unresolved/default sentinel (what
    // `CheckerOptions::default()` carries). The driver resolves it to a
    // concrete module kind before checking, so the checker must not emit a
    // module-kind import.meta diagnostic for a bare `None` — neither the
    // sub-ES2020 TS1343 nor the Node-output TS1470.
    let codes = codes(
        "export const u = import.meta.url;",
        "a.ts",
        ModuleKind::None,
    );
    assert!(
        !codes.contains(&TS1343) && !codes.contains(&TS1470),
        "unresolved/default module (None) must not emit an import.meta module diagnostic; got {codes:?}"
    );
}

#[test]
fn nodenext_commonjs_file_emits_ts1470() {
    let codes = codes(
        "export const u = import.meta.url;",
        "legacy.cjs",
        ModuleKind::NodeNext,
    );
    assert!(
        codes.contains(&TS1470),
        "nodenext .cjs (CommonJS output) should report TS1470; got {codes:?}"
    );
    assert!(
        !codes.contains(&TS1343),
        "nodenext must not report TS1343; got {codes:?}"
    );
}

#[test]
fn document_body_append_child_uses_inherited_dom_node_member() {
    for target in [ScriptTarget::ESNext, ScriptTarget::ES5] {
        let options = CheckerOptions {
            module: ModuleKind::ESNext,
            target,
            ..CheckerOptions::default()
        };
        let codes: Vec<u32> = check_source(
            r#"
export const image = new Image();
document.body.appendChild(image);
"#,
            "dom-access.ts",
            options,
        )
        .into_iter()
        .map(|d| d.code)
        .collect();
        assert!(
            !codes.contains(&2339),
            "`HTMLElement` should inherit `Node.appendChild` under target {target:?}; got {codes:?}"
        );
    }
}

#[test]
fn document_body_append_child_survives_later_import_meta_global_augmentation() {
    let lib_files = load_lib_files(&["es5.d.ts", "dom.d.ts"]);
    let options = CheckerOptions {
        module: ModuleKind::ESNext,
        target: ScriptTarget::ES5,
        ..CheckerOptions::default()
    };
    let diagnostics = check_multi_file_with_libs(
        &[
            (
                "example.ts",
                r##"
export const image = new Image();
document.body.appendChild(image);
"##,
            ),
            (
                "augmentations.ts",
                r##"
declare global {
    interface ImportMeta {
        wellKnownProperty: { a: number, b: string, c: boolean };
    }
}
"##,
            ),
        ],
        "example.ts",
        options,
        &lib_files,
    );
    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2339),
        "global `ImportMeta` augmentation must not hide inherited `HTMLElement` DOM members; got {diagnostics:#?}"
    );
}

#[test]
fn import_meta_fixture_keeps_document_body_append_child() {
    let lib_files = load_compiled_lib_files(&["lib.es5.d.ts", "lib.dom.d.ts"]);
    let files = [
        (
            "example.ts",
            r##"
(async () => {
  const response = await fetch(new URL("../hamsters.jpg", import.meta.url).toString());
  const blob = await response.blob();

  const size = import.meta.scriptElement.dataset.size || 300;

  const image = new Image();
  image.src = URL.createObjectURL(blob);
  image.width = image.height = size;

  document.body.appendChild(image);
})();
"##,
        ),
        (
            "moduleLookingFile01.ts",
            r##"
export let x = import.meta;
export let y = import.metal;
export let z = import.import.import.malkovich;
"##,
        ),
        (
            "scriptLookingFile01.ts",
            r##"
let globalA = import.meta;
let globalB = import.metal;
let globalC = import.import.import.malkovich;
"##,
        ),
        (
            "assignmentTargets.ts",
            r##"
export const foo: ImportMeta = import.meta.blah = import.meta.blue = import.meta;
import.meta = foo;

// @Filename augmentations.ts
declare global {
  interface ImportMeta {
    wellKnownProperty: { a: number, b: string, c: boolean };
  }
}

const { a, b, c } = import.meta.wellKnownProperty;
"##,
        ),
    ];
    for target in [ScriptTarget::ESNext, ScriptTarget::ES5] {
        for module in [
            ModuleKind::ESNext,
            ModuleKind::CommonJS,
            ModuleKind::System,
            ModuleKind::ES2020,
        ] {
            let options = CheckerOptions {
                module,
                target,
                ..CheckerOptions::default()
            };
            let diagnostics = check_multi_file_with_libs(&files, "example.ts", options, &lib_files);
            let append_child_errors: Vec<_> = diagnostics
                .iter()
                .filter(|diag| {
                    (diag.code == 2339 && diag.message_text.contains("appendChild"))
                        || (diag.code == 2345 && diag.message_text.contains("HTMLImageElement"))
                })
                .collect();
            assert!(
                append_child_errors.is_empty(),
                "fixture should not report DOM appendChild drift under module {module:?} target {target:?}; got {diagnostics:#?}"
            );
        }
    }
}
