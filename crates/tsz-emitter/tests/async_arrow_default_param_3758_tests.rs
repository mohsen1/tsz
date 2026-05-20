//! Issue #3758: when downleveling an async arrow with a default parameter,
//! tsc moves the entire parameter list into the inner generator function
//! and forwards arguments via `(...args_<n>) => __awaiter(this,
//! [...args_<n>], void 0, function* (<orig params>) { ... })`. This makes
//! the default-initializer expression evaluate inside the generator, so
//! a synchronous throw turns into a rejected promise instead of escaping
//! the call site.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print_named_with_opts;

fn emit_es2015_cjs(source: &str) -> String {
    let opts = PrintOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    parse_and_print_named_with_opts("a.ts", source, opts)
}

/// The exact repro from the issue: default param `x = fail()` must move
/// into the generator so the throw becomes a rejected promise.
#[test]
fn async_arrow_with_default_param_moves_param_into_generator() {
    let source = r#"
function fail(): number { throw new Error("boom"); }
export async function outer() {
  return async (x = fail()) => x;
}
"#;
    let output = emit_es2015_cjs(source);
    assert!(
        output.contains(
            "(...args_1) => __awaiter(this, [...args_1], void 0, function* (x = fail()) "
        ),
        "expected rest-spread forwarding with default in generator. Output:\n{output}"
    );
    assert!(
        !output.contains("(x = fail()) => __awaiter"),
        "must NOT leave the default initializer on the outer arrow. Output:\n{output}"
    );
}

/// Multiple defaults — all preserved on the inner generator.
#[test]
fn async_arrow_with_multiple_defaults_preserves_all() {
    let source = r#"
declare function f(): number;
declare function g(): string;
export const arrow = async (a = f(), b = g()) => a;
"#;
    let output = emit_es2015_cjs(source);
    assert!(
        output.contains("function* (a = f(), b = g())"),
        "expected both defaults preserved on generator. Output:\n{output}"
    );
    assert!(
        output.contains("(...args_1) => __awaiter(void 0, [...args_1], void 0,"),
        "expected rest-spread forwarding. Output:\n{output}"
    );
}

/// Mixed: required arg + defaulted arg -> preserves required leading params on
/// the outer arrow and forwards the moved default tail via rest-spread.
#[test]
fn async_arrow_mixed_required_and_default_forwards_via_rest() {
    let source = r#"
declare function init(): number;
export const arrow = async (a: number, b = init()) => a + b;
"#;
    let output = emit_es2015_cjs(source);
    assert!(
        output.contains(
            "(a_1, ...args_1) => __awaiter(void 0, [a_1, ...args_1], void 0, function* (a, b = init())"
        ),
        "expected leading param plus rest forwarding, generator with all original params. Output:\n{output}"
    );
}

/// Async arrow with no defaults must NOT trigger the new path — confirm we
/// keep the original lowering shape (`() => __awaiter(...)` with bare params
/// on the outer arrow).
#[test]
fn async_arrow_without_defaults_unchanged() {
    let source = r#"
export const arrow = async (a: number) => a;
"#;
    let output = emit_es2015_cjs(source);
    assert!(
        !output.contains("[...args"),
        "expected no rest-spread forwarding for default-free arrow. Output:\n{output}"
    );
    assert!(
        output.contains("(a) => __awaiter(void 0, void 0, void 0, function* () "),
        "expected the existing simple lowering. Output:\n{output}"
    );
}
