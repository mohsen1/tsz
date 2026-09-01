//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/type_queries/data/content_predicates.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN bc744dbf605a3a9782e774c2d5c8f376065df1f7935326baa2b9d02060498e99 1883 follows_preserved_application_display_alias
    #[test]
    fn follows_preserved_application_display_alias() {
        let interner = TypeInterner::new();
        let structural = interner.object(vec![]);
        let alias = interner.application(TypeId::OBJECT, vec![TypeId::UNKNOWN]);
        interner.store_display_alias(structural, alias);

        assert!(contains_application_unknown_arg(&interner, structural));
    }
// TSZ_INLINE_TEST_END bc744dbf605a3a9782e774c2d5c8f376065df1f7935326baa2b9d02060498e99

// TSZ_INLINE_TEST_BEGIN cf2774f030c9f95c64e982bc938a77866e909401def2f18cf523adfa45ad5c9c 1904 plain_method_object_body_has_no_conditional
    // A generic interface body shaped like `{ m(): void; v?: T }`: a method
    // member plus a data member, no conditional anywhere. This is the #13554
    // case that must be consumable cross-file, so the gate must report `false`.
    #[test]
    fn plain_method_object_body_has_no_conditional() {
        let interner = TypeInterner::new();
        let m = interner.intern_string("m");
        let v = interner.intern_string("v");
        let method = interner.function(FunctionShape::new(vec![], TypeId::VOID));
        let body = interner.object(vec![
            PropertyInfo::method(m, method),
            PropertyInfo::opt(v, TypeId::NUMBER),
        ]);
        let mut resolve = |_: DefId| None;
        assert!(!contains_conditional_through_aliases(
            &interner,
            body,
            &mut resolve
        ));
    }
// TSZ_INLINE_TEST_END cf2774f030c9f95c64e982bc938a77866e909401def2f18cf523adfa45ad5c9c

// TSZ_INLINE_TEST_BEGIN 61c6b9b426507bc422f264ffe6768115114bd7357a0b6ae4f2742563b727156b 1927 method_returning_alias_to_conditional_is_detected
    // A method whose return type applies an alias whose body is a conditional
    // (`read(): MappedResponseType<R, T>`). The standard content walk treats
    // the application base as an opaque leaf, so resolution must follow the
    // alias to find the conditional. Detected through both the object property
    // and the applied alias.
    #[test]
    fn method_returning_alias_to_conditional_is_detected() {
        let interner = TypeInterner::new();
        let cond = interner.conditional(ConditionalType {
            check_type: TypeId::STRING,
            extends_type: TypeId::STRING,
            true_type: TypeId::NUMBER,
            false_type: TypeId::BOOLEAN,
            is_distributive: false,
        });
        let def = DefId(7);
        let alias_app = interner.application(interner.lazy(def), vec![TypeId::STRING]);
        let method = interner.function(FunctionShape::new(vec![], alias_app));
        let read = interner.intern_string("read");
        let body = interner.object(vec![PropertyInfo::method(read, method)]);

        let mut resolve = |d: DefId| (d == def).then_some(cond);
        assert!(contains_conditional_through_aliases(
            &interner,
            body,
            &mut resolve
        ));

        // When the alias body is unavailable, the conditional behind it cannot
        // be observed and the body is treated as inert (no false gating).
        let mut unresolved = |_: DefId| None;
        assert!(!contains_conditional_through_aliases(
            &interner,
            body,
            &mut unresolved
        ));
    }
// TSZ_INLINE_TEST_END 61c6b9b426507bc422f264ffe6768115114bd7357a0b6ae4f2742563b727156b

// TSZ_INLINE_TEST_BEGIN 42d5d3f9bbb443434c0d4cea76e96dd98e09bcdff3c7c8bd609cef63b3c07f71 1962 direct_conditional_member_is_detected
    // A directly-present conditional member is detected without alias
    // resolution.
    #[test]
    fn direct_conditional_member_is_detected() {
        let interner = TypeInterner::new();
        let cond = interner.conditional(ConditionalType {
            check_type: TypeId::STRING,
            extends_type: TypeId::STRING,
            true_type: TypeId::NUMBER,
            false_type: TypeId::BOOLEAN,
            is_distributive: false,
        });
        let p = interner.intern_string("p");
        let body = interner.object(vec![PropertyInfo::new(p, cond)]);
        let mut resolve = |_: DefId| None;
        assert!(contains_conditional_through_aliases(
            &interner,
            body,
            &mut resolve
        ));
    }
// TSZ_INLINE_TEST_END 42d5d3f9bbb443434c0d4cea76e96dd98e09bcdff3c7c8bd609cef63b3c07f71
