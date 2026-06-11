// Interface vs type-literal member-pipeline parity.
//
// Both forms lower through the shared `collect_object_type_members` /
// `finish_object_type_parts` pipeline, so a structurally identical
// `interface I { ... }` and `type T = { ... }` must intern to the same
// `TypeId`: same method-overload merging, index-signature merging, duplicate
// member conflict handling, accessor merging, and late-bound detection.

/// Parse a source containing one interface (by name) and one type literal,
/// returning both from a single arena so lowered `TypeId`s are comparable.
fn parse_interface_and_type_literal(
    source: &str,
    interface_name: &str,
) -> (NodeArena, Vec<NodeIndex>, NodeIndex) {
    let arena = parse_and_take_arena(source);
    let mut declarations = Vec::new();
    let mut literal_idx = NodeIndex::NONE;
    for i in 0..arena.len() {
        let idx = NodeIndex(i as u32);
        let Some(node) = arena.get(idx) else {
            continue;
        };
        if node.kind == syntax_kind_ext::INTERFACE_DECLARATION
            && let Some(interface) = arena.get_interface(node)
            && let Some(name_node) = arena.get(interface.name)
            && let Some(ident) = arena.get_identifier(name_node)
            && ident.escaped_text == interface_name
        {
            declarations.push(idx);
        }
        if node.kind == syntax_kind_ext::TYPE_LITERAL && literal_idx == NodeIndex::NONE {
            literal_idx = idx;
        }
    }
    assert!(
        !declarations.is_empty(),
        "Could not find interface '{interface_name}'"
    );
    assert!(
        literal_idx != NodeIndex::NONE,
        "Could not find type literal"
    );
    (arena, declarations, literal_idx)
}

/// Lower both forms with one interner and assert they produce the same type.
fn assert_interface_literal_parity(source: &str, interface_name: &str) -> (TypeInterner, TypeId) {
    let (arena, declarations, literal_idx) =
        parse_interface_and_type_literal(source, interface_name);
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);
    let interface_type = lowering.lower_interface_declarations(&declarations);
    let literal_type = lowering.lower_type(literal_idx);
    assert_ne!(interface_type, TypeId::ERROR, "interface lowering failed");
    assert_eq!(
        interface_type, literal_type,
        "interface vs type-literal lowering diverged for: {source}"
    );
    (interner, literal_type)
}

#[test]
fn test_member_parity_properties_optional_readonly() {
    assert_interface_literal_parity(
        "interface Alpha { qty: number; readonly tag?: string; }
         type AlphaLit = { qty: number; readonly tag?: string; };",
        "Alpha",
    );
}

#[test]
fn test_member_parity_single_method() {
    assert_interface_literal_parity(
        "interface Beacon { ping(msg: string): number; }
         type BeaconLit = { ping(msg: string): number; };",
        "Beacon",
    );
}

#[test]
fn test_member_parity_generic_method() {
    assert_interface_literal_parity(
        "interface Wrapper { wrap<T>(value: T): T[]; }
         type WrapperLit = { wrap<T>(value: T): T[]; };",
        "Wrapper",
    );
}

#[test]
fn test_member_parity_method_overloads() {
    let (interner, type_id) = assert_interface_literal_parity(
        "interface Chord { play(): void; play(note: string): boolean; }
         type ChordLit = { play(): void; play(note: string): boolean; };",
        "Chord",
    );

    // Overloads of the same method merge into ONE property whose type carries
    // both call signatures (previously the type-literal path produced two
    // duplicate properties named `play`).
    match interner.lookup(type_id).expect("type should exist") {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1, "overloads must merge");
            let prop = &shape.properties[0];
            assert_eq!(interner.resolve_atom(prop.name), "play");
            assert!(prop.is_method);
            match interner.lookup(prop.type_id).expect("member type") {
                TypeData::Callable(callable_id) => {
                    let callable = interner.callable_shape(callable_id);
                    assert_eq!(callable.call_signatures.len(), 2);
                }
                other => panic!("Expected Callable member, got {other:?}"),
            }
        }
        other => panic!("Expected Object type, got {other:?}"),
    }
}

#[test]
fn test_member_parity_call_and_construct_signatures() {
    assert_interface_literal_parity(
        "interface Factory { (count: number): string; new (count: number): string; ready: boolean; }
         type FactoryLit = { (count: number): string; new (count: number): string; ready: boolean; };",
        "Factory",
    );
}

#[test]
fn test_member_parity_string_and_number_index_signatures() {
    assert_interface_literal_parity(
        "interface Grid { [cell: string]: unknown; length: number; }
         type GridLit = { [cell: string]: unknown; length: number; };",
        "Grid",
    );
}

#[test]
fn test_member_parity_distinct_string_pattern_index_signatures() {
    let (interner, type_id) = assert_interface_literal_parity(
        "interface Dataset { [key: `data-${string}`]: string; [key: `aria-${string}`]: string; }
         type DatasetLit = { [key: `data-${string}`]: string; [key: `aria-${string}`]: string; };",
        "Dataset",
    );

    // Distinct string-keyed patterns merge by unioning the key types
    // (previously the type-literal path kept only the LAST index signature,
    // silently dropping the `data-${string}` pattern).
    match interner.lookup(type_id).expect("type should exist") {
        TypeData::ObjectWithIndex(shape_id) => {
            let shape = interner.object_shape(shape_id);
            let string_index = shape
                .string_index
                .as_ref()
                .expect("expected merged string index signature");
            match interner
                .lookup(string_index.key_type)
                .expect("key type should exist")
            {
                TypeData::Union(list_id) => {
                    assert_eq!(interner.type_list(list_id).len(), 2);
                }
                other => panic!("Expected union of key patterns, got {other:?}"),
            }
        }
        other => panic!("Expected ObjectWithIndex type, got {other:?}"),
    }
}

#[test]
fn test_member_parity_duplicate_conflicting_members() {
    assert_interface_literal_parity(
        "interface Clash { v: string; v: number; }
         type ClashLit = { v: string; v: number; };",
        "Clash",
    );
}

#[test]
fn test_member_parity_get_set_accessors() {
    assert_interface_literal_parity(
        "interface Gauge { get level(): number; set level(value: number); }
         type GaugeLit = { get level(): number; set level(value: number); };",
        "Gauge",
    );
}

#[test]
fn test_member_parity_late_bound_computed_member() {
    let (interner, type_id) = assert_interface_literal_parity(
        "declare const marker: symbol;
         interface Tagged { [marker]: string; }
         type TaggedLit = { [marker]: string; };",
        "Tagged",
    );

    match interner.lookup(type_id).expect("type should exist") {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            assert!(
                shape.flags.contains(ObjectFlags::HAS_LATE_BOUND_MEMBERS),
                "unresolved computed member must mark the type late-bound"
            );
        }
        other => panic!("Expected Object type, got {other:?}"),
    }
}

#[test]
fn test_member_parity_negative_different_shapes_differ() {
    let (arena, declarations, literal_idx) = parse_interface_and_type_literal(
        "interface Pair { first: string; }
         type PairLit = { first: number; };",
        "Pair",
    );
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);
    let interface_type = lowering.lower_interface_declarations(&declarations);
    let literal_type = lowering.lower_type(literal_idx);
    assert_ne!(
        interface_type, literal_type,
        "structurally different shapes must not collapse"
    );
}
