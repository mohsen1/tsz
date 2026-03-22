//! Unit tests for the symbols module in tsz-binder.
//!
//! Tests cover SymbolId, Symbol, `SymbolTable`, `SymbolArena`, and `symbol_flags`.

use tsz_binder::{Symbol, SymbolArena, SymbolId, SymbolTable, symbol_flags};

// =============================================================================
// SymbolId Tests
// =============================================================================

mod symbol_id_tests {
    use super::*;

    #[test]
    fn none_is_none() {
        assert!(SymbolId::NONE.is_none());
        assert!(!SymbolId::NONE.is_some());
    }

    #[test]
    fn none_value_is_max_u32() {
        assert_eq!(SymbolId::NONE.0, u32::MAX);
    }

    #[test]
    fn some_values_are_not_none() {
        let id = SymbolId(0);
        assert!(id.is_some());
        assert!(!id.is_none());

        let id = SymbolId(42);
        assert!(id.is_some());
        assert!(!id.is_none());

        let id = SymbolId(u32::MAX - 1);
        assert!(id.is_some());
        assert!(!id.is_none());
    }

    #[test]
    fn equality_and_ordering() {
        let id1 = SymbolId(1);
        let id2 = SymbolId(2);
        let id3 = SymbolId(1);

        assert_eq!(id1, id3);
        assert_ne!(id1, id2);
        assert!(id1 < id2);
        assert!(id2 > id1);
    }
}

// =============================================================================
// Symbol Tests
// =============================================================================

mod symbol_tests {
    use super::*;

    #[test]
    fn new_creates_symbol_with_flags_and_name() {
        let id = SymbolId(0);
        let symbol = Symbol::new(id, symbol_flags::FUNCTION, "myFunc".to_string());

        assert_eq!(symbol.id, id);
        assert_eq!(symbol.flags, symbol_flags::FUNCTION);
        assert_eq!(symbol.escaped_name, "myFunc");
    }

    #[test]
    fn new_initializes_empty_declarations() {
        let symbol = Symbol::new(SymbolId(0), symbol_flags::NONE, "x".to_string());
        assert!(symbol.declarations.is_empty());
    }

    #[test]
    fn new_initializes_default_values() {
        let symbol = Symbol::new(SymbolId(0), symbol_flags::NONE, "x".to_string());

        assert!(symbol.exports.is_none());
        assert!(symbol.members.is_none());
        assert!(!symbol.is_exported);
        assert!(!symbol.is_type_only);
        assert!(symbol.import_module.is_none());
        assert!(symbol.import_name.is_none());
        assert!(!symbol.is_umd_export);
    }

    #[test]
    fn has_flags_true_when_all_flags_present() {
        let composite = symbol_flags::FUNCTION | symbol_flags::EXPORT_VALUE;
        let symbol = Symbol::new(SymbolId(0), composite, "f".to_string());

        assert!(symbol.has_flags(symbol_flags::FUNCTION));
        assert!(symbol.has_flags(symbol_flags::EXPORT_VALUE));
        assert!(symbol.has_flags(composite));
    }

    #[test]
    fn has_flags_false_when_some_flags_missing() {
        let symbol = Symbol::new(SymbolId(0), symbol_flags::FUNCTION, "f".to_string());

        assert!(!symbol.has_flags(symbol_flags::FUNCTION | symbol_flags::CLASS));
    }

    #[test]
    fn has_flags_true_for_none() {
        let symbol = Symbol::new(SymbolId(0), symbol_flags::FUNCTION, "f".to_string());
        assert!(symbol.has_flags(symbol_flags::NONE));
    }

    #[test]
    fn has_any_flags_true_when_any_flag_present() {
        let composite = symbol_flags::FUNCTION | symbol_flags::CLASS;
        let symbol = Symbol::new(SymbolId(0), composite, "f".to_string());

        assert!(symbol.has_any_flags(symbol_flags::FUNCTION));
        assert!(symbol.has_any_flags(symbol_flags::CLASS));
        assert!(symbol.has_any_flags(symbol_flags::FUNCTION | symbol_flags::INTERFACE));
    }

    #[test]
    fn has_any_flags_false_when_no_flags_match() {
        let symbol = Symbol::new(SymbolId(0), symbol_flags::FUNCTION, "f".to_string());

        assert!(!symbol.has_any_flags(symbol_flags::CLASS));
        assert!(!symbol.has_any_flags(symbol_flags::CLASS | symbol_flags::INTERFACE));
    }

    #[test]
    fn has_any_flags_false_for_none() {
        let symbol = Symbol::new(SymbolId(0), symbol_flags::FUNCTION, "f".to_string());
        assert!(!symbol.has_any_flags(symbol_flags::NONE));
    }
}

