#[test]
fn test_same_file_generic_namespace_type_stays_unqualified() {
    let source = r#"
export namespace C {
    export class A<T> {}
    export class B {}
}

export const value = null as any;
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let c_sym = binder
        .file_locals
        .get("C")
        .expect("missing namespace symbol");
    let c_symbol = binder.symbols.get(c_sym).expect("missing namespace data");
    let exports = c_symbol
        .exports
        .as_ref()
        .expect("expected namespace exports");
    let a_sym = exports.get("A").expect("missing class A symbol");
    let b_sym = exports.get("B").expect("missing class B symbol");

    let interner = TypeInterner::new();
    let a_def = tsz_solver::DefId(9101);
    let b_def = tsz_solver::DefId(9102);
    let value_type = interner.application(interner.lazy(a_def), vec![interner.lazy(b_def)]);

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.def_to_symbol.insert(a_def, a_sym);
    type_cache.def_to_symbol.insert(b_def, b_sym);

    let current_arena = Arc::new(parser.arena.clone());
    let arena_addr = Arc::as_ptr(&current_arena) as usize;
    let mut arena_to_path = FxHashMap::default();
    arena_to_path.insert(arena_addr, "test.ts".to_string());

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    emitter.set_current_arena(current_arena, "test.ts".to_string());
    emitter.set_arena_to_path(arena_to_path);
    let printed = emitter.print_type_id(value_type);

    assert!(
        printed == "C.A<C.B>",
        "Expected same-file generic type to stay local: {printed}"
    );
    assert!(
        !printed.contains("import(\"./test\").C.B"),
        "Did not expect same-file type references to be import-qualified: {printed}"
    );
}

#[test]
fn test_display_alias_preserves_generic_class_type_arguments() {
    let source = r#"
export namespace C {
    export class A<T> {}
    export class B {}
}
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let c_sym = binder
        .file_locals
        .get("C")
        .expect("missing namespace symbol");
    let c_symbol = binder.symbols.get(c_sym).expect("missing namespace data");
    let exports = c_symbol
        .exports
        .as_ref()
        .expect("expected namespace exports");
    let a_sym = exports.get("A").expect("missing class A symbol");
    let b_sym = exports.get("B").expect("missing class B symbol");

    let interner = TypeInterner::new();
    let a_def = tsz_solver::DefId(9201);
    let b_def = tsz_solver::DefId(9202);
    let app_type = interner.application(interner.lazy(a_def), vec![interner.lazy(b_def)]);
    let evaluated_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: Some(a_sym),
    });
    interner.store_display_alias(evaluated_type, app_type);

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.def_to_symbol.insert(a_def, a_sym);
    type_cache.def_to_symbol.insert(b_def, b_sym);

    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let printed = emitter.print_type_id(evaluated_type);

    assert_eq!(printed, "C.A<C.B>");
}

#[test]
fn test_type_application_elides_trailing_default_type_argument() {
    let (parser, _root) = parse_test_source("");
    let binder = BinderState::new();

    let interner = TypeInterner::new();
    let promise_def = DefId(9301);
    let resolve_atom = interner.intern_string("ResolveType");
    let reject_atom = interner.intern_string("RejectType");
    let promise_type = interner.application(
        interner.lazy(promise_def),
        vec![TypeId::STRING, TypeId::ANY],
    );

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .def_to_name
        .insert(promise_def, "TPromise".to_string());
    type_cache.def_type_params.insert(
        promise_def.0,
        vec![
            tsz_solver::types::TypeParamInfo {
                name: resolve_atom,
                constraint: None,
                default: None,
                is_const: false,
            },
            tsz_solver::types::TypeParamInfo {
                name: reject_atom,
                constraint: None,
                default: Some(TypeId::ANY),
                is_const: false,
            },
        ],
    );

    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let printed = emitter.print_type_id(promise_type);

    assert_eq!(printed, "TPromise<string>");
}

#[test]
fn test_object_literal_enum_values_preserve_typeof_and_widen_members() {
    let output = emit_dts_with_binding(
        r#"
namespace m1 {
    export enum e {
        weekday,
        weekend,
        holiday,
    }
}

var d = {
    me: { en: m1.e },
    mh: m1.e.holiday,
};
"#,
    );

    assert!(
        output.contains("en: typeof m1.e;"),
        "Expected enum object value to emit typeof enum: {output}"
    );
    assert!(
        output.contains("mh: m1.e;"),
        "Expected enum member value to widen to enum type: {output}"
    );
    assert!(
        !output.contains("mh: m1.e.holiday;"),
        "Did not expect enum member literal to leak into anonymous object type: {output}"
    );
}

