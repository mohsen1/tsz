use crate::test_utils::check_source_diagnostics;

fn diagnostic_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|diag| diag.code)
        .collect()
}

#[test]
fn method_this_parameter_accepts_same_class_instance_receiver() {
    let codes = diagnostic_codes(
        r#"
class Handler {
  value = 10;

  handle(this: Handler, x: number): number {
    return this.value + x;
  }
}

const h = new Handler();
h.handle(5);
"#,
    );

    assert!(
        codes.is_empty(),
        "expected no diagnostics for same class instance receiver; got {codes:?}"
    );
}

#[test]
fn method_this_parameter_accepts_renamed_same_class_instance_receiver() {
    let codes = diagnostic_codes(
        r#"
class Runner {
  count = 1;

  run(this: Runner, step: number): number {
    return this.count + step;
  }
}

const r = new Runner();
r.run(2);
"#,
    );

    assert!(
        codes.is_empty(),
        "expected no diagnostics for renamed same class receiver; got {codes:?}"
    );
}

#[test]
fn method_this_parameter_rejects_unbound_call_without_receiver() {
    let codes = diagnostic_codes(
        r#"
class Owner {
  value = 1;

  handle(this: Owner, x: number): number {
    return this.value + x;
  }
}

const borrowed = new Owner().handle;
borrowed(1);
"#,
    );

    assert!(
        codes.contains(&2684),
        "expected TS2684 for unbound call without receiver; got {codes:?}"
    );
}

#[test]
fn explicit_this_annotation_different_class_default_param_no_error() {
    // When a method has `this: OtherClass`, default parameter values that call
    // `this.method()` should use OtherClass as the this type, not the enclosing class.
    let codes = diagnostic_codes(
        r#"
class Example {
    getNumber(): number { return 1; }
}
class Weird {
    doSomething(this: Example, a = this.getNumber()) {
        return a;
    }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "expected no diagnostics for explicit this annotation in default param; got {codes:?}"
    );
}

#[test]
fn explicit_this_annotation_free_function_default_param_no_error() {
    // Same rule applies to free functions with explicit `this:` annotations.
    let codes = diagnostic_codes(
        r#"
class Example {
    getNumber(): number { return 1; }
}
function weird(this: Example, a = this.getNumber()) {
    return a;
}
"#,
    );
    assert!(
        codes.is_empty(),
        "expected no diagnostics for explicit this annotation in free function default param; got {codes:?}"
    );
}

#[test]
fn explicit_this_annotation_different_name_no_error() {
    // Renamed type parameter variable shouldn't matter — the rule is structural.
    let codes = diagnostic_codes(
        r#"
class Provider {
    getValue(): string { return ""; }
}
class Consumer {
    process(this: Provider, x = this.getValue()) {
        return x;
    }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "expected no diagnostics when this annotation refers to a differently-named class; got {codes:?}"
    );
}

// --- Issue #14843 -----------------------------------------------------------
// An object-literal method with an explicit `this:` parameter and NO return
// annotation must resolve `this` against that declared parameter type during
// speculative return-type inference (tsc's `getThisTypeOfSignature`), not
// against the in-construction object-literal type. Otherwise an absent member
// access re-enters the method being inferred (spurious TS7023) and the TS2339
// "property does not exist" prints the object literal instead of the `this:`
// type.

use crate::test_utils::check_source_strict_messages;

/// Assert that `source` produces exactly one TS2339 whose displayed receiver is
/// `expected_receiver` and no TS7023 — the fixed shape for an explicit-`this:`
/// member accessing a member absent on the declared receiver.
fn assert_single_ts2339_on_receiver(source: &str, expected_receiver: &str) {
    let diags = check_source_strict_messages(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 7023),
        "explicit `this:` must not produce a spurious TS7023; got {diags:?}"
    );
    let ts2339: Vec<&String> = diags
        .iter()
        .filter(|(code, _)| *code == 2339)
        .map(|(_, msg)| msg)
        .collect();
    assert_eq!(
        ts2339.len(),
        1,
        "expected exactly one TS2339 for the absent member; got {diags:?}"
    );
    assert!(
        ts2339[0].contains(expected_receiver),
        "TS2339 receiver must be the explicit `this:` type {expected_receiver}; got {:?}",
        ts2339[0]
    );
}

#[test]
fn object_literal_method_explicit_this_no_spurious_circular_return() {
    // Witness A: `x` exists on the object literal but not on `Ctx`.
    // tsc 5.9.3: a single TS2339 against `Ctx`; tsz previously added TS7023 and
    // printed the object-literal type as the receiver.
    assert_single_ts2339_on_receiver(
        r#"
interface Ctx { kind: string; }
const obj = {
  x: 1,
  bad(this: Ctx) { return this.x; }
};
"#,
        "'Ctx'",
    );
}

#[test]
fn object_literal_method_explicit_this_void_no_spurious_circular_return() {
    // Witness B: `this: void`, member on neither side.
    assert_single_ts2339_on_receiver(
        r#"
const obj2 = {
  x: 1,
  bad(this: void) { return this.x; }
};
"#,
        "'void'",
    );
}

#[test]
fn object_literal_method_explicit_this_member_present_is_clean() {
    // Control from the triage note: when the member is present on the `this:`
    // type both tools are clean — must remain clean.
    let diags = check_source_strict_messages(
        r#"
interface Ctx { x: number; }
const obj = { x: 1, bad(this: Ctx) { return this.x; } };
"#,
    );
    assert!(
        diags.is_empty(),
        "member present on the `this:` type must be clean; got {diags:?}"
    );
}

#[test]
fn object_literal_method_explicit_inline_this_resolves_members() {
    // The explicit `this:` may be an inline object type whose member differs
    // from the object literal; it must resolve against the inline type.
    let diags = check_source_strict_messages(
        r#"
const obj = { x: 1, bad(this: { y: string }) { return this.y; } };
"#,
    );
    assert!(
        diags.is_empty(),
        "inline explicit `this:` member access must resolve cleanly; got {diags:?}"
    );
}

#[test]
fn object_literal_getter_genuine_self_reference_still_reports_circular() {
    // Negative case: a getter with NO explicit `this:` that genuinely references
    // its own inferred return type must still report TS7023.
    let diags = check_source_strict_messages(
        r#"
const obj = {
  get x() { return this.x + 1; }
};
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 7023),
        "genuine getter self-reference must still report TS7023; got {diags:?}"
    );
}

