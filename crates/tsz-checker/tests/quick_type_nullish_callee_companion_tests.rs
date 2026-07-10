//! Tests for the possibly-nullish callee companion diagnostic on
//! declaration-inferred call initializers.
//!
//! Structural rule: when a variable-like declaration (variable declaration,
//! property declaration, parameter, or binding-element default) infers its
//! type from a call-expression initializer, tsc computes that type through
//! `getQuickTypeOfExpression`, which re-checks the callee with
//! `checkNonNullExpression`. A possibly-nullish callee therefore reports the
//! `reportObjectPossiblyNullOrUndefinedError` family (TS18047/TS18048/TS18049
//! for entity names, TS2531/TS2532/TS2533 otherwise) in addition to
//! TS2721/TS2722/TS2723 from call resolution. Optional-chain calls, annotated
//! declarations, assignments, arguments, returns, and bare statements do not
//! run the callee re-check and only report the invoke-family diagnostic.
//!
//! Fixes issue #15677.

use crate::test_utils::{check_source_strict_codes, check_source_strict_messages};

/// Assert the declaration-inferred quick-type path fired: TS2722 plus the
/// TS18048 companion.
fn assert_companion_fires(source: &str, context: &str) {
    let codes = check_source_strict_codes(source);
    assert!(
        codes.contains(&2722) && codes.contains(&18048),
        "{context}: expected TS2722 + TS18048; got: {codes:?}"
    );
}

/// Assert the call reported only the invoke-family TS2722 with no
/// quick-type companion.
fn assert_invoke_only(source: &str, context: &str) {
    let codes = check_source_strict_codes(source);
    assert!(
        codes.contains(&2722),
        "{context}: expected TS2722; got: {codes:?}"
    );
    assert!(
        !codes.contains(&18048),
        "{context}: unexpected TS18048 companion; got: {codes:?}"
    );
}

// =========================================================================
// Companion fires: variable declarations
// =========================================================================

#[test]
fn const_from_optional_method_call_emits_invoke_and_companion() {
    let messages = check_source_strict_messages(
        "declare const obj: { run?: () => void };\nconst out = obj.run();\n",
    );
    assert!(
        messages.iter().any(|(code, _)| *code == 2722),
        "expected TS2722; got: {messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|(code, msg)| *code == 18048 && msg == "'obj.run' is possibly 'undefined'."),
        "expected TS18048 companion naming the callee; got: {messages:?}"
    );
}

#[test]
fn issue_witness_key_remapped_homomorphic_mapped_type_getter() {
    // Direct witness from issue #15677: optional member produced by an
    // `as`-remapped homomorphic mapped type keeps its optionality, and the
    // declaration-inferred call reports both TS2722 and TS18048.
    let messages = check_source_strict_messages(
        "type Src = { a?: number; b: string };\n\
         type Getters<T> = { [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K] };\n\
         declare const g: Getters<Src>;\n\
         const x = g.getA();\n",
    );
    assert!(
        messages.iter().any(|(code, _)| *code == 2722),
        "expected TS2722; got: {messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|(code, msg)| *code == 18048 && msg == "'g.getA' is possibly 'undefined'."),
        "expected TS18048 companion; got: {messages:?}"
    );
}

#[test]
fn identity_key_remap_without_intrinsic_keeps_optionality_pairing() {
    let messages = check_source_strict_messages(
        "type Model = { load?: () => number };\n\
         type Same<T> = { [P in keyof T as P]: T[P] };\n\
         declare const store: Same<Model>;\n\
         const value = store.load();\n",
    );
    assert!(
        messages.iter().any(|(code, _)| *code == 2722),
        "expected TS2722; got: {messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|(code, msg)| *code == 18048 && msg == "'store.load' is possibly 'undefined'."),
        "expected TS18048 companion; got: {messages:?}"
    );
}

