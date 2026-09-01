#[test]
fn type_formatter_cache_statistics_account_for_atom_cache_entries_and_size() {
    let db = TypeInterner::new();
    let atom = db.intern_string("cachedName");
    let mut fmt = TypeFormatter::new(&db);

    let empty_stats = fmt.cache_statistics();
    assert_eq!(empty_stats.atom_cache_entries, 0);
    assert_eq!(empty_stats.application_reduction_cache_entries, 0);
    assert_eq!(empty_stats.recursive_alias_base_cache_entries, 0);
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
fn type_formatter_cache_statistics_account_for_application_memos() {
    let db = TypeInterner::new();
    let fmt = TypeFormatter::new(&db);

    let empty_stats = fmt.cache_statistics();
    assert_eq!(empty_stats.application_reduction_cache_entries, 0);
    assert_eq!(empty_stats.recursive_alias_base_cache_entries, 0);

    fmt.application_reduction_cache
        .borrow_mut()
        .insert(
            TypeId::STRING,
            Some(application_reduction::ApplicationDisplayReduction::Type(
                TypeId::NUMBER,
            )),
        );
    fmt.recursive_alias_base_cache
        .borrow_mut()
        .insert(TypeId::OBJECT, false);

    let populated_stats = fmt.cache_statistics();
    assert_eq!(populated_stats.application_reduction_cache_entries, 1);
    assert_eq!(populated_stats.recursive_alias_base_cache_entries, 1);
    assert!(
        populated_stats.estimated_size_bytes > empty_stats.estimated_size_bytes,
        "populated formatter application memo caches must be visible to residency estimates",
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
        single_quoted_name: false, non_widening: false,
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
            origin: crate::types::TypeParamOrigin::User,
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
            origin: crate::types::TypeParamOrigin::User,
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
            origin: crate::types::TypeParamOrigin::User,
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
    // The error-type sentinel renders as `any` (tsc's `errorType` display).
    assert_eq!(fmt.format(TypeId::ERROR), "any");
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
fn number_literal_union_uses_tsc_numeric_order() {
    let db = TypeInterner::new();
    let one = db.literal_number(1.0);
    let minus_one = db.literal_number(-1.0);
    let two = db.literal_number(2.0);
    let zero = db.literal_number(0.0);

    let union = db.union(vec![minus_one, zero, one, two]);

    // TypeScript 7 runs with `stableTypeOrdering`, whose `compareTypes` orders
    // numeric literal types by value, so allocation order does not leak into
    // the rendered union. Negative values sort ahead of zero.
    let mut fmt = TypeFormatter::new(&db);
    assert_eq!(fmt.format(union), "-1 | 0 | 1 | 2");
}

#[test]
fn number_literal_union_is_numeric_sorted_regardless_of_allocation_order() {
    let db = TypeInterner::new();
    // Intern `2` first so allocation order disagrees with numeric order.
    let two = db.literal_number(2.0);
    let one = db.literal_number(1.0);

    let union = db.union(vec![one, two]);

    let mut fmt = TypeFormatter::new(&db);
    assert_eq!(fmt.format(union), "1 | 2");
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
        construct_signatures: vec![CallSignature::new(vec![], TypeId::ANY)],
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
        origin: crate::types::TypeParamOrigin::User,
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

    // tsc renders the factored numeric-literal remainder numerically ascending,
    // regardless of allocation order (oracle tsc 7.0.2
    // inDoesNotOperateOnPrimitiveTypes: `T & (0 | 1 | 2)`).
    assert_eq!(fmt.format(union), "T & (0 | 1 | 2)");
}

#[test]
fn format_union_of_intersections_display_order_independent_of_alloc_order() {
    // The factored numeric-literal remainder sorts numerically ascending and is
    // therefore independent of allocation order: interning `one` before `two`
    // (lower alloc-order) must not change the `0 | 1 | 2` display (oracle tsc
    // 7.0.2 `U & (1 | 2 | 0)` → `U & (0 | 1 | 2)`). The sort is applied directly
    // rather than round-tripping through `interner.union()` (which re-sorts by
    // alloc-order and would reintroduce order dependence).
    let db = TypeInterner::new();
    let k = db.type_param(TypeParamInfo {
        name: db.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
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
    assert_eq!(fmt.format(union), "K & (0 | 1 | 2)");
}

#[test]
fn format_union_of_intersections_does_not_factor_different_common_parts() {
    let db = TypeInterner::new();
    let t = db.type_param(TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let u = db.type_param(TypeParamInfo {
        name: db.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
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
        origin: crate::types::TypeParamOrigin::User,
    });
    let u = db.type_param(TypeParamInfo {
        name: db.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
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
        origin: crate::types::TypeParamOrigin::User,
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
        symbol_index: None,
        symbol: None,
        flags: Default::default(),
    });
    let index_only = db.object_with_index(crate::types::ObjectShape {
        properties: vec![],
        string_index: None,
        number_index: Some(index_sig),
        symbol_index: None,
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
        origin: crate::types::TypeParamOrigin::User,
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
fn format_object_truncation_skips_omitted_property_formatting() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let omitted_nested_name = db.intern_string("omittedNested");
    let omitted_nested_type = db.object(vec![PropertyInfo::new(
        omitted_nested_name,
        TypeId::STRING,
    )]);
    let props: Vec<PropertyInfo> = (1..=24)
        .map(|i| {
            let type_id = if i == 18 {
                omitted_nested_type
            } else {
                TypeId::NUMBER
            };
            PropertyInfo::new(db.intern_string(&format!("p{i}")), type_id)
        })
        .collect();
    let obj = db.object(props);
    let result = fmt.format(obj);

    assert!(
        result.contains("... 6 more ..."),
        "Expected omitted-count marker for large object, got: {result}"
    );
    assert!(
        !result.contains("p18:"),
        "Expected omitted middle properties to stay out of the display, got: {result}"
    );
    assert!(
        !fmt.atom_cache.contains_key(&omitted_nested_name),
        "Expected omitted property types not to be formatted, got result {result} and atom cache stats: {:?}",
        fmt.cache_statistics(),
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
        symbol_index: None,
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
        symbol_index: None,
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
        symbol_index: None,
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
        symbol_index: None,
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
        symbol_index: None,
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
            symbol_index: None,
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
        symbol_index: None,
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
fn format_object_with_index_truncation_skips_omitted_property_formatting() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let omitted_nested_name = db.intern_string("omittedIndexedNested");
    let omitted_nested_type = db.object(vec![PropertyInfo::new(
        omitted_nested_name,
        TypeId::STRING,
    )]);
    let props: Vec<PropertyInfo> = (1..=24)
        .map(|i| {
            let type_id = if i == 18 {
                omitted_nested_type
            } else {
                TypeId::NUMBER
            };
            PropertyInfo::new(db.intern_string(&format!("p{i}")), type_id)
        })
        .collect();

    let shape = crate::types::ObjectShape {
        properties: props,
        string_index: None,
        number_index: Some(crate::types::IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        symbol_index: None,
        symbol: None,
        flags: Default::default(),
    };
    let obj = db.object_with_index(shape);
    let result = fmt.format(obj);

    assert!(
        result.contains("..."),
        "Expected omitted-count marker for indexed object, got: {result}"
    );
    assert!(
        !result.contains("p18:"),
        "Expected omitted middle properties to stay out of indexed display, got: {result}"
    );
    assert!(
        !fmt.atom_cache.contains_key(&omitted_nested_name),
        "Expected omitted indexed property types not to be formatted, got result {result} and atom cache stats: {:?}",
        fmt.cache_statistics(),
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
        symbol_index: None,
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
        symbol_index: None,
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
        symbol_index: None,
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
        symbol_index: None,
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

// A trailing rest parameter whose type is a concrete tuple expands to
// positional parameters, matching tsc's `signatureToString`
// (`(...rest: [A, B])` -> `(rest_0: A, rest_1: B)`).
fn format_rest_tuple_fn(
    db: &TypeInterner,
    rest_name: &str,
    elements: Vec<crate::types::TupleElement>,
) -> String {
    let tuple = db.tuple(elements);
    let func = db.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(db.intern_string(rest_name)),
            type_id: tuple,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let mut fmt = TypeFormatter::new(db);
    fmt.format(func).into_owned()
}

#[test]
fn format_rest_tuple_param_expands_unlabeled() {
    let db = TypeInterner::new();
    let elements = vec![
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
    ];
    assert_eq!(
        format_rest_tuple_fn(&db, "rest", elements),
        "(rest_0: string, rest_1: number) => void"
    );
}

#[test]
fn format_rest_tuple_param_uses_labels_and_rest_name() {
    let db = TypeInterner::new();
    // Labels win over the `{name}_{i}` fallback; the rest-parameter name drives
    // the fallback so a renamed rest (`args`) yields `args_0`, `args_1`.
    let labeled = vec![
        crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: Some(db.intern_string("a")),
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ];
    assert_eq!(
        format_rest_tuple_fn(&db, "rest", labeled),
        "(a: string, rest_1: number) => void"
    );

    let renamed = vec![
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
    ];
    assert_eq!(
        format_rest_tuple_fn(&db, "args", renamed),
        "(args_0: string, args_1: number) => void"
    );
}

#[test]
fn format_rest_tuple_param_trailing_variadic_keeps_bare_rest_name() {
    let db = TypeInterner::new();
    // `[string, ...number[]]` -> `(rest_0: string, ...rest: number[])`.
    let number_arr = db.array(TypeId::NUMBER);
    let elements = vec![
        crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: number_arr,
            name: None,
            optional: false,
            rest: true,
        },
    ];
    assert_eq!(
        format_rest_tuple_fn(&db, "rest", elements),
        "(rest_0: string, ...rest: number[]) => void"
    );
}

#[test]
fn format_rest_tuple_param_middle_rest_not_expanded() {
    let db = TypeInterner::new();
    // A rest element in a non-trailing position (`[string, ...number[], boolean]`)
    // can't become a parameter list, so tsc leaves it as the written tuple.
    let number_arr = db.array(TypeId::NUMBER);
    let elements = vec![
        crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: number_arr,
            name: None,
            optional: false,
            rest: true,
        },
        crate::types::TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ];
    let result = format_rest_tuple_fn(&db, "rest", elements);
    assert!(
        result.starts_with("(...rest: ["),
        "middle-rest tuple must stay unexpanded, got: {result}"
    );
}

#[test]
fn format_rest_array_param_not_expanded() {
    let db = TypeInterner::new();
    // A plain array rest (`...args: string[]`) is not a tuple and is never
    // expanded.
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
    let mut fmt = TypeFormatter::new(&db);
    assert_eq!(fmt.format(func), "(...args: string[]) => void");
}

#[test]
fn format_rest_readonly_tuple_param_expands_dropping_modifier() {
    let db = TypeInterner::new();
    // `readonly [string, number]` expands like a mutable tuple; tsc drops the
    // `readonly` modifier from the expanded positional parameters.
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
    let readonly_tuple = db.readonly_type(tuple);
    let func = db.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(db.intern_string("rest")),
            type_id: readonly_tuple,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let mut fmt = TypeFormatter::new(&db);
    assert_eq!(
        fmt.format(func),
        "(rest_0: string, rest_1: number) => void"
    );
}

// A trailing rest parameter typed via a type *alias* or *generic application*
// resolves to its underlying tuple and expands like an inline tuple, matching
// tsc's `signatureToString` (`type R = [a, b]; ...rest: R` renders like the
// tuple). The formatter reads the alias body / generic arguments through its
// definition store.
fn format_rest_typed_with_store(
    db: &TypeInterner,
    def_store: &crate::def::DefinitionStore,
    rest_name: &str,
    rest_type_id: TypeId,
) -> String {
    let func = db.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(db.intern_string(rest_name)),
            type_id: rest_type_id,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let mut fmt = TypeFormatter::new(db).with_def_store(def_store);
    fmt.format(func).into_owned()
}

fn unnamed_tuple_el(type_id: TypeId) -> crate::types::TupleElement {
    crate::types::TupleElement {
        type_id,
        name: None,
        optional: false,
        rest: false,
    }
}

#[test]
fn format_rest_alias_tuple_param_expands() {
    // `type R = [string, number]; declare const f: (...rest: R) => void`
    // renders `(rest_0: string, rest_1: number)` — tsc resolves the alias to its
    // tuple before expanding.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let tuple = db.tuple(vec![
        unnamed_tuple_el(TypeId::STRING),
        unnamed_tuple_el(TypeId::NUMBER),
    ]);
    let r_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("R"),
        vec![],
        tuple,
    ));
    assert_eq!(
        format_rest_typed_with_store(&db, &def_store, "rest", db.lazy(r_def)),
        "(rest_0: string, rest_1: number) => void"
    );
}

#[test]
fn format_rest_alias_tuple_param_preserves_labels() {
    // Named tuple members in the alias win over the `{name}_{i}` fallback:
    // `type R = [a: string, b: number]; ...rest: R` -> `(a: string, b: number)`.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let tuple = db.tuple(vec![
        crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: Some(db.intern_string("a")),
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(db.intern_string("b")),
            optional: false,
            rest: false,
        },
    ]);
    // A renamed binder (`Coords`) proves the resolution is structural, not
    // keyed on any particular alias name.
    let def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("Coords"),
        vec![],
        tuple,
    ));
    assert_eq!(
        format_rest_typed_with_store(&db, &def_store, "rest", db.lazy(def)),
        "(a: string, b: number) => void"
    );
}

#[test]
fn format_rest_nested_alias_tuple_param_expands() {
    // `type Inner = [string, number]; type R = Inner; ...rest: R` expands
    // through the alias chain to `(rest_0: string, rest_1: number)`.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let tuple = db.tuple(vec![
        unnamed_tuple_el(TypeId::STRING),
        unnamed_tuple_el(TypeId::NUMBER),
    ]);
    let inner_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("Inner"),
        vec![],
        tuple,
    ));
    let outer_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("R"),
        vec![],
        db.lazy(inner_def),
    ));
    assert_eq!(
        format_rest_typed_with_store(&db, &def_store, "rest", db.lazy(outer_def)),
        "(rest_0: string, rest_1: number) => void"
    );
}

