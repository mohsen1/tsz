//! Flow narrowing of a logical-assignment (`||=` / `??=`) whose LHS is a
//! property/element access.
//!
//! Structural rule: `target ||= rhs` / `target ??= rhs` narrows `target` to
//! non-undefined (and for `||=`, non-falsy) for the rest of the flow, for a
//! property-access target (`o.m`, `this.m`, `o.a.b`, `o["m"]`) exactly as it
//! already does for a plain local variable. `&&=` does NOT narrow `undefined`
//! out, matching `tsc`.
//!
//! Regression witness: a read of the member after the logical assignment used
//! to report a false TS18048/TS2532 because the compound-assignment flow path
//! bailed out for access references before applying the logical-assignment
//! narrowing that the local-variable path already performed.
//!
//! The fixtures use a self-contained `Box` interface (not the global `Map`) so
//! the assertions do not depend on which library globals the test harness
//! loads, and so renamed binders prove the fix is not name-keyed.

use tsz_common::options::checker::CheckerOptions;

fn codes(source: &str) -> Vec<u32> {
    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_source(source, "test.ts", opts)
        .iter()
        .map(|diag| diag.code)
        .collect()
}

fn assert_no_possibly_undefined(codes: &[u32], context: &str) {
    assert!(
        !codes.contains(&18048) && !codes.contains(&2532),
        "{context}: expected the member to be narrowed to non-undefined; got {codes:?}"
    );
}

fn assert_possibly_undefined(codes: &[u32], context: &str) {
    assert!(
        codes.contains(&18048) || codes.contains(&2532),
        "{context}: expected a possibly-undefined diagnostic; got {codes:?}"
    );
}

#[test]
fn bar_bar_equals_narrows_property_access_member() {
    let codes = codes(
        r#"
interface Box { value: number; }
declare function makeBox(): Box;
function f(o: { m?: Box }) {
  o.m ||= makeBox();
  const v: number = o.m.value;
}
"#,
    );
    assert_no_possibly_undefined(&codes, "o.m ||=");
}

#[test]
fn question_question_equals_narrows_property_access_member() {
    let codes = codes(
        r#"
interface Box { value: number; }
declare function makeBox(): Box;
function f(o: { m?: Box }) {
  o.m ??= makeBox();
  const v: number = o.m.value;
}
"#,
    );
    assert_no_possibly_undefined(&codes, "o.m ??=");
}

#[test]
fn question_question_equals_narrows_this_member() {
    let codes = codes(
        r#"
interface Box { value: number; }
declare function makeBox(): Box;
class C { m?: Box; g() { this.m ??= makeBox(); const v: number = this.m.value; } }
"#,
    );
    assert_no_possibly_undefined(&codes, "this.m ??=");
}

#[test]
fn bar_bar_equals_narrows_this_member() {
    let codes = codes(
        r#"
interface Box { value: number; }
declare function makeBox(): Box;
class C { m?: Box; g() { this.m ||= makeBox(); const v: number = this.m.value; } }
"#,
    );
    assert_no_possibly_undefined(&codes, "this.m ||=");
}

#[test]
fn bar_bar_equals_narrows_nested_property_access() {
    // Renamed binders (host/inner/payload) so the fix cannot key on a name.
    let codes = codes(
        r#"
interface Cell { count: number; }
declare function makeCell(): Cell;
function f(host: { inner: { payload?: Cell } }) {
  host.inner.payload ||= makeCell();
  const v: number = host.inner.payload.count;
}
"#,
    );
    assert_no_possibly_undefined(&codes, "host.inner.payload ||=");
}

#[test]
fn bar_bar_equals_narrows_element_access_member() {
    let codes = codes(
        r#"
interface Box { value: number; }
declare function makeBox(): Box;
function f(bag: { [k: string]: Box | undefined }) {
  bag["m"] ||= makeBox();
  const v: number = bag["m"].value;
}
"#,
    );
    assert_no_possibly_undefined(&codes, r#"bag["m"] ||="#);
}

#[test]
fn plain_local_logical_assignment_still_narrows() {
    // Negative control: the local-variable path must remain unchanged.
    let codes = codes(
        r#"
interface Box { value: number; }
declare function makeBox(): Box;
function ok(m?: Box) { m ||= makeBox(); const v: number = m.value; }
"#,
    );
    assert_no_possibly_undefined(&codes, "local m ||=");
}

#[test]
fn member_read_without_logical_assignment_still_errors() {
    // Negative: a bare optional member read still reports possibly-undefined.
    let codes = codes(
        r#"
interface Box { value: number; }
function f(o: { m?: Box }) {
  const v: number = o.m.value;
}
"#,
    );
    assert_possibly_undefined(&codes, "bare o.m read");
}

#[test]
fn amp_amp_equals_does_not_narrow_member_to_non_undefined() {
    // `&&=` only assigns when the LHS is already truthy, so the post-assignment
    // member is still possibly undefined — matches `tsc`.
    let codes = codes(
        r#"
interface Box { value: number; }
declare function makeBox(): Box;
function f(o: { m?: Box }) {
  o.m &&= makeBox();
  const v: number = o.m.value;
}
"#,
    );
    assert_possibly_undefined(&codes, "o.m &&=");
}
