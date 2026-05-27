#[test]
fn type_formatter_cache_statistics_account_for_atom_cache_entries_and_size() {
    let db = TypeInterner::new();
    let atom = db.intern_string("cachedName");
    let mut fmt = TypeFormatter::new(&db);

    let empty_stats = fmt.cache_statistics();
    assert_eq!(empty_stats.atom_cache_entries, 0);
    assert!(empty_stats.estimated_size_bytes > 0);

    assert_eq!(&*fmt.atom(atom), "cachedName");
    let populated_stats = fmt.cache_statistics();
    assert_eq!(populated_stats.atom_cache_entries, 1);
    assert!(populated_stats.estimated_size_bytes >= empty_stats.estimated_size_bytes);

    assert_eq!(&*fmt.atom(atom), "cachedName");
    assert_eq!(
        fmt.cache_statistics().atom_cache_entries,
        populated_stats.atom_cache_entries
    );
}

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
fn union_registered_to_nominal_interface_formats_structurally() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let primitive_key_union = db.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]);
    let audio_data = crate::def::DefinitionInfo::interface(
        db.intern_string("AudioData"),
        vec![],
        vec![PropertyInfo::new(
            db.intern_string("duration"),
            TypeId::NUMBER,
        )],
    );
    let audio_data_def = def_store.register(audio_data);
    def_store.register_type_to_def(primitive_key_union, audio_data_def);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(
        fmt.format(primitive_key_union),
        "string | number | symbol",
        "Nominal interface registrations must not repaint structural unions"
    );
}

#[test]
fn primitive_key_union_registered_to_type_alias_formats_structurally_without_origin() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let primitive_key_union = db.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]);
    let alias_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("AudioData"),
        vec![],
        primitive_key_union,
    ));
    def_store.register_type_to_def(primitive_key_union, alias_def);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(
        fmt.format(primitive_key_union),
        "string | number | symbol",
        "The shared `keyof any` union must not be repainted by same-body aliases"
    );
}

#[test]
fn primitive_key_union_formats_as_property_key_in_diagnostic_mode() {
    let db = TypeInterner::new();
    let primitive_key_union = db.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]);

    let mut fmt = TypeFormatter::new(&db).with_diagnostic_mode();
    assert_eq!(fmt.format(primitive_key_union), "PropertyKey");
}

#[test]
fn primitive_key_union_formats_structurally_when_alias_collapse_is_opted_out() {
    // tsc strips the `aliasSymbol` from the constraint type before formatting
    // the TS2344 message, so opt-in callers (the constraint-not-satisfied
    // emitter) get the structural form. The default diagnostic surface still
    // collapses to `PropertyKey`; the opt-in is narrow and intentional.
    let db = TypeInterner::new();
    let primitive_key_union = db.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]);

    let mut fmt = TypeFormatter::new(&db)
        .with_diagnostic_mode()
        .with_expanded_primitive_key_union();
    assert_eq!(fmt.format(primitive_key_union), "string | number | symbol");
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
        is_symbol_named: false,
        single_quoted_name: false,
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
fn format_number_literal_uses_scientific_notation_above_1e21() {
    // Match `Number.prototype.toString()` for very large/small magnitudes
    // (regression: previously used Rust's default `f64` Display which
    // expanded `5.462437423415177e+244` to a 245-digit integer string,
    // causing `octalIntegerLiteralES6.ts` TS7053 message mismatches).
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    // Borderline: 1e21 itself is the threshold where tsc switches to
    // scientific notation.
    assert_eq!(fmt.format(db.literal_number(1e21)), "1e+21");
    assert_eq!(
        fmt.format(db.literal_number(5.462_437_423_415_177e244)),
        "5.462437423415177e+244"
    );
    // Very small finite values use negative exponent form.
    assert_eq!(fmt.format(db.literal_number(1e-7)), "1e-7");
}

#[test]
fn number_literal_union_uses_tsc_allocation_order() {
    let db = TypeInterner::new();
    let one = db.literal_number(1.0);
    let minus_one = db.literal_number(-1.0);
    let two = db.literal_number(2.0);
    let zero = db.literal_number(0.0);

    let union = db.union(vec![minus_one, zero, one, two]);

    let mut fmt = TypeFormatter::new(&db);
    assert_eq!(fmt.format(union), "0 | 1 | -1 | 2");
}

