//! Issue #3090: the JSX CommonJS automatic-runtime emitter generates
//! fixed `require` binding names like `jsx_runtime_1` and `react_1`.
//! When a user binding in the same file already declares one of those
//! names, tsc bumps the suffix (`jsx_runtime_2`, `react_2`, …) so the
//! generated `const` doesn't redeclare the user's identifier and the
//! JSX call routes through the imported namespace.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::emitter::JsxEmit;
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print_named_with_opts;

fn emit_cjs_jsx(source: &str) -> String {
    let opts = PrintOptions {
        jsx: JsxEmit::ReactJsx,
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2022,
        ..Default::default()
    };
    parse_and_print_named_with_opts("test.tsx", source, opts)
}

/// `const jsx_runtime_1 = ...` already exists; the auto-imported runtime
/// require must use `jsx_runtime_2` (or any non-colliding suffix).
#[test]
fn jsx_runtime_var_avoids_user_jsx_runtime_1_binding() {
    let source = "const jsx_runtime_1 = \"user binding\";\nexport const element = <div data-value={jsx_runtime_1} />;\n";
    let output = emit_cjs_jsx(source);

    let runtime_decl_count = output
        .lines()
        .filter(|line| line.contains("require(\"react/jsx-runtime\")"))
        .count();
    assert_eq!(
        runtime_decl_count, 1,
        "expected exactly one require for react/jsx-runtime, got output:\n{output}"
    );
    assert!(
        !output.contains("const jsx_runtime_1 = require"),
        "must not re-declare the user binding `jsx_runtime_1`. Output:\n{output}"
    );
    assert!(
        output.contains("const jsx_runtime_1 = \"user binding\""),
        "user binding must be preserved verbatim. Output:\n{output}"
    );
}

/// Double collision (`jsx_runtime_1` AND `jsx_runtime_1_1` already exist) —
/// hygienic helper increments the trailing `_<N>` suffix until it finds a
/// free candidate, so the require must end up using a name not already
/// present in source.
#[test]
fn jsx_runtime_var_skips_double_collision() {
    let source = "\
const jsx_runtime_1 = 1;
const jsx_runtime_1_1 = 2;
export const element = <div data-value={String(jsx_runtime_1)} />;
";
    let output = emit_cjs_jsx(source);

    let runtime_require: Option<&str> = output
        .lines()
        .find(|line| line.contains("require(\"react/jsx-runtime\")"));
    assert!(
        runtime_require.is_some(),
        "expected a runtime require, got:\n{output}"
    );
    let line = runtime_require.unwrap();
    assert!(
        !line.contains("const jsx_runtime_1 =") && !line.contains("const jsx_runtime_1_1 ="),
        "runtime var must avoid `jsx_runtime_1` AND `jsx_runtime_1_1`; got line: {line}\nfull output:\n{output}"
    );
    let user_decl_count = output
        .lines()
        .filter(|l| {
            l.starts_with("const jsx_runtime_1 = ") || l.starts_with("const jsx_runtime_1_1 = ")
        })
        .count();
    assert_eq!(
        user_decl_count, 2,
        "user bindings must be preserved verbatim. Output:\n{output}"
    );
}

/// No collision → keep the default `jsx_runtime_1`. Anchors that the fix
/// is collision-driven, not unconditional rename.
#[test]
fn jsx_runtime_var_unchanged_when_no_collision() {
    let source = "export const element = <div data-value=\"x\" />;\n";
    let output = emit_cjs_jsx(source);

    assert!(
        output.contains("const jsx_runtime_1 = require(\"react/jsx-runtime\")"),
        "expected default jsx_runtime_1 binding; got:\n{output}"
    );
}
