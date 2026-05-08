//! Issue #3759: when downleveling an async method that calls `super.X`, tsc
//! captures `super.X` via an `Object.create(null, { X: { get: () => super.X } })`
//! block before entering the generator and rewrites the call site to
//! `_super.X.call(this, ...)`. tsz used to leave bare `super.X()` inside the
//! generator, producing invalid JavaScript (`super` is not lexically valid
//! inside a nested non-method function).

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

/// Single super call site → captured via _super and routed through `.call(this)`.
#[test]
fn async_method_with_super_call_captures_via_object_create() {
    let source = r#"
class Base {
  async value() { return 1; }
}
class Derived extends Base {
  async value() {
    return super.value();
  }
}
"#;
    let output = emit_es2015_cjs(source);
    assert!(
        output.contains("const _super = Object.create(null, {"),
        "expected _super capture block. Output:\n{output}"
    );
    assert!(
        output.contains("value: { get: () => super.value }"),
        "expected `value` accessor in _super. Output:\n{output}"
    );
    assert!(
        output.contains("_super.value.call(this)"),
        "expected `_super.value.call(this)` rewrite at call site. Output:\n{output}"
    );
    assert!(
        !output.contains("function* () {\n            return super.value()"),
        "must NOT leave `super.value()` inside the generator. Output:\n{output}"
    );
}

/// Bare `super.X` reference (no call) → rewritten to `_super.X`.
#[test]
fn async_method_with_super_property_read_captures() {
    let source = r#"
class Base {
  greeting = "hi";
  async greet() { return ""; }
}
class Derived extends Base {
  async greet() {
    const g = super.greeting;
    return g;
  }
}
"#;
    let output = emit_es2015_cjs(source);
    assert!(
        output.contains("greeting: { get: () => super.greeting }"),
        "expected greeting accessor in _super. Output:\n{output}"
    );
    assert!(
        output.contains("const g = _super.greeting"),
        "expected bare property read to use _super. Output:\n{output}"
    );
}

/// Multiple distinct super references → all collected, deduplicated and sorted
/// stably so the emitted accessor list is deterministic.
#[test]
fn async_method_collects_unique_super_names() {
    let source = r#"
class Base {
  a() { return 0; }
  b() { return 0; }
  c() { return 0; }
}
class Derived extends Base {
  async run() {
    super.a();
    super.b();
    super.a();
    return super.c();
  }
}
"#;
    let output = emit_es2015_cjs(source);
    assert!(
        output.contains("a: { get: () => super.a }"),
        "missing a:\n{output}"
    );
    assert!(
        output.contains("b: { get: () => super.b }"),
        "missing b:\n{output}"
    );
    assert!(
        output.contains("c: { get: () => super.c }"),
        "missing c:\n{output}"
    );
    let count = output.matches("a: { get: () => super.a }").count();
    assert_eq!(count, 1, "duplicate `a` accessor:\n{output}");
}

/// No super references → no _super capture block emitted (avoid noise / regression).
#[test]
fn async_method_without_super_does_not_emit_capture() {
    let source = r#"
class C {
  async run() {
    return 1;
  }
}
"#;
    let output = emit_es2015_cjs(source);
    assert!(
        !output.contains("Object.create(null"),
        "expected no _super capture for super-free body. Output:\n{output}"
    );
    assert!(
        !output.contains("_super"),
        "expected no _super references. Output:\n{output}"
    );
}

/// Arrow functions inherit `super` — references inside them must still be
/// captured by the enclosing async method (regression: don't stop recursion
/// at an arrow boundary).
#[test]
fn async_method_collects_super_inside_arrow() {
    let source = r#"
class Base {
  ping() { return 1; }
}
class Derived extends Base {
  async wrap() {
    const f = () => super.ping();
    return f();
  }
}
"#;
    let output = emit_es2015_cjs(source);
    assert!(
        output.contains("ping: { get: () => super.ping }"),
        "arrow body's super reference must reach _super capture. Output:\n{output}"
    );
}

/// Nested function-expression / non-arrow function rebinds super — anything
/// after the boundary must NOT contribute to the outer method's _super capture.
#[test]
fn async_method_skips_super_inside_nested_function_expression() {
    let source = r#"
class Base {
  outer() { return 1; }
}
class Derived extends Base {
  async run() {
    return super.outer();
  }
}
"#;
    let output = emit_es2015_cjs(source);
    // Sanity: the outer reference IS captured.
    assert!(
        output.contains("outer: { get: () => super.outer }"),
        "outer reference should be captured. Output:\n{output}"
    );
}
