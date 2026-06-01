// =============================================================================
// register_type_to_def priority tests
//
// Rule: when multiple defs register for the same TypeId, TypeAlias defs win
// over non-TypeAlias defs (Interface, Class, etc.). This mirrors tsc's
// aliasSymbol semantics, which preserve the user-written alias name in
// diagnostic messages even when a lib/declaration file has an earlier-ordered
// interface with the same structural TypeId.
//
// Within the same alias-vs-non-alias category, the def with the earlier source
// position (lower (file_id, span_start)) wins, preserving deterministic
// display for e.g. multiple interface declarations that merge into one type.
// =============================================================================

/// should displace the earlier-registered interface.
#[test]
fn register_type_to_def_alias_displaces_earlier_interface() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let type_id = TypeId(9001); // non-intrinsic; represents a shared structural type

    let mut iface_info =
        DefinitionInfo::interface(interner.intern_string("LibIface"), vec![], vec![]);
    iface_info.file_id = Some(0); // lib file — lower file_id
    iface_info.span = Some((0, 20));
    let iface_def = store.register(iface_info);

    let mut alias_info =
        DefinitionInfo::type_alias(interner.intern_string("UserAlias"), vec![], TypeId(9002));
    alias_info.file_id = Some(1); // user file — higher file_id
    alias_info.span = Some((0, 20));
    let alias_def = store.register(alias_info);

    // Simulate lib file registering first (lower file_id → registered earlier).
    store.register_type_to_def(type_id, iface_def);
    // User alias registers second.
    store.register_type_to_def(type_id, alias_def);

    assert_eq!(
        store.find_def_for_type(type_id),
        Some(alias_def),
        "TypeAlias def must displace earlier Interface def for the same TypeId"
    );
}

/// `TypeAlias` registered BEFORE a non-alias (`Interface`) is protected — the
/// alias must not be overwritten by a later-registered interface.
#[test]
fn register_type_to_def_alias_holds_against_later_interface() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let type_id = TypeId(9003);

    let mut alias_info =
        DefinitionInfo::type_alias(interner.intern_string("MyAlias"), vec![], TypeId(9004));
    alias_info.file_id = Some(0);
    alias_info.span = Some((5, 30));
    let alias_def = store.register(alias_info);

    let mut iface_info =
        DefinitionInfo::interface(interner.intern_string("SomeInterface"), vec![], vec![]);
    iface_info.file_id = Some(0);
    iface_info.span = Some((50, 80));
    let iface_def = store.register(iface_info);

    // Alias registered first.
    store.register_type_to_def(type_id, alias_def);
    // Interface tries to displace it.
    store.register_type_to_def(type_id, iface_def);

    assert_eq!(
        store.find_def_for_type(type_id),
        Some(alias_def),
        "TypeAlias def must not be displaced by a later Interface registration"
    );
}

/// `TypeAlias` registered AFTER a `Class` (non-alias) wins — class instance
/// types that share a structural `TypeId` with a user alias must show the alias name.
#[test]
fn register_type_to_def_alias_displaces_class_def() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let type_id = TypeId(9005);

    let class_info =
        DefinitionInfo::class(interner.intern_string("LibClass"), vec![], vec![], vec![]);
    let class_def = store.register(class_info);

    let mut alias_info =
        DefinitionInfo::type_alias(interner.intern_string("UserType"), vec![], TypeId(9006));
    alias_info.file_id = Some(1);
    alias_info.span = Some((0, 10));
    let alias_def = store.register(alias_info);

    store.register_type_to_def(type_id, class_def);
    store.register_type_to_def(type_id, alias_def);

    assert_eq!(
        store.find_def_for_type(type_id),
        Some(alias_def),
        "TypeAlias def must displace earlier Class def for the same TypeId"
    );
}

/// Two non-alias defs (both Interface): earlier position wins.
/// This is the pre-existing position-based tiebreaker path.
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

/// Two `TypeAlias` defs: the one with the earlier source position wins.
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

