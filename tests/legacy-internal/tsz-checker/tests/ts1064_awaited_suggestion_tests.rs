//! TS1064's `Did you mean to write 'Promise[T]'?` suggestion renders the
//! annotation's **awaited** type, not the annotation itself.
//!
//! tsc's `checkAsyncFunctionReturnType` formats the message argument as
//! `typeToString(getAwaitedTypeNoAlias(returnType) || voidType)`. A type that
//! is not thenable is its own awaited type, so every non-thenable annotation
//! renders unchanged — which is exactly why rendering the annotation itself
//! went unnoticed. Only an annotation that *is* a thenable other than the
//! global `Promise` distinguishes the two, and a thenable whose `then` yields
//! no fulfillment payload awaits to nothing and falls back to `void`.
//!
//! Every row below is pinned against `typescript@7.0.2`
//! (`--noEmit --strict --target es2022 --lib es2022`); the binder names are
//! varied across rows so no assertion can be satisfied by a name check.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

/// The `Promise[...]` argument tsz renders inside TS1064's suggestion, for the
/// single TS1064 the source is expected to produce.
fn ts1064_suggestion(source: &str) -> String {
    let libs = load_default_lib_files();
    let messages: Vec<String> = check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .filter(|diagnostic| diagnostic.code == 1064)
    .map(|diagnostic| diagnostic.message_text)
    .collect();
    assert_eq!(
        messages.len(),
        1,
        "expected exactly one TS1064; got {messages:?}"
    );
    let message = &messages[0];
    let start = message
        .find("Did you mean to write '")
        .map(|at| at + "Did you mean to write '".len())
        .unwrap_or_else(|| panic!("TS1064 carries no suggestion clause: {message}"));
    let rest = &message[start..];
    let end = rest
        .rfind('\'')
        .unwrap_or_else(|| panic!("TS1064 suggestion is unterminated: {message}"));
    rest[..end].to_string()
}

/// A valid thenable annotation suggests its fulfillment payload, not itself.
#[test]
fn valid_thenable_annotation_suggests_its_payload() {
    assert_eq!(
        ts1064_suggestion(
            r#"
export {};
interface QqSettled { then(onDone: (value: number) => void): void }
declare const qqValue: QqSettled;
async function qqRun(): QqSettled { return qqValue; }
"#
        ),
        "Promise<number>"
    );
}

/// The payload is rendered structurally, exactly as tsc prints it.
#[test]
fn thenable_payload_object_type_is_rendered_structurally() {
    assert_eq!(
        ts1064_suggestion(
            r#"
export {};
interface WwBoxed { then(onDone: (value: { id: string }) => void): void }
declare const wwValue: WwBoxed;
async function wwRun(): WwBoxed { return wwValue; }
"#
        ),
        "Promise<{ id: string; }>"
    );
}

/// Control: a non-thenable annotation is its own awaited type, so the rendered
/// suggestion is unchanged by this rule. This is the row that shows the fix is
/// the awaited-vs-annotation choice and not a general printer change.
#[test]
fn non_thenable_class_annotation_suggests_itself() {
    assert_eq!(
        ts1064_suggestion(
            r#"
export {};
class EeThing { x = 1 }
declare const eeValue: EeThing;
async function eeRun(): EeThing { return eeValue; }
"#
        ),
        "Promise<EeThing>"
    );
}

/// Control: a primitive annotation is likewise its own awaited type.
#[test]
fn primitive_annotation_suggests_itself() {
    assert_eq!(
        ts1064_suggestion(
            r#"
export {};
declare const rrValue: string;
async function rrRun(): string { return rrValue; }
"#
        ),
        "Promise<string>"
    );
}

/// An *invalid* thenable — a callable `then` that yields no fulfillment
/// payload — awaits to nothing, and tsc's `|| voidType` fallback applies.
#[test]
fn invalid_thenable_annotation_suggests_void() {
    assert_eq!(
        ts1064_suggestion(
            r#"
export {};
interface TtBadParam { then(onDone: string): void }
declare const ttValue: TtBadParam;
async function ttRun(): TtBadParam { return ttValue; }
"#
        ),
        "Promise<void>"
    );
}

/// `getAwaitedTypeNoAlias` recurses, so a `then` whose payload is itself
/// thenable unwraps all the way to the settled value.
#[test]
fn nested_thenable_annotation_suggests_the_fully_awaited_type() {
    assert_eq!(
        ts1064_suggestion(
            r#"
export {};
interface YyInner { then(onDone: (value: number) => void): void }
interface YyOuter { then(onDone: (value: YyInner) => void): void }
declare const yyValue: YyOuter;
async function yyRun(): YyOuter { return yyValue; }
"#
        ),
        "Promise<number>"
    );
}

/// A generic `then` callback still yields its parameter's payload.
#[test]
fn generic_then_callback_annotation_suggests_its_payload() {
    assert_eq!(
        ts1064_suggestion(
            r#"
export {};
interface UuGeneric { then<R>(onDone: (value: string) => R): void }
declare const uuValue: UuGeneric;
async function uuRun(): UuGeneric { return uuValue; }
"#
        ),
        "Promise<string>"
    );
}

/// A `PromiseLike[T]` annotation already suggested `Promise[T]` before this
/// rule; pinning it guards the Promise-application unwrap that the awaited
/// walk now runs first.
#[test]
fn promise_like_annotation_still_suggests_its_type_argument() {
    assert_eq!(
        ts1064_suggestion(
            r#"
export {};
declare const iiValue: PromiseLike<boolean>;
async function iiRun(): PromiseLike<boolean> { return iiValue; }
"#
        ),
        "Promise<boolean>"
    );
}

/// The rule is a property of the annotation, not of the function form: an
/// async arrow reports the same suggestion as a declaration.
#[test]
fn async_arrow_annotation_suggests_its_payload() {
    assert_eq!(
        ts1064_suggestion(
            r#"
export {};
interface OoSettled { then(onDone: (value: number) => void): void }
declare const ooValue: OoSettled;
const ooRun = async (): OoSettled => ooValue;
"#
        ),
        "Promise<number>"
    );
}

/// ...and so does an async class method.
#[test]
fn async_method_annotation_suggests_its_payload() {
    assert_eq!(
        ts1064_suggestion(
            r#"
export {};
interface PpSettled { then(onDone: (value: number) => void): void }
declare const ppValue: PpSettled;
class PpHolder { async ppRun(): PpSettled { return ppValue; } }
"#
        ),
        "Promise<number>"
    );
}

/// An alias in front of a thenable does not change the awaited type, and the
/// suggestion follows the type rather than the alias spelling.
#[test]
fn alias_of_a_thenable_annotation_suggests_the_payload() {
    assert_eq!(
        ts1064_suggestion(
            r#"
export {};
interface AaSettled { then(onDone: (value: number) => void): void }
type AaAlias = AaSettled;
declare const aaValue: AaAlias;
async function aaRun(): AaAlias { return aaValue; }
"#
        ),
        "Promise<number>"
    );
}
