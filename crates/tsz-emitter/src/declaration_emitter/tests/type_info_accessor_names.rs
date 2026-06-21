use super::type_info::parse_test_source;
use super::*;
use crate::emitter::type_printer::TypePrinter;

#[test]
fn source_alias_split_accessors_preserve_setter_parameter_names() {
    let source = r#"
function makeThing() {
    type Box<U> = {
        get value(): string | U;
        set value(input: number | U);
        get other(): U;
        set other(next: U);
    }
    return null! as Box<number>;
}
"#;
    let (parser, _root) = parse_test_source(source);
    let emitter = DeclarationEmitter::new(&parser.arena);
    let setter_names = emitter.source_type_setter_parameter_names(&parser.arena, "Box<number>");
    let interner = TypeInterner::new();
    let mut value = PropertyInfo::new(interner.intern_string("value"), TypeId::STRING);
    value.write_type = TypeId::NUMBER;
    let mut other = PropertyInfo::new(interner.intern_string("other"), TypeId::STRING);
    other.write_type = TypeId::NUMBER;
    let object_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![value, other],
        string_index: None,
        number_index: None,
        symbol_index: None,
        symbol: None,
    });
    let setter_name = |name: &str| setter_names.get(name).cloned();
    let printed = TypePrinter::new(&interner)
        .with_setter_parameter_name_resolver(&setter_name)
        .print_type(object_type);

    assert!(
        printed.contains("set value(input: number)"),
        "Expected setter parameter name from source alias accessor: {printed}"
    );
    assert!(
        printed.contains("set other(next: number)"),
        "Expected renamed setter parameter from second source accessor: {printed}"
    );
    assert!(
        !printed.contains("set value(arg:") && !printed.contains("set other(arg:"),
        "Expected structural printer to use source setter names when the fact is present: {printed}"
    );
}