#[test]
fn null_callee_emits_ts2721_and_ts18047() {
    let messages = check_source_strict_messages(
        "declare const box: { open: (() => void) | null };\nconst r = box.open();\n",
    );
    assert!(
        messages.iter().any(|(code, _)| *code == 2721),
        "expected TS2721; got: {messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|(code, msg)| *code == 18047 && msg == "'box.open' is possibly 'null'."),
        "expected TS18047 companion; got: {messages:?}"
    );
}

#[test]
fn null_or_undefined_callee_emits_ts2723_and_ts18049() {
    let messages = check_source_strict_messages(
        "declare const cfg: { init: (() => void) | null | undefined };\nconst r = cfg.init();\n",
    );
    assert!(
        messages.iter().any(|(code, _)| *code == 2723),
        "expected TS2723; got: {messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|(code, msg)| *code == 18049
                && msg == "'cfg.init' is possibly 'null' or 'undefined'."),
        "expected TS18049 companion; got: {messages:?}"
    );
}

#[test]
fn bare_identifier_callee_names_identifier_in_companion() {
    let messages = check_source_strict_messages(
        "declare const handler: (() => void) | undefined;\nconst r = handler();\n",
    );
    assert!(
        messages
            .iter()
            .any(|(code, msg)| *code == 18048 && msg == "'handler' is possibly 'undefined'."),
        "expected TS18048 naming the identifier; got: {messages:?}"
    );
}

#[test]
fn var_declaration_gets_companion() {
    assert_companion_fires(
        "declare const o: { f?: () => number };\nvar w = o.f();\n",
        "var declarations infer through the quick-type path",
    );
}

#[test]
fn overloaded_optional_callee_still_gets_companion() {
    // The callee re-check runs before signature counting, so overloads
    // (multiple non-generic signatures) still pair the diagnostics.
    assert_companion_fires(
        "declare const o: { f?: { (): number; (s: string): string } };\nconst r = o.f();\n",
        "overloaded callee",
    );
}

#[test]
fn generic_optional_callee_still_gets_companion() {
    assert_companion_fires(
        "declare const o: { pick?: <T>(x?: T) => T };\nconst r = o.pick();\n",
        "generic callee",
    );
}

// =========================================================================
// Companion fires: other variable-like declarations
// =========================================================================

#[test]
fn class_property_initializer_gets_companion() {
    assert_companion_fires(
        "declare const src: { get?: () => number };\nclass Holder { slot = src.get(); }\n",
        "property declarations infer through the quick-type path",
    );
}

#[test]
fn static_class_property_initializer_gets_companion() {
    assert_companion_fires(
        "declare const src: { get?: () => number };\nclass Holder { static slot = src.get(); }\n",
        "static property declarations infer through the quick-type path",
    );
}

#[test]
fn parameter_default_gets_companion() {
    assert_companion_fires(
        "declare const env: { port?: () => number };\nfunction serve(p = env.port()) {}\n",
        "parameter defaults infer through the quick-type path",
    );
}

#[test]
fn binding_element_default_gets_companion_even_when_pattern_is_annotated() {
    assert_companion_fires(
        "declare const o: { f?: () => number };\nconst { n = o.f() }: { n?: number } = {};\n",
        "binding-element defaults always take the quick-type path",
    );
}

#[test]
fn destructuring_from_call_initializer_gets_companion() {
    assert_companion_fires(
        "declare const o: { f?: () => { p: number } };\nconst { p } = o.f();\n",
        "binding-pattern declarations infer through the quick-type path",
    );
}

#[test]
fn for_loop_initializer_declaration_gets_companion() {
    assert_companion_fires(
        "declare const o: { g?: () => number };\nfor (const n = o.g(); ;) { break; }\n",
        "for-init declarations infer through the quick-type path",
    );
}

// =========================================================================
// Companion fires: wrappers and non-entity-name callees
// =========================================================================

#[test]
fn parenthesized_initializer_still_gets_companion() {
    assert_companion_fires(
        "declare const o: { f?: () => number };\nconst r = ((o.f()));\n",
        "parentheses are skipped by the quick-type path",
    );
}

