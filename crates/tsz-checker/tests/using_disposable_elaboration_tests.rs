//! Regression tests for how a failed sync `using` declaration maps onto `tsc`'s
//! diagnostic, and, more broadly, for symbol-keyed missing-property elaboration
//! (TS2741) on a non-array target.
//!
//! Structural rule (#16872): the `using` check runs the ordinary assignability
//! relation against the global `Disposable` interface and lets the *shape* of
//! that failure choose the code — it does not apply TS2850 uniformly.
//!   * A single plain object type (interface / class / object literal /
//!     `declare const` of an object type) missing `[Symbol.dispose]` entirely
//!     falls through to the natural missing-property error, so the **primary**
//!     diagnostic is TS2741 (`Property '[Symbol.dispose]' is missing … but
//!     required in type 'Disposable'.`) with no TS2850.
//!   * A present-but-incompatible member, or a composite source (union /
//!     intersection / type parameter) whose constituents each miss the member,
//!     keeps the TS2850 head and nests the relation reason as the tail.
//!   * A bare whole-type mismatch (a primitive initializer) reports the flat
//!     TS2850 with no tail.
//!
//! `await using` (TS2851) always carries the flat top line.
//!
//! The companion rule for missing late-bound-symbol members: `tsc` lists those
//! members in TS2741/TS2739 for **non-array** targets (e.g. `Disposable`'s
//! `[Symbol.dispose]`) and omits them only for array-like targets (where the
//! iteration protocol makes them implicitly satisfied).
//!
//! Owner: solver `explain_object_failure` (reason) +
//! `error_reporter::assignability` (emission) +
//! `check_using_declaration_disposable` /
//! `classify_disposable_initializer_failure` (code selection).
//!
//! The cases are fully self-contained (an inline `Symbol`/`Disposable` so the
//! relation stays in one arena), and they vary the source member name where it
//! reaches the rendered output to lock the shape as structural, not a fixture
//! spelling (CLAUDE.md anti-hardcoding gate).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{DEFAULT_LIB_NAMES, check_source_with_libs, load_compiled_lib_files};
use tsz_common::common::{ModuleKind, ScriptTarget};