#[test]
fn number_literal_union_is_not_numeric_sorted() {
    let db = TypeInterner::new();
    let two = db.literal_number(2.0);
    let one = db.literal_number(1.0);

    let union = db.union(vec![one, two]);

    let mut fmt = TypeFormatter::new(&db);
    assert_eq!(fmt.format(union), "2 | 1");
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

// =================================================================
// Union formatting
// =================================================================

#[test]
fn format_union_two_members() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let union = db.union2(TypeId::STRING, TypeId::NUMBER);
    let result = fmt.format(union);
    assert!(result.contains("string"));
    assert!(result.contains("number"));
    assert!(result.contains(" | "));
}

#[test]
fn format_union_three_members() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let union = db.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    let result = fmt.format(union);
    assert!(result.contains("string"));
    assert!(result.contains("number"));
    assert!(result.contains("boolean"));
    // Should have exactly 2 "|" separators
    assert_eq!(result.matches(" | ").count(), 2);
}

#[test]
fn format_union_with_literal_members() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let s1 = db.literal_string("a");
    let s2 = db.literal_string("b");
    let union = db.union2(s1, s2);
    let result = fmt.format(union);
    assert!(result.contains("\"a\""));
    assert!(result.contains("\"b\""));
    assert!(result.contains(" | "));
}

#[test]
fn format_union_named_construct_callable_without_parentheses() {
    let db = TypeInterner::new();
    let mut symbols = tsz_binder::SymbolArena::new();
    let sym_id = symbols.alloc(tsz_binder::symbol_flags::INTERFACE, "ConstructableA".into());

    let constructable = db.callable(CallableShape {
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::ANY,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: Some(sym_id),
        is_abstract: false,
    });

    let union = db.union2(constructable, TypeId::STRING);
    let mut fmt = TypeFormatter::with_symbols(&db, &symbols);
    let rendered = fmt.format(union);
    assert!(rendered.contains("ConstructableA"));
    assert!(rendered.contains("string"));
    assert!(!rendered.contains("(ConstructableA)"));
}

