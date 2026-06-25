//! Regression tests for generated temp-variable numbering in `??=` (nullish
//! logical assignment) downlevel emit.
//!
//! When a `??=` target is a bare identifier (or `this`/`super`), tsc lowers it
//! inline — `y !== null && y !== void 0 ? y : (y = rhs)` — and allocates NO
//! temporary. tsz previously pre-allocated a hoisted value temp for *every*
//! `??=` regardless of its target, which advanced the shared temp counter and
//! shifted every later generated name by one (`_b` where tsc emits `_a`). These
//! tests pin the counter to tsc's behavior. See the `count_logical_assignment_
//! value_temps` gate in `helpers.rs`.

use tsz_common::common::ScriptTarget;
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print;

fn emit_es2017(source: &str) -> String {
    let opts = PrintOptions {
        target: ScriptTarget::ES2017,
        ..Default::default()
    };
    parse_and_lower_print(source, opts)
}

/// A bare-identifier `??=` allocates no temp, so the following optional chain
/// must take the first generated name `_a` (not `_b`).
#[test]
fn identifier_nullish_assignment_does_not_consume_temp() {
    let source = "declare const obj: any;\nlet y: any;\ny ??= 5;\nconst x = obj?.p?.q;\n";
    let output = emit_es2017(source);
    assert!(
        output.contains("(_a = obj === null"),
        "optional chain must use `_a` after a bare-identifier `??=`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_b"),
        "no `_b` should be generated — the `??=` must not reserve a temp.\nOutput:\n{output}"
    );
}

/// Two consecutive bare-identifier `??=` still reserve nothing: the chain stays
/// at `_a`.
#[test]
fn multiple_identifier_nullish_assignments_do_not_consume_temps() {
    let source =
        "declare const obj: any;\nlet y: any, z: any;\ny ??= 5;\nz ??= 6;\nconst x = obj?.p?.q;\n";
    let output = emit_es2017(source);
    assert!(
        output.contains("(_a = obj === null"),
        "optional chain must use `_a` after two bare-identifier `??=`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_b") && !output.contains("_c"),
        "no extra temps should be generated for identifier `??=`.\nOutput:\n{output}"
    );
}

/// A property-access `??=` *does* capture its target into a value temp `_a`, so
/// the following optional chain must take `_b`. This guards against the fix
/// over-correcting and dropping the temp the property path genuinely needs.
#[test]
fn property_nullish_assignment_still_consumes_temp() {
    let source = "declare const obj: any;\nlet o: any;\no.k ??= 5;\nconst x = obj?.p?.q;\n";
    let output = emit_es2017(source);
    assert!(
        output.contains("(_a = o.k) !== null"),
        "property `??=` must capture its target into `_a`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(_b = obj === null"),
        "optional chain must use `_b` after a property `??=`.\nOutput:\n{output}"
    );
}

/// A bare-identifier `??=` whose RHS is a non-simple `??` coalescing still
/// numbers correctly: the value temp consumed by the RHS `??` is `_a` and the
/// trailing chain is `_b`.
#[test]
fn identifier_nullish_assignment_with_coalescing_rhs() {
    let source =
        "declare const obj: any;\nlet y: any, b: any;\ny ??= obj.x ?? b;\nconst x = obj?.p?.q;\n";
    let output = emit_es2017(source);
    assert!(
        output.contains("(_a = obj.x)"),
        "RHS `??` value temp must be `_a`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(_b = obj === null"),
        "optional chain must use `_b`.\nOutput:\n{output}"
    );
}
