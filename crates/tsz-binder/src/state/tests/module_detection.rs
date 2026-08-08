//! `moduleDetection` and the file-level module-ness predicate.
//!
//! `tsc`'s `getSetExternalModuleIndicator` dispatches on
//! `getEmitModuleDetectionKind(options)` and installs one of three predicates:
//!
//! | kind | rule |
//! | --- | --- |
//! | `Force` | `isFileProbablyExternalModule(file) \|\| !file.isDeclarationFile` |
//! | `Legacy` | `isFileProbablyExternalModule(file)` |
//! | `Auto` | the above, plus `isFileForcedToBeModuleByFormat` |
//!
//! The binder owns that predicate here, because `is_external_module` is a
//! bind-time property of a source file that the checker consumes read-only.
//!
//! Every binder name below is arbitrary and none repeats across rows: nothing
//! in this family may key on an identifier string. Expectations are pinned
//! against a real `tsc` run, not against tsz's prior behavior.

use super::super::{BinderOptions, BinderState};
use tsz_common::options::module_detection::ModuleDetectionKind;
use tsz_parser::ParserState;

/// Bind `source` as `file_name` under a resolved `moduleDetection` kind.
fn is_external_module(source: &str, file_name: &str, kind: ModuleDetectionKind) -> bool {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::with_options(BinderOptions {
        module_detection: kind,
        ..BinderOptions::default()
    });
    binder.bind_source_file(parser.get_arena(), root);
    binder.is_external_module
}

// ---------------------------------------------------------------------------
// force: every non-declaration file is a module
// ---------------------------------------------------------------------------

#[test]
fn force_makes_a_plain_script_file_a_module() {
    assert!(
        is_external_module("var wombat;\n", "a.ts", ModuleDetectionKind::Force),
        "under force, a file with no module syntax is still a module"
    );
}

#[test]
fn force_keeps_a_file_with_module_syntax_a_module() {
    assert!(
        is_external_module("export {};\n", "b.ts", ModuleDetectionKind::Force),
        "module syntax is a module under every detection kind"
    );
}

#[test]
fn force_leaves_a_declaration_file_without_module_syntax_a_script() {
    // `!file.isDeclarationFile` is the whole force rule: a `.d.ts` with no
    // module syntax keeps declaring globals.
    assert!(
        !is_external_module(
            "declare var badger: number;\n",
            "globals.d.ts",
            ModuleDetectionKind::Force
        ),
        "force must not make a declaration file a module"
    );
}

#[test]
fn force_makes_a_declaration_file_with_module_syntax_a_module() {
    assert!(
        is_external_module(
            "declare var otter: number;\nexport {};\n",
            "typed.d.ts",
            ModuleDetectionKind::Force
        ),
        "a declaration file is a module when it carries module syntax of its own"
    );
}

#[test]
fn force_leaves_a_declaration_file_with_a_module_extension_a_script() {
    assert!(
        !is_external_module(
            "declare var marmot: number;\n",
            "globals.d.mts",
            ModuleDetectionKind::Force
        ),
        "`.d.mts` is a declaration file first; force must not reach it"
    );
}

// ---------------------------------------------------------------------------
// legacy: module syntax only, never format
// ---------------------------------------------------------------------------

#[test]
fn legacy_leaves_a_plain_script_file_a_script() {
    assert!(
        !is_external_module("var lynx;\n", "c.ts", ModuleDetectionKind::Legacy),
        "no module syntax, no module"
    );
}

#[test]
fn legacy_does_not_force_a_module_extension() {
    // The divergence Kyanite measured from the other side: under legacy, tsc's
    // predicate is `isFileProbablyExternalModule` alone, so `.mts` is a script.
    assert!(
        !is_external_module("var ibex;\n", "d.mts", ModuleDetectionKind::Legacy),
        "legacy never forces module-ness by file format"
    );
}

#[test]
fn legacy_does_not_force_a_commonjs_extension() {
    assert!(
        !is_external_module("var tapir;\n", "e.cts", ModuleDetectionKind::Legacy),
        "legacy never forces module-ness by file format"
    );
}

#[test]
fn legacy_still_honours_module_syntax() {
    assert!(
        is_external_module(
            "import { quokka } from \"./q\";\nquokka;\n",
            "f.ts",
            ModuleDetectionKind::Legacy
        ),
        "an import declaration is module syntax under every detection kind"
    );
}

#[test]
fn legacy_still_honours_an_exported_declaration() {
    assert!(
        is_external_module(
            "export const dingo = 1;\n",
            "g.ts",
            ModuleDetectionKind::Legacy
        ),
        "an exported declaration is module syntax under every detection kind"
    );
}

// ---------------------------------------------------------------------------
// import-equals: only `= require("...")` is module syntax, not `= A.B`
//
// tsc's `isAnExternalModuleIndicatorNode` counts an `ImportEqualsDeclaration`
// only when `isExternalModuleReference(node.moduleReference)` — i.e. the
// `= require("...")` form. An internal `import X = A.B` (entity-name
// reference) is a namespace alias, not module syntax, so its file stays a
// script. Binder names vary so nothing keys on an identifier string.
// ---------------------------------------------------------------------------

#[test]
fn import_equals_require_is_module_syntax() {
    assert!(
        is_external_module(
            "import wallaby = require(\"./w\");\nwallaby;\n",
            "h.ts",
            ModuleDetectionKind::Legacy
        ),
        "`import X = require(\"...\")` is an external module reference"
    );
}

