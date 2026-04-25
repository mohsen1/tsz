use tsz_common::common::ScriptTarget;
use tsz_emitter::emitter::JsxEmit;
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print_named_with_opts;

fn emit_jsx(source: &str, jsx: JsxEmit, target: ScriptTarget) -> String {
    let opts = PrintOptions {
        jsx,
        target,
        ..Default::default()
    };
    parse_and_print_named_with_opts("test.tsx", source, opts)
}

// =============================================================================
// Spread flattening: {...{...a, ...b}} → ...a, ...b
// =============================================================================

#[test]
fn classic_spread_flattening_object_assign() {
    // Classic mode, pre-ES2018: {...{...a, ...b}} should flatten into
    // Object.assign({}, a, b) instead of Object.assign({}, {...a, ...b})
    let source = r#"const el = <div {...{...a, ...b}} />;"#;
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ES2015);
    assert!(
        output.contains("Object.assign({}, a, b)"),
        "Expected flattened Object.assign({{}}, a, b), got: {output}"
    );
}

#[test]
fn classic_spread_flattening_es2018() {
    // Classic mode, ES2018+: {...{...a, ...b}} should flatten into
    // { ...a, ...b } instead of { ...{...a, ...b} }
    let source = r#"const el = <div {...{...a, ...b}} />;"#;
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ESNext);
    assert!(
        output.contains("{ ...a, ...b }"),
        "Expected flattened inline spread, got: {output}"
    );
}

#[test]
fn automatic_spread_flattening() {
    // Automatic JSX mode: {...{...a, ...b}} should flatten spreads
    let source = r#"const el = <div {...{...a, ...b}} />;"#;
    let output = emit_jsx(source, JsxEmit::ReactJsx, ScriptTarget::ESNext);
    assert!(
        output.contains("...a, ...b"),
        "Expected flattened spread in automatic mode, got: {output}"
    );
    // Should NOT contain nested object literal
    assert!(
        !output.contains("...{"),
        "Should not have nested spread-of-object, got: {output}"
    );
}

#[test]
fn no_flatten_when_object_has_non_spread_props() {
    // Object literal with a mix of spread and non-spread props should NOT flatten
    let source = r#"const el = <div {...{...a, x: 1}} />;"#;
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ESNext);
    // The object literal should be kept as-is since it has a non-spread property
    assert!(
        output.contains("{ ...{ ...a, x: 1 }"),
        "Mixed props object should not be flattened, got: {output}"
    );
}

#[test]
fn no_flatten_empty_object() {
    // Empty object literal: {...{}} should NOT flatten (nothing to flatten)
    let source = r#"const el = <div {...{}} />;"#;
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ESNext);
    // Should keep the empty object spread as-is
    assert!(
        output.contains("{}"),
        "Empty object spread should be preserved, got: {output}"
    );
}

#[test]
fn flatten_single_inner_spread() {
    // Single inner spread: {...{...props}} should flatten to ...props
    let source = r#"const el = <div {...{...props}} />;"#;
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ESNext);
    assert!(
        output.contains("...props"),
        "Single inner spread should flatten, got: {output}"
    );
}

#[test]
fn flatten_preserves_named_attrs() {
    // Named attrs mixed with flattened spread
    let source = r#"const el = <div className="foo" {...{...a, ...b}} id="bar" />;"#;
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ESNext);
    assert!(
        output.contains("className"),
        "Named attrs should be preserved, got: {output}"
    );
    assert!(
        output.contains("...a"),
        "Inner spread a should be flattened, got: {output}"
    );
    assert!(
        output.contains("...b"),
        "Inner spread b should be flattened, got: {output}"
    );
    assert!(
        output.contains("id"),
        "Named attr id should be preserved, got: {output}"
    );
}

#[test]
fn classic_spread_no_flatten_variable() {
    // Regular spread (not an object literal): {...props} should NOT try to flatten
    let source = r#"const el = <div {...props} />;"#;
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ESNext);
    assert!(
        output.contains("props"),
        "Regular spread should pass through, got: {output}"
    );
}

// =============================================================================
// Target-appropriate spread prop handling (committed in 8717f7d)
// =============================================================================

#[test]
fn classic_object_assign_pre_es2018() {
    // Classic mode with pre-ES2018 target should use Object.assign
    let source = r#"const el = <div className="test" {...props} />;"#;
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ES2015);
    assert!(
        output.contains("Object.assign("),
        "Pre-ES2018 classic mode should use Object.assign, got: {output}"
    );
}

#[test]
fn classic_inline_spread_es2018() {
    // Classic mode with ES2018+ target should use inline spread
    let source = r#"const el = <div className="test" {...props} />;"#;
    let output = emit_jsx(source, JsxEmit::React, ScriptTarget::ESNext);
    assert!(
        !output.contains("Object.assign"),
        "ES2018+ classic mode should NOT use Object.assign, got: {output}"
    );
    assert!(
        output.contains("...props"),
        "ES2018+ classic mode should use inline spread, got: {output}"
    );
}

#[test]
fn automatic_object_assign_pre_es2018() {
    // Automatic JSX mode with pre-ES2018 target should use Object.assign for spreads
    let source = r#"const el = <div className="test" {...props}>child</div>;"#;
    let output = emit_jsx(source, JsxEmit::ReactJsx, ScriptTarget::ES2015);
    assert!(
        output.contains("Object.assign("),
        "Pre-ES2018 automatic mode with spreads should use Object.assign, got: {output}"
    );
}

#[test]
fn automatic_inline_spread_es2018() {
    // Automatic JSX mode with ES2018+ target should use inline spread
    let source = r#"const el = <div className="test" {...props}>child</div>;"#;
    let output = emit_jsx(source, JsxEmit::ReactJsx, ScriptTarget::ESNext);
    assert!(
        !output.contains("Object.assign"),
        "ES2018+ automatic mode should NOT use Object.assign, got: {output}"
    );
}
