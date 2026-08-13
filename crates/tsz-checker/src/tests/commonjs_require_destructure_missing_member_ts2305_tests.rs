//! A name destructured directly from a `require(...)` call is checked like
//! an ES named import, not like a generic property access.
//!
//! Structural rule: `const { a } = require("mod")` (a "require variable
//! declaration") is the same shape a real `import { a } from "mod"` allows,
//! so `tsc` validates each such binding name against the module's exported
//! members and reports a genuinely missing one as `TS2305` ("Module has no
//! exported member"), not `TS2339` ("Property does not exist on type"). The
//! resulting binding also resolves to the error type, so it does not cascade
//! into downstream identical-type checks. `tsz`'s `get_binding_element_type_with_request`
//! (`state/variable_checking/destructuring.rs`) instead ran every destructure
//! through ordinary structural property access — correct for other
//! destructuring sources, but wrong for this specific require-call shape.
//!
//! The check only fires for a flat binding element whose local name is a
//! plain identifier (the same shape a named import specifier allows); a
//! binding element that destructures its value further
//! (`{ a: { nested } }`), or a `require` identifier shadowed by a local
//! declaration, falls back to the pre-existing property-access behavior.

use crate::context::CheckerOptions;
use crate::diagnostics::{Diagnostic, diagnostic_codes};
use crate::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

fn check(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
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
}

fn codes(diags: &[Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

/// The `commonJSAliasedExport.ts` / `bug43713.js` salsa conformance witness:
/// `module.exports = donkey; module.exports.funky = funky;` is illegal
/// (`TS2309`), and the consuming `require()`-destructure of the never-legal
/// `funky` binding must report `TS2305`, not `TS2339`.
#[test]
fn require_destructure_of_illegal_export_reports_ts2305() {
    let diags = check(
        &[
            (
                "producer.js",
                "const donkey = (ast) => ast;\n\
                 function funky(declaration) {\n    return false;\n}\n\
                 module.exports = donkey;\n\
                 module.exports.funky = funky;\n",
            ),
            (
                "consumer.js",
                "const { funky } = require('./producer');\n\
                 /** @type {boolean} */\n\
                 var diddy\n\
                 var diddy = funky(1)\n",
            ),
        ],
        "consumer.js",
    );
    assert!(
        codes(&diags).contains(&diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER),
        "expected TS2305, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
    // The require()-destructure diagnostic itself must be TS2305 at the
    // destructure site, and the resulting `any`-shaped funky's later call
    // must not cascade into TS2403 on the `var diddy` redeclaration — the
    // binding element resolves to the error type, matching tsc.
    assert!(
        !codes(&diags).contains(
            &diagnostic_codes::SUBSEQUENT_VARIABLE_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_VARIABLE_MUST_BE_OF_TYP
        ),
        "TS2403 must not cascade from the unresolved require()-destructured name, got: {:?}",
        diags.iter().map(|d| (d.code, d.message_text.clone())).collect::<Vec<_>>(),
    );
}

/// Renamed binders: same structural shape, different identifiers throughout
/// (module path, producer/consumer names, exported/missing member name).
#[test]
fn require_destructure_of_illegal_export_reports_ts2305_renamed_binders() {
    let diags = check(
        &[
            (
                "widget.js",
                "const base = (input) => input;\n\
                 function helper(value) {\n    return true;\n}\n\
                 module.exports = base;\n\
                 module.exports.helper = helper;\n",
            ),
            (
                "app.js",
                "const { helper } = require('./widget');\n\
                 var flag = helper(1);\n",
            ),
        ],
        "app.js",
    );
    assert!(
        codes(&diags).contains(&diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER),
        "expected TS2305, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

/// Positive control: when the required module genuinely exports the
/// destructured name (a plain object-literal export, not an illegal
/// export-assignment-plus-property shape), no TS2305/TS2339 fires.
#[test]
fn require_destructure_of_genuine_export_is_clean() {
    let diags = check(
        &[
            (
                "producer.js",
                "module.exports = { funky: function (x) { return true; } };\n",
            ),
            (
                "consumer.js",
                "const { funky } = require('./producer');\n\
                 var x = funky(1);\n",
            ),
        ],
        "consumer.js",
    );
    assert!(
        !codes(&diags).contains(&diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER),
        "did not expect TS2305, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
    assert!(
        !codes(&diags).contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "did not expect TS2339, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

/// Renamed binding (`{ funky: f }`) still routes through the require-import
/// check — the rename target is still a plain identifier — and the missing
/// member is reported under its original exported name, not the local alias.
#[test]
fn require_destructure_renamed_binding_reports_ts2305_on_original_name() {
    let diags = check(
        &[
            (
                "producer.js",
                "const donkey = (ast) => ast;\n\
                 function funky(declaration) {\n    return false;\n}\n\
                 module.exports = donkey;\n\
                 module.exports.funky = funky;\n",
            ),
            (
                "consumer.js",
                "const { funky: f } = require('./producer');\nvar y = f;\n",
            ),
        ],
        "consumer.js",
    );
    assert!(
        codes(&diags).contains(&diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER),
        "expected TS2305, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
    let msg = diags
        .iter()
        .find(|d| d.code == diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER)
        .map(|d| d.message_text.clone())
        .unwrap_or_default();
    assert!(
        msg.contains("'funky'"),
        "TS2305 must name the original exported member, not the local alias: {msg}",
    );
}

/// Negative control: a binding element that destructures its value *further*
/// (`{ funky: { nested } }`) is not the flat shape a named import specifier
/// allows, so it keeps the pre-existing structural property-access check
/// (`TS2339`) instead of the require-import `TS2305` path.
#[test]
fn require_destructure_nested_pattern_keeps_property_access_ts2339() {
    let diags = check(
        &[
            (
                "producer.js",
                "const donkey = (ast) => ast;\n\
                 function funky(declaration) {\n    return false;\n}\n\
                 module.exports = donkey;\n\
                 module.exports.funky = funky;\n",
            ),
            (
                "consumer.js",
                "const { funky: { nested } } = require('./producer');\n",
            ),
        ],
        "consumer.js",
    );
    assert!(
        !codes(&diags).contains(&diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER),
        "nested destructuring must not use the require-import TS2305 path, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
    assert!(
        codes(&diags).contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "expected the ordinary TS2339 property-access diagnostic, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}
