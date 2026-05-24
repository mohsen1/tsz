use super::type_info::parse_test_source;
use super::*;

// =============================================================================
// Anonymous constructor object type body — property initializer syntax
// =============================================================================
//
// Structural rule: when a function returns a class expression and the
// declaration emit synthesizes an anonymous constructor object type
// (`{ new (...args: any[]): { ...members... } }`), the body of that
// constructor object type is an *object type literal*, not a class
// declaration. Object type literals do not permit `name = value`
// initializer syntax — only `name: T` annotation syntax. Member emit
// must follow object-type-literal rules in that context.
//
// These tests cover at least two literal kinds (string, number, boolean)
// and two binding-variable names so the structural fix is not keyed on
// a particular identifier or literal value spelling.

#[test]
fn test_anon_ctor_object_type_readonly_string_literal_uses_colon_not_eq() {
    let output = emit_dts_with_usage_analysis(
        r#"
type Constructor<T = {}> = new (...args: any[]) => T;
function Tagged<B extends Constructor>(Base: B) {
    return class extends Base {
        readonly tag = "hello";
    };
}
class Item {}
export const TaggedItem = Tagged(Item);
"#,
    );

    assert!(
        output.contains("readonly tag: \"hello\""),
        "Expected `readonly tag: \"hello\"` colon form in anonymous constructor object type: {output}"
    );
    assert!(
        !output.contains("readonly tag = "),
        "Expected no `=` initializer form in object type literal: {output}"
    );
}

#[test]
fn test_anon_ctor_object_type_readonly_number_literal_uses_colon_not_eq() {
    let output = emit_dts_with_usage_analysis(
        r#"
type Constructor<T = {}> = new (...args: any[]) => T;
function Stamped<C extends Constructor>(Source: C) {
    return class extends Source {
        readonly version = 42;
        readonly count = 0;
    };
}
class Doc {}
export const StampedDoc = Stamped(Doc);
"#,
    );

    assert!(
        output.contains("readonly version: 42"),
        "Expected number literal `readonly version: 42` colon form: {output}"
    );
    assert!(
        output.contains("readonly count: 0"),
        "Expected number literal `readonly count: 0` colon form: {output}"
    );
    assert!(
        !output.contains("readonly version = ") && !output.contains("readonly count = "),
        "Expected no `=` initializer form for object-type-literal numeric properties: {output}"
    );
}

#[test]
fn test_anon_ctor_object_type_readonly_boolean_literal_uses_colon_not_eq() {
    let output = emit_dts_with_usage_analysis(
        r#"
type Constructor<T = {}> = new (...args: any[]) => T;
function Flagged<TBase extends Constructor>(Base: TBase) {
    return class extends Base {
        readonly enabled = true;
        readonly hidden = false;
    };
}
class Widget {}
export const FlaggedWidget = Flagged(Widget);
"#,
    );

    assert!(
        output.contains("readonly enabled: true"),
        "Expected `readonly enabled: true` colon form: {output}"
    );
    assert!(
        output.contains("readonly hidden: false"),
        "Expected `readonly hidden: false` colon form: {output}"
    );
    assert!(
        !output.contains("readonly enabled = ") && !output.contains("readonly hidden = "),
        "Expected no `=` initializer form for object-type-literal boolean properties: {output}"
    );
}

#[test]
fn test_anon_ctor_object_type_static_readonly_literal_uses_colon_not_eq() {
    // Static members emitted into the constructor object type's outer
    // intersection arm (`{ new(...): { ... }; readonly STATIC_NAME: ... }`)
    // are also in object-type-literal context and must use `:` form.
    let output = emit_dts_with_usage_analysis(
        r#"
type Constructor<T = {}> = new (...args: any[]) => T;
function Branded<B extends Constructor>(Base: B) {
    return class extends Base {
        static readonly BRAND = "MyBrand";
        static readonly VERSION = 1;
    };
}
class Thing {}
export const BrandedThing = Branded(Thing);
"#,
    );

    assert!(
        output.contains("readonly BRAND: \"MyBrand\""),
        "Expected static `readonly BRAND: \"MyBrand\"` colon form: {output}"
    );
    assert!(
        output.contains("readonly VERSION: 1"),
        "Expected static `readonly VERSION: 1` colon form: {output}"
    );
    assert!(
        !output.contains("readonly BRAND = ") && !output.contains("readonly VERSION = "),
        "Expected no `=` initializer form for static members in object type literal: {output}"
    );
}

