//! Regression tests for the receiver type rendered in `TS7053`
//! ("Element implicitly has an 'any' type ... can't be used to index type 'X'").
//!
//! Structural rule (one sentence): `tsc` renders
//! `typeToString(getApparentType(objectType))` for the receiver, so the `object`
//! intrinsic prints as its apparent type `{}` (not the raw `object`), and a bare
//! primitive prints as its boxed wrapper interface.
//!
//! Witness: `superstruct` `src/structs/types.ts(377,21)` / `src/utils.ts(9,32)`
//! index a value whose type is the `object` intrinsic and previously rendered
//! `object` where `tsc` renders `{}`.

use crate::test_utils::check_source_code_messages;

const TS7053: u32 = 7053;

fn ts7053_messages(source: &str) -> Vec<String> {
    check_source_code_messages(source)
        .into_iter()
        .filter(|(code, _)| *code == TS7053)
        .map(|(_, message)| message)
        .collect()
}

/// The reported repro: indexing an `object`-typed receiver with a `string` key.
/// `tsc` renders the apparent type `{}`, not the raw `object`.
#[test]
fn object_intrinsic_receiver_renders_apparent_empty_object() {
    let source = r#"
function h(o: object, k: string) {
  return o[k];
}
"#;
    let messages = ts7053_messages(source);
    assert_eq!(
        messages.len(),
        1,
        "expected exactly one TS7053 for `o[k]` on an `object` receiver; got: {messages:?}",
    );
    let message = &messages[0];
    assert!(
        message.contains("can't be used to index type '{}'"),
        "TS7053 must render the apparent type `{{}}` for the `object` intrinsic; got: {message:?}",
    );
    assert!(
        !message.contains("index type 'object'"),
        "TS7053 must not render the raw `object` intrinsic; got: {message:?}",
    );
}

/// Anti-hardcoding: the receiver display must not depend on the parameter or
/// alias identifiers chosen by the user. Renaming every binder keeps the `{}`
/// rendering, and an alias of `object` behaves identically.
#[test]
fn object_intrinsic_receiver_display_is_name_agnostic() {
    let source = r#"
type Bag = object;
function lookup(container: Bag, slot: string) {
  return container[slot];
}
"#;
    let messages = ts7053_messages(source);
    assert_eq!(
        messages.len(),
        1,
        "expected exactly one TS7053 for an aliased `object` receiver; got: {messages:?}",
    );
    assert!(
        messages[0].contains("can't be used to index type '{}'"),
        "an alias of `object` resolves to the same apparent type `{{}}`; got: {:?}",
        messages[0],
    );
}

/// Control: a receiver that is already the empty object type `{}` keeps
/// rendering `{}` (apparent type is the identity here), confirming the change
/// is specific to the apparent-type mapping and does not perturb the common
/// path.
#[test]
fn empty_object_receiver_still_renders_empty_object() {
    let source = r#"
function h(o: {}, k: string) {
  return o[k];
}
"#;
    let messages = ts7053_messages(source);
    assert_eq!(
        messages.len(),
        1,
        "expected exactly one TS7053 for `o[k]` on a `{{}}` receiver; got: {messages:?}",
    );
    assert!(
        messages[0].contains("can't be used to index type '{}'"),
        "an already-empty-object receiver renders `{{}}` unchanged; got: {:?}",
        messages[0],
    );
}

/// Control: a named interface receiver is unaffected — its apparent type is the
/// interface itself, so it keeps rendering its nominal name (not collapsed to
/// `{}`).
#[test]
fn named_interface_receiver_keeps_nominal_display() {
    let source = r#"
interface Shape { area: number; }
function h(o: Shape, k: string) {
  return o[k];
}
"#;
    let messages = ts7053_messages(source);
    assert_eq!(
        messages.len(),
        1,
        "expected exactly one TS7053 for `o[k]` on a `Shape` receiver; got: {messages:?}",
    );
    assert!(
        messages[0].contains("can't be used to index type 'Shape'"),
        "a named interface keeps its nominal display, not `{{}}`; got: {:?}",
        messages[0],
    );
}
