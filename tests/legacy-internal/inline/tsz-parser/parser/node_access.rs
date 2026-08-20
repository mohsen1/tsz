//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-parser/src/parser/node_access.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN b3dc82b5b71fb934b9d8c6f5c0ce609313016e7ee6a1aa6552920fc302679d54 1164 returns_true_for_synthesized_recovery_placeholder
    #[test]
    fn returns_true_for_synthesized_recovery_placeholder() {
        let mut arena = NodeArena::with_capacity(8);
        let idx = arena.add_identifier(
            SyntaxKind::Identifier as u16,
            0,
            0,
            IdentifierData {
                atom: AstAtom::NONE,
                escaped_text: IdentText::empty(),
                original_text: None,
            },
        );
        assert!(arena.is_missing_recovery_identifier(idx));
    }
// TSZ_INLINE_TEST_END b3dc82b5b71fb934b9d8c6f5c0ce609313016e7ee6a1aa6552920fc302679d54

// TSZ_INLINE_TEST_BEGIN 176f12ff2e9c9988931ea8cd7c314f2c966c91ffc67b851bf2f405b3db2f1d25 1180 returns_false_for_real_named_identifier
    #[test]
    fn returns_false_for_real_named_identifier() {
        let mut arena = NodeArena::with_capacity(8);
        // A real identifier has a non-NONE atom AND non-empty escaped_text;
        // either condition alone is enough for the helper to reject it.
        let idx = arena.add_identifier(
            SyntaxKind::Identifier as u16,
            0,
            3,
            IdentifierData {
                atom: AstAtom(1),
                escaped_text: IdentText::from("foo"),
                original_text: None,
            },
        );
        assert!(!arena.is_missing_recovery_identifier(idx));
    }
// TSZ_INLINE_TEST_END 176f12ff2e9c9988931ea8cd7c314f2c966c91ffc67b851bf2f405b3db2f1d25

// TSZ_INLINE_TEST_BEGIN a4804f7cf8ffb186be52745db6075869af8b51698ac55441d16046344cc9bacc 1198 returns_false_when_only_atom_is_set
    #[test]
    fn returns_false_when_only_atom_is_set() {
        let mut arena = NodeArena::with_capacity(8);
        let idx = arena.add_identifier(
            SyntaxKind::Identifier as u16,
            0,
            0,
            IdentifierData {
                atom: AstAtom(1),
                escaped_text: IdentText::empty(),
                original_text: None,
            },
        );
        assert!(!arena.is_missing_recovery_identifier(idx));
    }
// TSZ_INLINE_TEST_END a4804f7cf8ffb186be52745db6075869af8b51698ac55441d16046344cc9bacc

// TSZ_INLINE_TEST_BEGIN 99b30498751a2b322746edd11e373fa21ac5df13df2fa2d3692b4ae80c91e266 1214 returns_false_when_only_escaped_text_is_set
    #[test]
    fn returns_false_when_only_escaped_text_is_set() {
        let mut arena = NodeArena::with_capacity(8);
        let idx = arena.add_identifier(
            SyntaxKind::Identifier as u16,
            0,
            3,
            IdentifierData {
                atom: AstAtom::NONE,
                escaped_text: IdentText::from("x"),
                original_text: None,
            },
        );
        assert!(!arena.is_missing_recovery_identifier(idx));
    }
// TSZ_INLINE_TEST_END 99b30498751a2b322746edd11e373fa21ac5df13df2fa2d3692b4ae80c91e266

// TSZ_INLINE_TEST_BEGIN 2d2d01558432521008b3737b28b584a5683c4373fb972b582c6cca258e5096c2 1230 returns_false_for_non_identifier_node
    #[test]
    fn returns_false_for_non_identifier_node() {
        let arena = NodeArena::with_capacity(8);
        // Default-init NodeIndex points at nothing — get() returns None.
        assert!(!arena.is_missing_recovery_identifier(NodeIndex::NONE));
    }
// TSZ_INLINE_TEST_END 2d2d01558432521008b3737b28b584a5683c4373fb972b582c6cca258e5096c2