#[test]
fn format_union_of_intersections_factors_common_type_parameter() {
    let db = TypeInterner::new();
    let t = db.type_param(TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let two = db.literal_number(2.0);
    let one = db.literal_number(1.0);
    let zero = db.literal_number(0.0);

    let union = db.union(vec![
        db.intersection2(t, zero),
        db.intersection2(t, one),
        db.intersection2(t, two),
    ]);
    let mut fmt = TypeFormatter::new(&db);

    assert_eq!(fmt.format(union), "T & (0 | 2 | 1)");
}

#[test]
fn format_union_of_intersections_display_order_independent_of_alloc_order() {
    // Regression: the previous code round-tripped through `interner.union()`,
    // which re-sorts by alloc-order and discards the numeric display sort.
    // When `one` is interned before `two` (lower alloc-order), that round-trip
    // would produce `0 | 1 | 2` instead of the correct `0 | 2 | 1`.
    // The sibling test `format_union_of_intersections_factors_common_type_parameter`
    // happened to pass even with the bug because it interned `two` before `one`.
    let db = TypeInterner::new();
    let k = db.type_param(TypeParamInfo {
        name: db.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let one = db.literal_number(1.0); // interned first → lower alloc-order than two
    let two = db.literal_number(2.0);
    let zero = db.literal_number(0.0);

    let union = db.union(vec![
        db.intersection2(k, zero),
        db.intersection2(k, two),
        db.intersection2(k, one),
    ]);
    let mut fmt = TypeFormatter::new(&db);
    assert_eq!(fmt.format(union), "K & (0 | 2 | 1)");
}

#[test]
fn format_union_of_intersections_does_not_factor_different_common_parts() {
    let db = TypeInterner::new();
    let t = db.type_param(TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let u = db.type_param(TypeParamInfo {
        name: db.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    });

    let union = db.union(vec![
        db.intersection2(t, db.literal_number(0.0)),
        db.intersection2(u, db.literal_number(1.0)),
    ]);
    let mut fmt = TypeFormatter::new(&db);
    let rendered = fmt.format(union);

    assert!(rendered.contains("(T & 0)"), "got: {rendered}");
    assert!(rendered.contains("(U & 1)"), "got: {rendered}");
}

#[test]
fn format_large_union_truncation() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    // Create a union with more members than max_union_members (default: 10)
    let members: Vec<TypeId> = (0..15).map(|i| db.literal_number(i as f64)).collect();
    let union = db.union_preserve_members(members);
    let result = fmt.format(union);
    // Should truncate with "..."
    assert!(
        result.contains("..."),
        "Large union should be truncated, got: {result}"
    );
}

// =================================================================
// Intersection formatting
// =================================================================

#[test]
fn format_intersection_two_type_params() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let t = db.type_param(TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let u = db.type_param(TypeParamInfo {
        name: db.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let inter = db.intersection2(t, u);
    let result = fmt.format(inter);
    assert!(result.contains("T"));
    assert!(result.contains("U"));
    assert!(result.contains(" & "));
}

#[test]
fn format_intersection_uses_display_properties_for_anonymous_object_member() {
    let db = TypeInterner::new();
    let foo_prop = db.intern_string("fooProp");
    let widened = PropertyInfo::new(foo_prop, TypeId::STRING);
    let display = PropertyInfo::new(foo_prop, db.literal_string("frizzlebizzle"));
    let fresh = db
        .factory()
        .object_fresh_with_display(vec![widened], vec![display]);
    let t = db.type_param(TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    });

    let intersection = db.intersection2(fresh, t);
    let mut fmt = TypeFormatter::new(&db).with_display_properties();
    let result = fmt.format(intersection);

    assert!(
        result.contains("{ fooProp: \"frizzlebizzle\"; }"),
        "Expected fresh-object display properties inside intersection, got: {result}"
    );
    assert!(result.contains(" & "));
}

#[test]
fn format_intersection_preserves_anonymous_objects() {
    // tsc's `typeToString` preserves the intersection form (`A & B`) for
    // IntersectionType values, even when every member is an anonymous object
    // literal type. A merged single-object display is only produced when the
    // type is already stored as a single object (e.g. via spread/apparent-type
    // computation). See intersectionsAndOptionalProperties.ts and
    // jsxEmptyExpressionNotCountedAsChild2.tsx for cases that depend on this.
    let db = TypeInterner::new();

    let a_prop = PropertyInfo::new(db.intern_string("a"), TypeId::NULL);
    let b_prop = PropertyInfo::new(db.intern_string("b"), TypeId::STRING);

    let obj_a = db.factory().object(vec![a_prop]);
    let obj_b = db.factory().object(vec![b_prop]);

    let intersection = db.intersection2(obj_a, obj_b);
    let mut fmt = TypeFormatter::new(&db);
    let result = fmt.format(intersection);

    assert!(
        result.contains(" & "),
        "Intersection of anonymous objects should keep `&` display, got: {result}"
    );
    assert!(
        result.contains("a: null") && result.contains("b: string"),
        "Intersection display should contain both members' properties, got: {result}"
    );
}

#[test]
fn format_intersection_drops_redundant_index_signature_member() {
    let db = TypeInterner::new();

    let index_sig = crate::types::IndexSignature {
        key_type: TypeId::NUMBER,
        value_type: TypeId::STRING,
        readonly: true,
        param_name: None,
    };
    let with_props = db.object_with_index(crate::types::ObjectShape {
        properties: vec![
            PropertyInfo::new(db.intern_string("a"), TypeId::NUMBER),
            PropertyInfo::new(db.intern_string("b"), TypeId::NUMBER),
        ],
        string_index: None,
        number_index: Some(index_sig),
        symbol: None,
        flags: Default::default(),
    });
    let index_only = db.object_with_index(crate::types::ObjectShape {
        properties: vec![],
        string_index: None,
        number_index: Some(index_sig),
        symbol: None,
        flags: Default::default(),
    });

    let intersection = db.intersection2(with_props, index_only);
    let mut fmt = TypeFormatter::new(&db);
    let result = fmt.format(intersection);

    assert!(
        result.contains("readonly [x: number]: string"),
        "Expected retained index signature, got: {result}"
    );
    assert!(
        result.contains("a: number") && result.contains("b: number"),
        "Expected named properties to remain, got: {result}"
    );
    assert!(
        !result.contains(" & "),
        "Expected redundant index-only member to be removed, got: {result}"
    );
}

#[test]
fn format_intersection_preserves_named_types() {
    // Intersections with named types (type params) should NOT be flattened
    let db = TypeInterner::new();

    let a_prop = PropertyInfo::new(db.intern_string("a"), TypeId::NULL);
    let obj_a = db.factory().object(vec![a_prop]);
    let t = db.type_param(TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    });

    let intersection = db.intersection2(obj_a, t);
    let mut fmt = TypeFormatter::new(&db);
    let result = fmt.format(intersection);

    // Should preserve intersection form: `{ a: null; } & T`
    assert!(
        result.contains(" & "),
        "Intersection with type param should not be flattened, got: {result}"
    );
}

#[test]
fn capitalize_primitive_intersection_members_number() {
    // tsc shows `Number` (capitalized) for `number` members in intersections
    let db = TypeInterner::new();
    let brand_prop = PropertyInfo::new(db.intern_string("__brand"), TypeId::STRING);
    let obj = db.factory().object(vec![brand_prop]);
    let intersection = db.intersection2(TypeId::NUMBER, obj);

    let mut fmt = TypeFormatter::new(&db).with_capitalize_primitive_intersection_members();
    let result = fmt.format(intersection);
    assert!(
        result.starts_with("Number"),
        "Primitive member `number` should be capitalized to `Number` in intersections, got: {result}"
    );
}

#[test]
fn capitalize_primitive_intersection_members_string() {
    let db = TypeInterner::new();
    let brand_prop = PropertyInfo::new(db.intern_string("tag"), TypeId::NUMBER);
    let obj = db.factory().object(vec![brand_prop]);
    let intersection = db.intersection2(TypeId::STRING, obj);

    let mut fmt = TypeFormatter::new(&db).with_capitalize_primitive_intersection_members();
    let result = fmt.format(intersection);
    assert!(
        result.starts_with("String"),
        "Primitive member `string` should be capitalized to `String`, got: {result}"
    );
}

#[test]
fn skip_application_alias_for_intersections_expands_branded_primitive() {
    // When skip_application_alias_for_intersections is set, an Intersection
    // whose display_alias points to an Application should show the structural form.
    let db = TypeInterner::new();
    let brand_prop = PropertyInfo::new(db.intern_string("__brand"), TypeId::STRING);
    let obj = db.factory().object(vec![brand_prop]);
    let intersection = db.intersection2(TypeId::NUMBER, obj);

    // Simulate Brand<string> → number & { __brand: string } with display_alias
    let app = db.application(db.lazy(crate::def::DefId(1)), vec![TypeId::STRING]);
    db.store_display_alias(intersection, app);

    // Without flag: follows alias and would format the application
    // With flag: shows structural intersection instead
    let mut fmt = TypeFormatter::new(&db)
        .with_skip_application_alias_for_intersections()
        .with_capitalize_primitive_intersection_members();
    let result = fmt.format(intersection);
    assert!(
        result.contains(" & "),
        "Should show structural intersection, not application alias, got: {result}"
    );
    assert!(
        result.starts_with("Number"),
        "Primitive member should be capitalized, got: {result}"
    );
    assert!(
        result.contains("__brand"),
        "Object member should be visible, got: {result}"
    );
}

// =================================================================
// Object type formatting
// =================================================================

#[test]
fn format_empty_object() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let obj = db.object(vec![]);
    assert_eq!(fmt.format(obj), "{}");
}

#[test]
fn format_object_single_property() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let obj = db.object(vec![PropertyInfo::new(
        db.intern_string("x"),
        TypeId::NUMBER,
    )]);
    assert_eq!(fmt.format(obj), "{ x: number; }");
}

#[test]
fn format_object_multiple_properties() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let obj = db.object(vec![
        PropertyInfo::new(db.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(db.intern_string("y"), TypeId::STRING),
    ]);
    let result = fmt.format(obj);
    assert!(result.contains("x: number"));
    assert!(result.contains("y: string"));
}

#[test]
fn format_object_readonly_property() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let mut prop = PropertyInfo::new(db.intern_string("x"), TypeId::NUMBER);
    prop.readonly = true;
    let obj = db.object(vec![prop]);
    let result = fmt.format(obj);
    assert!(
        result.contains("readonly x: number"),
        "Expected 'readonly x: number', got: {result}"
    );
}

