//! Behavioral fences for the provisional (mid-build) class-instance window
//! (#16055).
//!
//! The zod witness — a false `TS2349` on a method call through
//! `ZodType<any, any, any> | ZodTypeAny` — needs the real project's shape to
//! reproduce, so these fences pin the surrounding behavior instead: a method
//! call through the union of a class application and an alias of the same
//! application stays clean (the shape whose two representations must reduce
//! to one identity), while a genuinely missing member and a genuinely wrong
//! member type still report — the fix keeps in-window applications opaque,
//! and these negatives prove the opacity does not swallow real diagnostics
//! once the class publishes. Binder names are varied across cases so nothing
//! pins on a spelling. Every expectation is pinned against `tsc` 7.0.2.

use std::sync::{Arc, OnceLock};
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_source_with_libs, diagnostic_code_message_refs, load_lib_files,
};

fn es5_libs() -> &'static Vec<Arc<LibFile>> {
    static LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    LIBS.get_or_init(|| load_lib_files(&["es5.d.ts"]))
}

fn codes(source: &str) -> Vec<u32> {
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), es5_libs())
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn assert_clean(source: &str, context: &str) {
    let diagnostics =
        check_source_with_libs(source, "test.ts", CheckerOptions::default(), es5_libs());
    assert!(
        diagnostics.is_empty(),
        "{context}: expected no diagnostics, got {:#?}",
        diagnostic_code_message_refs(&diagnostics),
    );
}

#[test]
fn method_call_through_union_of_application_and_its_alias_is_clean() {
    assert_clean(
        r#"
class Chassis<A, B, C> {
  wire(): ChassisAny { return this as unknown as ChassisAny; }
  probe(x: A, y: B): C { return null as unknown as C; }
}
type ChassisAny = Chassis<any, any, any>;
type Sockets = [ChassisAny, ...ChassisAny[]];
function drive<T extends Sockets>(sockets: T) {
  sockets.map((socket) => socket.probe(1, 2));
}
"#,
        "tsc 7.0.2 is clean: both tuple-slot representations are one class identity",
    );
}

#[test]
fn method_call_through_union_of_application_and_its_alias_is_clean_renamed_binders() {
    assert_clean(
        r#"
class Relay<In, Out, Err> {
  fuse(): AnyRelay { return this as unknown as AnyRelay; }
  send(a: In, b: Out): Err { return null as unknown as Err; }
}
type AnyRelay = Relay<any, any, any>;
type Wires = [AnyRelay, ...AnyRelay[]];
function pulse<W extends Wires>(wires: W) {
  wires.map((wire) => wire.send(1, 2));
}
"#,
        "renamed binders: same structural shape must stay clean",
    );
}

#[test]
fn missing_member_through_the_same_union_shape_still_reports_ts2339() {
    assert_eq!(
        codes(
            r#"
class Chassis<A, B, C> {
  wire(): ChassisAny { return this as unknown as ChassisAny; }
  probe(x: A, y: B): C { return null as unknown as C; }
}
type ChassisAny = Chassis<any, any, any>;
type Sockets = [ChassisAny, ...ChassisAny[]];
function drive<T extends Sockets>(sockets: T) {
  sockets.map((socket) => socket.detach(1));
}
"#
        ),
        vec![2339],
        "tsc 7.0.2 reports TS2339: in-window opacity must not swallow a real miss",
    );
}

#[test]
fn published_generic_class_members_materialize_with_argument_substitution() {
    assert_eq!(
        codes(
            r#"
class Cargo<T> { load!: T; }
const box = new Cargo<number>();
const wrong: string = box.load;
"#
        ),
        vec![2322],
        "tsc 7.0.2 reports TS2322: after the build window closes, members \
         materialize against the instantiation arguments as before",
    );
}
