//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/type_queries/global_interfaces/mod.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 9c735cf6d9887c21f0f46350b81325a38bb6021e65ee9d4a40bfd4a382c479c7 281 identity_is_false_when_lib_not_loaded
    /// Lib-not-loaded ordering / `noLib`: with empty boxed registries the
    /// identity tier must answer `false` for everything, including types that
    /// are structurally indistinguishable from the lib interface.
    #[test]
    fn identity_is_false_when_lib_not_loaded() {
        let interner = TypeInterner::new();
        let object_like = object_like_shape(&interner);
        let lazy = interner.lazy(DefId(7));
        for candidate in [object_like, lazy, TypeId::OBJECT, TypeId::FUNCTION] {
            assert!(!is_global_interface_by_identity(
                &interner,
                candidate,
                IntrinsicKind::Object
            ));
            assert!(!is_global_interface_by_identity(
                &interner,
                candidate,
                IntrinsicKind::Function
            ));
        }
        let env = TypeEnvironment::new();
        assert!(!is_global_interface_by_identity_with_resolver(
            &interner,
            &env,
            object_like,
            IntrinsicKind::Object
        ));
    }
// TSZ_INLINE_TEST_END 9c735cf6d9887c21f0f46350b81325a38bb6021e65ee9d4a40bfd4a382c479c7

// TSZ_INLINE_TEST_BEGIN a0f59e5e874761b022d78d7a0493f35ff76767e5553e47e44adff2c0417bba25 313 renamed_object_shaped_interface_is_not_identity_matched
    /// A user interface that is structurally identical to `Object` (renamed,
    /// e.g. `interface MyObjectLike { constructor: ...; hasOwnProperty: ...;
    /// ... }`) must NOT match the identity tier; only the registered boxed
    /// type does. The structural fallback intentionally still matches it —
    /// that is the documented compatibility hazard the identity tier exists
    /// to replace.
    #[test]
    fn renamed_object_shaped_interface_is_not_identity_matched() {
        let interner = TypeInterner::new();
        let user_iface = object_like_shape(&interner);
        // Register a DIFFERENT type as the real boxed Object.
        let real_object = interner.object(vec![PropertyInfo::new(
            interner.intern_string("toString"),
            TypeId::ANY,
        )]);
        interner.register_boxed_type(IntrinsicKind::Object, real_object);

        assert!(!is_global_interface_by_identity(
            &interner,
            user_iface,
            IntrinsicKind::Object
        ));
        assert!(is_global_interface_by_identity(
            &interner,
            real_object,
            IntrinsicKind::Object
        ));
        // Documented fallback hazard: the shared structural matcher still
        // accepts the impostor shape.
        assert!(matches_global_object_interface_shape(&interner, user_iface));
        assert!(is_global_object_interface(&interner, user_iface));
    }
// TSZ_INLINE_TEST_END a0f59e5e874761b022d78d7a0493f35ff76767e5553e47e44adff2c0417bba25

// TSZ_INLINE_TEST_BEGIN 3cf1b56b830c93d3271ba540ca568fdbcf6dc0bad0cfc0e7ddc0162551a3d34a 342 lazy_def_id_identity_matches_after_registration
    /// `Lazy(DefId)` forms registered as boxed def ids match the identity
    /// tier through both the interner and a resolver registry.
    #[test]
    fn lazy_def_id_identity_matches_after_registration() {
        let interner = TypeInterner::new();
        let def_id = DefId(42);
        let lazy = interner.lazy(def_id);
        let other_lazy = interner.lazy(DefId(43));

        let mut env = TypeEnvironment::new();
        env.register_boxed_def_id(IntrinsicKind::Function, def_id);
        assert!(is_global_interface_by_identity_with_resolver(
            &interner,
            &env,
            lazy,
            IntrinsicKind::Function
        ));
        assert!(!is_global_interface_by_identity_with_resolver(
            &interner,
            &env,
            other_lazy,
            IntrinsicKind::Function
        ));
        // Interner-registry tier (resolver may be a different instance).
        interner.register_boxed_def_id(IntrinsicKind::Object, def_id);
        assert!(is_global_interface_by_identity(
            &interner,
            lazy,
            IntrinsicKind::Object
        ));
        assert!(!is_global_interface_by_identity(
            &interner,
            other_lazy,
            IntrinsicKind::Object
        ));
    }
