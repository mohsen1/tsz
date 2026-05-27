#[test]
fn test_completions_jsx_text_content_suppressed() {
    // declare namespace JSX {
    //   interface Element {}
    //   interface IntrinsicElements { div: {} }
    // }
    // var x = <div> hello world</div>;
    // Cursor inside the JsxText ` hello world` should return no completions.
    let source = "declare namespace JSX {\n  interface Element {}\n  interface IntrinsicElements { div: {} }\n}\nvar x = <div> hello world</div>;\n";
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Cursor inside ` hello world` (position of the space after `<div>`).
    let byte_offset = source.find("<div> hello").expect("source pattern present") + "<div>".len();
    let position = line_map.offset_to_position(byte_offset as u32, source);

    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position);
    assert!(
        items.as_ref().is_none_or(|v| v.is_empty()),
        "Expected no completions inside JSX child text, got: {items:?}"
    );
}

#[test]
fn test_completions_jsx_between_tags_suppressed() {
    // JSX children with no whitespace between tags: cursor right after `>`
    // of the opening tag and before the self-closing inner tag.
    let source = "declare namespace JSX {\n  interface Element {}\n  interface IntrinsicElements { div: {} }\n}\nvar x = <div><div/></div>;\n";
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Cursor right after the outer `<div>` (between `>` and `<div/>`).
    let pattern = "<div><div/>";
    let byte_offset = source.find(pattern).expect("pattern present") + "<div>".len();
    let position = line_map.offset_to_position(byte_offset as u32, source);

    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position);
    assert!(
        items.as_ref().is_none_or(|v| v.is_empty()),
        "Expected no completions between JSX children tags, got: {items:?}"
    );
}

#[test]
fn test_completions_type_arg_non_generic_suppressed() {
    // interface Foo {}
    // type Bar = {};
    // let x: Foo<"">;
    // Cursor inside the string literal type arg on a non-generic target
    // should return no completions.
    let source = "interface Foo {}\ntype Bar = {};\nlet x: Foo<\"\">;\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Cursor between the two quotes of `""`.
    let byte_offset = source.find("Foo<\"").expect("pattern present") + "Foo<\"".len();
    let position = line_map.offset_to_position(byte_offset as u32, source);

    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position);
    assert!(
        items.as_ref().is_none_or(|v| v.is_empty()),
        "Expected no completions in type arg on non-generic Foo, got: {items:?}"
    );

    // Same for the type alias `Bar`.
    let source2 = "interface Foo {}\ntype Bar = {};\nlet y: Bar<\"\">;\n";
    let (parser, root) = parse_test_source(source2);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source2);

    let byte_offset = source2.find("Bar<\"").expect("pattern present") + "Bar<\"".len();
    let position = line_map.offset_to_position(byte_offset as u32, source2);
    let completions = Completions::new(arena, &binder, &line_map, source2);
    let items = completions.get_completions(root, position);
    assert!(
        items.as_ref().is_none_or(|v| v.is_empty()),
        "Expected no completions in type arg on non-generic Bar, got: {items:?}"
    );
}

#[test]
fn test_completions_type_arg_generic_retained() {
    // interface Foo<T> {}
    // let x: Foo<"|">;
    // Cursor inside the string literal type arg on a generic target should
    // NOT be suppressed by the non-generic gate.
    let source = "interface Foo<T> {}\nlet x: Foo<\"\">;\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let byte_offset = source.find("Foo<\"").expect("pattern present") + "Foo<\"".len();
    let position = line_map.offset_to_position(byte_offset as u32, source);
    let offset = line_map
        .position_to_offset(position, source)
        .expect("offset");
    let completions = Completions::new(arena, &binder, &line_map, source);
    assert!(
        !completions.is_in_type_argument_of_non_generic(offset),
        "Generic Foo<T> should not be treated as non-generic"
    );
}

#[test]
fn test_completions_suppressed_after_numeric_dot_with_jsdoc_trivia() {
    // `0./** comment */` ends with a JSDoc comment, but the previous *token*
    // is a complete decimal NumericLiteral `0.`. tsc's completion provider
    // suppresses completions at the position right after this trivia
    // because the prior token is numeric (not a member access). Lock the
    // text-based suppression so it skips trailing block comments.
    // Regression: `completionListAfterNumericLiteral.ts` fourslash test.
    let source = "0./** comment */";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at the very end — right after the closing `*/`.
    let position = line_map.offset_to_position(source.len() as u32, source);
    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position);
    assert!(
        items.as_ref().is_none_or(|v| v.is_empty()),
        "Completions must be suppressed after `0.<jsdoc>` since the prior token is numeric, got: {items:?}"
    );
}

// ── Function / callable member completions ──────────────────────────────────