#[test]
fn test_top_level_class_declaration_still_uses_eq_initializer_form() {
    // Negative case: regular class declarations (not inside an anonymous
    // constructor object type) must still emit `readonly name = value`
    // to match tsc's class-declaration emit, so the fix does not over-apply.
    let output = emit_dts_with_usage_analysis(
        r#"
export class Direct {
    readonly tag = "hello";
    readonly version = 42;
    readonly enabled = true;
    static readonly BRAND = "MyBrand";
}
"#,
    );

    assert!(
        output.contains("readonly tag = \"hello\""),
        "Expected top-level class to keep `readonly tag = \"hello\"` initializer form: {output}"
    );
    assert!(
        output.contains("readonly version = 42"),
        "Expected top-level class to keep `readonly version = 42` initializer form: {output}"
    );
    assert!(
        output.contains("readonly enabled = true"),
        "Expected top-level class to keep `readonly enabled = true` initializer form: {output}"
    );
    assert!(
        output.contains("readonly BRAND = \"MyBrand\""),
        "Expected top-level class to keep `static readonly BRAND = \"MyBrand\"`: {output}"
    );
}

fn build_abstract_constructor_with_index_sig(
    interner: &TypeInterner,
    method_name: &str,
) -> tsz_solver::TypeId {
    let args = interner.intern_string("args");
    let method = interner.intern_string(method_name);
    let x = interner.intern_string("x");
    let void_fn = interner.function(FunctionShape::new(Vec::new(), TypeId::VOID));
    let instance_shape = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![PropertyInfo::method(method, void_fn)],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: Some(x),
        }),
        number_index: None,
        symbol: None,
    });
    interner.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature::new(
            vec![ParamInfo {
                name: Some(args),
                type_id: interner.array(TypeId::ANY),
                optional: false,
                rest: true,
            }],
            instance_shape,
        )],
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: true,
    })
}

fn find_call_by_callee(
    arena: &tsz_parser::parser::node::NodeArena,
    callee_name: &str,
) -> NodeIndex {
    arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            if node.kind != syntax_kind_ext::CALL_EXPRESSION {
                return None;
            }
            let call = arena.get_call_expr(node)?;
            (arena.get_identifier_text(call.expression) == Some(callee_name))
                .then_some(NodeIndex(idx as u32))
        })
        .unwrap_or_else(|| panic!("missing call expression for callee `{callee_name}`"))
}

#[test]
fn test_mixin_call_prefers_solver_type_for_index_signature() {
    // When the checker's type cache has a TypeId for a mixin call expression,
    // the emitter must use it directly. Text-based reconstruction cannot reproduce
    // call-site refinements like `[x: string]: any` from abstract constructor constraints.
    let source = r#"
type AbstractConstructor<T = {}> = abstract new (...args: any[]) => T;
function Mixin<TBase extends AbstractConstructor>(base: TBase) {
    abstract class Mixed extends base {
        abstract mixinMethod(): void;
    }
    return Mixed;
}
abstract class AbstractBase {}
export const C = Mixin(AbstractBase);
"#;
    let (parser, root) = parse_test_source(source);
    let call_idx = find_call_by_callee(&parser.arena, "Mixin");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let solver_type = build_abstract_constructor_with_index_sig(&interner, "mixinMethod");

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(call_idx.0, solver_type);

    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let type_text = emitter
        .call_expression_source_return_type_text(call_idx)
        .expect("expected call return type text");

    assert!(
        type_text.contains("[x: string]: any"),
        "Solver TypeId with index signature should be used when cached: {type_text}"
    );
    assert!(
        type_text.contains("mixinMethod"),
        "Own instance members must be preserved alongside the index signature: {type_text}"
    );
}