// =============================================================================
// SymbolTable Tests
// =============================================================================

mod symbol_table_tests {
    use super::*;

    #[test]
    fn new_creates_empty_table() {
        let table = SymbolTable::new();
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn default_creates_empty_table() {
        let table = SymbolTable::default();
        assert!(table.is_empty());
    }

    #[test]
    fn set_and_get() {
        let mut table = SymbolTable::new();
        let id = SymbolId(1);

        table.set("foo".to_string(), id);

        assert_eq!(table.get("foo"), Some(id));
        assert_eq!(table.get("bar"), None);
    }

    #[test]
    fn set_overwrites_existing() {
        let mut table = SymbolTable::new();

        table.set("foo".to_string(), SymbolId(1));
        table.set("foo".to_string(), SymbolId(2));

        assert_eq!(table.get("foo"), Some(SymbolId(2)));
    }

    #[test]
    fn has_returns_true_for_existing_names() {
        let mut table = SymbolTable::new();
        table.set("foo".to_string(), SymbolId(1));

        assert!(table.has("foo"));
        assert!(!table.has("bar"));
    }

    #[test]
    fn remove_deletes_entry() {
        let mut table = SymbolTable::new();
        table.set("foo".to_string(), SymbolId(1));

        let removed = table.remove("foo");

        assert_eq!(removed, Some(SymbolId(1)));
        assert!(!table.has("foo"));
        assert!(table.is_empty());
    }

    #[test]
    fn remove_returns_none_for_missing() {
        let mut table = SymbolTable::new();
        assert_eq!(table.remove("nonexistent"), None);
    }

    #[test]
    fn clear_empties_table() {
        let mut table = SymbolTable::new();
        table.set("a".to_string(), SymbolId(1));
        table.set("b".to_string(), SymbolId(2));

        table.clear();

        assert!(table.is_empty());
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn len_returns_correct_count() {
        let mut table = SymbolTable::new();

        assert_eq!(table.len(), 0);

        table.set("a".to_string(), SymbolId(1));
        assert_eq!(table.len(), 1);

        table.set("b".to_string(), SymbolId(2));
        assert_eq!(table.len(), 2);

        table.set("a".to_string(), SymbolId(3)); // Overwrite
        assert_eq!(table.len(), 2);

        table.remove("a");
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn iter_yields_all_entries() {
        let mut table = SymbolTable::new();
        table.set("a".to_string(), SymbolId(1));
        table.set("b".to_string(), SymbolId(2));

        let entries: Vec<_> = table.iter().collect();
        assert_eq!(entries.len(), 2);

        let names: Vec<&String> = entries.iter().map(|(name, _)| *name).collect();
        assert!(names.contains(&&"a".to_string()));
        assert!(names.contains(&&"b".to_string()));
    }
}

// =============================================================================
// SymbolArena Tests
// =============================================================================

mod symbol_arena_tests {
    use super::*;

    #[test]
    fn new_creates_empty_arena() {
        let arena = SymbolArena::new();
        assert!(arena.is_empty());
        assert_eq!(arena.len(), 0);
    }

    #[test]
    fn default_creates_empty_arena() {
        let arena = SymbolArena::default();
        assert!(arena.is_empty());
    }

    #[test]
    fn alloc_returns_sequential_ids() {
        let mut arena = SymbolArena::new();

        let id1 = arena.alloc(symbol_flags::FUNCTION, "f1".to_string());
        let id2 = arena.alloc(symbol_flags::CLASS, "c1".to_string());
        let id3 = arena.alloc(symbol_flags::INTERFACE, "i1".to_string());

        assert_eq!(id1, SymbolId(0));
        assert_eq!(id2, SymbolId(1));
        assert_eq!(id3, SymbolId(2));
    }

    #[test]
    fn alloc_creates_symbol_with_correct_data() {
        let mut arena = SymbolArena::new();

        let id = arena.alloc(symbol_flags::FUNCTION, "myFunc".to_string());
        let symbol = arena.get(id).unwrap();

        assert_eq!(symbol.id, id);
        assert_eq!(symbol.flags, symbol_flags::FUNCTION);
        assert_eq!(symbol.escaped_name, "myFunc");
    }

    #[test]
    fn get_returns_none_for_none_id() {
        let arena = SymbolArena::new();
        assert!(arena.get(SymbolId::NONE).is_none());
    }

    #[test]
    fn get_mut_returns_none_for_none_id() {
        let mut arena = SymbolArena::new();
        assert!(arena.get_mut(SymbolId::NONE).is_none());
    }

    #[test]
    fn get_returns_symbol_for_valid_id() {
        let mut arena = SymbolArena::new();
        let id = arena.alloc(symbol_flags::FUNCTION, "f".to_string());

        let symbol = arena.get(id);
        assert!(symbol.is_some());
        assert_eq!(symbol.unwrap().escaped_name, "f");
    }

    #[test]
    fn get_mut_allows_modification() {
        let mut arena = SymbolArena::new();
        let id = arena.alloc(symbol_flags::FUNCTION, "f".to_string());

        {
            let symbol = arena.get_mut(id).unwrap();
            symbol.flags |= symbol_flags::EXPORT_VALUE;
        }

        let symbol = arena.get(id).unwrap();
        assert!(symbol.has_flags(symbol_flags::EXPORT_VALUE));
    }

    #[test]
    fn alloc_from_copies_symbol_with_new_id() {
        let mut arena = SymbolArena::new();

        // Create original symbol
        let orig_id = arena.alloc(symbol_flags::CLASS, "MyClass".to_string());
        {
            let orig = arena.get_mut(orig_id).unwrap();
            orig.is_exported = true;
            orig.declarations.push(tsz_parser::NodeIndex::NONE);
        }

        // Clone it - need to get the symbol first, then alloc_from takes a reference
        let orig_symbol = arena.get(orig_id).unwrap().clone();
        let cloned_id = arena.alloc_from(&orig_symbol);

        // IDs should be different
        assert_ne!(orig_id, cloned_id);

        // Data should be copied
        let cloned = arena.get(cloned_id).unwrap();
        assert_eq!(cloned.escaped_name, "MyClass");
        assert_eq!(cloned.flags, symbol_flags::CLASS);
        assert!(cloned.is_exported);
        assert_eq!(cloned.declarations.len(), 1);

        // But id field should be updated to new ID
        assert_eq!(cloned.id, cloned_id);
    }

    #[test]
    fn clear_empties_arena() {
        let mut arena = SymbolArena::new();
        arena.alloc(symbol_flags::FUNCTION, "f".to_string());
        arena.alloc(symbol_flags::CLASS, "c".to_string());

        arena.clear();

        assert!(arena.is_empty());
        assert_eq!(arena.len(), 0);
    }

    #[test]
    fn iter_yields_all_symbols() {
        let mut arena = SymbolArena::new();
        arena.alloc(symbol_flags::FUNCTION, "f".to_string());
        arena.alloc(symbol_flags::CLASS, "c".to_string());

        let symbols: Vec<_> = arena.iter().collect();
        assert_eq!(symbols.len(), 2);

        let names: Vec<&str> = symbols.iter().map(|s| s.escaped_name.as_str()).collect();
        assert!(names.contains(&"f"));
        assert!(names.contains(&"c"));
    }

    #[test]
    fn with_capacity_preallocates() {
        let arena = SymbolArena::with_capacity(100);
        assert!(arena.is_empty());
        assert_eq!(arena.len(), 0);
        // Internal capacity is not directly accessible, but we can verify it works
    }

    #[test]
    fn new_with_base_offsets_ids() {
        let arena = SymbolArena::new_with_base(1000);
        let mut arena = arena;

        let id = arena.alloc(symbol_flags::FUNCTION, "f".to_string());

        assert_eq!(id, SymbolId(1000));
    }

    #[test]
    fn new_with_base_get_rejects_ids_below_base() {
        let arena = SymbolArena::new_with_base(1000);

        // ID from default arena (0) should not be found
        assert!(arena.get(SymbolId(0)).is_none());
    }

    #[test]
    fn new_with_base_get_mut_rejects_ids_below_base() {
        let mut arena = SymbolArena::new_with_base(1000);

        // ID from default arena (0) should not be found
        assert!(arena.get_mut(SymbolId(0)).is_none());
    }

    #[test]
    fn find_by_name_returns_first_match() {
        let mut arena = SymbolArena::new();
        arena.alloc(symbol_flags::FUNCTION, "foo".to_string());
        arena.alloc(symbol_flags::CLASS, "bar".to_string());
        arena.alloc(symbol_flags::INTERFACE, "baz".to_string());

        let found = arena.find_by_name("bar");
        assert_eq!(found, Some(SymbolId(1)));

        let not_found = arena.find_by_name("qux");
        assert_eq!(not_found, None);
    }

    #[test]
    fn find_all_by_name_returns_all_matches() {
        let mut arena = SymbolArena::new();

        // Create multiple symbols with same name (shadowing scenario)
        arena.alloc(symbol_flags::FUNCTION, "x".to_string());
        arena.alloc(symbol_flags::CLASS, "y".to_string());
        arena.alloc(symbol_flags::FUNCTION, "x".to_string()); // Duplicate name

        let all_x = arena.find_all_by_name("x");
        assert_eq!(all_x.len(), 2);
        assert_eq!(all_x, vec![SymbolId(0), SymbolId(2)]);

        let all_y = arena.find_all_by_name("y");
        assert_eq!(all_y.len(), 1);

        let all_z = arena.find_all_by_name("z");
        assert!(all_z.is_empty());
    }

    #[test]
    fn deserialization_rebuilds_name_index() {
        // Build an arena with several symbols.
        let mut arena = SymbolArena::new();
        arena.alloc(symbol_flags::FUNCTION, "alpha".to_string());
        arena.alloc(symbol_flags::CLASS, "beta".to_string());
        arena.alloc(symbol_flags::INTERFACE, "alpha".to_string()); // duplicate name

        // Verify the original arena works.
        assert_eq!(arena.find_by_name("alpha"), Some(SymbolId(0)));
        assert_eq!(arena.find_all_by_name("alpha").len(), 2);
        assert_eq!(arena.find_by_name("beta"), Some(SymbolId(1)));

        // Serialize and deserialize via JSON (the name_index is #[serde(skip)]).
        let json = serde_json::to_string(&arena).expect("serialize");
        let deserialized: SymbolArena = serde_json::from_str(&json).expect("deserialize");

        // The deserialized arena must have the same lookup behavior —
        // the custom Deserialize impl should have rebuilt the name index.
        assert_eq!(deserialized.find_by_name("alpha"), Some(SymbolId(0)));
        assert_eq!(deserialized.find_all_by_name("alpha").len(), 2);
        assert_eq!(deserialized.find_by_name("beta"), Some(SymbolId(1)));
        assert_eq!(deserialized.find_by_name("nonexistent"), None);
        assert!(deserialized.find_all_by_name("nonexistent").is_empty());
    }
}

// =============================================================================
// Symbol Flag Composite Tests
// =============================================================================

#[allow(clippy::assertions_on_constants)]
mod symbol_flags_tests {
    use super::*;

    #[test]
    fn none_is_zero() {
        assert_eq!(symbol_flags::NONE, 0);
    }

    #[test]
    fn variable_composite_includes_both_scoped_kinds() {
        const {
            assert!(symbol_flags::VARIABLE & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0);
        }
        const {
            assert!(symbol_flags::VARIABLE & symbol_flags::BLOCK_SCOPED_VARIABLE != 0);
        }
        assert_eq!(
            symbol_flags::VARIABLE,
            symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE
        );
    }

    #[test]
    fn enum_composite_includes_regular_and_const() {
        const { assert!(symbol_flags::ENUM & symbol_flags::REGULAR_ENUM != 0) }
        const { assert!(symbol_flags::ENUM & symbol_flags::CONST_ENUM != 0) }
        assert_eq!(
            symbol_flags::ENUM,
            symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM
        );
    }

    #[test]
    fn accessor_composite_includes_get_and_set() {
        const { assert!(symbol_flags::ACCESSOR & symbol_flags::GET_ACCESSOR != 0) }
        const { assert!(symbol_flags::ACCESSOR & symbol_flags::SET_ACCESSOR != 0) }
        assert_eq!(
            symbol_flags::ACCESSOR,
            symbol_flags::GET_ACCESSOR | symbol_flags::SET_ACCESSOR
        );
    }

    #[test]
    fn value_includes_common_value_kinds() {
        const { assert!(symbol_flags::VALUE & symbol_flags::VARIABLE != 0) }
        const { assert!(symbol_flags::VALUE & symbol_flags::FUNCTION != 0) }
        const { assert!(symbol_flags::VALUE & symbol_flags::CLASS != 0) }
        const { assert!(symbol_flags::VALUE & symbol_flags::ENUM != 0) }
    }

    #[test]
    fn type_includes_common_type_kinds() {
        const { assert!(symbol_flags::TYPE & symbol_flags::CLASS != 0) }
        const { assert!(symbol_flags::TYPE & symbol_flags::INTERFACE != 0) }
        const { assert!(symbol_flags::TYPE & symbol_flags::TYPE_ALIAS != 0) }
        const { assert!(symbol_flags::TYPE & symbol_flags::TYPE_PARAMETER != 0) }
    }

    #[test]
    fn namespace_includes_modules() {
        const { assert!(symbol_flags::NAMESPACE & symbol_flags::VALUE_MODULE != 0) }
        const { assert!(symbol_flags::NAMESPACE & symbol_flags::NAMESPACE_MODULE != 0) }
    }
}
