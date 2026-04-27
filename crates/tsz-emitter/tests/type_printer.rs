use super::*;

#[test]
fn test_primitive_types() {
    // For now we can't easily test without a real TypeInterner
    // In the future we'll need to set up a mock or test fixture
    assert!(TypeId::STRING.is_intrinsic());
    assert!(TypeId::NUMBER.is_intrinsic());
    assert!(TypeId::BOOLEAN.is_intrinsic());
}

#[test]
fn object_type_flat_format_without_indent() {
    let interner = tsz_solver::TypeInterner::new();
    let name_x = interner.intern_string("x");

    let obj = interner.object(vec![tsz_solver::types::PropertyInfo::new(
        name_x,
        TypeId::NUMBER,
    )]);

    // Without indent_level, uses flat single-line format
    let printer = TypePrinter::new(&interner);
    let result = printer.print_type(obj);
    assert_eq!(result, "{ x: number }");
}

#[test]
fn object_type_multiline_format_at_indent_zero() {
    let interner = tsz_solver::TypeInterner::new();
    let name_x = interner.intern_string("x");

    let obj = interner.object(vec![tsz_solver::types::PropertyInfo::new(
        name_x,
        TypeId::NUMBER,
    )]);

    // With indent_level(0), formats multi-line for .d.ts output
    let printer = TypePrinter::new(&interner).with_indent_level(0);
    let result = printer.print_type(obj);
    assert_eq!(result, "{\n    x: number;\n}");
}

#[test]
fn object_type_multiline_format_at_indent_one() {
    let interner = tsz_solver::TypeInterner::new();
    let name_x = interner.intern_string("x");

    let obj = interner.object(vec![tsz_solver::types::PropertyInfo::new(
        name_x,
        TypeId::BOOLEAN,
    )]);

    // At indent level 1, members indented 8 spaces, closing brace 4 spaces
    let printer = TypePrinter::new(&interner).with_indent_level(1);
    let result = printer.print_type(obj);
    assert_eq!(result, "{\n        x: boolean;\n    }");
}

#[test]
fn object_type_multiline_readonly_property() {
    let interner = tsz_solver::TypeInterner::new();
    let name_p = interner.intern_string("primaryPath");

    let obj = interner.object(vec![tsz_solver::types::PropertyInfo::readonly(
        name_p,
        TypeId::ANY,
    )]);

    let printer = TypePrinter::new(&interner).with_indent_level(0);
    let result = printer.print_type(obj);
    assert_eq!(result, "{\n    readonly primaryPath: any;\n}");
}

/// Regression: `readonly [...T]` must print with the tuple spread preserved,
/// not collapsed to `readonly T`. Mirrors tsc diagnostic output for variadic
/// tuple parameters like `r: readonly [...T]` in
/// `conformance/types/tuple/variadicTuples1.ts`.
#[test]
fn readonly_spread_tuple_prints_spread_syntax() {
    use tsz_solver::types::{TupleElement, TypeData, TypeParamInfo};

    let interner = tsz_solver::TypeInterner::new();
    let unknown_array = interner.array(TypeId::UNKNOWN);
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(unknown_array),
        default: None,
        is_const: false,
        variance: tsz_solver::TypeParamVariance::None,
    }));

    let spread_tuple = interner.tuple(vec![TupleElement {
        type_id: t_param,
        name: None,
        optional: false,
        rest: true,
    }]);
    let readonly_spread = interner.intern(TypeData::ReadonlyType(spread_tuple));

    let printer = TypePrinter::new(&interner);
    assert_eq!(printer.print_type(spread_tuple), "[...T]");
    assert_eq!(printer.print_type(readonly_spread), "readonly [...T]");
}