#[test]
fn format_rest_generic_alias_application_expands() {
    // `type Pair<T> = [T, number]; ...rest: Pair<string>` instantiates to
    // `[string, number]` and expands to `(rest_0: string, rest_1: number)`.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let t_param = TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let t = db.type_param(t_param);
    let body = db.tuple(vec![unnamed_tuple_el(t), unnamed_tuple_el(TypeId::NUMBER)]);
    let pair_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("Pair"),
        vec![t_param],
        body,
    ));
    def_store.register_type_to_def(body, pair_def);
    let app = db.application(db.lazy(pair_def), vec![TypeId::STRING]);
    assert_eq!(
        format_rest_typed_with_store(&db, &def_store, "rest", app),
        "(rest_0: string, rest_1: number) => void"
    );
}

#[test]
fn format_rest_array_alias_param_not_expanded() {
    // `type R = number[]; ...rest: R` is not a tuple, so tsc keeps the written
    // alias form (`...rest: R`) rather than expanding.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let arr = db.array(TypeId::NUMBER);
    let def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("R"),
        vec![],
        arr,
    ));
    let result = format_rest_typed_with_store(&db, &def_store, "rest", db.lazy(def));
    assert!(
        result.starts_with("(...rest:") && !result.contains("rest_0"),
        "an array-bodied alias rest must stay unexpanded, got: {result}"
    );
}