#[test]
fn format_object_many_properties_truncated() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    // tsc starts truncating large object displays (roughly 22+ members),
    // preserving a long head and the tail property.
    let props: Vec<PropertyInfo> = (1..=24)
        .map(|i| PropertyInfo::new(db.intern_string(&format!("p{i}")), TypeId::NUMBER))
        .collect();
    let obj = db.object(props);
    let result = fmt.format(obj);
    assert!(
        result.contains("... 6 more ..."),
        "Expected omitted-count marker for large object, got: {result}"
    );
    assert!(
        result.contains("p24: number"),
        "Expected tail property preservation in truncated object display, got: {result}"
    );
}

#[test]
fn format_object_hides_duplicate_internal_default_alias() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let shared = TypeId::NUMBER;
    let obj = db.object(vec![
        PropertyInfo::new(db.intern_string("default"), shared),
        PropertyInfo::new(db.intern_string("_default"), shared),
        PropertyInfo::new(db.intern_string("value"), TypeId::STRING),
    ]);
    let result = fmt.format(obj);

    assert!(
        result.contains("default: number"),
        "Expected real default export to remain visible, got: {result}"
    );
    assert!(
        !result.contains("_default"),
        "Expected duplicate internal `_default` alias to be hidden, got: {result}"
    );
    assert!(
        result.contains("value: string"),
        "Expected unrelated properties to remain visible, got: {result}"
    );
}