#[test]
fn test_mixin_call_solver_path_is_name_independent() {
    // The solver-preference path keys on TypeId, not type-parameter spelling.
    // Renaming `TBase` to `T` or `K` must not change whether the index signature appears.
    for tparam in ["TBase", "T", "K"] {
        let source = format!(
            r#"
type AC = abstract new (...args: any[]) => any;
function M<{tparam} extends AC>(base: {tparam}) {{
    abstract class Mix extends base {{ abstract m(): void; }}
    return Mix;
}}
abstract class B {{}}
export const C = M(B);
"#
        );
        let (parser, root) = parse_test_source(&source);
        let call_idx = find_call_by_callee(&parser.arena, "M");

        let mut binder = BinderState::new();
        binder.bind_source_file(&parser.arena, root);

        let interner = TypeInterner::new();
        let solver_type = build_abstract_constructor_with_index_sig(&interner, "m");

        let mut type_cache = crate::type_cache_view::TypeCacheView::default();
        type_cache.node_types.insert(call_idx.0, solver_type);

        let emitter =
            DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
        let type_text = emitter
            .call_expression_source_return_type_text(call_idx)
            .expect("expected call return type text");

        assert!(
            type_text.contains("[x: string]: any"),
            "Type param name `{tparam}` should not affect index-signature emission: {type_text}"
        );
    }
}

#[cfg(test)]
mod no_implicit_this_tests {
    use super::*;

    // Structural rule: when a non-static method in an object literal body only
    // returns `this`, the `this` type is a circular self-reference that cannot be
    // represented in DTS.  tsc elides it as `/*elided*/ any`.  tsz must match.
    //
    // Tests use at least two different method-name spellings to prove the fix
    // is structural, not tied to a specific identifier.

    #[test]
    fn test_object_literal_method_returning_this_emits_elided_any() {
        // Three methods named func1/func2/func3 all returning `this`.
        let source = r#"
function createObj() {
    return {
        func1() {
            return this;
        },
        func2() {
            return this;
        },
        func3() {
            return this;
        }
    };
}
"#;
        let result = emit_dts_with_binding(source);
        // Must be compact — the solver's recursive expansion would be 5000+ lines.
        let lines = result.lines().count();
        assert!(
            lines < 30,
            "Expected compact DTS (< 30 lines), got {lines} lines:\n{result}"
        );
        // Each method must use /*elided*/ any, not a recursive object expansion.
        assert!(
            result.contains("func1(): /*elided*/ any"),
            "Expected 'func1(): /*elided*/ any' but got:\n{result}"
        );
        assert!(
            result.contains("func2(): /*elided*/ any"),
            "Expected 'func2(): /*elided*/ any' but got:\n{result}"
        );
    }

    #[test]
    fn test_object_literal_method_returning_this_different_names() {
        // Same structural shape, different method names (greet / respond / reset).
        let source = r#"
function makeHandler() {
    return {
        greet() {
            return this;
        },
        respond() {
            return this;
        },
        reset() {
            return this;
        }
    };
}
"#;
        let result = emit_dts_with_binding(source);
        let lines = result.lines().count();
        assert!(
            lines < 30,
            "Expected compact DTS (< 30 lines), got {lines} lines:\n{result}"
        );
        assert!(
            result.contains("greet(): /*elided*/ any"),
            "Expected 'greet(): /*elided*/ any' but got:\n{result}"
        );
        assert!(
            result.contains("respond(): /*elided*/ any"),
            "Expected 'respond(): /*elided*/ any' but got:\n{result}"
        );
    }

    #[test]
    fn test_class_method_returning_this_still_emits_this() {
        // In a class body, `this` is the polymorphic instance type and must
        // remain `this` in DTS (not be replaced with `any`).
        let source = r#"
export class Builder {
    build() {
        return this;
    }
    reset() {
        return this;
    }
}
"#;
        let result = emit_dts_with_binding(source);
        assert!(
            result.contains("build(): this"),
            "Expected 'build(): this' for class method but got:\n{result}"
        );
        assert!(
            result.contains("reset(): this"),
            "Expected 'reset(): this' for class method but got:\n{result}"
        );
        // Must NOT use /*elided*/ any in a class context.
        assert!(
            !result.contains("build(): /*elided*/ any"),
            "Class method must not use /*elided*/ any but got:\n{result}"
        );
    }
}
