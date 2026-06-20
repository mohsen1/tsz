//! ES5/ES3 down-level parity for destructuring patterns in a catch clause.
//!
//! Structural rule:
//!   When the target is ES5 or below (no destructuring syntax) and a catch
//!   clause binds a destructuring pattern (`catch ({ message }) { ... }`), tsc
//!   binds the caught value to a synthesized catch parameter and extracts the
//!   pattern into a leading `var` statement inside the catch block:
//!     `catch (_a) { var message = _a.message; ... }`.
//!   tsz previously emitted the destructuring pattern verbatim, which is not
//!   valid ES5 and throws at runtime.
//!
//! The decision keys purely on the syntactic shape (ES5 target + binding
//! pattern), never on the chosen identifier names, so the binder names are
//! varied across cases to guard against name-based hardcoding. The extraction
//! reuses the shared ES5 destructuring lowering, so defaults, nested patterns,
//! renamed properties, array patterns, and object rest are all covered.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print as parse_lower_emit;

fn es5_opts() -> PrintOptions {
    PrintOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::CommonJS,
        ..Default::default()
    }
}

/// A bare object pattern lowers to a temp parameter plus a single extraction.
#[test]
fn catch_object_pattern_extracts_into_leading_var() {
    let source =
        "try {} catch ({ message }) { use(message); }\ndeclare function use(x: any): void;\n";
    let output = parse_lower_emit(source, es5_opts());
    assert!(
        output.contains("catch (_a) {") && output.contains("var message = _a.message;"),
        "object pattern in catch must lower to `catch (_a) {{ var message = _a.message; }}`.\nOutput:\n{output}"
    );
    // The destructuring pattern must not survive verbatim in ES5 output.
    assert!(
        !output.contains("catch ({ message })"),
        "ES5 output must not retain the binding pattern.\nOutput:\n{output}"
    );
}

/// Multiple properties extract as a comma-separated `var` list.
#[test]
fn catch_object_pattern_multiple_props() {
    let source = "try {} catch ({ reason, detail }) { sink(reason, detail); }\ndeclare function sink(a: any, b: any): void;\n";
    let output = parse_lower_emit(source, es5_opts());
    assert!(
        output.contains("var reason = _a.reason, detail = _a.detail;"),
        "multiple properties must extract as a comma list.\nOutput:\n{output}"
    );
}

/// A renamed property reads the source key, not the local binding name.
#[test]
fn catch_object_pattern_renamed_property() {
    let source =
        "try {} catch ({ message: msg }) { use(msg); }\ndeclare function use(x: any): void;\n";
    let output = parse_lower_emit(source, es5_opts());
    assert!(
        output.contains("var msg = _a.message;"),
        "renamed property must read `_a.message` and bind `msg`.\nOutput:\n{output}"
    );
}

/// A defaulted property emits the `=== void 0` guard.
#[test]
fn catch_object_pattern_default_value() {
    let source =
        "try {} catch ({ code = 500 }) { use(code); }\ndeclare function use(x: any): void;\n";
    let output = parse_lower_emit(source, es5_opts());
    assert!(
        output.contains("code = ") && output.contains("=== void 0 ? 500 :"),
        "defaulted property must guard with `=== void 0 ? 500 : ...`.\nOutput:\n{output}"
    );
}

/// A nested object pattern reads through the access path.
#[test]
fn catch_nested_object_pattern() {
    let source = "try {} catch ({ outer: { inner } }) { use(inner); }\ndeclare function use(x: any): void;\n";
    let output = parse_lower_emit(source, es5_opts());
    assert!(
        output.contains("var inner = _a.outer.inner;"),
        "nested pattern must read `_a.outer.inner`.\nOutput:\n{output}"
    );
}

/// An array pattern extracts by index (no `downlevelIteration`).
#[test]
fn catch_array_pattern_extracts_by_index() {
    let source = "try {} catch ([head, tail]) { sink(head, tail); }\ndeclare function sink(a: any, b: any): void;\n";
    let output = parse_lower_emit(source, es5_opts());
    assert!(
        output.contains("var head = _a[0], tail = _a[1];"),
        "array pattern must extract `_a[0]`/`_a[1]`.\nOutput:\n{output}"
    );
}

/// Object rest pulls in the `__rest` helper and excludes the named key.
#[test]
fn catch_object_pattern_with_rest() {
    let source = "try {} catch ({ label, ...others }) { sink(label, others); }\ndeclare function sink(a: any, b: any): void;\n";
    let output = parse_lower_emit(source, es5_opts());
    assert!(
        output.contains("var label = _a.label, others = __rest(_a, [\"label\"]);"),
        "object rest must lower via `__rest(_a, [\"label\"])`.\nOutput:\n{output}"
    );
}

/// Renaming every binder keeps the same structural lowering — no name-based
/// fast path. This mirrors the multi-prop case with different identifiers.
#[test]
fn catch_pattern_lowering_is_name_agnostic() {
    let source = "try {} catch ({ alpha, beta: renamed }) { sink(alpha, renamed); }\ndeclare function sink(a: any, b: any): void;\n";
    let output = parse_lower_emit(source, es5_opts());
    assert!(
        output.contains("var alpha = _a.alpha, renamed = _a.beta;"),
        "lowering must follow structure regardless of binder names.\nOutput:\n{output}"
    );
}

/// A simple identifier catch binding is left untouched at ES5.
#[test]
fn catch_simple_identifier_unchanged() {
    let source = "try {} catch (err) { use(err); }\ndeclare function use(x: any): void;\n";
    let output = parse_lower_emit(source, es5_opts());
    assert!(
        output.contains("catch (err) {"),
        "a plain identifier binding must be preserved verbatim.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var err = "),
        "a plain identifier binding must not be re-extracted.\nOutput:\n{output}"
    );
}

/// An empty pattern extracts nothing, so no `var` statement is emitted — the
/// synthesized parameter must not be followed by an invalid `var ;`.
#[test]
fn catch_empty_object_pattern_emits_no_var() {
    let source = "try {} catch ({}) { log(); }\ndeclare function log(): void;\n";
    let output = parse_lower_emit(source, es5_opts());
    assert!(
        output.contains("catch (_a) {") && !output.contains("var ;"),
        "empty object pattern must not emit an invalid `var ;`.\nOutput:\n{output}"
    );
}

/// An empty array pattern is likewise extraction-free.
#[test]
fn catch_empty_array_pattern_emits_no_var() {
    let source = "try {} catch ([]) { log(); }\ndeclare function log(): void;\n";
    let output = parse_lower_emit(source, es5_opts());
    assert!(
        output.contains("catch (_a) {") && !output.contains("var ;"),
        "empty array pattern must not emit an invalid `var ;`.\nOutput:\n{output}"
    );
}

/// An array pattern with a leading elision reads the correct index.
#[test]
fn catch_array_pattern_with_hole() {
    let source =
        "try {} catch ([, second]) { use(second); }\ndeclare function use(x: any): void;\n";
    let output = parse_lower_emit(source, es5_opts());
    assert!(
        output.contains("var second = _a[1];"),
        "an elided first element must shift the index to `_a[1]`.\nOutput:\n{output}"
    );
}

/// At ES2015+ (native destructuring) the catch pattern is preserved verbatim.
#[test]
fn catch_object_pattern_preserved_at_es2015() {
    let source =
        "try {} catch ({ message }) { use(message); }\ndeclare function use(x: any): void;\n";
    let opts = PrintOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);
    assert!(
        output.contains("catch ({ message })"),
        "ES2015 supports destructuring catch bindings natively.\nOutput:\n{output}"
    );
}