#[test]
fn class_method_explicit_this_void_absent_member_reports_on_void() {
    // Adjacent: class method with explicit `this:` was already correct and must
    // stay correct (single TS2339 on `void`, no TS7023).
    assert_single_ts2339_on_receiver(
        r#"
class C {
  x = 1;
  bad(this: void) { return this.x; }
}
"#,
        "'void'",
    );
}

#[test]
fn standalone_function_explicit_this_void_absent_member_reports_on_void() {
    assert_single_ts2339_on_receiver("function bad(this: void) { return this.x; }\n", "'void'");
}

#[test]
fn object_literal_function_property_explicit_this_no_spurious_circular_return() {
    // The same rule applies to a `function`-expression property (not just method
    // shorthand): an explicit `this:` parameter governs `this`, so an absent
    // member reports a single TS2339 against the declared type with no TS7023.
    assert_single_ts2339_on_receiver(
        r#"
interface Ctx { kind: string; }
const obj = {
  x: 1,
  bad: function (this: Ctx) { return this.x; }
};
"#,
        "'Ctx'",
    );
}

#[test]
fn object_literal_function_property_explicit_this_member_present_is_clean() {
    let diags = check_source_strict_messages(
        r#"
interface Ctx { x: number; }
const obj = { x: 1, bad: function (this: Ctx) { return this.x; } };
"#,
    );
    assert!(
        diags.is_empty(),
        "function-expression property with member present on `this:` must be clean; got {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Re-entrant object-literal `this` container (M6 mobx canary FP).
//
// When one object literal's method (with an *inferred* return type) references
// another literal's method, checking the first literal forces the second
// literal's member types while the first literal's synthetic `this` is still on
// the this-stack. The enclosing literal's method must still resolve `this` to
// its *own* literal, not to the referring literal on the stack. Renamed from
// the mobx witness (traps / arrayExtensions).
// ---------------------------------------------------------------------------

fn ts2339_codes(source: &str) -> Vec<u32> {
    check_source_strict_messages(source)
        .into_iter()
        .filter(|(code, _)| *code == 2339)
        .map(|(code, _)| code)
        .collect()
}

#[test]
fn reentrant_object_literal_method_this_uses_own_literal() {
    // `gateTraps.fetchMember` (inferred return) references `toolkit.grabFirst`,
    // forcing `toolkit`'s member types while gateTraps' `this` is on the stack.
    // `this.chopRange` inside toolkit's methods must resolve to `toolkit`.
    let codes = ts2339_codes(
        r#"
const gateTraps = {
    fetchMember(box: any, key: any) {
        if (key === "size") { return 0; }
        return toolkit.grabFirst;
    },
    storeMember(box: any, key: any, value: any): boolean { return true; },
    sealGuard() {}
};
var toolkit = {
    wipeAll(): any[] { return this.chopRange(0); },
    chopRange(begin: number, howMany?: number): any[] { return [begin, howMany]; },
    grabFirst() { return this.chopRange(0, 1)[0]; }
};
"#,
    );
    assert!(
        codes.is_empty(),
        "re-entrant forcing must not leak the referring literal's `this`; got TS2339 {codes:?}"
    );
}

#[test]
fn reentrant_object_literal_method_this_renamed_binders() {
    // Same structure, entirely different identifiers — proves the fix is not
    // keyed on any particular binder name.
    let codes = ts2339_codes(
        r#"
const proxyHooks = {
    peek(target: any, prop: any) {
        if (prop === "len") { return 0; }
        return helpers.head;
    },
    poke(target: any, prop: any, val: any): boolean { return true; },
    freeze() {}
};
var helpers = {
    clearAll(): any[] { return this.slice(0); },
    slice(from: number, count?: number): any[] { return [from, count]; },
    head() { return this.slice(0, 1)[0]; }
};
"#,
    );
    assert!(
        codes.is_empty(),
        "renamed re-entrant forcing must stay clean; got TS2339 {codes:?}"
    );
}

#[test]
fn reentrant_object_literal_method_this_reversed_declaration_order() {
    // The forced literal is declared *before* the referring one (B-before-A).
    let codes = ts2339_codes(
        r#"
var toolkit = {
    wipeAll(): any[] { return this.chopRange(0); },
    chopRange(begin: number, howMany?: number): any[] { return [begin, howMany]; },
    grabFirst() { return this.chopRange(0, 1)[0]; }
};
const gateTraps = {
    fetchMember(box: any, key: any) {
        if (key === "size") { return 0; }
        return toolkit.grabFirst;
    },
    sealGuard() {}
};
"#,
    );
    assert!(
        codes.is_empty(),
        "reversed-order re-entrant forcing must stay clean; got TS2339 {codes:?}"
    );
}

#[test]
fn reentrant_object_literal_function_expression_property_this_uses_own_literal() {
    // The forced members are `function` expression properties (own `this`),
    // not shorthand methods — the twin push path.
    let codes = ts2339_codes(
        r#"
const gateTraps = {
    fetchMember: function (box: any, key: any) {
        if (key === "size") { return 0; }
        return toolkit.grabFirst;
    },
    sealGuard: function () {}
};
var toolkit = {
    chopRange: function (begin: number, howMany?: number): any[] { return [begin, howMany]; },
    grabFirst: function () { return this.chopRange(0, 1)[0]; }
};
"#,
    );
    assert!(
        codes.is_empty(),
        "function-expression property re-entrant forcing must stay clean; got TS2339 {codes:?}"
    );
}

#[test]
fn reentrant_object_literal_getter_this_uses_own_literal() {
    // Getter form (accessor path was already immune) — regression guard.
    let codes = ts2339_codes(
        r#"
const gateTraps = {
    fetchMember(box: any, key: any) {
        if (key === "size") { return 0; }
        return toolkit.head;
    },
    sealGuard() {}
};
var toolkit = {
    chopRange(begin: number, howMany?: number): any[] { return [begin, howMany]; },
    get head() { return this.chopRange(0, 1)[0]; }
};
"#,
    );
    assert!(
        codes.is_empty(),
        "getter re-entrant forcing must stay clean; got TS2339 {codes:?}"
    );
}

#[test]
fn object_literal_method_missing_property_this_still_reports_ts2339() {
    // Negative guard: a genuine missing-member access on the literal's own
    // `this` must still report TS2339 (the fix must not suppress real errors).
    let codes = ts2339_codes(
        r#"
var toolkit = {
    chopRange(begin: number): any[] { return [begin]; },
    grabFirst() { return this.doesNotExist(0, 1); }
};
"#,
    );
    assert_eq!(
        codes,
        vec![2339],
        "a real missing member on the literal's own `this` must still be TS2339"
    );
}
