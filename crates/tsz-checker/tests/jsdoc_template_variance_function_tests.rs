use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

#[test]
fn jsdoc_function_template_in_modifier_emits_ts1274() {
    let source = r#"
/**
 * @template in T
 * @param {T} x
 */
function f(x) {}
"#;

    let diagnostics = check_source(
        source,
        "a.js",
        CheckerOptions {
            check_js: true,
            strict: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics.iter().any(|d| d.code == 1274),
        "expected TS1274 for @template in on function JSDoc, got: {diagnostics:?}"
    );
}

#[test]
fn jsdoc_template_variance_typedefs_do_not_emit_ts7006() {
    let source = r#"
/**
 * @template in T
 * @typedef {Object} Contravariant
 * @property {(x: T) => void} f
 */

/**
 * @template in out T
 * @typedef {Object} Invariant
 * @property {(x: T) => T} f
 */
"#;

    let diagnostics = check_source(
        source,
        "a.js",
        CheckerOptions {
            check_js: true,
            strict: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics.iter().all(|d| d.code != 7006),
        "did not expect TS7006 on JSDoc typedef function-type params, got: {diagnostics:?}"
    );
}

#[test]
fn jsdoc_template_variance_typedef_usage_still_suppresses_ts7006() {
    let source = r#"
/**
 * @template in T
 * @typedef {Object} Contravariant
 * @property {(x: T) => void} f
 */

/**
 * @type {Contravariant<unknown>}
 */
let super_contravariant = { f: (x) => {} };

/**
 * @type {Contravariant<string>}
 */
let sub_contravariant = { f: (x) => {} };

super_contravariant = sub_contravariant;
sub_contravariant = super_contravariant;

/**
 * @template in out T
 * @typedef {Object} Invariant
 * @property {(x: T) => T} f
 */

/**
 * @type {Invariant<unknown>}
 */
let super_invariant = { f: (x) => {} };

/**
 * @type {Invariant<string>}
 */
let sub_invariant = { f: (x) => { return \"\" } };

super_invariant = sub_invariant;
sub_invariant = super_invariant;
"#;

    let diagnostics = check_source(
        source,
        "a.js",
        CheckerOptions {
            check_js: true,
            strict: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics.iter().all(|d| d.code != 7006),
        "did not expect TS7006 for contextual callbacks under JSDoc variance typedef usage, got: {diagnostics:?}"
    );
}

#[test]
fn jsdoc_template_typedef_usage_without_variance_has_no_ts7006() {
    let source = r#"
/**
 * @template T
 * @typedef {Object} Contravariant
 * @property {(x: T) => void} f
 */

/**
 * @type {Contravariant<unknown>}
 */
let super_contravariant = { f: (x) => {} };

/**
 * @type {Contravariant<string>}
 */
let sub_contravariant = { f: (x) => {} };

super_contravariant = sub_contravariant;
sub_contravariant = super_contravariant;
"#;

    let diagnostics = check_source(
        source,
        "a.js",
        CheckerOptions {
            check_js: true,
            strict: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics.iter().all(|d| d.code != 7006),
        "did not expect TS7006 in non-variance JSDoc typedef usage, got: {diagnostics:?}"
    );
}