#[test]
fn format_object_keeps_distinct_internal_default_alias() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let obj = db.object(vec![
        PropertyInfo::new(db.intern_string("default"), TypeId::NUMBER),
        PropertyInfo::new(db.intern_string("_default"), TypeId::STRING),
    ]);
    let result = fmt.format(obj);

    assert!(
        result.contains("_default: string"),
        "Expected `_default` to remain when it is not a duplicate of `default`, got: {result}"
    );
}

#[test]
fn format_object_with_string_index_signature() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let shape = crate::types::ObjectShape {
        properties: vec![],
        string_index: Some(crate::types::IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol: None,
        flags: Default::default(),
    };
    let obj = db.object_with_index(shape);
    let result = fmt.format(obj);
    assert!(
        result.contains("[x: string]: number"),
        "Expected string index signature with default param name 'x', got: {result}"
    );
}

#[test]
fn format_object_with_index_hides_duplicate_internal_default_alias() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let shape = crate::types::ObjectShape {
        properties: vec![
            PropertyInfo::new(db.intern_string("default"), TypeId::NUMBER),
            PropertyInfo::new(db.intern_string("_default"), TypeId::NUMBER),
        ],
        string_index: Some(crate::types::IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol: None,
        flags: Default::default(),
    };
    let obj = db.object_with_index(shape);
    let result = fmt.format(obj);

    assert!(
        result.contains("[x: string]: number"),
        "Expected index signature to remain visible, got: {result}"
    );
    assert!(
        result.contains("default: number"),
        "Expected real default export to remain visible, got: {result}"
    );
    assert!(
        !result.contains("_default"),
        "Expected duplicate internal `_default` alias to be hidden in object-with-index display, got: {result}"
    );
}

#[test]
fn format_object_with_number_index_signature() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let shape = crate::types::ObjectShape {
        properties: vec![],
        string_index: None,
        number_index: Some(crate::types::IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        symbol: None,
        flags: Default::default(),
    };
    let obj = db.object_with_index(shape);
    let result = fmt.format(obj);
    assert!(
        result.contains("[x: number]: string"),
        "Expected number index signature with default param name 'x', got: {result}"
    );
}

#[test]
fn format_object_with_readonly_number_index_signature() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let shape = crate::types::ObjectShape {
        properties: vec![],
        string_index: None,
        number_index: Some(crate::types::IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: true,
            param_name: None,
        }),
        symbol: None,
        flags: Default::default(),
    };
    let obj = db.object_with_index(shape);
    let result = fmt.format(obj);
    assert!(
        result.contains("readonly [x: number]: string"),
        "Expected readonly number index signature, got: {result}"
    );
}

#[test]
fn format_object_with_readonly_string_index_signature() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let shape = crate::types::ObjectShape {
        properties: vec![],
        string_index: Some(crate::types::IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
        number_index: None,
        symbol: None,
        flags: Default::default(),
    };
    let obj = db.object_with_index(shape);
    let result = fmt.format(obj);
    assert!(
        result.contains("readonly [x: string]: number"),
        "Expected readonly string index signature, got: {result}"
    );
}