/// Rendered object member order must not depend on string-interning order.
///
/// `#16309` (evidence #3): a synthesized object whose properties carry no
/// `declaration_order` used to render its members in stored (Atom-id) order,
/// which is string-interning order. Under the parallel checker the interner is
/// shared across worker threads, so that order is thread-schedule dependent and
/// the same type rendered its members differently run to run. The display
/// tiebreak is now content-based, so two interners that assign the property
/// names opposite Atom ids must render the object identically.
#[test]
fn object_member_display_order_is_interning_order_independent() {
    fn render(intern_first: &str, intern_second: &str) -> String {
        let db = TypeInterner::new();
        // Fix the Atom-id assignment order for the two names up front; whichever
        // is interned first gets the lower id and would sort first in storage.
        db.intern_string(intern_first);
        db.intern_string(intern_second);
        // A synthesized `{ value; done }` object — neither property carries a
        // declaration order (mirrors the `IteratorReturnResult` witness).
        let done = PropertyInfo::new(db.intern_string("done"), TypeId::BOOLEAN);
        let value = PropertyInfo::new(db.intern_string("value"), TypeId::NUMBER);
        let obj = db.object(vec![done, value]);
        let mut fmt = TypeFormatter::new(&db);
        fmt.format(obj).into_owned()
    }

    let value_lower_id = render("value", "done");
    let done_lower_id = render("done", "value");
    assert_eq!(
        value_lower_id, done_lower_id,
        "object member display order leaked interning order into the diagnostic",
    );
    // The deterministic order is content-based (alphabetical): `done` < `value`.
    assert_eq!(done_lower_id, "{ done: boolean; value: number; }");
}

/// A first/synthesized member (`declaration_order == 0`) must render ahead of a
/// later member, not in alphabetical position.
///
/// Expando types assign the first property a `declaration_order` of `0`
/// (`Foo.inVariableInit = 1` before the later `Foo.bla = ...`), and tsc renders
/// it first (insertion order). The deterministic display tiebreak must preserve
/// that — keying purely on the property name would wrongly sort the `0` member
/// by its name (#16309 fix must not regress `expandoFunctionNestedAssigments`).
#[test]
fn object_member_display_keeps_first_synthesized_member_ahead() {
    let db = TypeInterner::new();
    // `zebra` is the first-assigned (synthesized) member: declaration_order 0.
    let zebra = PropertyInfo::new(db.intern_string("zebra"), TypeId::NUMBER);
    // `apple` is a later member with a real declaration order.
    let apple = PropertyInfo {
        declaration_order: 2,
        ..PropertyInfo::new(db.intern_string("apple"), TypeId::NUMBER)
    };
    let obj = db.object(vec![zebra, apple]);
    let mut fmt = TypeFormatter::new(&db);
    // Insertion order (zebra first), NOT alphabetical (apple first).
    assert_eq!(fmt.format(obj).into_owned(), "{ zebra: number; apple: number; }");
}
