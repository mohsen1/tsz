//! A chained `exports = module.exports = X` keeps the real export type.
//!
//! An assignment expression takes its target's declared type, so an ambient
//! `declare var module: { exports: any }` — what node typings and hand-written
//! `node.d.ts` shims provide — makes the chain resolve to an error type. That
//! renders as `any`, and every property access on the module is then silently
//! accepted.
//!
//! tsc collects the assigned value instead, so the module keeps its real export
//! type and `exports.<name>` is still checked against it. Verified against the
//! pinned tsc 7.0.2.

use crate::context::CheckerOptions;
use crate::test_utils::{check_multi_file_with_libs_stamped, load_lib_files};

fn js_diags(files: &[(&str, &str)], entry: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..CheckerOptions::default()
    };
    check_multi_file_with_libs_stamped(files, entry, options, &load_lib_files(&["es5.d.ts"]))
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

const MISSING_PROPERTY: u32 = 2339;

const NODE_SHIM: &str = concat!(
    "declare function require(name: string): any;\n",
    "declare var exports: any;\n",
    "declare var module: { exports: any };\n",
);

const CHAIN_SRC: &str = concat!(
    "exports = module.exports = C\n",
    "exports.f = n => n + 1\n",
    "function C() {\n",
    "    this.p = 1\n",
    "}\n",
);

// The ambient-shim variant (`node.d.ts` + `semver.js`, witness
// `moduleExportAlias2`) is covered by the conformance corpus and by direct
// CLI verification against tsc: this multi-file unit harness does not apply
// the ambient declaration the way the driver does, so asserting it here
// would not exercise the rescue.

/// The same file without the ambient shim already worked; it must keep working,
/// and must keep naming the callable type rather than the instance type `C`.
#[test]
fn chain_without_ambient_shim_still_reports_the_callable_type() {
    let diags = js_diags(&[("alias.js", CHAIN_SRC)], "alias.js");
    assert!(
        diags.iter().any(|(code, msg)| *code == MISSING_PROPERTY
            && msg.contains("'f'")
            && msg.contains("() => void")),
        "expected TS2339 on '() => void', got: {diags:?}"
    );
}

/// A plain (unchained) whole-module export is untouched by the rescue.
#[test]
fn unchained_whole_module_export_is_unaffected() {
    let src = "module.exports = function () { }\nmodule.exports.f = function (a) { };\n";
    let diags = js_diags(&[("mod.js", src)], "mod.js");
    assert!(
        diags
            .iter()
            .any(|(code, msg)| *code == MISSING_PROPERTY && msg.contains("() => void")),
        "expected TS2339 on '() => void', got: {diags:?}"
    );
}

/// A chain exporting an object literal keeps its members — the rescue must not
/// turn a legitimate member access into an error.
#[test]
fn chain_exporting_an_object_literal_keeps_its_members() {
    let src = "exports = module.exports = { a: 1 }\nexports.a;\n";
    let diags = js_diags(&[("node.d.ts", NODE_SHIM), ("obj.js", src)], "obj.js");
    assert!(
        !diags.iter().any(|(code, _)| *code == MISSING_PROPERTY),
        "expected no TS2339, got: {diags:?}"
    );
}
