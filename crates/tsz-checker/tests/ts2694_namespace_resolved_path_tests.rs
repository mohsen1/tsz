//! TS2694's "Namespace '{0}' has no exported member '{1}'." names the
//! *resolved file path* (extension stripped), not the literal specifier text
//! — for both the TS-syntax `import("./mod").Member` form and the JSDoc
//! `import('./mod').Member` / `typeof import('./mod').a.b` forms, and
//! regardless of whether the specifier is relative or bare. Oracle-verified
//! against pinned `typescript@7.0.2`:
//!
//!   type M = import('./types.js').Missing;
//!   // tsc: Namespace '"/abs/path/to/types"' has no exported member 'Missing'.
//!
//! This is a different rule from `typeof import("...")` type *printing*
//! (`imported_namespace_display_module_name`), which deliberately keeps the
//! literal relative specifier — see the comments at that function's call
//! sites for the distinction. An ambient `declare module "x"` (no backing
//! file) has no resolved path to report, so it keeps the literal specifier;
//! that fallback path is pinned here too, as a regression guard.
//!
//! Owner: `crates/tsz-checker/src/state/type_resolution/import_type.rs`
//! (`import_type_resolved_display_name`), reused by the JSDoc string-parse
//! paths in `jsdoc/resolution/import_reference.rs` and
//! `jsdoc/params_type_strings.rs`.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

const TS2694: u32 = 2694;

fn check(files: &[(&str, &str)], entry: &str, allow_js: bool) -> Vec<(u32, String)> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            allow_js,
            check_js: allow_js,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn only_ts2694(diags: &[(u32, String)]) -> &str {
    let matches: Vec<_> = diags.iter().filter(|(c, _)| *c == TS2694).collect();
    assert_eq!(matches.len(), 1, "expected exactly one TS2694: {diags:?}");
    &matches[0].1
}

/// Like `only_ts2694`, but tolerant of #17176's pre-existing (separately
/// owned) duplicate-anchor bug on the JSDoc `@type` path: a `@type` import-
/// type miss can fire TS2694 twice at two anchors. Both firings must still
/// carry the fixed, resolved-path text — that duplication is out of scope
/// here, the message content is not.
fn ts2694_messages(diags: &[(u32, String)]) -> Vec<&str> {
    let matches: Vec<&str> = diags
        .iter()
        .filter(|(c, _)| *c == TS2694)
        .map(|(_, m)| m.as_str())
        .collect();
    assert!(
        !matches.is_empty(),
        "expected at least one TS2694: {diags:?}"
    );
    matches
}

/// A relative specifier that resolves through directory-index resolution
/// (`./sub` -> `sub/index.d.ts`) must render the *resolved* path
/// (`"sub/index"`), not the literal specifier stem (`"sub"`) the pre-fix
/// code produced by stripping `./` off the source text.
#[test]
fn ts_import_type_bare_member_uses_resolved_path_not_literal_specifier() {
    let diags = check(
        &[
            ("sub/index.d.ts", "export interface Q {}\n"),
            ("test.ts", "type X = import(\"./sub\").Missing;\n"),
        ],
        "test.ts",
        false,
    );
    assert_eq!(
        only_ts2694(&diags),
        "Namespace '\"sub/index\"' has no exported member 'Missing'."
    );
}

/// Same rule for a nested-segment miss (`import("./sub").Q.Missing`): the
/// namespace half of the display must still be the resolved path.
#[test]
fn ts_import_type_nested_segment_uses_resolved_path_not_literal_specifier() {
    let diags = check(
        &[
            (
                "sub/index.d.ts",
                "export declare namespace Q {\n  function method(): void;\n}\n",
            ),
            ("test.ts", "type X = import(\"./sub\").Q.Missing;\n"),
        ],
        "test.ts",
        false,
    );
    assert_eq!(
        only_ts2694(&diags),
        "Namespace '\"sub/index\".Q' has no exported member 'Missing'."
    );
}

/// An ambient `declare module "x"` has no backing file — its symbol name
/// really is the literal specifier in tsc, so the resolved-path rule must
/// NOT apply there. Regression guard for the fallback path.
#[test]
fn ts_import_type_ambient_module_keeps_literal_specifier() {
    let diags = check(
        &[(
            "test.ts",
            concat!(
                "declare module \"my-amb-mod\" {\n",
                "  export const X: number;\n",
                "}\n",
                "type M = import(\"my-amb-mod\").Missing;\n",
            ),
        )],
        "test.ts",
        false,
    );
    assert_eq!(
        only_ts2694(&diags),
        "Namespace '\"my-amb-mod\"' has no exported member 'Missing'."
    );
}

/// JSDoc `import('./sub').Missing` (the string-parse path, distinct from the
/// TS-syntax resolver above) must apply the same resolved-path rule.
#[test]
fn jsdoc_import_type_bare_member_uses_resolved_path_not_literal_specifier() {
    let diags = check(
        &[
            ("sub/index.js", "exports.FOO = \"foo\";\n"),
            (
                "test.js",
                "/** @type {import('./sub').Missing} */\nlet x;\n",
            ),
        ],
        "test.js",
        true,
    );
    for message in ts2694_messages(&diags) {
        assert_eq!(
            message,
            "Namespace '\"sub/index\"' has no exported member 'Missing'."
        );
    }
}

/// JSDoc `typeof import('./sub').Missing` (the value-position walk in
/// `params_type_strings.rs`) must apply the same resolved-path rule.
///
/// This path always appends a `.export=` qualifier (oracle: tsc omits it for
/// a plain CommonJS module with no `export =` target — confirmed divergent,
/// e.g. `/tmp/repro5` in this PR's investigation). That's a pre-existing,
/// separate display quirk this PR does not touch (same class as the
/// `.export=` divergence documented in
/// `ts2694_typeof_import_qualified_type_only_member_tests.rs`); this test
/// pins today's actual text so the resolved-path half of the fix (`"sub/index"`,
/// not `"sub"`) has a regression guard.
#[test]
fn jsdoc_typeof_import_walk_uses_resolved_path_not_literal_specifier() {
    let diags = check(
        &[
            ("sub/index.js", "exports.FOO = \"foo\";\n"),
            (
                "test.js",
                "/** @param {typeof import('./sub').Missing} p */\nfunction f(p) {}\n",
            ),
        ],
        "test.js",
        true,
    );
    assert_eq!(
        only_ts2694(&diags),
        "Namespace '\"sub/index\".export=' has no exported member 'Missing'."
    );
}
