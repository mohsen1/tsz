//! Regression tests for checked-JS `@overload` call resolution.

use crate::test_utils::check_js_source_code_messages;

fn messages_for_code(messages: &[(u32, String)], code: u32) -> Vec<&str> {
    messages
        .iter()
        .filter_map(|(diag_code, message)| (*diag_code == code).then_some(message.as_str()))
        .collect()
}

#[test]
fn jsdoc_overload_call_uses_overload_return_not_implementation_return() {
    let messages = check_js_source_code_messages(
        r#"
/**
 * @overload
 * @param {number} x
 * @returns {string}
 */
/**
 * @param {number} x
 * @returns {string | number}
 */
function f(x) { return x; }

/** @type {boolean} */
const b = f(1);
"#,
    );

    let ts2322 = messages_for_code(&messages, 2322);
    assert_eq!(ts2322.len(), 1, "expected one TS2322, got {messages:?}");
    assert!(
        ts2322[0].contains("Type 'string' is not assignable to type 'boolean'"),
        "expected selected overload return in diagnostic, got {ts2322:?}"
    );
    assert!(
        !ts2322[0].contains("string | number"),
        "implementation return should not be used for calls with JSDoc overloads: {ts2322:?}"
    );
}

#[test]
fn jsdoc_overload_call_selects_by_argument_type() {
    let messages = check_js_source_code_messages(
        r#"
/**
 * @overload
 * @param {number} value
 * @returns {string}
 */
/**
 * @overload
 * @param {string} value
 * @returns {number}
 */
/**
 * @param {number | string} value
 * @returns {string | number}
 */
function choose(value) { return value; }

/** @type {string} */
const s = choose(1);

/** @type {number} */
const n = choose("x");

/** @type {boolean} */
const b = choose("x");
"#,
    );

    let ts2322 = messages_for_code(&messages, 2322);
    assert_eq!(ts2322.len(), 1, "expected one TS2322, got {messages:?}");
    assert!(
        ts2322[0].contains("Type 'number' is not assignable to type 'boolean'"),
        "expected string-argument overload return in diagnostic, got {ts2322:?}"
    );
}

#[test]
fn jsdoc_without_overload_keeps_implementation_signature_callable() {
    let messages = check_js_source_code_messages(
        r#"
/**
 * @param {number} x
 * @returns {string | number}
 */
function plain(x) { return x; }

/** @type {boolean} */
const b = plain(1);
"#,
    );

    let ts2322 = messages_for_code(&messages, 2322);
    assert_eq!(ts2322.len(), 1, "expected one TS2322, got {messages:?}");
    assert!(
        ts2322[0].contains("string | number"),
        "non-overload JSDoc should keep implementation return visible: {ts2322:?}"
    );
}