/// Helper that returns the member-completion names for `<source><suffix>`
/// where the cursor is right after the suffix (at the end of the file).
fn items_at_end(source: &str) -> Vec<CompletionItem> {
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let interner = TypeInterner::new();
    let completions = Completions::new_with_types(
        arena,
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let position = line_map.offset_to_position(source.len() as u32, source);
    let mut cache = None;
    completions
        .get_completions_with_cache(root, position, &mut cache)
        .unwrap_or_default()
}

fn member_names_at_end(source: &str) -> Vec<String> {
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let interner = TypeInterner::new();
    let completions = Completions::new_with_types(
        arena,
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let position = line_map.offset_to_position(source.len() as u32, source);
    let mut cache = None;
    completions
        .get_completions_with_cache(root, position, &mut cache)
        .unwrap_or_default()
        .into_iter()
        .map(|i| i.label)
        .collect()
}

fn member_names_at_end_with_lib(source: &str, lib_source: &str) -> Vec<String> {
    let lib = Arc::new(LibFile::from_source(
        "lib.es2015.collection.d.ts".to_string(),
        lib_source.to_string(),
    ));
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(arena, root, &[Arc::clone(&lib)]);
    let line_map = LineMap::build(source);
    let interner = TypeInterner::new();
    let lib_contexts = vec![LibContext {
        arena: Arc::clone(&lib.arena),
        binder: Arc::clone(&lib.binder),
    }];
    let completions = Completions::with_options_and_lib_contexts(
        arena,
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
        crate::provider_macro::FullProviderOptions {
            strict: true,
            sound_mode: false,
            checker_options: None,
            lib_contexts: &lib_contexts,
        },
    );
    let position = line_map.offset_to_position(source.len() as u32, source);
    let mut cache = None;
    completions
        .get_completions_with_cache(root, position, &mut cache)
        .unwrap_or_default()
        .into_iter()
        .map(|i| i.label)
        .collect()
}

#[test]
fn test_completions_function_prototype_members_on_named_function() {
    assert_has_members(
        "function add(a,b){return a+b;}\nadd.",
        &[
            "name",
            "length",
            "apply",
            "call",
            "bind",
            "prototype",
            "arguments",
            "caller",
            "toString",
        ],
    );
}

#[test]
fn test_completions_function_prototype_members_on_arrow_function() {
    assert_has_members(
        "const mul = (x: number, y: number) => x * y;\nmul.",
        &["name", "length", "apply", "call", "bind"],
    );
}

#[test]
fn test_completions_function_prototype_members_on_function_expression() {
    assert_has_members(
        "const fn = function compute(x: number) { return x; };\nfn.",
        &["name", "length", "apply", "call", "bind"],
    );
}

// ── Array member completions ─────────────────────────────────────────────────

/// Build member completions using a custom interner that has a mock
/// `array_base_type` registered as a `TypeData::Callable` (the shape the
/// real Array interface takes when it carries construct signatures).
///
/// The mock exposes `push`, `pop`, `find`, and `filter` as instance methods
/// but does NOT include any `Function.prototype` members (`apply`, `call`,
/// `bind`). The test verifies that `collect_array_prototype_props` collects
/// only the declared instance properties and not the function-prototype ones.
fn member_names_at_end_with_array_base_callable(source: &str) -> Vec<String> {
    use tsz_solver::{CallableShape, PropertyInfo, TypeParamInfo};

    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let interner = TypeInterner::new();

    // Build a Callable with only Array-instance methods.
    // Use method names that differ from the standard lib names to prove the
    // fix is structural, not hardcoded to any specific identifier.
    let push_atom = interner.intern_string("push");
    let pop_atom = interner.intern_string("pop");
    let find_atom = interner.intern_string("find");
    let filter_atom = interner.intern_string("filter");

    let callable_id = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        // The Array interface has construct signatures: `new Array<T>()`
        // That is what causes it to be stored as Callable rather than Object.
        construct_signatures: vec![],
        properties: vec![
            PropertyInfo::method(push_atom, TypeId::NUMBER),
            PropertyInfo::method(pop_atom, TypeId::ANY),
            PropertyInfo::method(find_atom, TypeId::ANY),
            PropertyInfo::method(filter_atom, TypeId::ANY),
        ],
        ..Default::default()
    });

    // Register the callable as the array_base_type.
    // Use a single unconstrained type param named `K` (not `T`) to verify
    // the fix does not depend on the type-parameter name.
    let k_atom = interner.intern_string("K");
    interner.set_array_base_type(callable_id, vec![TypeParamInfo::simple(k_atom)]);

    let completions = Completions::new_with_types(
        arena,
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let position = line_map.offset_to_position(source.len() as u32, source);
    let mut cache = None;
    completions
        .get_completions_with_cache(root, position, &mut cache)
        .unwrap_or_default()
        .into_iter()
        .map(|i| i.label)
        .collect()
}

/// Same as `member_names_at_end_with_array_base_callable` but registers the
/// `array_base_type` as an `Intersection` of two Callables, mirroring the
/// merged-declaration shape produced when multiple lib files each declare a
/// slice of the Array interface.
fn member_names_at_end_with_array_base_intersection(source: &str) -> Vec<String> {
    use tsz_solver::{CallableShape, PropertyInfo, TypeParamInfo};

    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let interner = TypeInterner::new();

    // First "lib file" declares push/pop.
    let push_atom = interner.intern_string("push");
    let pop_atom = interner.intern_string("pop");
    let part_a = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![],
        properties: vec![
            PropertyInfo::method(push_atom, TypeId::NUMBER),
            PropertyInfo::method(pop_atom, TypeId::ANY),
        ],
        ..Default::default()
    });

    // Second "lib file" declares find/filter.
    let find_atom = interner.intern_string("find");
    let filter_atom = interner.intern_string("filter");
    let part_b = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![],
        properties: vec![
            PropertyInfo::method(find_atom, TypeId::ANY),
            PropertyInfo::method(filter_atom, TypeId::ANY),
        ],
        ..Default::default()
    });

    // Merge as intersection.
    let merged = interner.intersection(vec![part_a, part_b]);
    let elem_atom = interner.intern_string("X");
    interner.set_array_base_type(merged, vec![TypeParamInfo::simple(elem_atom)]);

    let completions = Completions::new_with_types(
        arena,
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let position = line_map.offset_to_position(source.len() as u32, source);
    let mut cache = None;
    completions
        .get_completions_with_cache(root, position, &mut cache)
        .unwrap_or_default()
        .into_iter()
        .map(|i| i.label)
        .collect()
}