// TSZ_INLINE_TEST_END 3cf1b56b830c93d3271ba540ca568fdbcf6dc0bad0cfc0e7ddc0162551a3d34a

// TSZ_INLINE_TEST_BEGIN 311397b077c6bfe444ee51e8f657f9b0518934e3c1ef75a5888aaec20c7fee93 379 function_structural_fallback_requires_apply_call_bind_and_cap
    /// Structural Function fallback: requires all of `apply`/`call`/`bind`
    /// and rejects shapes above the property-count cap.
    #[test]
    fn function_structural_fallback_requires_apply_call_bind_and_cap() {
        let interner = TypeInterner::new();
        let with_bind = function_like_shape(&interner, true);
        let without_bind = function_like_shape(&interner, false);
        assert!(matches_global_function_interface_shape(
            &interner, with_bind
        ));
        assert!(!matches_global_function_interface_shape(
            &interner,
            without_bind
        ));

        // 21 properties incl. apply/call/bind: over the cap, must not match.
        let mut props: Vec<PropertyInfo> = (0..18)
            .map(|i| PropertyInfo::new(interner.intern_string(&format!("p{i}")), TypeId::ANY))
            .collect();
        for name in ["apply", "call", "bind"] {
            props.push(PropertyInfo::new(interner.intern_string(name), TypeId::ANY));
        }
        let oversized = interner.object(props);
        assert!(!matches_global_function_interface_shape(
            &interner, oversized
        ));
        // Intrinsics never match the structural tier.
        assert!(!matches_global_function_interface_shape(
            &interner,
            TypeId::FUNCTION
        ));
    }
// TSZ_INLINE_TEST_END 311397b077c6bfe444ee51e8f657f9b0518934e3c1ef75a5888aaec20c7fee93

// TSZ_INLINE_TEST_BEGIN 85c203d1a9336b2424e53d95afb0ecbdf4f74a830af26eb2ba0398b75084ba86 412 object_structural_fallback_requires_all_probe_members
    /// Structural Object fallback requires `propertyIsEnumerable` (the
    /// unified, strictest historical probe) and respects the 7-property cap.
    #[test]
    fn object_structural_fallback_requires_all_probe_members() {
        let interner = TypeInterner::new();
        let full = object_like_shape(&interner);
        assert!(matches_global_object_interface_shape(&interner, full));

        let missing_enumerable = interner.object(
            ["constructor", "toString", "hasOwnProperty", "isPrototypeOf"]
                .iter()
                .map(|name| PropertyInfo::new(interner.intern_string(name), TypeId::ANY))
                .collect(),
        );
        assert!(!matches_global_object_interface_shape(
            &interner,
            missing_enumerable
        ));

        // 8 properties: over the cap (derived interfaces like Boolean).
        let mut props: Vec<PropertyInfo> = [
            "constructor",
            "toString",
            "toLocaleString",
            "valueOf",
            "hasOwnProperty",
            "isPrototypeOf",
            "propertyIsEnumerable",
        ]
        .iter()
        .map(|name| PropertyInfo::new(interner.intern_string(name), TypeId::ANY))
        .collect();
        props.push(PropertyInfo::new(
            interner.intern_string("extra"),
            TypeId::ANY,
        ));
        let oversized = interner.object(props);
        assert!(!matches_global_object_interface_shape(&interner, oversized));
    }
// TSZ_INLINE_TEST_END 85c203d1a9336b2424e53d95afb0ecbdf4f74a830af26eb2ba0398b75084ba86
