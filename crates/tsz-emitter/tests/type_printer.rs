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