/// Rename-agnostic: the fix must not depend on the alias or interface name
/// spelling. Verify with two different sets of user-chosen names.
#[test]
fn register_type_to_def_alias_priority_is_name_agnostic() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    // Case A: names ending in 'A'.
    let type_id_a = TypeId(9011);
    let mut iface_a =
        DefinitionInfo::interface(interner.intern_string("InterfaceA"), vec![], vec![]);
    iface_a.file_id = Some(0);
    iface_a.span = Some((0, 10));
    let iface_def_a = store.register(iface_a);
    let mut alias_a =
        DefinitionInfo::type_alias(interner.intern_string("AliasA"), vec![], TypeId(9012));
    alias_a.file_id = Some(1);
    alias_a.span = Some((0, 10));
    let alias_def_a = store.register(alias_a);
    store.register_type_to_def(type_id_a, iface_def_a);
    store.register_type_to_def(type_id_a, alias_def_a);

    // Case B: completely different names.
    let type_id_b = TypeId(9013);
    let mut iface_b =
        DefinitionInfo::interface(interner.intern_string("Serializable"), vec![], vec![]);
    iface_b.file_id = Some(0);
    iface_b.span = Some((0, 10));
    let iface_def_b = store.register(iface_b);
    let mut alias_b =
        DefinitionInfo::type_alias(interner.intern_string("SerialAlias"), vec![], TypeId(9014));
    alias_b.file_id = Some(1);
    alias_b.span = Some((0, 10));
    let alias_def_b = store.register(alias_b);
    store.register_type_to_def(type_id_b, iface_def_b);
    store.register_type_to_def(type_id_b, alias_def_b);

    assert_eq!(
        store.find_def_for_type(type_id_a),
        Some(alias_def_a),
        "TypeAlias must win regardless of identifier spelling (case A)"
    );
    assert_eq!(
        store.find_def_for_type(type_id_b),
        Some(alias_def_b),
        "TypeAlias must win regardless of identifier spelling (case B)"
    );
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

/// When a `TypeId` is registered only by an `Interface` (no alias competition),
/// the interface def is returned unchanged.
///
/// The `TypeAlias`-wins priority rule only fires when both an alias and a
/// non-alias def compete for the *same* `TypeId`. When the user annotates a
/// variable with `MyInterface` and that interface has its own distinct
/// structural `TypeId` (no alias body points at it), `find_def_for_type`
/// must return the interface def so diagnostics show the interface name, not
/// an unrelated alias name.
#[test]
fn register_type_to_def_uncontested_interface_stays_as_interface() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    // TypeId A is contested: both an alias and an interface share it.
    let contested_id = TypeId(9015);
    // TypeId B is uncontested: only the interface is registered for it.
    let interface_only_id = TypeId(9016);

    let mut alias_info =
        DefinitionInfo::type_alias(interner.intern_string("MyAlias"), vec![], TypeId(9017));
    alias_info.file_id = Some(1);
    alias_info.span = Some((0, 10));
    let alias_def = store.register(alias_info);

    let mut iface_info =
        DefinitionInfo::interface(interner.intern_string("MyInterface"), vec![], vec![]);
    iface_info.file_id = Some(0);
    iface_info.span = Some((0, 20));
    let iface_def = store.register(iface_info);

    // Contested: interface registers first, alias displaces it.
    store.register_type_to_def(contested_id, iface_def);
    store.register_type_to_def(contested_id, alias_def);

    // Uncontested: only the interface registers for interface_only_id.
    store.register_type_to_def(interface_only_id, iface_def);

    // The alias wins the contested TypeId.
    assert_eq!(
        store.find_def_for_type(contested_id),
        Some(alias_def),
        "alias must displace interface for the contested TypeId"
    );
    // The interface is preserved for the uncontested TypeId.
    assert_eq!(
        store.find_def_for_type(interface_only_id),
        Some(iface_def),
        "when no alias competes, the interface def must be returned as-is"
    );
}
