//! A JS assignment declaration (expando write) binds a member only when the
//! write appears in the SAME file as the host's declaration.
//!
//! Structural rule (oracle-pinned against tsc 7.0.2, tsconfig-sentinel method,
//! both `noImplicitAny` configs, global script files):
//!
//! > A property write to an expando-capable host (`function f() {}`,
//! > `var o = {}`, `class C {}`) declared in a DIFFERENT file declares
//! > nothing: the write and every read of that member report `TS2339`
//! > subject to the ordinary receiver rules. The `noImplicitAny`-off
//! > open-container leniency still silences `{}`-typed receivers; function
//! > and class receivers report `TS2339` under BOTH configs. Members written
//! > in the host's OWN file remain visible (and typed) from every file.
//!
//! `tsz` previously implemented the strada-era cross-file merge through three
//! cooperating sites: the binder's unresolved-root recording
//! (`record_unresolved_root_expando_write`, removed), the checker's
//! cross-file write predicates (`root_symbol_supports_js_direct_expando_write`
//! and `is_expando_function_assignment`, now gated on the host's declaring
//! file), and a checked-JS function-write suppression at the `TS2339`
//! emission site (`error_reporter/properties.rs`, same gate). Corpus witness:
//! `conformance/salsa/typeFromPropertyAssignment12` (missing `TS2339`).
//!
//! The cells below pin the `noImplicitAny`-ON half of the oracle matrix. The
//! OFF half (open-container leniency silences the `{}`-receiver cells while
//! function/class receivers keep erroring) is verified through the production
//! driver: this multi-file test harness resolves the receiver's shape with a
//! nominal symbol the driver does not attach, so the leniency gate
//! (`js_open_object_receiver_under_implicit_any`) never fires here and the
//! OFF cells would pin harness behavior, not compiler behavior.

use crate::CheckerOptions;
use crate::test_utils::check_multi_file_with_global_index;

fn cross_file_codes(host_source: &str, writer_source: &str, no_implicit_any: bool) -> Vec<u32> {
    check_multi_file_with_global_index(
        &[("host.js", host_source), ("writer.js", writer_source)],
        "writer.js",
        CheckerOptions {
            no_implicit_any,
            ..CheckerOptions::default()
        },
    )
    .iter()
    .map(|d| d.code)
    .collect()
}

// ===========================================================================
// Foreign-file writes onto a `var X = {}` host: member is NOT declared.
// ===========================================================================

/// noImplicitAny ON: a scalar-RHS write in a foreign file declares nothing —
/// both the write and the read report TS2339 on the `{}` receiver.
#[test]
fn foreign_scalar_write_and_read_report_ts2339_under_no_implicit_any() {
    let codes = cross_file_codes(
        "var shared = {};\n",
        "shared.extra = 2;\nshared.extra;\n",
        true,
    );
    assert_eq!(
        codes,
        vec![2339, 2339],
        "foreign-file write must not declare `extra`; got {codes:?}"
    );
}

/// A function-expression RHS gets no special treatment: the foreign-file
/// write is still TS2339 under noImplicitAny (this was a recorded shape the
/// old unresolved-root path declared).
#[test]
fn foreign_function_rhs_write_reports_ts2339_under_no_implicit_any() {
    let codes = cross_file_codes(
        "var registry = {};\n",
        "registry.handler = function () {};\nregistry.handler;\n",
        true,
    );
    assert_eq!(
        codes,
        vec![2339, 2339],
        "function-RHS foreign write must not declare `handler`; got {codes:?}"
    );
}

/// A class-expression RHS on a foreign `{}` host — the exact motivating shape
/// of the old unresolved-root recording (`Outer.Inner = class {}`) — is
/// TS2339 under noImplicitAny.
#[test]
fn foreign_class_rhs_write_reports_ts2339_under_no_implicit_any() {
    let on = cross_file_codes("var Outer = {};\n", "Outer.Inner = class {};\n", true);
    assert_eq!(
        on,
        vec![2339],
        "class-RHS foreign write must be TS2339 under noImplicitAny, got {on:?}"
    );
}

// ===========================================================================
// Foreign-file writes onto function hosts: TS2339 under BOTH configs.
// ===========================================================================

/// The open-container leniency is an object-shape rule: a callable receiver
/// (`function fhost() {}` from another file) reports TS2339 on the foreign
/// write and read even with noImplicitAny off.
#[test]
fn foreign_write_to_function_host_reports_ts2339_in_both_configs() {
    for nia in [true, false] {
        let codes = cross_file_codes("function fhost() {}\n", "fhost.tag = 1;\nfhost.tag;\n", nia);
        assert_eq!(
            codes,
            vec![2339, 2339],
            "foreign write/read on a function host must be TS2339 (noImplicitAny={nia}), got {codes:?}"
        );
    }
}

// ===========================================================================
// Host-file-declared members stay visible (and typed) from every file.
// ===========================================================================

/// Members written in the host's own file are ordinary declared members for
/// cross-file readers: no TS2339, and the member keeps its inferred type
/// (assigning a string to the `number` member is TS2322).
#[test]
fn host_file_declared_member_reads_cleanly_and_keeps_its_type_cross_file() {
    for nia in [true, false] {
        let codes = cross_file_codes(
            "var App = {};\nApp.version = 1;\n",
            "App.version;\nvar t = App.version;\nt = \"no\";\n",
            nia,
        );
        assert_eq!(
            codes,
            vec![2322],
            "cross-file read of a host-file member must succeed and stay typed (noImplicitAny={nia}), got {codes:?}"
        );
    }
}

/// An undeclared member is still TS2339 under noImplicitAny even though the
/// host legitimately hosts OTHER members — the member set is exactly what the
/// host's own file declared.
#[test]
fn undeclared_member_on_hosting_root_still_reports_ts2339() {
    let codes = cross_file_codes(
        "var App = {};\nApp.version = 1;\n",
        "App.other = 3;\nApp.other;\n",
        true,
    );
    assert_eq!(
        codes,
        vec![2339, 2339],
        "undeclared `other` must be TS2339 even though `version` exists, got {codes:?}"
    );
}

// ===========================================================================
// Same-file expando declaration is unaffected.
// ===========================================================================

/// Same-file control through the single-file harness: writes in the host's
/// own file keep declaring members for `{}`-var and function hosts alike.
/// (The multi-file harness distorts this cell — see the module doc — so the
/// control runs the same path the rest of the single-file expando suites use;
/// the production driver's multi-file behavior is pinned by the conformance
/// salsa family and was verified through the CLI.)
#[test]
fn same_file_expando_declaration_still_works() {
    let codes: Vec<u32> = crate::test_utils::check_js_source_codes_with_options(
        "var box = {};\nbox.item = 2;\nbox.item;\nfunction tool() {}\ntool.mode = \"a\";\ntool.mode;\n",
        "test.js",
        CheckerOptions {
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        codes.is_empty(),
        "same-file expando declarations must stay silent, got {codes:?}"
    );
}