#[test]
fn format_object_with_symbol_index_signature() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    // Symbol index signatures are stored in the string_index field with key_type == SYMBOL.
    // The printer must display `symbol` (not `string`) as the key type.
    for param_name in [None, Some("key"), Some("sym")] {
        let shape = crate::types::ObjectShape {
            properties: vec![],
            string_index: Some(crate::types::IndexSignature {
                key_type: TypeId::SYMBOL,
                value_type: TypeId::STRING,
                readonly: false,
                param_name: param_name.map(|n| db.intern_string(n)),
            }),
            number_index: None,
            symbol: None,
            flags: Default::default(),
        };
        let obj = db.object_with_index(shape);
        let result = fmt.format(obj);
        assert!(
            result.contains("]: string") && result.contains(": symbol]"),
            "Expected symbol index signature display (param={param_name:?}), got: {result}"
        );
        assert!(
            !result.contains(": string]"),
            "Must not display 'string' as the index key type for a symbol index sig (param={param_name:?}), got: {result}"
        );
    }
}

#[test]
fn format_object_with_index_many_properties_truncated() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let mut props: Vec<PropertyInfo> = (1..=20)
        .map(|i| PropertyInfo::new(db.intern_string(&format!("p{i}")), TypeId::NUMBER))
        .collect();
    let mut tail = PropertyInfo::new(
        db.intern_string("[Symbol.unscopables]"),
        db.object(vec![PropertyInfo::new(
            db.intern_string("a"),
            TypeId::NUMBER,
        )]),
    );
    tail.readonly = true;
    props.push(tail);

    let shape = crate::types::ObjectShape {
        properties: props,
        string_index: None,
        number_index: Some(crate::types::IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        symbol: None,
        flags: Default::default(),
    };
    let obj = db.object_with_index(shape);
    let result = fmt.format(obj);
    assert!(
        result.contains("... 4 more ..."),
        "Expected omitted-count marker for indexed object truncation, got: {result}"
    );
    assert!(
        result.contains("readonly [Symbol.unscopables]:"),
        "Expected tail symbol property preservation in indexed-object truncation, got: {result}"
    );
}

#[test]
fn format_object_with_index_prefers_symbol_tail_over_later_string_member() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let mut props: Vec<PropertyInfo> = (1..=24)
        .map(|i| PropertyInfo::new(db.intern_string(&format!("p{i}")), TypeId::NUMBER))
        .collect();
    let mut symbol_tail = PropertyInfo::new(
        db.intern_string("[Symbol.unscopables]"),
        db.object(vec![PropertyInfo::new(
            db.intern_string("a"),
            TypeId::NUMBER,
        )]),
    );
    symbol_tail.readonly = true;
    props.push(symbol_tail);
    props.push(PropertyInfo::new(db.intern_string("flat"), TypeId::NUMBER));

    let shape = crate::types::ObjectShape {
        properties: props,
        string_index: None,
        number_index: Some(crate::types::IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        symbol: None,
        flags: Default::default(),
    };
    let obj = db.object_with_index(shape);
    let result = fmt.format(obj);
    assert!(
        result.contains("readonly [Symbol.unscopables]:"),
        "Expected indexed-object truncation to preserve the last symbol-named member, got: {result}"
    );
    assert!(
        result.contains("readonly [Symbol.unscopables]: { ...; }"),
        "Expected preserved symbol tail to collapse nested object detail, got: {result}"
    );
    assert!(
        !result.contains("flat: number"),
        "Expected later string members to be omitted when a symbol tail is preserved, got: {result}"
    );
}

#[test]
fn format_object_with_symbol_index_signature_renders_symbol_key_type() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    // { [key: symbol]: string } — symbol-indexed type.
    // The `key_type` field stores TypeId::SYMBOL; the formatter must use it
    // rather than hardcoding "string" based on the storage slot name.
    let shape = crate::types::ObjectShape {
        properties: vec![],
        string_index: Some(crate::types::IndexSignature {
            key_type: TypeId::SYMBOL,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: Some(db.intern_string("key")),
        }),
        number_index: None,
        symbol: None,
        flags: Default::default(),
    };
    let obj = db.object_with_index(shape);
    let result = fmt.format(obj);
    assert!(
        result.contains("[key: symbol]: string"),
        "Expected symbol index signature to render as '[key: symbol]: ...', got: {result}"
    );
    assert!(
        !result.contains("[key: string]"),
        "Must not render symbol index as '[key: string]', got: {result}"
    );
}

