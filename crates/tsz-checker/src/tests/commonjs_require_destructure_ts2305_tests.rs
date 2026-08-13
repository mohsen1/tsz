//! `const { x } = require('./mod')` destructuring resolves `x` through the
//! module's *named-export* surface, not generic structural property lookup.
//!
//! Structural rule: when a static (non-computed) destructured name is absent
//! from a CommonJS `require()` target's own type, `tsc` reports TS2305 ("has
//! no exported member"), matching `import { x } from './mod'` — never TS2339
//! ("property does not exist"), which generic destructuring uses for every
//! other missing-property source (object literals, classes, etc.). `tsz`
//! previously ran every object-destructuring miss through the same TS2339
//! path (`crates/tsz-checker/src/state/variable_checking/destructuring.rs`,
//! `get_binding_element_type_with_request`'s `PropertyNotFound` arm) because
//! that generic path is never differentiated by where the parent type came
//! from. It also returned `TypeId::ANY` for the miss instead of the `ERROR`
//! sentinel the sibling `parent_type == TypeId::UNKNOWN` branch already uses
//! "to suppress cascading diagnostics" — so a real `ANY`-vs-declared-type
//! mismatch downstream (e.g. a JS var-redeclaration consistency check) could
//! fire a second, spurious diagnostic off the same miss.
//!
//! The fix detects a top-level `require()` destructuring source
//! (`require_module_specifier_for_binding_pattern`) and, when the target
//! resolves to a JS module with a CommonJS export surface
//! (`js_commonjs_require_target_is_js_module`), reports TS2305 via
//! `emit_no_exported_member_error` and returns `TypeId::ERROR` instead.
//!
//! Oracle repro: `TypeScript/tests/cases/conformance/salsa/commonJSAliasedExport.ts`
//! (`module.exports = donkey; module.exports.funky = funky;` — a TS7
//! export-assignment conflict, TS2309, which drops `funky` from the module's
//! real merged type per `JsExportSurface::suppresses_expando_merge`, even
//! though `funky` still appears in the syntactic `named_exports` list the
//! plain `has_named_export` check alone can't tell apart from a genuine
//! export).

use crate::context::CheckerOptions;
use crate::diagnostics::diagnostic_codes;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;

/// Mirrors `js_commonjs_default_reexport_ts2305_tests::check` — sets
/// `report_unresolved_imports` (off by default on `test_utils::check_multi_file`)
/// so the TS2305 path actually runs.
fn check(files: &[(&str, &str)], entry: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let options = CheckerOptions {
        module: ModuleKind::CommonJS,
        allow_js: true,
        check_js: true,
        ..CheckerOptions::default()
    };

    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let entry_idx = file_names
        .iter()
        .position(|name| name == entry)
        .unwrap_or_else(|| panic!("entry_file {entry:?} not found in files"));
    let (resolved_module_paths, resolved_modules) =
        crate::module_resolution::build_module_resolution_maps(&file_names);

    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &types,
        file_names[entry_idx].clone(),
        options,
    );
    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    checker.ctx.set_lib_contexts(Vec::new());
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);
    checker.ctx.report_unresolved_imports = true;

    checker.prime_module_augmentation_bodies();
    checker.check_source_file(roots[entry_idx]);
    checker.ctx.diagnostics.clone()
}

