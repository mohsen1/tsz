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
