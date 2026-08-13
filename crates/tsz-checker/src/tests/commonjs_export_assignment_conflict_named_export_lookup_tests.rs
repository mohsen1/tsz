//! A `module.exports = X` mixed with a sibling `module.exports.p = ...` is an
//! illegal combination (`tsc` reports TS2309, "An export assignment cannot be
//! used in a module with other exported elements.") — the module's exported
//! type stays exactly `X`, with the sibling property never folded in. Two
//! defects on the requiring side, both fixed here:
//!
//! 1. `JsExportSurface::to_type_id` already respects the conflict
//!    (`suppresses_expando_merge` keeps the merged type as `X` alone), but
//!    `JsExportSurface::lookup_named_export` — the query boundary every
//!    `require()`/import consumer uses to answer "does this module export
//!    `p`?" — did not consult the same suppression. It found `p` straight in
//!    `named_exports` regardless, so a `const { p } = require(...)`
//!    destructure in a THIRD file silently resolved `p` to the export's
//!    checker-computed type, even though the type `require(...)` actually
//!    produced (`X`) never carried `p`. Fixed at the query-boundary owner:
//!    `lookup_named_export` now returns `None` for a `named_exports` entry
//!    whenever `suppresses_expando_merge()` holds, matching what
//!    `to_type_id` already produces. `prototype_members` is untouched — a
//!    different mechanism, not implicated in this conflict.
//! 2. Once (1) makes the property genuinely missing, the destructuring
//!    property-not-found path (`state/variable_checking/destructuring.rs`)
//!    reported the generic TS2339 ("Property does not exist on type") for
//!    *any* `const { p } = require(...)` miss, JS `require()` included — not
//!    just this conflict. `tsc` types a JS `require()` result as a module
//!    instance type, so a missing destructured property there is diagnosed
//!    like a named `import { p } from "mod"` miss: TS2305 ("has no exported
//!    member"). Fixed via a new
//!    `commonjs_require_destructure_module_specifier` helper that recognizes
//!    a binding pattern's initializer as a `require()` call into a real
//!    CommonJS export surface and redirects the diagnostic.
//!
//! Verified against the pinned tsc 7.0.2 oracle (both the exact conformance
//! fixture `conformance/salsa/commonJSAliasedExport.ts`/`bug43713.js`, and
//! direct CLI runs — every case below is byte-for-byte diagnostic-matched
//! against a local `typescript@7.0.2` install).

use crate::context::CheckerOptions;
use crate::test_utils::{check_multi_file_with_libs_stamped, load_lib_files};

fn js_codes(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..CheckerOptions::default()
    };
    check_multi_file_with_libs_stamped(files, entry, options, &load_lib_files(&["es5.d.ts"]))
        .into_iter()
        .map(|d| d.code)
        .collect()
}

const NO_EXPORTED_MEMBER: u32 = 2305;
const EXPORT_ASSIGNMENT_CONFLICT: u32 = 2309;
const MISSING_PROPERTY: u32 = 2339;

/// Direct repro: `module.exports = <arrow fn>` plus a sibling
/// `module.exports.funky = funky` — a `require()` destructure of `funky` in a
/// third file must see TS2305, not resolve `funky`'s type.
#[test]
fn arrow_function_export_conflict_hides_sibling_from_requiring_file() {
    let module_src = concat!(
        "const donkey = (ast) => ast;\n",
        "function funky(declaration) { return false; }\n",
        "module.exports = donkey;\n",
        "module.exports.funky = funky;\n",
    );
    let use_src = "const { funky } = require('./mod.js');\nfunky;\n";
    let codes = js_codes(&[("mod.js", module_src), ("use.js", use_src)], "use.js");
    assert!(
        codes.contains(&NO_EXPORTED_MEMBER),
        "expected TS2305 for the requiring file, got: {codes:?}"
    );

    let mod_codes = js_codes(&[("mod.js", module_src), ("use.js", use_src)], "mod.js");
    assert!(
        mod_codes.contains(&EXPORT_ASSIGNMENT_CONFLICT),
        "expected TS2309 in the exporting file, got: {mod_codes:?}"
    );
    assert!(
        mod_codes.contains(&MISSING_PROPERTY),
        "expected TS2339 on the sibling property write itself, got: {mod_codes:?}"
    );
}