#[test]
fn object_type_nested_multiline() {
    let interner = tsz_solver::TypeInterner::new();
    let name_inner = interner.intern_string("inner");
    let name_a = interner.intern_string("a");

    // Create inner object: { a: number }
    let inner_obj = interner.object(vec![tsz_solver::types::PropertyInfo::new(
        name_a,
        TypeId::NUMBER,
    )]);

    // Create outer object: { inner: { a: number } }
    let outer_obj = interner.object(vec![tsz_solver::types::PropertyInfo::new(
        name_inner, inner_obj,
    )]);

    let printer = TypePrinter::new(&interner).with_indent_level(0);
    let result = printer.print_type(outer_obj);
    assert_eq!(result, "{\n    inner: {\n        a: number;\n    };\n}");
}

#[test]
fn empty_object_type_stays_inline() {
    let interner = tsz_solver::TypeInterner::new();
    let obj = interner.object(vec![]);

    // Even with indent_level set, empty objects stay as {}
    let printer = TypePrinter::new(&interner).with_indent_level(0);
    let result = printer.print_type(obj);
    assert_eq!(result, "{}");
}

#[test]
fn local_import_alias_uses_bare_name_when_alias_is_emitted() {
    let interner = tsz_solver::TypeInterner::new();
    let mut symbols = SymbolArena::new();
    let thing = symbols.alloc(symbol_flags::ALIAS, "Thing".to_string());
    symbols
        .get_mut(thing)
        .expect("missing alias symbol")
        .import_module = Some("pkg".to_string());

    let module_path_resolver = |sym_id| (sym_id == thing).then(|| "pkg".to_string());
    let alias_name_resolver = |sym_id| sym_id == thing;

    let printer = TypePrinter::new(&interner)
        .with_symbols(&symbols)
        .with_module_path_resolver(&module_path_resolver)
        .with_local_import_alias_name_resolver(&alias_name_resolver);

    assert_eq!(
        printer.print_named_symbol_reference(thing, false),
        Some("Thing".to_string())
    );
}

#[test]
fn local_import_alias_falls_back_to_import_qualified_name_when_elided() {
    let interner = tsz_solver::TypeInterner::new();
    let mut symbols = SymbolArena::new();
    let thing = symbols.alloc(symbol_flags::ALIAS, "Thing".to_string());
    symbols
        .get_mut(thing)
        .expect("missing alias symbol")
        .import_module = Some("inner/other.js".to_string());

    let module_path_resolver = |sym_id| (sym_id == thing).then(|| "inner/other".to_string());
    let alias_name_resolver = |_sym_id| false;

    let printer = TypePrinter::new(&interner)
        .with_symbols(&symbols)
        .with_module_path_resolver(&module_path_resolver)
        .with_local_import_alias_name_resolver(&alias_name_resolver);

    assert_eq!(
        printer.print_named_symbol_reference(thing, false),
        Some("import(\"inner/other\").Thing".to_string())
    );
}

#[test]
fn string_literal_type_escapes_double_quotes() {
    let interner = tsz_solver::TypeInterner::new();
    let lit_type = interner.literal_string("he said \"hi\"");

    let printer = TypePrinter::new(&interner);
    let result = printer.print_type(lit_type);
    assert_eq!(
        result, "\"he said \\\"hi\\\"\"",
        "String literal types must escape double quotes"
    );
}

#[test]
fn string_literal_type_escapes_newline() {
    let interner = tsz_solver::TypeInterner::new();
    let lit_type = interner.literal_string("line1\nline2");

    let printer = TypePrinter::new(&interner);
    let result = printer.print_type(lit_type);
    assert_eq!(
        result, "\"line1\\nline2\"",
        "String literal types must escape newlines"
    );
}

#[test]
fn string_literal_type_escapes_backslash() {
    let interner = tsz_solver::TypeInterner::new();
    let lit_type = interner.literal_string("back\\slash");

    let printer = TypePrinter::new(&interner);
    let result = printer.print_type(lit_type);
    assert_eq!(
        result, "\"back\\\\slash\"",
        "String literal types must escape backslashes"
    );
}

