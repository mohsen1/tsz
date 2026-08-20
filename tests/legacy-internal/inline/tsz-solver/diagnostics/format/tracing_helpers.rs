//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/diagnostics/format/tracing_helpers.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN efa3f339b50df37061c3e98ce7687bd0b23e8132e7240cdb710290fe02eb7116 121 display_intrinsic_types
    #[test]
    fn display_intrinsic_types() {
        let interner = TypeInterner::new();
        let db: &dyn TypeDatabase = &interner;

        assert_eq!(TypeDisplay::new(db, TypeId::STRING).to_string(), "string");
        assert_eq!(TypeDisplay::new(db, TypeId::NUMBER).to_string(), "number");
        assert_eq!(TypeDisplay::new(db, TypeId::BOOLEAN).to_string(), "boolean");
        assert_eq!(TypeDisplay::new(db, TypeId::ANY).to_string(), "any");
        assert_eq!(TypeDisplay::new(db, TypeId::UNKNOWN).to_string(), "unknown");
        assert_eq!(TypeDisplay::new(db, TypeId::NEVER).to_string(), "never");
        assert_eq!(TypeDisplay::new(db, TypeId::VOID).to_string(), "void");
        assert_eq!(
            TypeDisplay::new(db, TypeId::UNDEFINED).to_string(),
            "undefined"
        );
        assert_eq!(TypeDisplay::new(db, TypeId::NULL).to_string(), "null");
        assert_eq!(TypeDisplay::new(db, TypeId::OBJECT).to_string(), "object");
        assert_eq!(TypeDisplay::new(db, TypeId::BIGINT).to_string(), "bigint");
        assert_eq!(TypeDisplay::new(db, TypeId::SYMBOL).to_string(), "symbol");
        assert_eq!(
            TypeDisplay::new(db, TypeId::FUNCTION).to_string(),
            "Function"
        );
        assert_eq!(TypeDisplay::new(db, TypeId::ERROR).to_string(), "error");
        assert_eq!(
            TypeDisplay::new(db, TypeId::BOOLEAN_TRUE).to_string(),
            "true"
        );
        assert_eq!(
            TypeDisplay::new(db, TypeId::BOOLEAN_FALSE).to_string(),
            "false"
        );
    }
// TSZ_INLINE_TEST_END efa3f339b50df37061c3e98ce7687bd0b23e8132e7240cdb710290fe02eb7116

// TSZ_INLINE_TEST_BEGIN 18d35a439f24caf3f1b1847b9c5dd592b360bdad560e8ca565292c12b4a2f018 156 display_literal_types
    #[test]
    fn display_literal_types() {
        let interner = TypeInterner::new();
        let db: &dyn TypeDatabase = &interner;

        let str_lit = interner.literal_string("hello");
        assert_eq!(TypeDisplay::new(db, str_lit).to_string(), r#""hello""#);

        let num_lit = interner.literal_number(42.0);
        assert_eq!(TypeDisplay::new(db, num_lit).to_string(), "42");
    }
// TSZ_INLINE_TEST_END 18d35a439f24caf3f1b1847b9c5dd592b360bdad560e8ca565292c12b4a2f018

// TSZ_INLINE_TEST_BEGIN 92116add357b25c94b42925df90f0a14e84fca9bfaf1441f8c8dae9f9ac185cd 168 display_union_types
    #[test]
    fn display_union_types() {
        let interner = TypeInterner::new();
        let db: &dyn TypeDatabase = &interner;

        let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let result = TypeDisplay::new(db, union).to_string();
        assert!(
            result.contains("string") && result.contains("number"),
            "union display should contain both members: {result}"
        );
    }
// TSZ_INLINE_TEST_END 92116add357b25c94b42925df90f0a14e84fca9bfaf1441f8c8dae9f9ac185cd

// TSZ_INLINE_TEST_BEGIN 8bbd43f92e31c5d278a0959d1a950171f8742c82fb9a3d4ea06bc4785d96097b 181 display_none_type
    #[test]
    fn display_none_type() {
        let interner = TypeInterner::new();
        let db: &dyn TypeDatabase = &interner;

        // TypeId::NONE (0) has no type data — should not panic
        let result = TypeDisplay::new(db, TypeId::NONE).to_string();
        assert!(!result.is_empty(), "NONE type should produce some output");
    }
// TSZ_INLINE_TEST_END 8bbd43f92e31c5d278a0959d1a950171f8742c82fb9a3d4ea06bc4785d96097b

// TSZ_INLINE_TEST_BEGIN fb456c42690f625eca3c1b84553812ba5b047c74e6c9c15118c2e1c699470e58 191 relation_display_format
    #[test]
    fn relation_display_format() {
        let interner = TypeInterner::new();
        let db: &dyn TypeDatabase = &interner;

        let display = RelationDisplay::new(db, TypeId::NUMBER, TypeId::STRING);
        assert_eq!(display.to_string(), "number <: string");
    }
// TSZ_INLINE_TEST_END fb456c42690f625eca3c1b84553812ba5b047c74e6c9c15118c2e1c699470e58

// TSZ_INLINE_TEST_BEGIN 7883dac45817b6c4d411c845747490dad1c48e5df8062a7b64e73cb59814efc6 200 display_does_not_panic_on_unknown_type_id
    #[test]
    fn display_does_not_panic_on_unknown_type_id() {
        let interner = TypeInterner::new();
        let db: &dyn TypeDatabase = &interner;

        // High TypeId that doesn't exist — should not panic
        let result = TypeDisplay::new(db, TypeId(99999)).to_string();
        assert!(!result.is_empty());
    }
// TSZ_INLINE_TEST_END 7883dac45817b6c4d411c845747490dad1c48e5df8062a7b64e73cb59814efc6