#[test]
fn parenthesized_callee_gets_object_variant_companion() {
    let messages = check_source_strict_messages(
        "declare const o: { f?: () => number };\nconst r = (o.f)();\n",
    );
    assert!(
        messages.iter().any(|(code, _)| *code == 2722),
        "expected TS2722; got: {messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|(code, msg)| *code == 2532 && msg == "Object is possibly 'undefined'."),
        "parenthesized callees are not entity names, so tsc reports TS2532; got: {messages:?}"
    );
}

#[test]
fn element_access_callee_gets_object_variant_companion() {
    let messages = check_source_strict_messages(
        "declare const o: { [k: string]: (() => void) | undefined };\nconst r = o[\"f\"]();\n",
    );
    assert!(
        messages
            .iter()
            .any(|(code, msg)| *code == 2532 && msg == "Object is possibly 'undefined'."),
        "element-access callees are not entity names, so tsc reports TS2532; got: {messages:?}"
    );
}

#[test]
fn this_property_callee_gets_object_variant_companion() {
    let messages = check_source_strict_messages(
        "class W {\n  job?: () => number;\n  go() { const r = this.job(); }\n}\n",
    );
    assert!(
        messages
            .iter()
            .any(|(code, msg)| *code == 2532 && msg == "Object is possibly 'undefined'."),
        "`this.x` is not an entity name, so tsc reports TS2532; got: {messages:?}"
    );
}

#[test]
fn entity_name_of_100_utf16_units_falls_back_to_object_variant() {
    // `receiver.` (9 units) + 91-unit member = 100 units: tsc switches to the
    // anonymous `Object is possibly 'undefined'.` form at 100.
    let member = "m".repeat(91);
    let source = format!(
        "declare const receiver: {{ {member}?: () => number }};\nconst r = receiver.{member}();\n"
    );
    let messages = check_source_strict_messages(&source);
    assert!(
        messages
            .iter()
            .any(|(code, msg)| *code == 2532 && msg == "Object is possibly 'undefined'."),
        "names at 100 UTF-16 units use the Object form; got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|(code, _)| *code == 18048),
        "no named TS18048 at 100 UTF-16 units; got: {messages:?}"
    );
}

#[test]
fn entity_name_of_99_utf16_units_keeps_named_variant() {
    let member = "m".repeat(90);
    let source = format!(
        "declare const receiver: {{ {member}?: () => number }};\nconst r = receiver.{member}();\n"
    );
    let codes = check_source_strict_codes(&source);
    assert!(
        codes.contains(&18048) && !codes.contains(&2532),
        "names below 100 UTF-16 units keep the named TS18048 form; got: {codes:?}"
    );
}

// =========================================================================
// Companion does NOT fire: non-inferring contexts
// =========================================================================

#[test]
fn bare_statement_call_reports_invoke_only() {
    assert_invoke_only(
        "declare const o: { f?: () => void };\no.f();\n",
        "statement calls do not take the quick-type path",
    );
}

#[test]
fn annotated_variable_reports_invoke_only() {
    assert_invoke_only(
        "declare const o: { f?: () => void };\nconst r: void = o.f();\n",
        "annotated declarations do not take the quick-type path",
    );
}

#[test]
fn annotated_class_property_reports_invoke_only() {
    assert_invoke_only(
        "declare const o: { f?: () => number };\nclass H { p: number | undefined = o.f(); }\n",
        "annotated properties do not take the quick-type path",
    );
}

#[test]
fn annotated_parameter_default_reports_invoke_only() {
    assert_invoke_only(
        "declare const o: { f?: () => number };\nfunction u(p: number | undefined = o.f()) {}\n",
        "annotated parameters do not take the quick-type path",
    );
}

#[test]
fn assignment_reports_invoke_only() {
    assert_invoke_only(
        "declare const o: { f?: () => number };\nlet t;\nt = o.f();\n",
        "assignments do not take the quick-type path",
    );
}