/// Same conflict, a renamed destructuring binding (`funky: local`) — the
/// lookup runs by the exported property name, not the local binding name.
#[test]
fn renamed_binding_element_still_hides_the_conflicted_sibling() {
    let module_src = concat!(
        "const donkey = (ast) => ast;\n",
        "function funky(declaration) { return false; }\n",
        "module.exports = donkey;\n",
        "module.exports.funky = funky;\n",
    );
    let use_src = "const { funky: local } = require('./mod.js');\nlocal;\n";
    let codes = js_codes(&[("mod.js", module_src), ("use.js", use_src)], "use.js");
    assert!(
        codes.contains(&NO_EXPORTED_MEMBER),
        "expected TS2305 for a renamed binding, got: {codes:?}"
    );
}

/// A class value in the `module.exports = X` position is a different
/// direct-export shape than a plain function — the conflict still holds.
#[test]
fn class_export_conflict_also_hides_the_sibling() {
    let module_src = concat!(
        "class Donkey { }\n",
        "function funky(declaration) { return false; }\n",
        "module.exports = Donkey;\n",
        "module.exports.funky = funky;\n",
    );
    let use_src = "const { funky } = require('./mod.js');\nfunky;\n";
    let codes = js_codes(&[("mod.js", module_src), ("use.js", use_src)], "use.js");
    assert!(
        codes.contains(&NO_EXPORTED_MEMBER),
        "expected TS2305 for a class direct-export conflict, got: {codes:?}"
    );
}

/// Negative control: no whole-module `module.exports = X` reassignment at
/// all, only plain named-property exports — no conflict, so `funky` remains a
/// perfectly valid named export and must NOT report TS2305.
#[test]
fn plain_named_export_without_reassignment_is_unaffected() {
    let module_src = "function funky(declaration) { return false; }\nexports.funky = funky;\n";
    let use_src = "const { funky } = require('./mod.js');\nfunky;\n";
    let codes = js_codes(&[("mod.js", module_src), ("use.js", use_src)], "use.js");
    assert!(
        !codes.contains(&NO_EXPORTED_MEMBER),
        "did not expect TS2305 for an unconflicted named export, got: {codes:?}"
    );
}

/// An object-literal direct export is not exempt: `tsc` (verified against the
/// 7.0.2 oracle) reports the same TS2309/TS2339/TS2305 trio for a sibling
/// write *after* the object-literal reassignment, exactly like the function
/// and class cases above — the conflict is syntactic (whole-module
/// reassignment plus a later named write), not conditioned on the RHS shape.
#[test]
fn object_literal_direct_export_conflict_also_hides_the_sibling() {
    let module_src = concat!(
        "module.exports = { a: 1 };\n",
        "module.exports.funky = function (d) { return d; };\n",
    );
    let use_src = "const { funky } = require('./mod.js');\nfunky;\n";
    let codes = js_codes(&[("mod.js", module_src), ("use.js", use_src)], "use.js");
    assert!(
        codes.contains(&NO_EXPORTED_MEMBER),
        "expected TS2305 for the object-literal conflict too, got: {codes:?}"
    );
}

/// Negative control: the property is inline in the object literal itself —
/// there is no *later* sibling write augmenting the export after the
/// reassignment, so there is nothing for TS2309 to conflict with, and
/// `funky` is a perfectly ordinary member of the exported literal.
#[test]
fn object_literal_inline_member_without_augmentation_is_unaffected() {
    let module_src = concat!(
        "function funky(d) { return d; }\n",
        "module.exports = { a: 1, funky: funky };\n",
    );
    let use_src = "const { funky } = require('./mod.js');\nfunky;\n";
    let codes = js_codes(&[("mod.js", module_src), ("use.js", use_src)], "use.js");
    assert!(
        !codes.contains(&NO_EXPORTED_MEMBER),
        "did not expect TS2305 for an inline object-literal member, got: {codes:?}"
    );
}
