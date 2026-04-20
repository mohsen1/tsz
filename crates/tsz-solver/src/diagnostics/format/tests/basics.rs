use super::*;

#[test]
fn union_null_at_end() {
    let db = TypeInterner::new();
    // Create union: null | string  (null first in storage order)
    // union_preserve_members keeps the input order in storage
    let union_id = db.union_preserve_members(vec![TypeId::NULL, TypeId::STRING]);

    let mut fmt = TypeFormatter::new(&db);
    let result = fmt.format(union_id);
    // null should appear at end, not beginning
    assert_eq!(result, "string | null");
}

#[test]
fn union_undefined_at_end() {
    let db = TypeInterner::new();
    let union_id = db.union_preserve_members(vec![TypeId::UNDEFINED, TypeId::NUMBER]);

    let mut fmt = TypeFormatter::new(&db);
    let result = fmt.format(union_id);
    assert_eq!(result, "number | undefined");
}

#[test]
fn union_null_and_undefined_at_end() {
    let db = TypeInterner::new();
    let union_id = db.union_preserve_members(vec![TypeId::NULL, TypeId::UNDEFINED, TypeId::STRING]);

    let mut fmt = TypeFormatter::new(&db);
    let result = fmt.format(union_id);
    // Non-nullish first, then null, then undefined
    assert_eq!(result, "string | null | undefined");
}

#[test]
fn union_no_nullish_unchanged() {
    let db = TypeInterner::new();
    let union_id = db.union_preserve_members(vec![TypeId::NUMBER, TypeId::STRING]);

    let mut fmt = TypeFormatter::new(&db);
    let result = fmt.format(union_id);
    // Union members are sorted by tsc's type creation order (string=8, number=9)
    assert_eq!(result, "string | number");
}

#[test]
fn needs_property_name_quotes_basic() {
    // Valid identifiers: no quotes needed
    assert!(!super::needs_property_name_quotes("foo"));
    assert!(!super::needs_property_name_quotes("_private"));
    assert!(!super::needs_property_name_quotes("$jquery"));
    assert!(!super::needs_property_name_quotes("camelCase"));
    assert!(!super::needs_property_name_quotes("PascalCase"));
    assert!(!super::needs_property_name_quotes("x"));

    // Numeric: no quotes needed
    assert!(!super::needs_property_name_quotes("0"));
    assert!(!super::needs_property_name_quotes("42"));

    // Names with hyphens/spaces/etc: quotes needed
    assert!(super::needs_property_name_quotes("data-prop"));
    assert!(super::needs_property_name_quotes("aria-label"));
    assert!(super::needs_property_name_quotes("my name"));
    assert!(super::needs_property_name_quotes(""));
}