#[test]
fn argument_position_reports_invoke_only() {
    assert_invoke_only(
        "declare const o: { f?: () => number };\ndeclare function sink(v: unknown): void;\nsink(o.f());\n",
        "call arguments do not take the quick-type path",
    );
}

#[test]
fn return_position_reports_invoke_only() {
    assert_invoke_only(
        "declare const o: { f?: () => number };\nfunction relay() { return o.f(); }\n",
        "return expressions do not take the quick-type path",
    );
}

#[test]
fn as_assertion_initializer_reports_invoke_only() {
    assert_invoke_only(
        "declare const o: { f?: () => number };\nconst r = o.f() as unknown;\n",
        "assertion initializers short-circuit the quick-type path",
    );
}

#[test]
fn satisfies_initializer_reports_invoke_only() {
    assert_invoke_only(
        "declare const o: { f?: () => number };\nconst r = o.f() satisfies unknown;\n",
        "satisfies initializers do not take the quick-type path",
    );
}

#[test]
fn enum_member_initializer_reports_invoke_only() {
    assert_invoke_only(
        "declare const o: { f?: () => number };\nenum E { A = o.f() }\n",
        "enum members are not variable-like declarations",
    );
}

// =========================================================================
// Companion does NOT fire: optional chains and non-null assertions
// =========================================================================

#[test]
fn optional_call_chain_reports_nothing() {
    let codes =
        check_source_strict_codes("declare const o: { f?: () => number };\nconst r = o.f?.();\n");
    assert!(
        !codes.contains(&2722) && !codes.contains(&18048),
        "`?.()` guards the callee; got: {codes:?}"
    );
}

#[test]
fn non_null_asserted_callee_reports_nothing() {
    let codes =
        check_source_strict_codes("declare const o: { f?: () => number };\nconst r = o.f!();\n");
    assert!(
        !codes.contains(&2722) && !codes.contains(&18048),
        "`!` removes the nullish callee slice; got: {codes:?}"
    );
}

// =========================================================================
// Negative optionality case: required members stay required
// =========================================================================

#[test]
fn required_member_call_reports_nothing() {
    let codes =
        check_source_strict_codes("declare const o: { f: () => number };\nconst r = o.f();\n");
    assert!(
        !codes.contains(&2722) && !codes.contains(&18048),
        "required members must not gain optionality; got: {codes:?}"
    );
}

#[test]
fn identity_key_remap_keeps_required_member_required() {
    let codes = check_source_strict_codes(
        "type Model = { save: () => number };\n\
         type Same<T> = { [P in keyof T as P]: T[P] };\n\
         declare const store: Same<Model>;\n\
         const value = store.save();\n",
    );
    assert!(
        !codes.contains(&2722) && !codes.contains(&18048),
        "remapped required members must not gain optionality; got: {codes:?}"
    );
}

// =========================================================================
// Property-access reporter parity: 100-unit entity-name fallback
// =========================================================================

#[test]
fn property_access_on_long_named_receiver_uses_object_variant() {
    // The same reporter serves possibly-nullish property accesses; receivers
    // whose rendered name reaches 100 UTF-16 units use the Object form.
    let receiver = "r".repeat(100);
    let source = format!(
        "declare const {receiver}: {{ p: number }} | undefined;\nconst v = {receiver}.p;\n"
    );
    let messages = check_source_strict_messages(&source);
    assert!(
        messages
            .iter()
            .any(|(code, msg)| *code == 2532 && msg == "Object is possibly 'undefined'."),
        "receiver names at 100 UTF-16 units use the Object form; got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|(code, _)| *code == 18048),
        "no named TS18048 at 100 UTF-16 units; got: {messages:?}"
    );
}

#[test]
fn property_access_on_short_named_receiver_keeps_named_variant() {
    let messages = check_source_strict_messages(
        "declare const conn: { p: number } | undefined;\nconst v = conn.p;\n",
    );
    assert!(
        messages
            .iter()
            .any(|(code, msg)| *code == 18048 && msg == "'conn' is possibly 'undefined'."),
        "short receiver names keep the named TS18048 form; got: {messages:?}"
    );
}
