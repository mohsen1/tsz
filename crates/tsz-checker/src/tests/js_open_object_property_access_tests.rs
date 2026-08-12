//! Unknown property access on anonymous object shapes in a JS file.
//!
//! `tsc` 7.0.2 has no `noImplicitAny`-gated leniency here: an access that
//! misses on the receiver's type reports `TS2339` regardless of
//! `noImplicitAny`. The cases that stay silent are the ones the *expando
//! declaration machinery* owns — a write that itself declares the member on an
//! expando-capable container (a `var` initialized with an empty object
//! literal, or `exports.x` in a CommonJS module without an export-assignment
//! mix). A read of a member no write declared is `TS2339` even on those
//! containers. Verified against the pinned tsc 7.0.2 (`--allowJs --checkJs`,
//! each case run with and without `--noImplicitAny`):
//!
//! ```text
//! var o = {}; o.nope            // TS2339 either way (read never declared)
//! var o = {}; o.nope = 1        // silent either way (write declares it)
//! var o = { a: 1 }; o.nope      // TS2339 either way
//! var o = { a: 1 }; o.nope = 1  // TS2339 either way (non-empty init: not expando)
//! var N = {}; N.c = {}; N.c.a = 1; N.c.b  // only `N.c.b` is TS2339
//! function f() {}; f.nope       // TS2339 either way
//! module.exports = { z: 1 }; exports.zag = 2  // TS2309 + TS2339 either way
//! ```

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn js_codes_with(source: &str, no_implicit_any: bool) -> Vec<u32> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        no_implicit_any,
        ..CheckerOptions::default()
    };
    check_source(source, "test.js", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn ts_codes(source: &str) -> Vec<u32> {
    check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// TS2339 must fire with `noImplicitAny` both off and on.
fn js_reports_2339_both_configs(source: &str) -> bool {
    js_codes_with(source, false).contains(&2339) && js_codes_with(source, true).contains(&2339)
}

/// No TS2339 with `noImplicitAny` either off or on.
fn js_silent_2339_both_configs(source: &str) -> bool {
    !js_codes_with(source, false).contains(&2339) && !js_codes_with(source, true).contains(&2339)
}

// --- Reads of a member no write declared: TS2339 regardless of noImplicitAny. ---

#[test]
fn read_on_empty_literal_container_reports_both_configs() {
    assert!(js_reports_2339_both_configs("var o = {}\no.nope\n"));
}

#[test]
fn read_on_non_empty_literal_reports_both_configs() {
    assert!(js_reports_2339_both_configs("var o = { a: 1 }\no.nope\n"));
}

/// A renamed binder and a different property, so the rule is structural rather
/// than tied to any particular spelling.
#[test]
fn read_rule_is_not_name_specific() {
    assert!(js_reports_2339_both_configs(
        "var registry = { first: 1 }\nregistry.second\n"
    ));
}

/// Nested container built by property assignment: the writes are expando
/// declarations and stay silent, but the read of a member no write declared
/// still reports.
#[test]
fn undeclared_read_on_nested_assigned_container_reports() {
    let source = "var N = {}\nN.commands = {}\nN.commands.a = 1\nN.commands.b\n";
    assert!(js_reports_2339_both_configs(source));
}

#[test]
fn function_receiver_undeclared_read_reports_both_configs() {
    assert!(js_reports_2339_both_configs("function f() {}\nf.nope\n"));
}

// --- Writes that are expando declarations: silent regardless of noImplicitAny. ---

#[test]
fn write_to_empty_literal_container_is_a_declaration() {
    assert!(js_silent_2339_both_configs("var o = {}\no.nope = 1\n"));
}

#[test]
fn nested_expando_writes_are_declarations() {
    assert!(js_silent_2339_both_configs(
        "var N = {}\nN.commands = {}\nN.commands.a = 1\n"
    ));
}

#[test]
fn expando_declared_member_read_is_clean() {
    assert!(js_silent_2339_both_configs(
        "var o = {}\no.zag = 2\nvar u = o.zag\n"
    ));
}

#[test]
fn exports_expando_write_without_export_assignment_is_clean() {
    assert!(js_silent_2339_both_configs("exports.zag = 2\n"));
}

// --- Non-expando writes: TS2339 regardless of noImplicitAny. ---

// KNOWN GAP (not pinned here): `var o = { zig: 1 }; o.zag = 2` should be
// TS2339 both configs — tsc's expando rule (`getExpandoInitializer`) accepts
// an object-literal initializer only when it is EMPTY, while tsz's expando
// machinery (binder `expression_flow.rs` registration and the checker's
// `root_symbol_supports_js_expando_*` predicates) accepts any object literal
// and classifies the write as a declaration. Pre-existing false negative,
// independent of the removed suppression gate; tracked in its own issue.

/// The export-assignment mix (TS2309 surface): `exports`/`module.exports` are
/// typed as the export= target, so a sibling write to an undeclared member
/// reports against that target's type.
#[test]
fn export_assignment_mix_exports_write_reports_both_configs() {
    let source = "module.exports = { zig: 1 }\nexports.zag = 2\n";
    assert!(js_reports_2339_both_configs(source));
}

#[test]
fn export_assignment_mix_module_exports_write_reports_both_configs() {
    let source = "module.exports = { zig: 1 }\nmodule.exports.zag = 2\n";
    assert!(js_reports_2339_both_configs(source));
}

#[test]
fn export_assignment_mix_via_variable_target_reports_both_configs() {
    let source = "var o = { zig: 1 }\nmodule.exports = o\nexports.zag = 2\n";
    assert!(js_reports_2339_both_configs(source));
}

#[test]
fn export_assignment_mix_undeclared_read_reports_both_configs() {
    let source = "module.exports = { zig: 1 }\nvar v = exports.zag\n";
    assert!(js_reports_2339_both_configs(source));
}

// --- Declared shapes keep reporting, with or without noImplicitAny. ---

/// Witness `typeFromPropertyAssignment28`: a class instance carries a nominal
/// symbol, so unknown members report.
#[test]
fn class_instance_receiver_still_reports() {
    let source = "class C { constructor() { this.p = 1 } }\nvar c = new C()\nc.nope\n";
    assert!(js_reports_2339_both_configs(source));
}

#[test]
fn string_receiver_still_reports() {
    assert!(js_reports_2339_both_configs("var s = \"x\"\ns.nope\n"));
}

#[test]
fn array_receiver_still_reports() {
    assert!(js_reports_2339_both_configs("var a = [1, 2]\na.nope\n"));
}

// --- TypeScript files are unaffected. ---

#[test]
fn typescript_object_literal_receiver_still_reports() {
    assert!(ts_codes("var o = {}\no.nope\n").contains(&2339));
}

#[test]
fn typescript_annotated_object_receiver_still_reports() {
    let source = "const p: { a: number } = { a: 1 };\np.b;\n";
    assert!(ts_codes(source).contains(&2339));
}