#[test]
fn string_literal_type_escapes_tab() {
    let interner = tsz_solver::TypeInterner::new();
    let lit_type = interner.literal_string("col1\tcol2");

    let printer = TypePrinter::new(&interner);
    let result = printer.print_type(lit_type);
    assert_eq!(
        result, "\"col1\\tcol2\"",
        "String literal types must escape tabs"
    );
}

#[test]
fn quoted_property_name_escapes_special_chars() {
    let interner = tsz_solver::TypeInterner::new();
    // Create a property with a name containing a double quote
    let name = interner.intern_string("say \"hi\"");
    let obj = interner.object(vec![tsz_solver::types::PropertyInfo::new(
        name,
        TypeId::STRING,
    )]);

    let printer = TypePrinter::new(&interner);
    let result = printer.print_type(obj);
    assert!(
        result.contains("\"say \\\"hi\\\"\""),
        "Property names with quotes must be escaped: {result}"
    );
}

#[test]
fn index_access_parenthesizes_union_container() {
    let interner = tsz_solver::TypeInterner::new();
    // Create (string | number)[K] -- the union container needs parens
    let union_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let key_str = interner.literal_string("length");
    let idx_access = interner.index_access(union_type, key_str);

    let printer = TypePrinter::new(&interner);
    let result = printer.print_type(idx_access);
    assert!(
        result.starts_with("(string | number)["),
        "Union in indexed access object position needs parens: {result}"
    );
}

#[test]
fn external_module_symbol_is_not_treated_as_global() {
    let interner = tsz_solver::TypeInterner::new();
    let mut symbols = SymbolArena::new();
    let thing = symbols.alloc(0, "Thing".to_string());

    let module_path_resolver = |sym_id| (sym_id == thing).then(|| "inner/other".to_string());

    let printer = TypePrinter::new(&interner)
        .with_symbols(&symbols)
        .with_module_path_resolver(&module_path_resolver);

    assert_eq!(
        printer.print_named_symbol_reference(thing, false),
        Some("import(\"inner/other\").Thing".to_string())
    );
}

#[test]
fn reserved_keyword_property_names_are_not_quoted() {
    let interner = tsz_solver::TypeInterner::new();
    // In ES5+ and TypeScript, reserved words are valid property names
    // and tsc emits them unquoted in .d.ts output.
    let name_delete = interner.intern_string("delete");
    let name_class = interner.intern_string("class");
    let name_for = interner.intern_string("for");

    let obj = interner.object(vec![
        tsz_solver::types::PropertyInfo::new(name_class, TypeId::STRING),
        tsz_solver::types::PropertyInfo::new(name_delete, TypeId::BOOLEAN),
        tsz_solver::types::PropertyInfo::new(name_for, TypeId::NUMBER),
    ]);

    let printer = TypePrinter::new(&interner);
    let result = printer.print_type(obj);
    assert!(
        result.contains("delete: boolean"),
        "Reserved keyword 'delete' should not be quoted: {result}"
    );
    assert!(
        result.contains("class: string"),
        "Reserved keyword 'class' should not be quoted: {result}"
    );
    assert!(
        result.contains("for: number"),
        "Reserved keyword 'for' should not be quoted: {result}"
    );
}

#[test]
fn mapped_type_multiline_format_with_indent() {
    let interner = tsz_solver::TypeInterner::new();

    let param_name = interner.intern_string("K");
    let mapped = interner.mapped(tsz_solver::types::MappedType {
        type_param: tsz_solver::types::TypeParamInfo {
            name: param_name,
            constraint: None,
            default: None,
            is_const: false,
            variance: tsz_solver::TypeParamVariance::None,
        },
        constraint: TypeId::STRING,
        template: TypeId::NUMBER,
        name_type: None,
        readonly_modifier: None,
        optional_modifier: None,
    });

    // With indent context, mapped types use multi-line format (matching tsc)
    let printer = TypePrinter::new(&interner).with_indent_level(0);
    let result = printer.print_type(mapped);
    assert_eq!(
        result, "{\n    [K in string]: number;\n}",
        "Mapped type with indent should be multi-line"
    );
}