fn codes(diags: &[crate::diagnostics::Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

/// The oracle repro: a `module.exports = donkey` / `module.exports.funky =
/// funky` conflict (TS2309) means `funky` is not a real export even though it
/// is syntactically written — destructuring it must report TS2305, not
/// TS2339, and must not cascade into a spurious downstream diagnostic.
#[test]
fn require_destructure_of_conflict_excluded_property_reports_ts2305() {
    let diags = check(
        &[
            (
                "./mod.js",
                r#"const donkey = (ast) => ast;
function funky(declaration) { return false; }
module.exports = donkey;
module.exports.funky = funky;
"#,
            ),
            (
                "./usage.js",
                r#"const { funky } = require('./mod');
/** @type {boolean} */
var diddy
var diddy = funky(1)
"#,
            ),
        ],
        "./usage.js",
    );
    let cs = codes(&diags);
    assert!(
        cs.contains(&diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER),
        "expected TS2305, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
    assert!(
        !cs.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "TS2305 should replace TS2339 for this miss, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
    assert!(
        !cs.contains(
            &diagnostic_codes::SUBSEQUENT_VARIABLE_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_VARIABLE_MUST_BE_OF_TYP
        ),
        "the ERROR-typed miss must not cascade into a var-redeclaration TS2403, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

/// Plain missing export, no `module.exports = X` conflict at all — the
/// simplest case the structural rule must also cover, not just the
/// conflict-suppression edge case above.
#[test]
fn require_destructure_of_plain_missing_export_reports_ts2305() {
    let diags = check(
        &[
            (
                "./mod.js",
                r#"module.exports = { a: 1 };
"#,
            ),
            (
                "./usage.js",
                r#"const { b } = require('./mod');
"#,
            ),
        ],
        "./usage.js",
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

/// Negative control: a property that genuinely exists on the required
/// module's type must destructure cleanly — the fix only redirects the
/// diagnostic *kind* for a miss, it must not turn a hit into a miss.
#[test]
fn require_destructure_of_existing_export_reports_nothing() {
    let diags = check(
        &[
            (
                "./mod.js",
                r#"module.exports = { a: 1 };
"#,
            ),
            (
                "./usage.js",
                r#"const { a } = require('./mod');
"#,
            ),
        ],
        "./usage.js",
    );
    assert!(
        diags.is_empty(),
        "expected no diagnostics, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

/// Renamed binders (not the oracle repro's `funky`/`donkey`/`diddy`) exercise
/// the same structural rule, not a name-specific special case.
#[test]
fn require_destructure_of_conflict_excluded_property_renamed_binders() {
    let diags = check(
        &[
            (
                "./widget.js",
                r#"const gadget = () => 1;
function extra() { return 2; }
module.exports = gadget;
module.exports.extra = extra;
"#,
            ),
            (
                "./consumer.js",
                r#"const { extra } = require('./widget');
"#,
            ),
        ],
        "./consumer.js",
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

/// Negative control: plain `var mod1 = require(...)` (no destructuring) must
/// keep reporting TS2339 for a missing property access — the fix is scoped to
/// destructuring's named-export resolution, not `require()` results in
/// general. Mirrors the oracle
/// `moduleExportWithExportPropertyAssignment.ts` family.
#[test]
fn require_without_destructure_keeps_reporting_ts2339() {
    let diags = check(
        &[
            (
                "./mod.js",
                r#"module.exports = function () {};
module.exports.f = function () {};
"#,
            ),
            (
                "./usage.js",
                r#"var mod1 = require('./mod');
mod1.f();
"#,
            ),
        ],
        "./usage.js",
    );
    let cs = codes(&diags);
    assert!(
        cs.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "expected TS2339, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
    assert!(
        !cs.contains(&diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER),
        "non-destructuring access must not gain TS2305, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

/// The conflict is syntactic (whole-module reassignment plus a later named
/// write), not conditioned on the RHS shape — a class value in the
/// `module.exports = X` position must hide the sibling exactly like the
/// arrow-function case above.
#[test]
fn require_destructure_of_conflict_excluded_property_reports_ts2305_for_class_export() {
    let diags = check(
        &[
            (
                "./mod.js",
                r#"class Donkey {}
function funky(declaration) { return false; }
module.exports = Donkey;
module.exports.funky = funky;
"#,
            ),
            (
                "./usage.js",
                r#"const { funky } = require('./mod');
"#,
            ),
        ],
        "./usage.js",
    );
    assert!(
        codes(&diags).contains(&diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER),
        "expected TS2305 for a class direct-export conflict, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

/// Same conflict, object-literal RHS — the third distinct
/// `module.exports = X` shape (function/class/object-literal) the structural
/// rule must cover uniformly.
#[test]
fn require_destructure_of_conflict_excluded_property_reports_ts2305_for_object_literal_export() {
    let diags = check(
        &[
            (
                "./mod.js",
                r#"module.exports = { a: 1 };
module.exports.funky = function (d) { return d; };
"#,
            ),
            (
                "./usage.js",
                r#"const { funky } = require('./mod');
"#,
            ),
        ],
        "./usage.js",
    );
    assert!(
        codes(&diags).contains(&diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER),
        "expected TS2305 for the object-literal conflict too, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}
