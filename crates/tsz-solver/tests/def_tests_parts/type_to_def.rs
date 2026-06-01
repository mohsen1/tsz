// =============================================================================
// register_type_to_def invariant tests
//
// Rule: when multiple defs register for the same TypeId, the def with the
// earlier source position (lower (file_id, span_start)) wins, preserving
// deterministic display for e.g. multiple interface declarations that merge
// into one type, or lib defs that are encountered before user-file defs.
//
// Intrinsic TypeIds (number, string, boolean, etc.) are unconditionally
// rejected and never stored in the type_to_def map; the formatter handles
// them via its intrinsic short-circuit.
//
// Note: the `aliasSymbol` problem described in issue #10963 (lib type name
// showing instead of user alias name) requires instance-level alias tracking
// at the annotation site, not a global priority rule here. A global rule that
// makes TypeAlias always win causes conformance regressions because the same
// structural TypeId can be legitimately referenced by both an interface name
// and an alias name in different call sites (e.g., typed array families).
// =============================================================================

/// Two `Interface` defs for the same `TypeId`: the one with the earlier source
/// position wins regardless of registration order.
#[test]
fn register_type_to_def_interface_vs_interface_earlier_position_wins() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let type_id = TypeId(9007);

    let mut early_info =
        DefinitionInfo::interface(interner.intern_string("EarlyIface"), vec![], vec![]);
    early_info.file_id = Some(0);
    early_info.span = Some((0, 10));
    let early_def = store.register(early_info);

    let mut late_info =
        DefinitionInfo::interface(interner.intern_string("LateIface"), vec![], vec![]);
    late_info.file_id = Some(1);
    late_info.span = Some((0, 10));
    let late_def = store.register(late_info);

    // Register in reverse order to prove it's position-based, not arrival-order-based.
    store.register_type_to_def(type_id, late_def);
    store.register_type_to_def(type_id, early_def);

    assert_eq!(
        store.find_def_for_type(type_id),
        Some(early_def),
        "among two Interface defs, the one with the earlier (file_id, span_start) must win"
    );
}

/// Two `TypeAlias` defs for the same `TypeId`: the one with the earlier source
/// position wins regardless of registration order.
#[test]
fn register_type_to_def_alias_vs_alias_earlier_position_wins() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let type_id = TypeId(9008);

    let mut first_info =
        DefinitionInfo::type_alias(interner.intern_string("FirstAlias"), vec![], TypeId(9009));
    first_info.file_id = Some(0);
    first_info.span = Some((0, 15));
    let first_def = store.register(first_info);

    let mut second_info =
        DefinitionInfo::type_alias(interner.intern_string("SecondAlias"), vec![], TypeId(9010));
    second_info.file_id = Some(1);
    second_info.span = Some((0, 15));
    let second_def = store.register(second_info);

    // Register second first to prove it's position-based, not arrival-order-based.
    store.register_type_to_def(type_id, second_def);
    store.register_type_to_def(type_id, first_def);

    assert_eq!(
        store.find_def_for_type(type_id),
        Some(first_def),
        "among two TypeAlias defs, the one with the earlier (file_id, span_start) must win"
    );
}

/// A `TypeAlias` with a LATER position than an `Interface` for the same `TypeId`
/// does NOT displace the interface — position wins, not kind. This is the
/// boundary condition for #10963: fixing that requires instance-level alias
/// tracking, not a global kind priority.
#[test]
fn register_type_to_def_later_alias_does_not_displace_earlier_interface() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let type_id = TypeId(9011);

    // Interface at earlier position (file_id=0 → lower than user file).
    let mut iface_info =
        DefinitionInfo::interface(interner.intern_string("LibIface"), vec![], vec![]);
    iface_info.file_id = Some(0);
    iface_info.span = Some((0, 20));
    let iface_def = store.register(iface_info);

    // TypeAlias at later position (file_id=1).
    let mut alias_info =
        DefinitionInfo::type_alias(interner.intern_string("UserAlias"), vec![], TypeId(9012));
    alias_info.file_id = Some(1);
    alias_info.span = Some((0, 20));
    let alias_def = store.register(alias_info);

    store.register_type_to_def(type_id, iface_def);
    store.register_type_to_def(type_id, alias_def);

    // Interface wins because it has the earlier position.
    assert_eq!(
        store.find_def_for_type(type_id),
        Some(iface_def),
        "position-based rule: earlier-registered Interface keeps the slot over a later TypeAlias"
    );
    // Verify alias_def is also a valid registered def — this test is about
    // type_to_def display priority, not def registration correctness.
    assert!(store.contains(alias_def));
}

/// Intrinsic `TypeIds` (number, string, boolean, etc.) must never carry a
/// `type_to_def` mapping.  Their canonical display is the keyword
/// (`number`, `string`, ...) provided by the `TypeFormatter`'s intrinsic
/// short-circuit. If a checker path tries to register an intrinsic type
/// to a class/interface/alias def, that mapping later poisons
/// `find_def_for_type` lookups and produces wrong-source-name diagnostics
/// such as `Type 'FlatArray' is not assignable to type 'Boolean'.` for
/// `let b: Boolean; b = 1;` (where the source is the primitive `number`).
#[test]
fn test_register_type_to_def_rejects_intrinsic_type_ids() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();
    let name = interner.intern_string("FlatArray");
    let info = DefinitionInfo::type_alias(name, vec![], TypeId::NEVER);
    let def_id = store.register(info);

    // All intrinsic TypeIds covered by the formatter short-circuit must be
    // rejected. Listing them explicitly locks the invariant per intrinsic
    // (a future intrinsic addition that escapes the formatter's match arm
    // will still be safely rejected here).
    let intrinsics = [
        TypeId::ANY,
        TypeId::UNKNOWN,
        TypeId::NEVER,
        TypeId::VOID,
        TypeId::UNDEFINED,
        TypeId::NULL,
        TypeId::BOOLEAN,
        TypeId::BOOLEAN_TRUE,
        TypeId::BOOLEAN_FALSE,
        TypeId::NUMBER,
        TypeId::STRING,
        TypeId::BIGINT,
        TypeId::SYMBOL,
        TypeId::OBJECT,
        TypeId::FUNCTION,
        TypeId::ERROR,
    ];
    for &intrinsic in &intrinsics {
        store.register_type_to_def(intrinsic, def_id);
        assert_eq!(
            store.find_def_for_type(intrinsic),
            None,
            "intrinsic TypeId {} (is_intrinsic={}) must not be associated \
             with a user-named def via register_type_to_def",
            intrinsic.0,
            intrinsic.is_intrinsic(),
        );
    }
}

/// When a `TypeId` is only registered by a single def (no competition),
/// that def is returned regardless of kind. This baseline ensures the map
/// stores the first registration faithfully.
#[test]
fn register_type_to_def_uncontested_registration_is_preserved() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let iface_only_id = TypeId(9016);

    let mut iface_info =
        DefinitionInfo::interface(interner.intern_string("SoloInterface"), vec![], vec![]);
    iface_info.file_id = Some(0);
    iface_info.span = Some((0, 20));
    let iface_def = store.register(iface_info);

    store.register_type_to_def(iface_only_id, iface_def);

    assert_eq!(
        store.find_def_for_type(iface_only_id),
        Some(iface_def),
        "an uncontested registration must be returned as-is"
    );
}