fn check(body: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    // Use TypeScript's real compiled libs so the well-known `Symbol.dispose`
    // resolves and renders as `[Symbol.dispose]` (the stripped test-lib bundle
    // does not model well-known symbols faithfully). `esnext.disposable` carries
    // `Disposable`/`AsyncDisposable`; it is not in the default bundle.
    let mut names: Vec<String> = DEFAULT_LIB_NAMES
        .iter()
        .map(|name| format!("lib.{name}"))
        .collect();
    names.push("lib.esnext.disposable.d.ts".to_string());
    let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let libs = load_compiled_lib_files(&name_refs);
    assert!(
        !libs.is_empty(),
        "compiled TypeScript libs must be available for these tests",
    );
    check_source_with_libs(
        body,
        "test.ts",
        CheckerOptions {
            module: ModuleKind::ESNext,
            target: ScriptTarget::ESNext,
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
}

/// Full elaboration text (primary message plus every related-information line,
/// in order) of the single diagnostic with `code`, drawn from an
/// already-computed diagnostic set so a caller that also asserts on other codes
/// does not recompile the same source.
fn elaboration_of(diags: &[tsz_checker::diagnostics::Diagnostic], code: u32) -> String {
    let matching: Vec<_> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "Expected exactly one TS{code}. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    let mut lines = vec![matching[0].message_text.clone()];
    lines.extend(
        matching[0]
            .related_information
            .iter()
            .map(|info| info.message_text.clone()),
    );
    lines.join("\n")
}

/// [`elaboration_of`] over a freshly checked `body`.
fn elaboration(body: &str, code: u32) -> String {
    elaboration_of(&check(body), code)
}

/// Sync `using` whose initializer is a single plain object type missing
/// `[Symbol.dispose]` entirely: `tsc` falls through to the ordinary
/// assignability failure against `Disposable` and reports the missing-property
/// **TS2741 as the primary diagnostic** — there is no TS2850 at all (#16872).
/// The `TS2850` "wrong kind of thing" wording is reserved for a source that is
/// not a single object missing the member (a primitive, a composite type, or a
/// present-but-incompatible member); see the rows below.
///
/// The rows vary the declaration form (object literal / `declare const` object
/// / class instance / interface) and the binder/member names, so the rule is
/// pinned to the *failure mode* (single object missing the member) and to a
/// rendered source type that echoes the input — never a hard-coded spelling
/// (CLAUDE.md anti-hardcoding gate). Each rendered type name is carried in the
/// row so the message assertion cannot silently drift to a fixed literal.
#[test]
fn using_missing_dispose_reports_ts2741_primary_not_ts2850() {
    // (source, rendered source-type name in the TS2741 message)
    let rows = [
        (
            "declare const x: { foo: number };\nfunction f() { using r = x; }",
            "{ foo: number; }",
        ),
        (
            "declare const handle: { resourceId: string };\nfunction open() { using h = handle; }",
            "{ resourceId: string; }",
        ),
        (
            "function f() { using r = { notDispose() {} }; }",
            "{ notDispose(): void; }",
        ),
        (
            "class Handle { open() {} }\nfunction f() { using r = new Handle(); }",
            "Handle",
        ),
        (
            "interface Resource { id: string }\ndeclare const res: Resource;\nfunction f() { using r = res; }",
            "Resource",
        ),
    ];
    for (body, source_ty) in rows {
        let diags = check(body);
        assert!(
            !diags.iter().any(|d| d.code == 2850),
            "a single object missing `[Symbol.dispose]` must NOT raise TS2850 for {body:?}, got: {:?}",
            diags.iter().map(|d| d.code).collect::<Vec<_>>()
        );
        assert_eq!(
            elaboration_of(&diags, 2741),
            format!(
                "Property '[Symbol.dispose]' is missing in type '{source_ty}' but required \
                 in type 'Disposable'."
            ),
            "TS2741 message for {body:?}",
        );
    }
}

/// A *composite* source (a union whose constituents each miss the member) is
/// NOT a single object type, so `tsc` keeps the TS2850 head and nests the
/// relation's missing-property reason beneath it as the tail — the opposite
/// choice from the single-object rows above. This is the row that proves the
/// split is by source shape, not merely "is the member missing".
#[test]
fn using_missing_dispose_union_source_keeps_ts2850_with_tail() {
    let diags = check(
        r#"
declare const u: { foo: number } | { bar: string };
function f() { using r = u; }
"#,
    );
    assert_eq!(
        elaboration_of(&diags, 2850),
        "The initializer of a 'using' declaration must be either an object with a \
         '[Symbol.dispose]()' method, or be 'null' or 'undefined'.\n\
         Property '[Symbol.dispose]' is missing in type '{ foo: number; }' but required \
         in type 'Disposable'.",
    );
    // The union keeps TS2850 as its primary — the missing-property line only
    // ever appears in the tail, never as the leading diagnostic.
    assert!(
        !diags.iter().any(|d| d.code == 2741),
        "union source must not surface TS2741 as a top-level diagnostic, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// A primitive initializer is the "wrong kind of thing": `tsc` reports the flat
/// TS2850 with **no** elaboration tail (the generic
/// `Type 'number' is not assignable to type 'Disposable'.` frame is replaced by
/// the TS2850 head and leaves nothing behind). Regression against re-attaching
/// that redundant whole-type line as a tail.
#[test]
fn using_primitive_initializer_reports_flat_ts2850() {
    for body in [
        "function f() { using r = 42; }",
        r#"function f() { using r = "handle"; }"#,
    ] {
        let diags = check(body);
        let ts2850 = diags
            .iter()
            .find(|d| d.code == 2850)
            .unwrap_or_else(|| panic!("expected TS2850 for {body:?}"));
        assert!(
            ts2850.related_information.is_empty(),
            "a primitive `using` initializer must carry no tail, got: {:?}",
            ts2850
                .related_information
                .iter()
                .map(|i| i.message_text.clone())
                .collect::<Vec<_>>()
        );
        assert!(
            !diags.iter().any(|d| d.code == 2741),
            "a primitive initializer must not surface TS2741, got: {:?}",
            diags.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }
}

/// `await using` (TS2851) carries NO tail in `tsc`, so the sync-only routing
/// must leave it flat — a single message line, no related information.
#[test]
fn await_using_missing_dispose_stays_flat() {
    let diags = check(
        r#"
declare const x: { foo: number };
async function f() { await using r = x; }
"#,
    );
    let ts2851 = diags
        .iter()
        .find(|d| d.code == 2851)
        .expect("expected TS2851 for await using on a non-disposable object");
    assert!(
        ts2851.related_information.is_empty(),
        "await using (TS2851) must carry no elaboration tail, got: {:?}",
        ts2851
            .related_information
            .iter()
            .map(|i| i.message_text.clone())
            .collect::<Vec<_>>()
    );
}

/// A genuinely disposable object must not error at all — the tail machinery only
/// runs on a real failure, and the error decision itself is unchanged.
#[test]
fn using_disposable_object_is_clean() {
    let diags = check(
        r#"
declare const x: { [Symbol.dispose](): void; foo: number };
function f() { using r = x; }
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2850),
        "a `[Symbol.dispose]`-bearing object must not raise TS2850, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// The broader explain fix: a plain assignment to `Disposable` whose source lacks
/// only the symbol member now produces the symbol-keyed TS2741 (matching `tsc`),
/// instead of collapsing to a flat `TypeMismatch` with no member listed. This is
/// the same `explain_object_failure` path the `using` tail rides, exercised on a
/// surface that does not depend on the disposable feature gate.
#[test]
fn plain_assignment_to_disposable_lists_symbol_member() {
    let text = elaboration(
        r#"
declare const x: { foo: number };
const d: Disposable = x;
"#,
        2741,
    );
    assert_eq!(
        text,
        "Property '[Symbol.dispose]' is missing in type '{ foo: number; }' but required \
         in type 'Disposable'.",
    );
}

/// Full elaboration as `(depth, text)` pairs — the primary message at depth 0,
/// then each related line at its rendered nesting depth + 1 (so the primary is
/// the shallowest). Locks both the wording AND the `tsc`-style progressive
/// indentation, which a flat text join cannot distinguish.
fn elaboration_with_depths(body: &str, code: u32) -> Vec<(u8, String)> {
    let diags = check(body);
    let matching: Vec<_> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "Expected exactly one TS{code}. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    let d = matching[0];
    let mut out = vec![(0u8, d.message_text.clone())];
    out.extend(
        d.related_information
            .iter()
            .map(|info| (info.depth + 1, info.message_text.clone())),
    );
    out
}

/// Sync `using` whose `[Symbol.dispose]` method has an incompatible *signature*
/// (an extra required parameter): the TS2850 top line keeps its wording and the
/// relation-derived tail drills straight to the specific incompatibility, with
/// no redundant `Type '{…}' is not assignable to type 'Disposable'.` wrapper
/// frame (the TS2850 head message already conveys it) and with the arity leaf
/// nested one level under the function-type line — matching `tsc`. Regression
/// for #16859 (a type-mismatch, not missing-property, Disposable failure).
#[test]
fn using_type_mismatch_dispose_signature_attaches_incompatible_tail() {
    let lines = elaboration_with_depths(
        r#"
function f() { using r = { [Symbol.dispose](extra: number) {} }; }
"#,
        2850,
    );
    assert_eq!(
        lines,
        vec![
            (
                0u8,
                "The initializer of a 'using' declaration must be either an object with a \
                 '[Symbol.dispose]()' method, or be 'null' or 'undefined'."
                    .to_string(),
            ),
            (
                1,
                "Types of property '[Symbol.dispose]' are incompatible.".to_string(),
            ),
            (
                2,
                "Type '(extra: number) => void' is not assignable to type '() => void'."
                    .to_string(),
            ),
            (
                3,
                "Target signature provides too few arguments. Expected 1 or more, but got 0."
                    .to_string(),
            ),
        ],
    );
}

/// Same rule, different parameter name — the tail must echo the renamed source
/// signature, never a hard-coded `extra` (CLAUDE.md anti-hardcoding gate).
#[test]
fn using_type_mismatch_tail_is_param_name_independent() {
    let lines = elaboration_with_depths(
        r#"
function open() { using h = { [Symbol.dispose](spare: string) {} }; }
"#,
        2850,
    );
    assert_eq!(
        lines,
        vec![
            (
                0u8,
                "The initializer of a 'using' declaration must be either an object with a \
                 '[Symbol.dispose]()' method, or be 'null' or 'undefined'."
                    .to_string(),
            ),
            (
                1,
                "Types of property '[Symbol.dispose]' are incompatible.".to_string(),
            ),
            (
                2,
                "Type '(spare: string) => void' is not assignable to type '() => void'."
                    .to_string(),
            ),
            (
                3,
                "Target signature provides too few arguments. Expected 1 or more, but got 0."
                    .to_string(),
            ),
        ],
    );
}

/// The void-return exception is unchanged by the structural gate: a
/// `[Symbol.dispose]` method that *returns a value* is still a valid `Disposable`
/// (anything is assignable to a `void`-returning target position), so no TS2850
/// fires. Locks that the gate rejects only genuine signature failures.
#[test]
fn using_dispose_value_return_is_clean() {
    let diags = check(
        r#"
function f() { using r = { [Symbol.dispose]() { return 1; } }; }
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2850),
        "a value-returning `[Symbol.dispose]` must not raise TS2850 (void-return exception), got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// `await using` whose `[Symbol.asyncDispose]` returns `void` (not a
/// `PromiseLike`) and which carries no sync `[Symbol.dispose]` fallback must
/// report TS2851 — the signature-level rejection the structural gate newly
/// catches. Per `tsc`, TS2851 carries no elaboration tail, so it stays flat.
#[test]
fn await_using_async_dispose_wrong_return_reports_flat_ts2851() {
    let diags = check(
        r#"
async function f() { await using r = { [Symbol.asyncDispose](): void {} }; }
"#,
    );
    let ts2851 = diags.iter().find(|d| d.code == 2851).unwrap_or_else(|| {
        panic!(
            "expected TS2851 for a void-returning `[Symbol.asyncDispose]`, got: {:?}",
            diags.iter().map(|d| d.code).collect::<Vec<_>>()
        )
    });
    assert!(
        ts2851.related_information.is_empty(),
        "await using (TS2851) must carry no elaboration tail, got: {:?}",
        ts2851
            .related_information
            .iter()
            .map(|i| i.message_text.clone())
            .collect::<Vec<_>>()
    );
}

/// #16862: a `using` initializer is not excess-property checked. A fresh object
/// literal carrying `[Symbol.dispose]` plus extra properties is a perfectly good
/// disposable, and `tsc` accepts it — #16858 passed the fresh literal type
/// straight into the relation, so freshness leaked in and produced a `TS2850`
/// whose tail read "Object literal may only specify known properties".
#[test]
fn using_object_literal_with_extra_properties_is_clean() {
    let diags = check(
        r#"
function f() { using r = { [Symbol.dispose]() {}, extra: 1 }; }
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2850),
        "extra properties beyond `[Symbol.dispose]` must not raise TS2850, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// The same object bound to a variable first was always accepted, because a
/// non-fresh source never reaches the excess-property check. Pairing the two
/// is what identifies the mechanism as freshness rather than a structural
/// mismatch — without this row, the fixture above only says "no error here".
#[test]
fn using_the_same_object_via_a_variable_is_also_clean() {
    let diags = check(
        r#"
const o = { [Symbol.dispose]() {}, extra: 1 };
function f() { using r = o; }
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2850),
        "a non-fresh disposable with extra properties must not raise TS2850, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `await using` rides the same relation against `AsyncDisposable`, so it needs
/// its own row — the sync arm is checked first and can mask the async one.
#[test]
fn await_using_object_literal_with_extra_properties_is_clean() {
    let diags = check(
        r#"
async function f() { await using r = { async [Symbol.asyncDispose]() {}, extra: 1 }; }
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2851),
        "extra properties beyond `[Symbol.asyncDispose]` must not raise TS2851, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Guard against over-correcting back to presence-only checking: widening
/// freshness must not weaken the signature-shape rejections #16858 added. A
/// non-callable `[Symbol.dispose]` is still an error.
#[test]
fn using_with_a_non_callable_dispose_still_errors() {
    let diags = check(
        r#"
function f() { using r = { [Symbol.dispose]: 42 }; }
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2850),
        "a non-callable `[Symbol.dispose]` must still raise TS2850, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// The other half of that guard: a `[Symbol.dispose]` with a required parameter
/// is structurally incompatible with `Disposable` and must stay rejected.
#[test]
fn using_with_a_required_parameter_on_dispose_still_errors() {
    let diags = check(
        r#"
function f() { using r = { [Symbol.dispose](x: number) {} }; }
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2850),
        "a `[Symbol.dispose]` with a required parameter must still raise TS2850, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Combined case: a fresh object literal carrying BOTH a signature-incompatible
/// `[Symbol.dispose]` and an extra property. `tsc` does not excess-property-check
/// a `using` initializer (#16862), so the tail must elaborate the signature
/// failure — never a leaked "Object literal may only specify known properties".
/// This is what pins the gate and the tail to the same freshness-widened source.
#[test]
fn using_type_mismatch_with_extra_properties_reports_signature_tail() {
    let lines = elaboration_with_depths(
        r#"
function f() { using r = { [Symbol.dispose](x: number) {}, extra: 1 }; }
"#,
        2850,
    );
    assert_eq!(
        lines,
        vec![
            (
                0u8,
                "The initializer of a 'using' declaration must be either an object with a \
                 '[Symbol.dispose]()' method, or be 'null' or 'undefined'."
                    .to_string(),
            ),
            (
                1,
                "Types of property '[Symbol.dispose]' are incompatible.".to_string(),
            ),
            (
                2,
                "Type '(x: number) => void' is not assignable to type '() => void'.".to_string(),
            ),
            (
                3,
                "Target signature provides too few arguments. Expected 1 or more, but got 0."
                    .to_string(),
            ),
        ],
    );
}

/// A third guard case alongside the two above: the freshness fix only stops
/// excess PROPERTIES from failing the relation -- it must not weaken presence
/// checking. A fresh object literal with no dispose member at all is still
/// rejected. Per #16872 the code is TS2741 (single object missing the member),
/// not TS2850 — the error still fires, only its code changed. The full
/// TS2741-vs-TS2850 split is asserted by
/// `using_fresh_literal_missing_dispose_reports_ts2741` and its siblings above.
#[test]
fn using_fresh_literal_missing_dispose_still_errors() {
    let diags = check(
        r#"
function f() { using r = { notDispose() {} }; }
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2741),
        "a fresh literal with no dispose member must still be rejected (now TS2741), got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    assert!(
        !diags.iter().any(|d| d.code == 2850),
        "the single-object missing-member row no longer uses TS2850, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}