#[test]
fn test_nested_namespace_enum_value_typeof_uses_relative_reference() {
    let output = emit_dts_with_binding(
        r#"
namespace A.B.C {
    export enum e {
        weekday,
        weekend,
    }
}
namespace A.B.D {
    export var d = {
        me: { en: A.B.C.e },
    };
}
"#,
    );

    assert!(
        output.contains("en: typeof B.C.e;"),
        "Expected enum object value typeof reference to be relative inside nested namespace: {output}"
    );
    assert!(
        !output.contains("en: typeof A.B.C.e;"),
        "Did not expect nested namespace typeof reference to stay fully qualified: {output}"
    );
}

#[test]
fn test_returned_auto_accessor_parameter_unknown_uses_parameter_type() {
    let source = r#"
function mixin<T extends { new (...args: any[]): {} }>(superclass: T) {
    return class extends superclass {};
}

export function wrapper<T>(value: T) {
    class BaseClass {
        accessor name = value;
    }
    return class MyClass extends mixin(BaseClass) {
        accessor name = value;
    };
}
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let wrapper = parser
        .arena
        .nodes
        .iter()
        .find_map(|node| {
            parser
                .arena
                .get_function(node)
                .filter(|func| parser.arena.get_identifier_text(func.name) == Some("wrapper"))
        })
        .expect("missing wrapper function");

    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let rewritten = emitter.rewrite_returned_auto_accessor_parameter_unknowns(
        wrapper,
        "{\n    new (): {\n        get name(): unknown;\n        set name(arg: unknown);\n    };\n}",
    );

    assert!(
        rewritten.contains("get name(): T;"),
        "Expected getter type to come from the accessor initializer parameter: {rewritten}"
    );
    assert!(
        rewritten.contains("set name(arg: T);"),
        "Expected setter type to come from the accessor initializer parameter: {rewritten}"
    );
}

/// Regression test for `declarationEmitShadowingInferNotRenamed`: a single
/// non-abstract construct signature must render as `new (...) => T` (matching
/// tsc), and an `Infer(T)` placeholder appearing inside the extends clause of
/// a conditional must render as `infer T` (not `T`, and not collapsed to a
/// `{ new(): { ... } }` object literal). Inside the conditional's true/false
/// branches the same `Infer(T)` collapses to the bare name `T`.
#[test]
fn test_constructor_with_infer_in_extends_renders_as_arrow_with_infer() {
    use tsz_solver::types::{ConditionalType, TypeParamInfo};

    let interner = TypeInterner::new();
    let t_atom = interner.intern_string("T");
    let t_param = interner.type_param(TypeParamInfo {
        name: t_atom,
        constraint: None,
        default: None,
        is_const: false,
    });
    let c_atom = interner.intern_string("C");
    let c_param_info = TypeParamInfo {
        name: c_atom,
        constraint: None,
        default: None,
        is_const: false,
    };
    let infer_c = interner.infer(c_param_info);

    // Build a non-abstract constructor type whose return is `infer C`.
    let ctor_type = interner.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature::new(Vec::new(), infer_c)],
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });

    // Build conditional `any extends (new () => infer C) ? C : never` and
    // verify both:
    //   - the extends clause renders as `new () => infer C`
    //   - the true branch references `C` as a bare name (no `infer`).
    let cond = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: ctor_type,
        true_type: infer_c,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });

    let parser = ParserState::new("test.ts".to_string(), String::new());
    let binder = BinderState::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let printed = emitter.print_type_id(cond);

    assert!(
        printed.contains("new () => infer C"),
        "Expected non-abstract single-construct callable to render as `new () => infer C` \
         when its return type is an Infer placeholder inside a conditional's extends clause: \
         {printed}"
    );
    assert!(
        !printed.contains("{\n    new (): infer C"),
        "Did not expect a single-construct callable to fall through to the \
         object-literal `{{ new (): T }}` form: {printed}"
    );
    // True branch references the same Infer placeholder; tsc prints just `C`.
    assert!(
        printed.contains("? C : "),
        "Expected the true branch to reference the inferred placeholder by bare \
         name `C`, not `infer C`: {printed}"
    );
}

