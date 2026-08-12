//! #17177: the TS2694 namespace name for a TS-syntax import type
//! (`import("./mod").Missing`) must render the module symbol's name — the
//! *resolved file path* with the extension removed — not the bare specifier
//! stem.
//!
//! Structural rule: `tsc` binds a module's synthetic namespace symbol as
//! `"${removeFileExtension(fileName)}"` (`bindSourceFileAsExternalModule`), so
//! the namespace text in `Namespace 'X' has no exported member 'Y'` is the
//! resolved path. The written specifier only coincides with that path for a
//! same-directory, extension-matching import; index resolution (`./pkg` ->
//! `pkg/index`), a subdirectory, or parent traversal all make the specifier
//! diverge from the resolved path, and `tsc` shows the resolved path in every
//! case. Diagnostic paths are normalized against the project root by the
//! conformance harness, so a same-directory import still reads as the bare
//! stem once normalized — the fix only changes the cases where the specifier
//! and the resolved path genuinely differ.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;

fn ts2694_messages(files: &[(&str, &str)], entry: &str) -> Vec<String> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    )
    .into_iter()
    .filter(|d| d.code == 2694)
    .map(|d| d.message_text)
    .collect()
}

/// TS2694 message texts for a JS/JSDoc entry (`--checkJs`).
///
/// The JSDoc import-type doubling/anchor bug (#17176) was fixed in #17184, so
/// these entries emit a single TS2694; the assertions check the exact
/// message(s) and thereby the namespace text.
fn ts2694_messages_js(files: &[(&str, &str)], entry: &str) -> Vec<String> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            check_js: true,
            ..Default::default()
        },
    )
    .into_iter()
    .filter(|d| d.code == 2694)
    .map(|d| d.message_text)
    .collect()
}

/// Same-directory import: the resolved path equals the specifier stem, so the
/// namespace text is unchanged (`"mod"`). Guards against a regression on the
/// common case.
#[test]
fn same_directory_import_type_renders_specifier_stem() {
    let msgs = ts2694_messages(
        &[
            ("mod.ts", "export interface Foo { a: number }\n"),
            (
                "main.ts",
                "type M = import('./mod').Missing;\ndeclare const m: M;\n",
            ),
        ],
        "main.ts",
    );
    assert_eq!(
        msgs,
        vec!["Namespace '\"mod\"' has no exported member 'Missing'.".to_string()],
        "same-directory import type should render the bare module stem"
    );
}

/// Index resolution: `./pkg` resolves to `pkg/index.ts`. `tsc` renders the
/// resolved path `"pkg/index"`, which the written specifier `./pkg` cannot
/// express. This is the core divergence the fix repairs.
#[test]
fn index_resolution_import_type_renders_resolved_path() {
    let msgs = ts2694_messages(
        &[
            ("pkg/index.ts", "export interface Foo { a: number }\n"),
            (
                "main.ts",
                "type M = import('./pkg').Missing;\ndeclare const m: M;\n",
            ),
        ],
        "main.ts",
    );
    assert_eq!(
        msgs,
        vec!["Namespace '\"pkg/index\"' has no exported member 'Missing'.".to_string()],
        "index-resolved import type should render the resolved path, not the specifier stem"
    );
}

/// Subdirectory import: `./sub/mod` resolves to `sub/mod.ts`; resolved path and
/// specifier stem agree, but exercise the multi-segment path.
#[test]
fn subdirectory_import_type_renders_resolved_path() {
    let msgs = ts2694_messages(
        &[
            ("sub/mod.ts", "export interface Foo { a: number }\n"),
            (
                "main.ts",
                "type M = import('./sub/mod').Missing;\ndeclare const m: M;\n",
            ),
        ],
        "main.ts",
    );
    assert_eq!(
        msgs,
        vec!["Namespace '\"sub/mod\"' has no exported member 'Missing'.".to_string()],
    );
}