#[test]
fn format_array_like_object_with_index_expands_to_locale_string_overload_display() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);
    let method = db.function(FunctionShape::new(vec![], TypeId::STRING));
    let includes = db.function(FunctionShape::new(
        vec![ParamInfo {
            name: Some(db.intern_string("searchElement")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        TypeId::BOOLEAN,
    ));
    let mut unscopables =
        PropertyInfo::new(db.intern_string("[Symbol.unscopables]"), TypeId::OBJECT);
    unscopables.readonly = true;

    let shape = crate::types::ObjectShape {
        properties: vec![
            PropertyInfo::new(db.intern_string("toString"), method),
            PropertyInfo::new(db.intern_string("toLocaleString"), method),
            PropertyInfo::new(db.intern_string("includes"), includes),
            unscopables,
        ],
        string_index: None,
        number_index: Some(crate::types::IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
        symbol: None,
        flags: Default::default(),
    };
    let obj = db.object_with_index(shape);
    let result = fmt.format(obj);

    assert!(
        result.contains("toLocaleString: { (): string; (locales: string | string[], options?: (NumberFormatOptions & DateTimeFormatOptions) | undefined): string; }"),
        "Expected Array toLocaleString overload display, got: {result}"
    );
}

#[test]
fn format_array_like_object_without_symbol_tail_preserves_array_display_shape() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);
    let method = db.function(FunctionShape::new(vec![], TypeId::STRING));
    let includes = db.function(FunctionShape::new(
        vec![ParamInfo {
            name: Some(db.intern_string("searchElement")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        TypeId::BOOLEAN,
    ));

    let mut properties = vec![
        PropertyInfo::new(db.intern_string("toString"), method),
        PropertyInfo::new(db.intern_string("toLocaleString"), method),
        PropertyInfo::new(db.intern_string("includes"), includes),
    ];
    properties.extend(
        (1..=27).map(|idx| PropertyInfo::new(db.intern_string(&format!("p{idx}")), TypeId::NUMBER)),
    );
    properties.push(PropertyInfo::new(db.intern_string("reduceRight"), method));

    let shape = crate::types::ObjectShape {
        properties,
        string_index: None,
        number_index: Some(crate::types::IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        symbol: None,
        flags: Default::default(),
    };
    let obj = db.object_with_index(shape);
    let result = fmt.format(obj);

    assert!(
        result.contains("toLocaleString: { (): string; (locales: string | string[], options?: (NumberFormatOptions & DateTimeFormatOptions) | undefined): string; }"),
        "Expected Array toLocaleString overload display, got: {result}"
    );
    assert!(
        result.contains("... 30 more ..."),
        "Expected tsc-style omitted count for array-like display, got: {result}"
    );
    assert!(
        result.contains("readonly [Symbol.unscopables]: { ...; }"),
        "Expected synthetic array symbol tail for truncated mapped-array display, got: {result}"
    );
    assert!(
        !result.contains("reduceRight:"),
        "Expected later string members to remain omitted behind the symbol tail, got: {result}"
    );
}

// =================================================================
// Function type formatting
// =================================================================

#[test]
fn format_function_no_params() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let func = db.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let result = fmt.format(func);
    assert_eq!(result, "() => void");
}

#[test]
fn format_function_two_params() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let func = db.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(db.intern_string("a")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(db.intern_string("b")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let result = fmt.format(func);
    assert_eq!(result, "(a: string, b: number) => boolean");
}

#[test]
fn format_function_rest_param() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let arr = db.array(TypeId::STRING);
    let func = db.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(db.intern_string("args")),
            type_id: arr,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let result = fmt.format(func);
    assert!(
        result.contains("...args"),
        "Expected rest param, got: {result}"
    );
}

#[test]
fn format_function_with_type_params() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let t_atom = db.intern_string("T");
    let t_param = db.type_param(TypeParamInfo {
        name: t_atom,
        constraint: None,
        default: None,
        is_const: false,
    });
    let func = db.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_atom,
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(db.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_param,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let result = fmt.format(func);
    assert!(result.contains("<T>"), "Expected type param, got: {result}");
    assert!(result.contains("x: T"));
    assert!(result.contains("=> T"));
}

#[test]
fn format_function_type_param_with_constraint() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let t_atom = db.intern_string("T");
    let t_param = db.type_param(TypeParamInfo {
        name: t_atom,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    });
    let func = db.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_atom,
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(db.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_param,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let result = fmt.format(func);
    assert!(
        result.contains("T extends string"),
        "Expected 'T extends string', got: {result}"
    );
}

#[test]
fn format_function_type_param_with_structural_array_constraint_uses_shorthand() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let t_atom = db.intern_string("T");
    let constraint = db.array(TypeId::ANY);
    let t_param = db.type_param(TypeParamInfo {
        name: t_atom,
        constraint: Some(constraint),
        default: None,
        is_const: false,
    });
    let func = db.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_atom,
            constraint: Some(constraint),
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(db.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = fmt.format(func);
    assert!(
        result.contains("<T extends any[]>"),
        "Expected structural array constraint shorthand, got: {result}"
    );
}

#[test]
fn format_function_type_param_with_non_primitive_array_constraint_uses_generic_form() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let t_atom = db.intern_string("T");
    let foo = PropertyInfo::new(db.intern_string("foo"), TypeId::STRING);
    let object = db.object(vec![foo]);
    let constraint = db.array(object);
    let t_param = db.type_param(TypeParamInfo {
        name: t_atom,
        constraint: Some(constraint),
        default: None,
        is_const: false,
    });
    let func = db.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_atom,
            constraint: Some(constraint),
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(db.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = fmt.format(func);
    assert!(
        result.contains("<T extends Array<{ foo: string; }>>"),
        "Expected non-primitive array constraint to preserve generic form, got: {result}"
    );
}

#[test]
fn format_function_type_param_with_array_application_constraint_preserves_generic_form() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let t_atom = db.intern_string("T");
    let array_name = db.unresolved_type_name(db.intern_string("Array"));
    let constraint = db.application(array_name, vec![TypeId::ANY]);
    let t_param = db.type_param(TypeParamInfo {
        name: t_atom,
        constraint: Some(constraint),
        default: None,
        is_const: false,
    });
    let func = db.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_atom,
            constraint: Some(constraint),
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(db.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = fmt.format(func);
    assert!(
        result.contains("<T extends Array<any>>"),
        "Expected explicit Array<T> constraint syntax to be preserved, got: {result}"
    );
}

#[test]
fn format_function_type_param_with_default() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let t_atom = db.intern_string("T");
    let t_param = db.type_param(TypeParamInfo {
        name: t_atom,
        constraint: None,
        default: Some(TypeId::STRING),
        is_const: false,
    });
    let func = db.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_atom,
            constraint: None,
            default: Some(TypeId::STRING),
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(db.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let result = fmt.format(func);
    assert!(
        result.contains("T = string"),
        "Expected 'T = string', got: {result}"
    );
}

#[test]
fn format_constructor_function() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let func = db.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });
    let result = fmt.format(func);
    assert!(
        result.contains("new "),
        "Constructor should start with 'new', got: {result}"
    );
}

// =================================================================
// Array/tuple formatting
// =================================================================

#[test]
fn format_array_primitive() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    assert_eq!(fmt.format(db.array(TypeId::STRING)), "string[]");
    assert_eq!(fmt.format(db.array(TypeId::NUMBER)), "number[]");
    assert_eq!(fmt.format(db.array(TypeId::BOOLEAN)), "boolean[]");
}

