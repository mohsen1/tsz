//! Regression coverage for #14730: TS7023 ("'{0}' implicitly has return type
//! 'any' …") must fire for an object-literal get-accessor only when its body
//! reads its *own* property circularly — i.e. through `this`, the variable the
//! literal initializes, or a transparent wrapper of those. A `.<name>` access
//! on an *unrelated* receiver (`ctx.path`, `mgr.clients`) reads a different
//! member's symbol and is not circular; `tsc` is clean there. The previous
//! detection keyed on the property-access *name* alone, so any same-named
//! access mis-fired.
//!
//! All expectations below were confirmed against `tsc --strict`.

use tsz_checker::test_utils::check_source_strict_codes;

fn count(source: &str, code: u32) -> usize {
    check_source_strict_codes(source)
        .into_iter()
        .filter(|c| *c == code)
        .count()
}

// ---------------------------------------------------------------------------
// False positives that the fix must clear (unrelated receiver).
// ---------------------------------------------------------------------------

#[test]
fn unrelated_receiver_property_access_is_clean_zod_shape() {
    // `ctx.path` reads `ParseContext.path`, not the getter `path`.
    let source = r#"
type ParsePath = { readonly component: string | number };
declare function pathToArray(path: ParsePath): (string | number)[];
declare const ctx: { path: ParsePath };
type RefinementCtx = { path: (string | number)[] };
const checkCtx: RefinementCtx = {
  get path() {
    return pathToArray(ctx.path);
  },
};
void checkCtx;
"#;
    assert_eq!(
        count(source, 7023),
        0,
        "a getter reading an unrelated receiver's same-named property is not circular"
    );
}

#[test]
fn unrelated_receiver_with_trailing_methods_has_no_accessor_cascade_msw_shape() {
    // The msw witness. The getter must not emit TS7023, and — once it stops
    // failing — must not add any cascading TS7006 on the trailing method params
    // beyond what the identical literal *without* the accessor already produces.
    //
    // tsz has a separate, pre-existing contextual-typing gap that emits one
    // TS7006 on `broadcast(data)` in return position even with no accessor
    // present (tsc is clean there); that gap is a distinct bug, out of scope
    // here. What this test pins is that the accessor itself contributes neither
    // a TS7023 nor any extra TS7006 — i.e. the cascade attributed to the
    // accessor failure is fully cleared.
    let with_getter = r#"
type WebSocketLink = {
  readonly clients: ReadonlySet<number>;
  broadcast(data: string): void;
};
declare const mgr: { clients: ReadonlySet<number> };
function make(): WebSocketLink {
  return {
    get clients() { return mgr.clients; },
    broadcast(data) { console.log(data); },
  };
}
void make;
"#;
    let without_getter = r#"
type WebSocketLink = {
  readonly clients: ReadonlySet<number>;
  broadcast(data: string): void;
};
declare const mgr: { clients: ReadonlySet<number> };
function make(): WebSocketLink {
  return {
    clients: mgr.clients,
    broadcast(data) { console.log(data); },
  };
}
void make;
"#;
    assert_eq!(
        count(with_getter, 7023),
        0,
        "unrelated `mgr.clients` is not circular"
    );
    assert_eq!(
        count(with_getter, 7006),
        count(without_getter, 7006),
        "the accessor must not add any cascading TS7006 over the no-accessor baseline"
    );
}

#[test]
fn unrelated_receiver_renamed_binders_is_clean() {
    // Anti-hardcoding: rename the getter and the accessed property to `q`.
    let source = r#"
declare const src: { q: number };
const obj = {
  get q() { return src.q; },
};
void obj;
"#;
    assert_eq!(
        count(source, 7023),
        0,
        "detection must be structural, not name-keyed"
    );
}

#[test]
fn shadowed_local_receiver_is_clean() {
    // The receiver `ctx` is a getter-local binding unrelated to the object.
    let source = r#"
const checkCtx = {
  get path() {
    let ctx = { path: 1 };
    return ctx.path;
  },
};
void checkCtx;
"#;
    assert_eq!(
        count(source, 7023),
        0,
        "a local receiver is not the object under construction"
    );
}

#[test]
fn bare_identifier_return_is_clean() {
    // Reading a captured outer variable (no property access) is never a
    // self-reference.
    let source = r#"
declare const path: number;
const obj = {
  get path() { return path; },
};
void obj;
"#;
    assert_eq!(
        count(source, 7023),
        0,
        "a bare identifier read is not a self-reference"
    );
}

// ---------------------------------------------------------------------------
// Genuine circularities that must still report (receiver IS the object).
// ---------------------------------------------------------------------------

#[test]
fn this_receiver_self_reference_still_reports() {
    let source = r#"
const o = {
  get x() { return this.x; },
};
void o;
"#;
    assert_eq!(
        count(source, 7023),
        1,
        "`this.x` inside `get x` is genuinely circular"
    );
}

#[test]
fn this_receiver_in_larger_expression_still_reports() {
    let source = r#"
const o = {
  get x() { return this.x + 1; },
};
void o;
"#;
    assert_eq!(count(source, 7023), 1, "`this.x + 1` is still circular");
}

#[test]
fn missing_this_member_in_getter_return_still_reports() {
    let source = r#"
const o = {
  get x() { return this.missing; },
};
void o;
"#;
    assert_eq!(
        count(source, 7023),
        1,
        "a getter return that reads a missing member through `this` is circular"
    );
}

#[test]
fn existing_this_member_in_getter_return_is_clean() {
    let source = r#"
const o = {
  get x() { return this.y; },
  y: 1,
};
void o;
"#;
    assert_eq!(
        count(source, 7023),
        0,
        "a getter may read an existing sibling member through `this` without circularity"
    );
}

#[test]
fn this_alias_missing_member_self_reference_still_reports() {
    let source = r#"
const o = {
  get x() {
    const self = this;
    return self.missing.deep.x;
  },
};
void o;
"#;
    assert_eq!(
        count(source, 7023),
        1,
        "aliases of `this` keep missing-member getter reads circular"
    );
}

#[test]
fn wrapped_this_receiver_self_reference_still_reports() {
    let source = r#"
const o = {
  get x() { return [this][0].x; },
};
void o;
"#;
    assert_eq!(
        count(source, 7023),
        1,
        "`[this][0].x` flows `this` to the receiver"
    );
}

#[test]
fn initializer_binding_receiver_self_reference_still_reports() {
    // `o.x` inside `o`'s own getter is circular exactly as `this.x` is.
    let source = r#"
const o = {
  get x() { return o.x; },
};
void o;
"#;
    assert_eq!(
        count(source, 7023),
        1,
        "reading the literal's own binding is circular"
    );
}

#[test]
fn conditional_this_branch_self_reference_still_reports() {
    let source = r#"
declare function cond(): boolean;
declare const ctx: { x: number };
const o = {
  get x() { return (cond() ? this : ctx).x; },
};
void o;
"#;
    assert_eq!(
        count(source, 7023),
        1,
        "a `this` conditional branch keeps the receiver circular"
    );
}

#[test]
fn renamed_binders_this_self_reference_still_reports() {
    // Anti-hardcoding negative twin: genuine circularity under a renamed getter.
    let source = r#"
const widget = {
  get extent() { return this.extent; },
};
void widget;
"#;
    assert_eq!(
        count(source, 7023),
        1,
        "genuine self-reference reports regardless of name"
    );
}