/// Nested namespace member: the module portion of the qualified namespace name
/// is the resolved path, with the traversed segment appended
/// (`"pkg/index".NS`).
#[test]
fn nested_namespace_member_uses_resolved_module_path() {
    let msgs = ts2694_messages(
        &[
            (
                "pkg/index.ts",
                "export namespace NS { export interface Foo {} }\n",
            ),
            (
                "main.ts",
                "type M = import('./pkg').NS.Bar;\ndeclare const m: M;\n",
            ),
        ],
        "main.ts",
    );
    assert_eq!(
        msgs,
        vec!["Namespace '\"pkg/index\".NS' has no exported member 'Bar'.".to_string()],
    );
}

/// Binder-name independence: renaming the files and the module directory does
/// not change the rule — the namespace text follows whatever path the
/// specifier resolves to, never a hard-coded name.
#[test]
fn resolved_path_is_binder_name_independent() {
    let msgs = ts2694_messages(
        &[
            (
                "widgets/entry.ts",
                "export interface Gadget { a: number }\n",
            ),
            (
                "consumer.ts",
                "type Q = import('./widgets/entry').Absent;\ndeclare const q: Q;\n",
            ),
        ],
        "consumer.ts",
    );
    assert_eq!(
        msgs,
        vec!["Namespace '\"widgets/entry\"' has no exported member 'Absent'.".to_string()],
    );
}

/// JSDoc parity: a `@type {import('./mod').Missing}` in a `.js` file resolves
/// the namespace name through the same rule. Same-directory import keeps the
/// bare stem.
#[test]
fn jsdoc_same_directory_import_type_renders_specifier_stem() {
    let msgs = ts2694_messages_js(
        &[
            ("mod.ts", "export interface Foo { a: number }\n"),
            ("a.js", "/** @type {import('./mod').Missing} */\nlet x;\n"),
        ],
        "a.js",
    );
    assert_eq!(
        msgs,
        vec!["Namespace '\"mod\"' has no exported member 'Missing'.".to_string()],
    );
}

/// JSDoc index resolution: `./pkg` -> `pkg/index.ts` must render the resolved
/// path, matching the TS-syntax path and `tsc`.
#[test]
fn jsdoc_index_resolution_import_type_renders_resolved_path() {
    let msgs = ts2694_messages_js(
        &[
            ("pkg/index.ts", "export interface Foo { a: number }\n"),
            ("a.js", "/** @type {import('./pkg').Missing} */\nlet x;\n"),
        ],
        "a.js",
    );
    assert_eq!(
        msgs,
        vec!["Namespace '\"pkg/index\"' has no exported member 'Missing'.".to_string()],
    );
}

/// The remaining site this family missed: JSDoc `@param {typeof
/// import(...).member}` (`params_type_strings.rs`'s value-position walk) must
/// apply the same resolved-path rule as the `@type`/`@typedef` and TS-syntax
/// paths above.
///
/// This site unconditionally appends a `.export=` qualifier regardless of
/// whether the target module actually has an `export =`/`module.exports =`
/// — oracle-verified (`typescript@7.0.2`) still wrong for a plain
/// named-export CommonJS module like this fixture (`tsc` omits `.export=`
/// there). That is a separate, pre-existing emission bug this test does not
/// fix; it pins today's `.export=`-suffixed text so the resolved-path half
/// (`"sub/index"`, not `"sub"`) has a regression guard.
#[test]
fn jsdoc_typeof_import_walk_param_tag_renders_resolved_path() {
    let msgs = ts2694_messages_js(
        &[
            ("sub/index.js", "exports.FOO = \"foo\";\n"),
            (
                "a.js",
                "/** @param {typeof import('./sub').Missing} p */\nfunction f(p) {}\n",
            ),
        ],
        "a.js",
    );
    assert_eq!(
        msgs,
        vec!["Namespace '\"sub/index\".export=' has no exported member 'Missing'.".to_string()],
    );
}