#[test]
fn format_array_of_function_parenthesized() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let func = db.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let arr = db.array(func);
    let result = fmt.format(arr);
    assert!(
        result.starts_with('(') && result.ends_with(")[]"),
        "Array of function should be parenthesized, got: {result}"
    );
}

#[test]
fn format_tuple_empty() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let tuple = db.tuple(vec![]);
    assert_eq!(fmt.format(tuple), "[]");
}

#[test]
fn format_tuple_single_element() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let tuple = db.tuple(vec![crate::types::TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    assert_eq!(fmt.format(tuple), "[string]");
}

#[test]
fn format_tuple_two_elements() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let tuple = db.tuple(vec![
        crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    assert_eq!(fmt.format(tuple), "[string, number]");
}

#[test]
fn format_tuple_named_elements() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let tuple = db.tuple(vec![
        crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: Some(db.intern_string("name")),
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(db.intern_string("age")),
            optional: false,
            rest: false,
        },
    ]);
    assert_eq!(fmt.format(tuple), "[name: string, age: number]");
}

#[test]
fn format_tuple_optional_element() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let tuple = db.tuple(vec![
        crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: true,
            rest: false,
        },
    ]);
    let result = fmt.format(tuple);
    assert_eq!(result, "[string, (number | undefined)?]");
}