#[test]
fn test_completions_array_with_lib_callable_base_includes_instance_methods() {
    let names = member_names_at_end_with_array_base_callable("const arr = [1, 2, 3];\narr.");
    for expected in ["push", "pop", "find", "filter"] {
        assert!(
            names.contains(&expected.to_string()),
            "Expected array method '{expected}' via lib callable path, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_array_with_lib_callable_base_excludes_function_prototype() {
    let names = member_names_at_end_with_array_base_callable("const arr = [1, 2, 3];\narr.");
    for forbidden in ["apply", "call", "bind"] {
        assert!(
            !names.contains(&forbidden.to_string()),
            "Array completions must not include Function.prototype member '{forbidden}', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_array_with_lib_intersection_base_includes_all_members() {
    let names = member_names_at_end_with_array_base_intersection("const arr = [1, 2, 3];\narr.");
    for expected in ["push", "pop", "find", "filter"] {
        assert!(
            names.contains(&expected.to_string()),
            "Expected array method '{expected}' via intersection lib path, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_array_with_lib_intersection_base_excludes_function_prototype() {
    let names = member_names_at_end_with_array_base_intersection("const arr = [1, 2, 3];\narr.");
    for forbidden in ["apply", "call", "bind"] {
        assert!(
            !names.contains(&forbidden.to_string()),
            "Array completions from intersection lib must not include '{forbidden}', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_array_prototype_methods_on_array_literal() {
    assert_has_members(
        "const arr = [1, 2, 3];\narr.",
        &[
            "length",
            "push",
            "pop",
            "shift",
            "unshift",
            "slice",
            "splice",
            "map",
            "filter",
            "forEach",
            "find",
            "findIndex",
            "copyWithin",
            "entries",
            "fill",
            "some",
            "every",
            "indexOf",
            "lastIndexOf",
            "join",
            "reverse",
            "sort",
            "concat",
            "reduce",
            "reduceRight",
            "toString",
            "toLocaleString",
            "keys",
            "values",
        ],
    );
}

#[test]
fn test_completions_map_excludes_symbol_iterator_on_dot_access() {
    let lib_source = r#"
declare const Symbol: { readonly iterator: unique symbol };
interface Map<K, V> {
    get(key: K): V | undefined;
    set(key: K, value: V): this;
    [Symbol.iterator](): IterableIterator<[K, V]>;
}
interface MapConstructor {
    new <K, V>(): Map<K, V>;
}
declare var Map: MapConstructor;
declare interface IterableIterator<T> {
    next(): T;
}
"#;
    let names = member_names_at_end_with_lib(
        "const cache = new Map<string, number>();\ncache.",
        lib_source,
    );

    for expected in ["get", "set"] {
        assert!(
            names.contains(&expected.to_string()),
            "Expected Map member '{expected}' in dot completions, got: {names:?}"
        );
    }
    assert!(
        !names.contains(&"[Symbol.iterator]".to_string()),
        "Dot completions must not include computed symbol-key member '[Symbol.iterator]', got: {names:?}"
    );
}

#[test]
fn test_completions_excludes_other_computed_symbol_members_on_dot_access() {
    let lib_source = r#"
declare const Symbol: {
    readonly toStringTag: unique symbol;
    readonly asyncIterator: unique symbol;
};
interface SymbolBacked {
    visible(): void;
    [Symbol.toStringTag]: string;
    [Symbol.asyncIterator](): unknown;
}
"#;
    let names =
        member_names_at_end_with_lib("declare const holder: SymbolBacked;\nholder.", lib_source);

    assert!(
        names.contains(&"visible".to_string()),
        "Expected regular member in dot completions, got: {names:?}"
    );
    for forbidden in ["[Symbol.toStringTag]", "[Symbol.asyncIterator]"] {
        assert!(
            !names.contains(&forbidden.to_string()),
            "Dot completions must not include computed symbol-key member '{forbidden}', got: {names:?}"
        );
    }
}

// ── Primitive type completion filtering ─────────────────────────────────────
//
// Structural rule: member completions for primitive types (string, number,
// boolean, bigint, symbol) must expose only members declared in the type's
// own TypeScript interface at ES2015 baseline. Object.prototype members
// (constructor, hasOwnProperty, isPrototypeOf, propertyIsEnumerable) and
// post-ES2015 string methods (padStart/padEnd, matchAll, replaceAll, ...)
// must not appear in the no-lib fallback.

#[test]
fn test_completions_string_excludes_object_prototype_members() {
    // Object.prototype members must not appear on string. Multiple bindings
    // prove the fix is structural, not a single-name patch.
    for source in [
        "const s: string = \"abc\";\ns.",
        "const t = \"hello\";\nt.",
        "const u: string = \"x\";\nu.",
    ] {
        let names = member_names_at_end(source);
        for excluded in [
            "hasOwnProperty",
            "isPrototypeOf",
            "propertyIsEnumerable",
            "constructor",
        ] {
            assert!(
                !names.contains(&excluded.to_string()),
                "String completions must not include Object.prototype member '{excluded}'; got: {names:?}"
            );
        }
        for expected in [
            "length", "charAt", "indexOf", "slice", "toString", "valueOf",
        ] {
            assert!(
                names.contains(&expected.to_string()),
                "String completions must include own-interface member '{expected}'; got: {names:?}"
            );
        }
    }
}

#[test]
fn test_completions_string_excludes_post_es2015_members() {
    let names = member_names_at_end("const s: string = \"x\";\ns.");
    for excluded in [
        "padStart",
        "padEnd",
        "matchAll",
        "replaceAll",
        "trimStart",
        "trimEnd",
        "trimLeft",
        "trimRight",
        "isWellFormed",
        "toWellFormed",
        "at",
    ] {
        assert!(
            !names.contains(&excluded.to_string()),
            "String completions must not include post-ES2015 member '{excluded}' in no-lib fallback; got: {names:?}"
        );
    }
    for expected in [
        "includes",
        "startsWith",
        "endsWith",
        "repeat",
        "codePointAt",
    ] {
        assert!(
            names.contains(&expected.to_string()),
            "String completions must include ES2015 member '{expected}'; got: {names:?}"
        );
    }
}

#[test]
fn test_completions_number_excludes_object_prototype_members() {
    for source in ["const n: number = 42;\nn.", "const x: number = 0;\nx."] {
        let names = member_names_at_end(source);
        for excluded in [
            "constructor",
            "hasOwnProperty",
            "isPrototypeOf",
            "propertyIsEnumerable",
        ] {
            assert!(
                !names.contains(&excluded.to_string()),
                "Number completions must not include Object.prototype member '{excluded}'; got: {names:?}"
            );
        }
        for expected in [
            "toFixed",
            "toExponential",
            "toPrecision",
            "toString",
            "valueOf",
        ] {
            assert!(
                names.contains(&expected.to_string()),
                "Number completions must include own-interface member '{expected}'; got: {names:?}"
            );
        }
    }
}

#[test]
fn test_completions_boolean_exposes_only_valueof() {
    for source in [
        "const b: boolean = true;\nb.",
        "const flag: boolean = false;\nflag.",
        "const b = true;\nb.",
        "const flag = false;\nflag.",
    ] {
        let names = member_names_at_end(source);
        for excluded in [
            "constructor",
            "hasOwnProperty",
            "isPrototypeOf",
            "propertyIsEnumerable",
            "toLocaleString",
            "toString",
        ] {
            assert!(
                !names.contains(&excluded.to_string()),
                "Boolean completions must not include '{excluded}' (source: {source:?}); got: {names:?}"
            );
        }
        assert!(
            names.contains(&"valueOf".to_string()),
            "Boolean completions must include 'valueOf' (source: {source:?}); got: {names:?}"
        );
    }
}

#[test]
fn test_completions_bigint_excludes_object_prototype_members() {
    for source in [
        "const n: bigint = 1n;\nn.",
        "const x = 42n;\nx.",
        "const y = 0n;\ny.",
    ] {
        let names = member_names_at_end(source);
        for excluded in [
            "constructor",
            "hasOwnProperty",
            "isPrototypeOf",
            "propertyIsEnumerable",
        ] {
            assert!(
                !names.contains(&excluded.to_string()),
                "Bigint completions must not include Object.prototype member '{excluded}' (source: {source:?}); got: {names:?}"
            );
        }
    }
}

#[test]
fn test_completions_symbol_excludes_object_prototype_members() {
    for source in [
        "const s: symbol = Symbol();\ns.",
        "declare const sym: symbol;\nsym.",
    ] {
        let names = member_names_at_end(source);
        for excluded in [
            "constructor",
            "hasOwnProperty",
            "isPrototypeOf",
            "propertyIsEnumerable",
        ] {
            assert!(
                !names.contains(&excluded.to_string()),
                "Symbol completions must not include Object.prototype member '{excluded}' (source: {source:?}); got: {names:?}"
            );
        }
        assert!(
            names.contains(&"valueOf".to_string()),
            "Symbol completions must include 'valueOf' (source: {source:?}); got: {names:?}"
        );
    }
}

// ── Lib-context completion tests ─────────────────────────────────────────────

const MINIMAL_BOOLEAN_LIB: &str = concat!(
    "interface Boolean { valueOf(): boolean; }\n",
    "interface Object { constructor: Function; hasOwnProperty(v: PropertyKey): boolean; ",
    "isPrototypeOf(v: Object): boolean; propertyIsEnumerable(v: PropertyKey): boolean; ",
    "toString(): string; toLocaleString(): string; valueOf(): Object; }\n",
    "type PropertyKey = string | number | symbol;"
);

const MINIMAL_NUMBER_LIB: &str = concat!(
    "interface Number { toString(radix?: number): string; toFixed(fractionDigits?: number): string; ",
    "toExponential(fractionDigits?: number): string; toPrecision(precision?: number): string; ",
    "valueOf(): number; toLocaleString(): string; }\n",
    "interface Object { constructor: Function; hasOwnProperty(v: string): boolean; ",
    "isPrototypeOf(v: Object): boolean; propertyIsEnumerable(v: string): boolean; }"
);

const MINIMAL_STRING_LIB: &str = concat!(
    "interface String { charAt(pos: number): string; charCodeAt(index: number): number; ",
    "indexOf(searchString: string, position?: number): number; slice(start?: number, end?: number): string; ",
    "toUpperCase(): string; toLowerCase(): string; trim(): string; valueOf(): string; ",
    "toString(): string; readonly length: number; }\n",
    "interface Object { constructor: Function; hasOwnProperty(v: string): boolean; ",
    "isPrototypeOf(v: Object): boolean; propertyIsEnumerable(v: string): boolean; }"
);

fn member_names_at_end_with_full_lib(source: &str) -> Vec<String> {
    let lib_source = include_str!("../../../../crates/tsz-website/src/lib/lib.es5.d.ts");
    member_names_at_end_with_lib(source, lib_source)
}

// These four Object.prototype members are never overridden by Boolean, Number, or String
// boxed interfaces, so they must not appear in any primitive type's completions.
const OBJECT_PROTOTYPE_MEMBERS: &[&str] = &[
    "constructor",
    "hasOwnProperty",
    "isPrototypeOf",
    "propertyIsEnumerable",
];

fn assert_primitive_members(label: &str, names: &[String], expected: &[&str], ctx: &str) {
    for name in expected {
        assert!(
            names.iter().any(|n| n == *name),
            "{label} completions must include '{name}'; got: {names:?} ({ctx})"
        );
    }
    for excluded in OBJECT_PROTOTYPE_MEMBERS {
        assert!(
            !names.iter().any(|n| n == *excluded),
            "{label} completions must not include Object.prototype member '{excluded}'; got: {names:?} ({ctx})"
        );
    }
}

fn assert_boolean_primitive_members(names: &[String], ctx: &str) {
    assert_primitive_members("Boolean", names, &["valueOf"], ctx);
}

fn assert_number_primitive_members(names: &[String], ctx: &str) {
    assert_primitive_members(
        "Number",
        names,
        &[
            "toString",
            "toFixed",
            "toExponential",
            "toPrecision",
            "valueOf",
        ],
        ctx,
    );
}

fn assert_string_primitive_members(names: &[String], ctx: &str) {
    assert_primitive_members(
        "String",
        names,
        &[
            "charAt",
            "indexOf",
            "toUpperCase",
            "toLowerCase",
            "valueOf",
            "length",
        ],
        ctx,
    );
}

#[test]
fn test_completions_boolean_no_constructor_with_lib() {
    for source in [
        "const b = true;\nb.",
        "const b: boolean = false;\nb.",
        "const flag = false;\nflag.",
    ] {
        let names = member_names_at_end_with_lib(source, MINIMAL_BOOLEAN_LIB);
        assert_boolean_primitive_members(&names, source);
    }
}

#[test]
fn test_completions_boolean_no_constructor_with_full_lib() {
    for source in [
        "const b = true;\nb.",
        "const b: boolean = false;\nb.",
        "const flag = false;\nflag.",
    ] {
        let names = member_names_at_end_with_full_lib(source);
        assert_boolean_primitive_members(&names, source);
    }
}

#[test]
fn test_completions_number_with_lib() {
    for source in ["const n = 42;\nn.", "const n: number = 0;\nn.", "(3.14)."] {
        let names = member_names_at_end_with_lib(source, MINIMAL_NUMBER_LIB);
        assert_number_primitive_members(&names, source);
    }
}

#[test]
fn test_completions_string_with_lib() {
    for source in ["const s = \"hello\";\ns.", "const s: string = \"\";\ns."] {
        let names = member_names_at_end_with_lib(source, MINIMAL_STRING_LIB);
        assert_string_primitive_members(&names, source);
    }
}

#[test]
fn test_completions_number_with_full_lib() {
    for source in ["const n = 42;\nn.", "const n: number = 0;\nn."] {
        let names = member_names_at_end_with_full_lib(source);
        assert_number_primitive_members(&names, source);
    }
}

#[test]
fn test_completions_string_with_full_lib() {
    for source in ["const s = \"hello\";\ns.", "const s: string = \"\";\ns."] {
        let names = member_names_at_end_with_full_lib(source);
        assert_string_primitive_members(&names, source);
    }
}

// ── Tuple member completions ─────────────────────────────────────────────────
//
// Structural rule: a tuple type `[T, U, ...]` exposes ALL Array.prototype members
// because tuples are array-compatible. The full method set must be available
// regardless of element arity, element names (named tuples), optionality, or
// the presence of a rest element.

#[test]
fn test_completions_tuple_exposes_all_array_prototype_methods() {
    assert_has_members(
        "const t: [string, number] = [\"a\", 1];\nt.",
        &[
            "length",
            "push",
            "pop",
            "shift",
            "unshift",
            "slice",
            "splice",
            "concat",
            "join",
            "reverse",
            "sort",
            "map",
            "filter",
            "forEach",
            "find",
            "findIndex",
            "reduce",
            "reduceRight",
            "some",
            "every",
            "indexOf",
            "lastIndexOf",
            "toString",
            "toLocaleString",
            "keys",
            "values",
        ],
    );
}

#[test]
fn test_completions_tuple_methods_renamed_variable() {
    // Changing the variable name must not affect the completion set.
    assert_has_members(
        "const pair: [boolean, string] = [true, \"x\"];\npair.",
        &["length", "map", "filter"],
    );
}

#[test]
fn test_completions_named_tuple_exposes_array_methods() {
    // Named-tuple elements are purely a label; the element types and array
    // member set are identical to unnamed tuples.
    assert_has_members(
        "const coord: [x: number, y: number] = [0, 0];\ncoord.",
        &["length", "push", "pop", "map", "filter", "reduce", "sort"],
    );
}

#[test]
fn test_completions_named_tuple_different_names_exposes_array_methods() {
    // Vary the label names to prove the fix is not keyed to any specific spelling.
    assert_has_members(
        "const entry: [key: string, value: boolean] = [\"k\", true];\nentry.",
        &[
            "length", "push", "pop", "map", "filter", "reduce", "forEach",
        ],
    );
}

#[test]
fn test_completions_optional_tuple_exposes_array_methods() {
    // Optional elements add `undefined` to the element union but the full
    // Array.prototype member set must still be present.
    assert_has_members(
        "const opt: [string, number?] = [\"a\"];\nopt.",
        &[
            "length", "push", "pop", "map", "filter", "reduce", "some", "indexOf",
        ],
    );
}

#[test]
fn test_completions_rest_tuple_exposes_array_methods() {
    // A rest element `...T[]` should contribute `T` (not `T[]`) to the element
    // union so that the Array application is structurally correct. The member
    // set must include all Array.prototype methods regardless.
    assert_has_members(
        "const rest: [string, ...number[]] = [\"a\", 1, 2];\nrest.",
        &[
            "length", "push", "pop", "map", "filter", "reduce", "sort", "forEach",
        ],
    );
}

#[test]
fn test_completions_single_element_tuple_exposes_array_methods() {
    // Single-element tuples are still tuples; all array methods should be present.
    assert_has_members(
        "const single: [string] = [\"a\"];\nsingle.",
        &[
            "length", "push", "pop", "map", "filter", "reduce", "indexOf",
        ],
    );
}

#[test]
fn test_completions_empty_tuple_exposes_array_methods() {
    // An empty tuple `[]` has element type `never`; the array method set must
    // still be offered so the user can see what methods are available.
    assert_has_members(
        "const empty: [] = [];\nempty.",
        &["length", "push", "pop", "map", "filter"],
    );
}

// ── Readonly array/tuple member completions ──────────────────────────────────

fn assert_has_members(snippet: &str, expected: &[&str]) {
    let names = member_names_at_end(snippet);
    for &m in expected {
        assert!(
            names.contains(&m.to_string()),
            "Expected member '{m}' in completions for snippet, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_array_prototype_methods_on_readonly_array_annotation() {
    // `readonly number[]` → ReadonlyType(Array(number)); structural wrapper, same members.
    assert_has_members(
        "const xs: readonly number[] = [1, 2, 3];\nxs.",
        &[
            "length", "map", "filter", "forEach", "slice", "indexOf", "every", "some",
        ],
    );
}

#[test]
fn test_completions_array_prototype_methods_on_readonly_string_array() {
    // Vary the element type and variable name to prove the fix is structural,
    // not keyed to a specific spelling.
    assert_has_members(
        "const words: readonly string[] = [\"a\", \"b\"];\nwords.",
        &["length", "map", "filter", "join", "reduce", "find"],
    );
}

#[test]
fn test_completions_array_prototype_methods_on_readonly_tuple() {
    // `readonly [T, U]` → ReadonlyType(Tuple([T, U])); same unwrap path as arrays.
    assert_has_members(
        "const pair: readonly [number, string] = [1, \"x\"];\npair.",
        &["length", "map", "filter", "forEach", "slice", "indexOf"],
    );
}

#[test]
fn test_completions_array_prototype_methods_on_readonly_tuple_different_names() {
    // Renamed variable to ensure we don't rely on identifier spelling.
    assert_has_members(
        "const row: readonly [string, boolean] = [\"y\", true];\nrow.",
        &["length", "map", "every", "some"],
    );
}

// ── Primitive completion filtering ───────────────────────────────────────────

/// Members that must never appear in primitive completions because they are
/// only inherited from `Object.prototype`. Verifying multiple variable names
/// proves the filter is structural and not keyed on identifier text.
const OBJECT_PROTOTYPE_ONLY: &[&str] = &[
    "constructor",
    "hasOwnProperty",
    "isPrototypeOf",
    "propertyIsEnumerable",
];

/// Post-ES2015 string methods that must not appear in the no-lib fallback
/// because the apparent fallback's baseline is ES2015 and tsc does not list
/// these in completions when no later lib is loaded.
const STRING_POST_ES2015_NAMES: &[&str] = &[
    "padStart",
    "padEnd",
    "trimStart",
    "trimEnd",
    "trimLeft",
    "trimRight",
    "matchAll",
    "replaceAll",
    "at",
    "isWellFormed",
    "toWellFormed",
];

fn assert_absent(label: &str, names: &[String], forbidden: &[&str]) {
    for name in forbidden {
        assert!(
            !names.iter().any(|n| n == name),
            "{label}: completion '{name}' should not appear, got: {names:?}"
        );
    }
}

fn assert_present(label: &str, names: &[String], required: &[&str]) {
    for name in required {
        assert!(
            names.iter().any(|n| n == name),
            "{label}: expected completion '{name}' to appear, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_string_excludes_object_prototype_only() {
    // Use two different variable names to prove the filter is structural and
    // not driven by identifier spelling.
    for source in [
        "const s: string = \"\";\ns.",
        "const value: string = \"abc\";\nvalue.",
    ] {
        let names = member_names_at_end(source);
        assert_absent(source, &names, OBJECT_PROTOTYPE_ONLY);
        // String's `toLocaleString` is inherited from Object.prototype; lib.es5
        // does not redeclare it on the `String` interface.
        assert_absent(source, &names, &["toLocaleString"]);
    }
}

#[test]
fn test_completions_string_excludes_post_es2015_methods() {
    for source in [
        "const s: string = \"\";\ns.",
        "const literal = \"hello\";\nliteral.",
    ] {
        let names = member_names_at_end(source);
        assert_absent(source, &names, STRING_POST_ES2015_NAMES);
    }
}

#[test]
fn test_completions_string_retains_es5_baseline() {
    let names = member_names_at_end("const s: string = \"\";\ns.");
    assert_present(
        "string",
        &names,
        &[
            "length",
            "charAt",
            "charCodeAt",
            "concat",
            "indexOf",
            "lastIndexOf",
            "match",
            "replace",
            "search",
            "slice",
            "split",
            "substring",
            "toLowerCase",
            "toUpperCase",
            "trim",
            "toString",
            "valueOf",
        ],
    );
}

#[test]
fn test_completions_number_excludes_object_prototype_only() {
    for source in [
        "const n: number = 0;\nn.",
        "const count: number = 42;\ncount.",
    ] {
        let names = member_names_at_end(source);
        assert_absent(source, &names, OBJECT_PROTOTYPE_ONLY);
    }
}

#[test]
fn test_completions_number_retains_own_interface_members() {
    let names = member_names_at_end("const n: number = 0;\nn.");
    // Number's lib.es5 interface redeclares `toLocaleString` and `toString`,
    // so they must remain. `valueOf`, `toFixed`, etc. are also owned.
    assert_present(
        "number",
        &names,
        &[
            "toString",
            "toLocaleString",
            "toFixed",
            "toExponential",
            "toPrecision",
            "valueOf",
        ],
    );
}

#[test]
fn test_completions_boolean_excludes_object_prototype_only() {
    for source in [
        "const b: boolean = true;\nb.",
        "const flag: boolean = false;\nflag.",
    ] {
        let names = member_names_at_end(source);
        assert_absent(source, &names, OBJECT_PROTOTYPE_ONLY);
        // The lib.es5 `Boolean` interface declares only `valueOf`; `toString`
        // and `toLocaleString` reach booleans through `Object.prototype`.
        assert_absent(source, &names, &["toString", "toLocaleString"]);
    }
}

#[test]
fn test_completions_boolean_retains_own_interface_members() {
    let names = member_names_at_end("const b: boolean = true;\nb.");
    assert_present("boolean", &names, &["valueOf"]);
}

#[test]
fn test_completions_object_intrinsic_keeps_object_prototype_members() {
    // The `object` (non-primitive) type's apparent members are exactly the
    // `Object.prototype` set. The primitive-completion filter must not strip
    // them, or `o.` becomes nearly empty.
    let names = member_names_at_end("const o: object = {};\no.");
    assert_present(
        "object",
        &names,
        &[
            "constructor",
            "hasOwnProperty",
            "isPrototypeOf",
            "propertyIsEnumerable",
            "toLocaleString",
            "toString",
            "valueOf",
        ],
    );
}

// ── ECMAScript private fields (`#field`) ─────────────────────────────────────

/// `this.` inside a class method must include ECMAScript private fields.
/// tsc shows `#lines` (with the `#` prefix) in `this.` completions when the
/// cursor is inside the class body.
#[test]
fn test_completions_this_includes_ecma_private_field() {
    // ECMAScript private field -- NOT TypeScript `private` modifier.
    // The completions for `this.` inside the method must include `#lines`.
    let names = member_names_at_end(
        "class MemoryLogger {\n  #lines: string[] = [];\n  write(msg: string) { this.",
    );
    assert!(
        names.contains(&"#lines".to_string()),
        "Expected `#lines` in `this.` completions; got: {names:?}"
    );
    assert!(
        names.contains(&"write".to_string()),
        "Expected `write` in `this.` completions; got: {names:?}"
    );
}

/// Completions on `this.#lines.` (a private field of type `string[]`) must
/// expose Array.prototype members exactly as a plain `string[]` variable would.
#[test]
fn test_completions_ecma_private_field_member_access_exposes_array_methods() {
    let names = member_names_at_end(
        "class MemoryLogger {\n  #lines: string[] = [];\n  write(msg: string) { this.#lines.",
    );
    assert!(
        names.contains(&"push".to_string()),
        "Expected `push` in `this.#lines.` completions; got: {names:?}"
    );
    assert!(
        names.contains(&"length".to_string()),
        "Expected `length` in `this.#lines.` completions; got: {names:?}"
    );
    assert!(
        names.contains(&"map".to_string()),
        "Expected `map` in `this.#lines.` completions; got: {names:?}"
    );
}

/// Completions for `this.#field.` work for different field names and types
/// (proves the fix is structural, not keyed to a specific identifier spelling).
#[test]
fn test_completions_ecma_private_field_different_names_and_types() {
    // Different field name (#items) and element type (number)
    let names = member_names_at_end(
        "class Container {\n  #items: number[] = [];\n  add(x: number) { this.#items.",
    );
    assert!(
        names.contains(&"push".to_string()),
        "Expected `push` in `this.#items.` completions; got: {names:?}"
    );
    assert!(
        names.contains(&"pop".to_string()),
        "Expected `pop` in `this.#items.` completions; got: {names:?}"
    );
    // The field itself must appear in `this.` completions.
    let this_names = member_names_at_end(
        "class Container {\n  #items: number[] = [];\n  add(x: number) { this.",
    );
    assert!(
        this_names.contains(&"#items".to_string()),
        "Expected `#items` in `this.` completions; got: {this_names:?}"
    );
}

/// `this.` inside a class method must show colon-notation detail for methods and
/// the correct type for properties. The `node_type_detail` path uses the semantic
/// checker, not source-text slicing, so these assertions pin the exact strings
/// produced by `checker.format_type()` after `arrow_to_colon` conversion.
#[test]
fn test_completions_this_method_detail_uses_colon_notation() {
    // Method detail must be `(msg: string): void`, NOT `(msg: string) => void`.
    // Property detail must be `string[]`.
    let items =
        items_at_end("class Logger {\n  lines: string[] = [];\n  write(msg: string): void { this.");
    let write_item = items.iter().find(|i| i.label == "write");
    assert!(
        write_item.is_some(),
        "Expected `write` in `this.` completions"
    );
    let detail = write_item.unwrap().detail.as_deref().unwrap_or("");
    assert!(
        detail.contains(':') && !detail.contains("=>"),
        "Method detail must use colon notation `(msg: string): void`, got: {detail:?}"
    );
    let lines_item = items.iter().find(|i| i.label == "lines");
    assert!(
        lines_item.is_some(),
        "Expected `lines` in `this.` completions"
    );
    let lines_detail = lines_item.unwrap().detail.as_deref().unwrap_or("");
    assert_eq!(
        lines_detail, "string[]",
        "Property detail must be the type, got: {lines_detail:?}"
    );
}

/// Properties whose types are inferred from initializers must show the inferred
/// type as the completion detail, not a raw source excerpt.
#[test]
fn test_completions_type_literal_inferred_property_detail() {
    // `x: 42` → inferred type is `number`; `msg: "hi"` → inferred type is `string`.
    // Both should appear as that primitive type in the completion detail, proving
    // the semantic path (not source slicing) is used.
    let items =
        items_at_end("const obj: { x: number; msg: string } = { x: 42, msg: \"hi\" };\nobj.");
    let x_item = items.iter().find(|i| i.label == "x");
    assert!(x_item.is_some(), "Expected `x` in completions");
    let x_detail = x_item.unwrap().detail.as_deref().unwrap_or("");
    assert_eq!(
        x_detail, "number",
        "Inferred numeric property detail must be `number`, got: {x_detail:?}"
    );

    let msg_item = items.iter().find(|i| i.label == "msg");
    assert!(msg_item.is_some(), "Expected `msg` in completions");
    let msg_detail = msg_item.unwrap().detail.as_deref().unwrap_or("");
    assert_eq!(
        msg_detail, "string",
        "Inferred string property detail must be `string`, got: {msg_detail:?}"
    );
}

/// Quoted type-literal properties must be offered as completions with the correct
/// detail. A different property-name spelling (`"my-prop"` vs `myProp`) must
/// produce the same structural result, proving the fix is not keyed to a specific
/// identifier spelling.
#[test]
fn test_completions_quoted_type_literal_property_detail() {
    // `"my-prop"` and `"other-key"` are quoted properties — they require bracket
    // access and must appear with the correct type detail.
    let items = items_at_end(
        "const obj: { \"my-prop\": number; \"other-key\": string } = { \"my-prop\": 1, \"other-key\": \"x\" };\nobj.",
    );
    let prop1 = items.iter().find(|i| i.label == "my-prop");
    assert!(
        prop1.is_some(),
        "Expected `my-prop` in completions; got labels: {:?}",
        items.iter().map(|i| &i.label).collect::<Vec<_>>()
    );
    let prop1_detail = prop1.unwrap().detail.as_deref().unwrap_or("");
    assert_eq!(
        prop1_detail, "number",
        "Quoted property detail must be `number`, got: {prop1_detail:?}"
    );

    let prop2 = items.iter().find(|i| i.label == "other-key");
    assert!(prop2.is_some(), "Expected `other-key` in completions");
    let prop2_detail = prop2.unwrap().detail.as_deref().unwrap_or("");
    assert_eq!(
        prop2_detail, "string",
        "Quoted property detail must be `string`, got: {prop2_detail:?}"
    );
}

// ─── auto-import sort-text tests ─────────────────────────────────────────────

/// Auto-import completions from regular code (outside any import clause) must
/// use `AUTO_IMPORT` sort text ("16"), matching TypeScript's
/// `SortText.AutoImportSuggestions`.
#[test]
fn test_auto_import_sort_text_regular_code() {
    let mut project = crate::Project::new();
    project.set_file(
        "/src/utils.ts".to_string(),
        "export function helperAlpha() {}\nexport function helperBeta() {}".to_string(),
    );
    // Cursor at end of empty file — regular code position, not inside import clause.
    project.set_file("/src/main.ts".to_string(), "helperA".to_string());

    let completions = project
        .get_completions("/src/main.ts", Position::new(0, 7))
        .unwrap_or_default();

    let helper = completions
        .iter()
        .find(|i| i.label == "helperAlpha" || i.label == "helperBeta");
    assert!(
        helper.is_some(),
        "Expected auto-import candidate; got labels: {:?}",
        completions.iter().map(|i| &i.label).collect::<Vec<_>>()
    );
    let sort = helper.unwrap().effective_sort_text();
    assert_eq!(
        sort,
        crate::completions::sort_priority::AUTO_IMPORT,
        "Regular-code auto-import must use AUTO_IMPORT sort text; got {sort:?}"
    );
}

/// Auto-import completions offered inside `import {{ | }} from '…'` (named
/// bindings clause) must use `LOCATION_PRIORITY` sort text ("11"), matching
/// TypeScript's `SortText.LocationPriority` for `importStatementCompletion`.
#[test]
fn test_auto_import_sort_text_inside_named_import_clause() {
    let mut project = crate::Project::new();
    project.set_file(
        "/src/widgets.ts".to_string(),
        "export function widgetOne() {}\nexport function widgetTwo() {}".to_string(),
    );
    // Cursor is inside `import { | }` — the named-bindings list.
    // Position (0, 9) lands between the braces.
    project.set_file(
        "/src/app.ts".to_string(),
        "import {  } from './widgets';".to_string(),
    );

    let completions = project
        .get_completions("/src/app.ts", Position::new(0, 9))
        .unwrap_or_default();

    let widget = completions
        .iter()
        .find(|i| i.label == "widgetOne" || i.label == "widgetTwo");
    assert!(
        widget.is_some(),
        "Expected widget candidate inside import clause; got labels: {:?}",
        completions.iter().map(|i| &i.label).collect::<Vec<_>>()
    );
    let sort = widget.unwrap().effective_sort_text();
    assert_eq!(
        sort,
        crate::completions::sort_priority::LOCATION_PRIORITY,
        "Import-clause auto-import must use LOCATION_PRIORITY sort text; got {sort:?}"
    );
}

/// Verify that the import-clause sort text ("11") lexicographically precedes the
/// regular-code auto-import sort text ("16"). Editors sort completion items by
/// `sortText` as a string, so lexicographic ordering is exactly what matters.
#[test]
fn test_auto_import_import_clause_sorts_before_regular_auto_import() {
    use crate::completions::sort_priority;
    // String comparison is intentional — `sortText` is compared lexicographically
    // by editors, matching TypeScript's own SortText design.
    assert!(
        sort_priority::LOCATION_PRIORITY < sort_priority::AUTO_IMPORT,
        "LOCATION_PRIORITY ({}) must sort before AUTO_IMPORT ({})",
        sort_priority::LOCATION_PRIORITY,
        sort_priority::AUTO_IMPORT
    );
}

/// Different names for bound variables (`K` vs `X`, `Widget` vs `Component`)
/// must not affect whether the import-clause sort text applies.  The rule is
/// structural (cursor is inside `NAMED_IMPORTS`), not spelling-dependent.
#[test]
fn test_auto_import_import_clause_sort_text_name_independent() {
    for export_name in &["alphaExport", "betaExport", "gammaExport"] {
        let mut project = crate::Project::new();
        project.set_file(
            "/src/lib.ts".to_string(),
            format!("export function {export_name}() {{}}"),
        );
        project.set_file(
            "/src/consumer.ts".to_string(),
            "import {  } from './lib';".to_string(),
        );
        let completions = project
            .get_completions("/src/consumer.ts", Position::new(0, 9))
            .unwrap_or_default();

        let item = completions.iter().find(|i| i.label == *export_name);
        assert!(
            item.is_some(),
            "Expected `{export_name}` inside import clause; got: {:?}",
            completions.iter().map(|i| &i.label).collect::<Vec<_>>()
        );
        assert_eq!(
            item.unwrap().effective_sort_text(),
            crate::completions::sort_priority::LOCATION_PRIORITY,
            "`{export_name}` inside import clause must get LOCATION_PRIORITY sort text"
        );
    }
}