#[test]
fn tuple_type_alias_preserved_in_format() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    // Create a tuple type: [number, string, boolean]
    let tuple_id = db.tuple(vec![
        crate::types::TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    // Register a type alias T1 = [number, string, boolean]
    let name = db.intern_string("T1");
    let info = crate::def::DefinitionInfo::type_alias(name, vec![], tuple_id);
    let def_id = def_store.register(info);
    // Map the tuple TypeId back to the alias definition so the formatter can
    // resolve tuple_id -> T1 via find_def_for_type (type_to_def index).
    def_store.register_type_to_def(tuple_id, def_id);

    // Without def_store: should show structural form
    let mut fmt = TypeFormatter::new(&db);
    let without_alias = fmt.format(tuple_id);
    assert_eq!(without_alias, "[number, string, boolean]");

    // With def_store: should show alias name
    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    let with_alias = fmt.format(tuple_id);
    assert_eq!(with_alias, "T1");
}

#[test]
fn object_type_with_hyphenated_property_quoted() {
    let db = TypeInterner::new();
    let name = db.intern_string("data-prop");
    let prop = PropertyInfo {
        name,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: crate::types::Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    };
    let obj = db.object(vec![prop]);
    let mut fmt = TypeFormatter::new(&db);
    let result = fmt.format(obj);
    assert_eq!(result, "{ \"data-prop\": boolean; }");
}

#[test]
fn mapped_type_preserves_param_name() {
    let db = TypeInterner::new();
    let mapped = db.mapped(MappedType {
        type_param: TypeParamInfo {
            name: db.intern_string("P"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: db.keyof(TypeId::STRING),
        template: TypeId::NUMBER,
        name_type: None,
        readonly_modifier: None,
        optional_modifier: None,
    });
    let mut fmt = TypeFormatter::new(&db);
    let result = fmt.format(mapped);
    assert!(
        result.contains("[P in "),
        "Expected [P in ...], got: {result}"
    );
}

#[test]
fn mapped_type_shows_optional_modifier() {
    let db = TypeInterner::new();
    let mapped = db.mapped(MappedType {
        type_param: TypeParamInfo {
            name: db.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        template: TypeId::NUMBER,
        name_type: None,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Add),
    });
    let mut fmt = TypeFormatter::new(&db);
    let result = fmt.format(mapped);
    assert!(
        result.contains("]?:"),
        "Expected ]?: in mapped type, got: {result}"
    );
}

#[test]
fn mapped_type_shows_readonly_modifier() {
    let db = TypeInterner::new();
    let mapped = db.mapped(MappedType {
        type_param: TypeParamInfo {
            name: db.intern_string("P"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        template: TypeId::NUMBER,
        name_type: None,
        readonly_modifier: Some(MappedModifier::Add),
        optional_modifier: None,
    });
    let mut fmt = TypeFormatter::new(&db);
    let result = fmt.format(mapped);
    assert!(
        result.contains("readonly [x: string]: number"),
        "Expected readonly index-signature display, got: {result}"
    );
}

// =================================================================
// Primitive type formatting
// =================================================================

#[test]
fn format_all_primitive_type_ids() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    assert_eq!(fmt.format(TypeId::NEVER), "never");
    assert_eq!(fmt.format(TypeId::UNKNOWN), "unknown");
    assert_eq!(fmt.format(TypeId::ANY), "any");
    assert_eq!(fmt.format(TypeId::VOID), "void");
    assert_eq!(fmt.format(TypeId::UNDEFINED), "undefined");
    assert_eq!(fmt.format(TypeId::NULL), "null");
    assert_eq!(fmt.format(TypeId::BOOLEAN), "boolean");
    assert_eq!(fmt.format(TypeId::NUMBER), "number");
    assert_eq!(fmt.format(TypeId::STRING), "string");
    assert_eq!(fmt.format(TypeId::BIGINT), "bigint");
    assert_eq!(fmt.format(TypeId::SYMBOL), "symbol");
    assert_eq!(fmt.format(TypeId::OBJECT), "object");
    assert_eq!(fmt.format(TypeId::FUNCTION), "Function");
    assert_eq!(fmt.format(TypeId::ERROR), "error");
}

// =================================================================
// Literal formatting
// =================================================================

#[test]
fn format_string_literal_with_special_chars() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let empty = db.literal_string("");
    assert_eq!(fmt.format(empty), "\"\"");

    let spaces = db.literal_string("hello world");
    assert_eq!(fmt.format(spaces), "\"hello world\"");
}

#[test]
fn format_number_literals() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    assert_eq!(fmt.format(db.literal_number(0.0)), "0");
    assert_eq!(fmt.format(db.literal_number(-1.0)), "-1");
    assert_eq!(fmt.format(db.literal_number(3.15)), "3.15");
    assert_eq!(fmt.format(db.literal_number(1e10)), "10000000000");
    assert_eq!(fmt.format(db.literal_number(f64::INFINITY)), "Infinity");
    assert_eq!(
        fmt.format(db.literal_number(f64::NEG_INFINITY)),
        "-Infinity"
    );
    assert_eq!(fmt.format(db.literal_number(f64::NAN)), "NaN");
}

#[test]
fn format_boolean_literals() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    assert_eq!(fmt.format(TypeId::BOOLEAN_TRUE), "true");
    assert_eq!(fmt.format(TypeId::BOOLEAN_FALSE), "false");
}

#[test]
fn format_bigint_literal() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let big = db.literal_bigint("123");
    assert_eq!(fmt.format(big), "123n");
}
