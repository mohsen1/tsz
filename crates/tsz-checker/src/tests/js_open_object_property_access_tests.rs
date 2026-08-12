//! Unknown property access on an anonymous object shape in a JS file.
//!
//! In a JS file `tsc` treats a value whose type is an *anonymous* object shape
//! as open — JS code routinely builds such containers up by property assignment,
//! often across files — so an unknown property access is an implicit `any` and
//! is reported only under `noImplicitAny`. Verified against the pinned tsc
//! 7.0.2:
//!
//! ```text
//! // a.js, --allowJs --checkJs
//! var o = {}; o.nope        // noImplicitAny off: silent | on: TS2339
//! var s = "x"; s.nope       // TS2339 either way ('string')
//! class K {}; new K().nope  // TS2339 either way ('K')
//! var a = [1]; a.nope       // TS2339 either way ('number[]')
//! ```
//!
//! The discriminator is the shape's nominal `symbol`: class instances and
//! interfaces carry one, anonymous literals do not. Arrays and primitives have
//! no object shape at all.

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

fn js_reports_2339(source: &str) -> bool {
    js_codes_with(source, false).contains(&2339)
}

// --- Open anonymous containers: silent when noImplicitAny is off. ---

#[test]
fn empty_object_literal_receiver_is_open() {
    assert!(!js_reports_2339("var o = {}\no.nope\n"));
}

#[test]
fn non_empty_object_literal_receiver_is_open() {
    assert!(!js_reports_2339("var o = { a: 1 }\no.nope\n"));
}

/// A renamed binder and a different property, so the rule is structural rather
/// than tied to any particular spelling.
#[test]
fn open_container_rule_is_not_name_specific() {
    assert!(!js_reports_2339(
        "var registry = { first: 1 }\nregistry.second\n"
    ));
}

/// The container shape JS code actually builds: a nested object extended by
/// property assignment, as in the `typeFromPropertyAssignment` corpus tests.
#[test]
fn nested_assigned_container_is_open() {
    let source = "var N = {}\nN.commands = {}\nN.commands.a = 1\nN.commands.b\n";
    assert!(!js_reports_2339(source));
}

#[test]
fn writes_to_an_open_container_are_also_silent() {
    assert!(!js_reports_2339("var o = {}\no.nope = 1\n"));
}

// --- noImplicitAny restores the diagnostic. ---

#[test]
fn no_implicit_any_reports_on_open_container() {
    assert!(js_codes_with("var o = {}\no.nope\n", true).contains(&2339));
}

// --- Declared shapes keep reporting, with or without noImplicitAny. ---

/// Witness `typeFromPropertyAssignment28`: a class instance carries a nominal
/// symbol, so it is not an open container.
#[test]
fn class_instance_receiver_still_reports() {
    let source = "class C { constructor() { this.p = 1 } }\nvar c = new C()\nc.nope\n";
    assert!(js_reports_2339(source));
    assert!(js_codes_with(source, true).contains(&2339));
}

#[test]
fn string_receiver_still_reports() {
    assert!(js_reports_2339("var s = \"x\"\ns.nope\n"));
}

#[test]
fn array_receiver_still_reports() {
    assert!(js_reports_2339("var a = [1, 2]\na.nope\n"));
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