#[test]
fn import_equals_entity_name_is_not_module_syntax() {
    // The reported witness (issue-family): `import await = foo.await;` in a
    // script must leave the file a script so `await` stays a legal top-level
    // identifier (no spurious TS1262). Use a neutral name to prove the rule
    // is structural, not a `await` special case.
    assert!(
        !is_external_module(
            "namespace ns { export const wombat = 1; }\nimport alias = ns.wombat;\nalias;\n",
            "i.ts",
            ModuleDetectionKind::Legacy
        ),
        "`import X = A.B` (entity-name reference) is a namespace alias, not module syntax"
    );
}

#[test]
fn import_equals_entity_name_stays_a_script_under_auto() {
    assert!(
        !is_external_module(
            "namespace ns { export const koala = 1; }\nimport alias = ns.koala;\nalias;\n",
            "j.ts",
            ModuleDetectionKind::Auto
        ),
        "auto detection must not promote an entity-name `import =` file to a module"
    );
}

#[test]
fn import_equals_entity_name_alongside_real_import_is_a_module() {
    // The entity-name `import =` contributes nothing, but a sibling external
    // import still makes the file a module — the gate is per-statement.
    assert!(
        is_external_module(
            "import { yak } from \"./y\";\nnamespace ns { export const x = 1; }\nimport alias = ns.x;\nyak;\nalias;\n",
            "k.ts",
            ModuleDetectionKind::Legacy
        ),
        "a real import declaration is a module indicator even beside an entity-name `import =`"
    );
}

// ---------------------------------------------------------------------------
// auto: syntax plus format, declaration files excluded from format forcing
// ---------------------------------------------------------------------------

#[test]
fn auto_leaves_a_plain_script_file_a_script() {
    assert!(
        !is_external_module("var caracal;\n", "h.ts", ModuleDetectionKind::Auto),
        "no module syntax and no forcing format"
    );
}

#[test]
fn auto_forces_a_module_extension() {
    assert!(
        is_external_module("var serval;\n", "i.mts", ModuleDetectionKind::Auto),
        "`.mts` forces module-ness by format under auto"
    );
}

#[test]
fn auto_forces_a_commonjs_extension() {
    assert!(
        is_external_module("var jerboa;\n", "j.cjs", ModuleDetectionKind::Auto),
        "`.cjs` forces module-ness by format under auto"
    );
}

#[test]
fn auto_leaves_a_declaration_file_with_a_module_extension_a_script() {
    // `isFileForcedToBeModuleByFormat` ends in `&& !file.isDeclarationFile`.
    // Getting this wrong hides every ambient global a `.d.mts` declares.
    assert!(
        !is_external_module(
            "declare var pangolin: number;\n",
            "ambient.d.mts",
            ModuleDetectionKind::Auto
        ),
        "`.d.mts` is not forced to be a module by its format"
    );
}

#[test]
fn auto_leaves_a_declaration_file_with_a_commonjs_extension_a_script() {
    assert!(
        !is_external_module(
            "declare var okapi: number;\n",
            "ambient.d.cts",
            ModuleDetectionKind::Auto
        ),
        "`.d.cts` is not forced to be a module by its format"
    );
}

#[test]
fn auto_makes_a_declaration_file_with_a_module_extension_a_module_when_it_has_syntax() {
    assert!(
        is_external_module(
            "declare var gerenuk: number;\nexport {};\n",
            "typed.d.mts",
            ModuleDetectionKind::Auto
        ),
        "module syntax still wins in a `.d.mts`"
    );
}

// ---------------------------------------------------------------------------
// The kind is the only thing that varies: same source, same name, three answers
// ---------------------------------------------------------------------------

#[test]
fn detection_kind_is_the_only_variable_for_a_bare_script() {
    let source = "var kinkajou;\n";
    assert!(
        !is_external_module(source, "k.ts", ModuleDetectionKind::Auto),
        "auto: script"
    );
    assert!(
        !is_external_module(source, "k.ts", ModuleDetectionKind::Legacy),
        "legacy: script"
    );
    assert!(
        is_external_module(source, "k.ts", ModuleDetectionKind::Force),
        "force: module"
    );
}

#[test]
fn detection_kind_is_the_only_variable_for_a_module_extension() {
    let source = "var numbat;\n";
    assert!(
        is_external_module(source, "l.mts", ModuleDetectionKind::Auto),
        "auto: forced by format"
    );
    assert!(
        !is_external_module(source, "l.mts", ModuleDetectionKind::Legacy),
        "legacy: format is ignored"
    );
    assert!(
        is_external_module(source, "l.mts", ModuleDetectionKind::Force),
        "force: non-declaration file"
    );
}

#[test]
fn detection_kind_never_changes_the_answer_for_module_syntax() {
    let source = "export const agouti = 1;\n";
    for kind in [
        ModuleDetectionKind::Auto,
        ModuleDetectionKind::Legacy,
        ModuleDetectionKind::Force,
    ] {
        assert!(
            is_external_module(source, "m.ts", kind),
            "module syntax is a module under {kind:?}"
        );
    }
}

#[test]
fn detection_kind_never_makes_a_bare_declaration_file_a_module() {
    let source = "declare var saiga: number;\n";
    for kind in [
        ModuleDetectionKind::Auto,
        ModuleDetectionKind::Legacy,
        ModuleDetectionKind::Force,
    ] {
        assert!(
            !is_external_module(source, "n.d.ts", kind),
            "a bare declaration file stays a script under {kind:?}"
        );
    }
}

#[test]
fn default_detection_kind_is_auto() {
    // `BinderOptions::default()` must keep the pre-existing behavior: every
    // construction site that has not been taught about the setting yet gets
    // tsc's own default.
    assert_eq!(
        BinderOptions::default().module_detection,
        ModuleDetectionKind::Auto
    );
    assert!(
        is_external_module("var zebu;\n", "o.mts", ModuleDetectionKind::default()),
        "the default kind behaves as auto"
    );
}