#[test]
fn test_inexact_optional_mapped_intersection_simplifies_for_inferred_emit() {
    let actual = r#"(x: {} & {
    [K in "foo" | "bar" | "baz" as undefined extends {
    foo?: string;
    bar: number;
    baz: undefined;
}[keyof unknown] ? keyof unknown : never]+?: undefined extends {
        foo?: string;
        bar: number;
        baz: undefined;
    }[keyof unknown] ? {
        foo?: string;
        bar: number;
        baz: undefined;
    }[keyof unknown] | undefined : {
        foo?: string;
        bar: number;
        baz: undefined;
    }[keyof unknown];
} & {
    [K in "foo" | "bar" | "baz" as undefined extends {
    foo?: string;
    bar: number;
    baz: undefined;
}[keyof unknown] ? never : keyof unknown]: {
        foo?: string;
        bar: number;
        baz: undefined;
    }[keyof unknown];
}) => null"#;

    let simplified = DeclarationEmitter::simplify_inexact_optional_mapped_intersection_text(actual)
        .expect("expected inexact optional mapped intersection to simplify");

    assert_eq!(
        simplified,
        "(x: {\n    foo?: string | undefined;\n    baz?: undefined;\n} & {\n    bar: number;\n}) => null"
    );
}

#[test]
fn test_nested_namespace_import_equals_alias_target_stored() {
    let source = r#"
export namespace m1 {
    export namespace inner {
        export class c1 {}
    }
    import alias = inner;
}
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);

    // prepare_import_metadata triggers collect_import_metadata_from_statements,
    // which must recurse through the EXPORT_DECLARATION wrapper that the TSZ
    // parser places around `export namespace m1 { ... }`.
    emitter.prepare_import_metadata(root);

    let stored: Vec<_> = emitter.local_namespace_alias_targets.iter().collect();
    assert!(
        !stored.is_empty(),
        "local_namespace_alias_targets should be non-empty after prepare_import_metadata; \
         got nothing (EXPORT_DECLARATION wrapper was likely not traversed)"
    );

    // The alias `import alias = inner` should be listed for (m1_sym.parent, "inner").
    let m1_id = binder.file_locals.get("m1").expect("Expected 'm1' symbol");
    let inner_sym_id = binder
        .symbols
        .get(m1_id)
        .and_then(|m1_sym| m1_sym.exports.as_ref())
        .and_then(|exports| exports.get("inner"))
        .expect("Expected 'inner' symbol to be an export of m1");

    let inner_sym = binder
        .symbols
        .get(inner_sym_id)
        .expect("Expected 'inner' symbol to exist");

    let key = (inner_sym.parent, "inner".to_string());
    let alias_names = emitter.local_namespace_alias_targets.get(&key);

    assert!(
        alias_names.is_some_and(|names| names.contains("alias")),
        "Expected (inner.parent, 'inner') to include 'alias' in local_namespace_alias_targets. \
         stored keys: {stored:?}, inner_sym.parent = {:?}",
        inner_sym.parent
    );
}

#[test]
fn test_toplevel_namespace_import_equals_alias_target_stored() {
    let source = r#"
export namespace glo_M1_public {
    export class c1 {}
}
import glo_im1_private = glo_M1_public;
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);

    emitter.prepare_import_metadata(root);

    let stored: Vec<_> = emitter.local_namespace_alias_targets.iter().collect();
    assert!(
        !stored.is_empty(),
        "Expected local_namespace_alias_targets to be non-empty. Got nothing. stored: {stored:?}"
    );

    // glo_M1_public is at top-level; its parent should be SymbolId::NONE
    let glo_sym_id = binder
        .file_locals
        .get("glo_M1_public")
        .expect("Expected 'glo_M1_public' symbol");
    let glo_sym = binder
        .symbols
        .get(glo_sym_id)
        .expect("Expected 'glo_M1_public' symbol to exist");

    let key = (glo_sym.parent, "glo_M1_public".to_string());
    let alias_names = emitter.local_namespace_alias_targets.get(&key);

    assert!(
        alias_names.is_some_and(|names| names.contains("glo_im1_private")),
        "Expected (glo_M1_public.parent={:?}, 'glo_M1_public') to include 'glo_im1_private'. \
         stored: {stored:?}",
        glo_sym.parent
    );
}

#[test]
fn test_duplicate_namespace_import_equals_alias_targets_are_ambiguous() {
    let source = r#"
namespace N {
    export class C {}
}
import A = N;
import B = N;
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);

    emitter.prepare_import_metadata(root);

    let n_sym_id = binder.file_locals.get("N").expect("Expected 'N' symbol");
    let n_sym = binder
        .symbols
        .get(n_sym_id)
        .expect("Expected 'N' symbol to exist");
    let key = (n_sym.parent, "N".to_string());
    let alias_names = emitter
        .local_namespace_alias_targets
        .get(&key)
        .expect("Expected aliases for namespace N");

    assert!(
        alias_names.contains("A") && alias_names.contains("B"),
        "Expected both duplicate aliases to be tracked. aliases: {alias_names:?}"
    );
    assert_eq!(
        emitter.resolve_namespace_import_alias(n_sym_id),
        None,
        "Expected duplicate local aliases for the same namespace target to be ambiguous"
    );
}
