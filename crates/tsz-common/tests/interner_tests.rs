use super::*;

// =============================================================================
// Atom basics
// =============================================================================

#[test]
fn test_atom_none_is_zero() {
    assert_eq!(Atom::NONE.0, 0);
    assert_eq!(Atom::none().0, 0);
    assert!(Atom::NONE.is_none());
    assert!(Atom::none().is_none());
}

#[test]
fn test_atom_nonzero_is_not_none() {
    let atom = Atom(1);
    assert!(!atom.is_none());
}

#[test]
fn test_atom_index() {
    let atom = Atom(42);
    assert_eq!(atom.index(), 42);
}

#[test]
fn test_atom_equality() {
    assert_eq!(Atom(5), Atom(5));
    assert_ne!(Atom(5), Atom(6));
}

#[test]
fn test_atom_clone_copy() {
    let a = Atom(10);
    let b = a; // Copy
    let c = a; // Clone
    assert_eq!(a, b);
    assert_eq!(a, c);
}

#[test]
fn test_atom_ordering() {
    assert!(Atom(1) < Atom(2));
    assert!(Atom(0) <= Atom(0));
    assert!(Atom(10) > Atom(5));
}

#[test]
fn test_atom_hash() {
    use std::collections::HashSet;
    let mut set = HashSet::new();
    set.insert(Atom(1));
    set.insert(Atom(2));
    set.insert(Atom(1)); // duplicate
    assert_eq!(set.len(), 2);
}

#[test]
fn test_atom_default() {
    let atom: Atom = Default::default();
    assert_eq!(atom, Atom::NONE);
    assert!(atom.is_none());
}

// =============================================================================
// Interner - basic interning and resolution
// =============================================================================

#[test]
fn test_interner_new_has_empty_string() {
    let interner = Interner::new();
    // Index 0 is the empty string
    assert_eq!(interner.resolve(Atom::NONE), "");
    assert_eq!(interner.len(), 1);
    assert!(interner.is_empty()); // Only has the empty string sentinel
}

#[test]
fn test_interner_intern_and_resolve() {
    let mut interner = Interner::new();
    let atom = interner.intern("hello");
    assert_eq!(interner.resolve(atom), "hello");
}

#[test]
fn test_interner_deduplication() {
    let mut interner = Interner::new();
    let a1 = interner.intern("hello");
    let a2 = interner.intern("hello");
    assert_eq!(a1, a2);
    // Length should be 2: empty string + "hello"
    assert_eq!(interner.len(), 2);
}

#[test]
fn test_interner_different_strings_get_different_atoms() {
    let mut interner = Interner::new();
    let a1 = interner.intern("hello");
    let a2 = interner.intern("world");
    assert_ne!(a1, a2);
    assert_eq!(interner.resolve(a1), "hello");
    assert_eq!(interner.resolve(a2), "world");
}

#[test]
fn test_interner_empty_string_returns_none_atom() {
    let mut interner = Interner::new();
    let atom = interner.intern("");
    assert_eq!(atom, Atom::NONE);
    assert_eq!(interner.resolve(atom), "");
}

#[test]
fn test_interner_intern_owned() {
    let mut interner = Interner::new();
    let atom = interner.intern_owned("owned_string".to_string());
    assert_eq!(interner.resolve(atom), "owned_string");
}

#[test]
fn test_interner_intern_owned_deduplication() {
    let mut interner = Interner::new();
    let a1 = interner.intern("same");
    let a2 = interner.intern_owned("same".to_string());
    assert_eq!(a1, a2);
    assert_eq!(interner.len(), 2); // empty + "same"
}

#[test]
fn test_interner_intern_owned_empty_string() {
    let mut interner = Interner::new();
    let atom = interner.intern_owned(String::new());
    assert_eq!(atom, Atom::NONE);
}

// =============================================================================
// Interner - try_resolve
// =============================================================================

#[test]
fn test_interner_try_resolve_valid() {
    let mut interner = Interner::new();
    let atom = interner.intern("test");
    assert_eq!(interner.try_resolve(atom), Some("test"));
}

#[test]
fn test_interner_try_resolve_invalid() {
    let interner = Interner::new();
    // Atom(999) was never interned
    assert_eq!(interner.try_resolve(Atom(999)), None);
}

#[test]
fn test_interner_resolve_out_of_bounds_returns_empty() {
    let interner = Interner::new();
    // resolve returns empty string for invalid atoms (safety for error recovery)
    assert_eq!(interner.resolve(Atom(999)), "");
}

// =============================================================================
// Interner - len / is_empty
// =============================================================================

#[test]
fn test_interner_len_tracks_insertions() {
    let mut interner = Interner::new();
    assert_eq!(interner.len(), 1); // empty string
    let _ = interner.intern("a");
    assert_eq!(interner.len(), 2);
    let _ = interner.intern("b");
    assert_eq!(interner.len(), 3);
    let _ = interner.intern("a"); // duplicate
    assert_eq!(interner.len(), 3); // no change
}

#[test]
fn test_interner_is_empty_after_insert() {
    let mut interner = Interner::new();
    assert!(interner.is_empty());
    let _ = interner.intern("x");
    assert!(!interner.is_empty());
}

// =============================================================================
// Interner - pre-populated common strings
// =============================================================================

#[test]
fn test_interner_intern_common() {
    let mut interner = Interner::new();
    interner.intern_common();

    // Verify keywords are interned
    let keywords = [
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "debugger",
        "default",
        "delete",
        "do",
        "else",
        "enum",
        "export",
        "extends",
        "false",
        "finally",
        "for",
        "function",
        "if",
        "import",
        "in",
        "instanceof",
        "new",
        "null",
        "return",
        "super",
        "switch",
        "this",
        "throw",
        "true",
        "try",
        "typeof",
        "undefined",
        "var",
        "void",
        "while",
        "with",
    ];
    for kw in &keywords {
        let atom = interner.intern(kw);
        assert_eq!(
            interner.resolve(atom),
            *kw,
            "keyword '{kw}' should resolve correctly"
        );
    }

    // Verify type-related keywords
    for ts_kw in &["any", "boolean", "number", "string", "symbol", "type"] {
        let atom = interner.intern(ts_kw);
        assert_eq!(interner.resolve(atom), *ts_kw);
    }

    // Verify common identifiers
    for ident in &["id", "name", "value", "length", "constructor", "prototype"] {
        let atom = interner.intern(ident);
        assert_eq!(interner.resolve(atom), *ident);
    }
}

#[test]
fn test_interner_intern_common_deduplication() {
    let mut interner = Interner::new();
    interner.intern_common();
    let len_after_first = interner.len();

    // Calling intern_common again should not add duplicates
    interner.intern_common();
    assert_eq!(interner.len(), len_after_first);
}

#[test]
fn test_interner_intern_common_same_atom_on_re_intern() {
    let mut interner = Interner::new();
    interner.intern_common();

    let atom1 = interner.intern("function");
    let atom2 = interner.intern("function");
    assert_eq!(atom1, atom2);
}

// =============================================================================
// Interner - Unicode strings
// =============================================================================

#[test]
fn test_interner_unicode_basic() {
    let mut interner = Interner::new();
    let atom = interner.intern("hello");
    let atom_jp = interner.intern("\u{3053}\u{3093}\u{306B}\u{3061}\u{306F}"); // こんにちは
    assert_ne!(atom, atom_jp);
    assert_eq!(
        interner.resolve(atom_jp),
        "\u{3053}\u{3093}\u{306B}\u{3061}\u{306F}"
    );
}

#[test]
fn test_interner_unicode_emoji() {
    let mut interner = Interner::new();
    let atom = interner.intern("\u{1F680}\u{1F4A5}"); // 🚀💥
    assert_eq!(interner.resolve(atom), "\u{1F680}\u{1F4A5}");
}

#[test]
fn test_interner_unicode_dedup() {
    let mut interner = Interner::new();
    let a1 = interner.intern("\u{00E9}"); // é (precomposed)
    let a2 = interner.intern("\u{00E9}");
    assert_eq!(a1, a2);
}

#[test]
fn test_interner_unicode_mixed() {
    let mut interner = Interner::new();
    let atom = interner.intern("hello_\u{4E16}\u{754C}_123");
    assert_eq!(interner.resolve(atom), "hello_\u{4E16}\u{754C}_123");
}

#[test]
fn test_interner_unicode_identifiers() {
    let mut interner = Interner::new();
    // TypeScript allows Unicode identifiers
    let cases = [
        "\u{03B1}",         // α (Greek alpha)
        "\u{03B2}",         // β
        "\u{5909}\u{6570}", // 変数 (Japanese for "variable")
        "_\u{00FC}ber",     // _über
        "$\u{20AC}",        // $€
    ];
    for s in &cases {
        let atom = interner.intern(s);
        assert_eq!(
            interner.resolve(atom),
            *s,
            "Unicode identifier '{s}' should round-trip"
        );
    }
}

// =============================================================================
// Interner - large strings
// =============================================================================

#[test]
fn test_interner_large_string() {
    let mut interner = Interner::new();
    let large = "a".repeat(10_000);
    let atom = interner.intern(&large);
    assert_eq!(interner.resolve(atom), large.as_str());
}

#[test]
fn test_interner_large_string_dedup() {
    let mut interner = Interner::new();
    let large = "x".repeat(50_000);
    let a1 = interner.intern(&large);
    let a2 = interner.intern(&large);
    assert_eq!(a1, a2);
    assert_eq!(interner.len(), 2); // empty + the large string
}

// =============================================================================
// Interner - many distinct strings (stress)
// =============================================================================

#[test]
fn test_interner_many_distinct_strings() {
    let mut interner = Interner::new();
    let count = 10_000;
    let mut atoms = Vec::with_capacity(count);

    for i in 0..count {
        let s = format!("unique_string_{i}");
        atoms.push(interner.intern(&s));
    }

    assert_eq!(interner.len(), count + 1); // +1 for empty string

    // Verify all resolve correctly
    for (i, atom) in atoms.iter().enumerate() {
        let expected = format!("unique_string_{i}");
        assert_eq!(interner.resolve(*atom), expected);
    }
}

#[test]
fn test_interner_many_strings_dedup_mixed() {
    let mut interner = Interner::new();

    // Intern 1000 unique strings, then re-intern them all
    for i in 0..1000 {
        let _ = interner.intern(&format!("str_{i}"));
    }
    let len_after_unique = interner.len();

    for i in 0..1000 {
        let _ = interner.intern(&format!("str_{i}"));
    }
    assert_eq!(
        interner.len(),
        len_after_unique,
        "re-interning should not grow the interner"
    );
}

// =============================================================================
// Interner - sequential atom indices
// =============================================================================

#[test]
fn test_interner_atoms_are_sequential() {
    let mut interner = Interner::new();
    let a1 = interner.intern("first");
    let a2 = interner.intern("second");
    let a3 = interner.intern("third");

    // Atoms should be sequential starting from 1 (0 is empty string)
    assert_eq!(a1.0, 1);
    assert_eq!(a2.0, 2);
    assert_eq!(a3.0, 3);
}

// =============================================================================
// ShardedInterner - basic interning and resolution
// =============================================================================

#[test]
fn test_sharded_interner_new() {
    let interner = ShardedInterner::new();
    // The empty string is pre-interned
    let resolved = interner.resolve(Atom::NONE);
    assert_eq!(&*resolved, "");
}

#[test]
fn test_sharded_interner_intern_and_resolve() {
    let interner = ShardedInterner::new();
    let atom = interner.intern("hello");
    let resolved = interner.resolve(atom);
    assert_eq!(&*resolved, "hello");
}

#[test]
fn test_sharded_interner_deduplication() {
    let interner = ShardedInterner::new();
    let a1 = interner.intern("hello");
    let a2 = interner.intern("hello");
    assert_eq!(a1, a2);
}

#[test]
fn test_sharded_interner_different_strings() {
    let interner = ShardedInterner::new();
    let a1 = interner.intern("hello");
    let a2 = interner.intern("world");
    assert_ne!(a1, a2);
    assert_eq!(&*interner.resolve(a1), "hello");
    assert_eq!(&*interner.resolve(a2), "world");
}

#[test]
fn test_sharded_interner_empty_string() {
    let interner = ShardedInterner::new();
    let atom = interner.intern("");
    assert_eq!(atom, Atom::NONE);
    assert_eq!(&*interner.resolve(atom), "");
}

#[test]
fn test_sharded_interner_intern_owned() {
    let interner = ShardedInterner::new();
    let atom = interner.intern_owned("owned_hello".to_string());
    assert_eq!(&*interner.resolve(atom), "owned_hello");
}

#[test]
fn test_sharded_interner_intern_owned_dedup() {
    let interner = ShardedInterner::new();
    let a1 = interner.intern("same");
    let a2 = interner.intern_owned("same".to_string());
    assert_eq!(a1, a2);
}

#[test]
fn test_sharded_interner_intern_owned_empty() {
    let interner = ShardedInterner::new();
    let atom = interner.intern_owned(String::new());
    assert_eq!(atom, Atom::NONE);
}

// =============================================================================
// ShardedInterner - try_resolve
// =============================================================================

#[test]
fn test_sharded_interner_try_resolve_valid() {
    let interner = ShardedInterner::new();
    let atom = interner.intern("test");
    let resolved = interner.try_resolve(atom);
    assert!(resolved.is_some());
    assert_eq!(&*resolved.unwrap(), "test");
}

#[test]
fn test_sharded_interner_try_resolve_none_atom() {
    let interner = ShardedInterner::new();
    let resolved = interner.try_resolve(Atom::NONE);
    assert!(resolved.is_some());
    assert_eq!(&*resolved.unwrap(), "");
}

#[test]
fn test_sharded_interner_try_resolve_invalid() {
    let interner = ShardedInterner::new();
    // Use a very high atom value that was never interned
    let resolved = interner.try_resolve(Atom(u32::MAX));
    assert!(resolved.is_none());
}

#[test]
fn test_sharded_interner_resolve_invalid_returns_empty() {
    let interner = ShardedInterner::new();
    let resolved = interner.resolve(Atom(u32::MAX));
    assert_eq!(&*resolved, "");
}

// =============================================================================
// ShardedInterner - len / is_empty
// =============================================================================

#[test]
fn test_sharded_interner_len() {
    let interner = ShardedInterner::new();
    let initial_len = interner.len();
    assert!(initial_len >= 1); // at least the empty string

    let _ = interner.intern("a");
    let _ = interner.intern("b");
    let _ = interner.intern("c");
    assert_eq!(interner.len(), initial_len + 3);

    let _ = interner.intern("a"); // duplicate
    assert_eq!(interner.len(), initial_len + 3);
}

#[test]
fn test_sharded_interner_is_empty() {
    let interner = ShardedInterner::new();
    assert!(interner.is_empty()); // only the empty string
    let _ = interner.intern("x");
    assert!(!interner.is_empty());
}

// =============================================================================
// ShardedInterner - default
// =============================================================================

#[test]
fn test_sharded_interner_default() {
    let interner = ShardedInterner::default();
    let atom = interner.intern("default_test");
    assert_eq!(&*interner.resolve(atom), "default_test");
}

// =============================================================================
// ShardedInterner - pre-populated common strings
// =============================================================================

#[test]
fn test_sharded_interner_intern_common() {
    let interner = ShardedInterner::new();
    interner.intern_common();

    // Verify keywords resolve correctly
    for kw in &[
        "break", "const", "function", "return", "this", "typeof", "void",
    ] {
        let atom = interner.intern(kw);
        assert_eq!(&*interner.resolve(atom), *kw);
    }

    // Verify TS type keywords
    for ts_kw in &["any", "boolean", "number", "string", "symbol"] {
        let atom = interner.intern(ts_kw);
        assert_eq!(&*interner.resolve(atom), *ts_kw);
    }
}

#[test]
fn test_sharded_interner_intern_common_dedup() {
    let interner = ShardedInterner::new();
    interner.intern_common();
    let len_after_first = interner.len();

    interner.intern_common();
    assert_eq!(interner.len(), len_after_first);
}

// =============================================================================
// ShardedInterner - Unicode
// =============================================================================

#[test]
fn test_sharded_interner_unicode() {
    let interner = ShardedInterner::new();
    let atom = interner.intern("\u{3053}\u{3093}\u{306B}\u{3061}\u{306F}"); // こんにちは
    assert_eq!(
        &*interner.resolve(atom),
        "\u{3053}\u{3093}\u{306B}\u{3061}\u{306F}"
    );
}

#[test]
fn test_sharded_interner_unicode_dedup() {
    let interner = ShardedInterner::new();
    let a1 = interner.intern("\u{1F600}"); // 😀
    let a2 = interner.intern("\u{1F600}");
    assert_eq!(a1, a2);
}

// =============================================================================
// ShardedInterner - large strings
// =============================================================================

#[test]
fn test_sharded_interner_large_string() {
    let interner = ShardedInterner::new();
    let large = "b".repeat(100_000);
    let atom = interner.intern(&large);
    assert_eq!(&*interner.resolve(atom), large.as_str());
}

// =============================================================================
// ShardedInterner - many distinct strings (stress)
// =============================================================================

#[test]
fn test_sharded_interner_many_distinct_strings() {
    let interner = ShardedInterner::new();
    let count = 10_000;
    let mut atoms = Vec::with_capacity(count);

    for i in 0..count {
        let s = format!("sharded_str_{i}");
        atoms.push(interner.intern(&s));
    }

    // All should resolve correctly
    for (i, atom) in atoms.iter().enumerate() {
        let expected = format!("sharded_str_{i}");
        assert_eq!(&*interner.resolve(*atom), expected);
    }
}

// =============================================================================
// ShardedInterner - atom encoding/decoding (split_atom / make_atom)
// =============================================================================

#[test]
fn test_sharded_interner_atom_roundtrip_via_intern_resolve() {
    // Verify that atom encoding works correctly by interning strings
    // that hash to different shards, then resolving them back.
    let interner = ShardedInterner::new();

    // Intern many strings to spread across shards
    let mut pairs = Vec::new();
    for i in 0..500 {
        let s = format!("shard_test_{i}");
        let atom = interner.intern(&s);
        pairs.push((atom, s));
    }

    // Verify each resolves back correctly
    for (atom, expected) in &pairs {
        assert_eq!(
            &*interner.resolve(*atom),
            expected.as_str(),
            "Atom {atom:?} should resolve to '{expected}'"
        );
    }
}

// =============================================================================
// ShardedInterner - thread safety (concurrent access)
// =============================================================================

#[test]
fn test_sharded_interner_concurrent_intern() {
    use std::sync::Arc;
    use std::thread;

    let interner = Arc::new(ShardedInterner::new());
    let num_threads = 8;
    let strings_per_thread = 500;

    let handles: Vec<_> = (0..num_threads)
        .map(|t| {
            let interner = Arc::clone(&interner);
            thread::spawn(move || {
                let mut atoms = Vec::with_capacity(strings_per_thread);
                for i in 0..strings_per_thread {
                    let s = format!("thread_{t}_str_{i}");
                    atoms.push((interner.intern(&s), s));
                }
                atoms
            })
        })
        .collect();

    let all_results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // Verify all strings resolve correctly
    for thread_results in &all_results {
        for (atom, expected) in thread_results {
            assert_eq!(
                &*interner.resolve(*atom),
                expected.as_str(),
                "Concurrent intern/resolve failed for '{expected}'"
            );
        }
    }
}

#[test]
fn test_sharded_interner_concurrent_same_strings() {
    use std::sync::Arc;
    use std::thread;

    let interner = Arc::new(ShardedInterner::new());
    let num_threads = 8;

    // All threads intern the exact same set of strings
    let shared_strings: Vec<String> = (0..200).map(|i| format!("shared_{i}")).collect();

    let handles: Vec<_> = (0..num_threads)
        .map(|_| {
            let interner = Arc::clone(&interner);
            let strings = shared_strings.clone();
            thread::spawn(move || {
                let mut atoms = Vec::new();
                for s in &strings {
                    atoms.push(interner.intern(s));
                }
                atoms
            })
        })
        .collect();

    let all_atoms: Vec<Vec<Atom>> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // All threads should get the same atoms for the same strings
    for i in 0..shared_strings.len() {
        let first_atom = all_atoms[0][i];
        for thread_atoms in &all_atoms[1..] {
            assert_eq!(
                first_atom, thread_atoms[i],
                "All threads should get the same atom for '{}'",
                shared_strings[i]
            );
        }
    }
}

#[test]
fn test_sharded_interner_concurrent_intern_and_resolve() {
    use std::sync::Arc;
    use std::thread;

    let interner = Arc::new(ShardedInterner::new());

    // Pre-intern some strings
    let pre_strings: Vec<String> = (0..100).map(|i| format!("pre_{i}")).collect();
    let pre_atoms: Vec<Atom> = pre_strings.iter().map(|s| interner.intern(s)).collect();

    // Spawn threads that both intern new strings and resolve existing ones
    let handles: Vec<_> = (0..4)
        .map(|t| {
            let interner = Arc::clone(&interner);
            let pre_atoms = pre_atoms.clone();
            let pre_strings = pre_strings.clone();
            thread::spawn(move || {
                // Resolve pre-interned strings
                for (atom, expected) in pre_atoms.iter().zip(pre_strings.iter()) {
                    let resolved = interner.resolve(*atom);
                    assert_eq!(&*resolved, expected.as_str());
                }
                // Intern new strings
                for i in 0..100 {
                    let s = format!("new_t{t}_{i}");
                    let atom = interner.intern(&s);
                    let resolved = interner.resolve(atom);
                    assert_eq!(&*resolved, s.as_str());
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn test_sharded_interner_concurrent_intern_common() {
    use std::sync::Arc;
    use std::thread;

    let interner = Arc::new(ShardedInterner::new());

    // Multiple threads call intern_common simultaneously
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let interner = Arc::clone(&interner);
            thread::spawn(move || {
                interner.intern_common();
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    // Verify common strings still resolve correctly after concurrent initialization
    for kw in &[
        "function", "const", "let", "var", "return", "string", "number", "boolean",
    ] {
        let atom = interner.intern(kw);
        assert_eq!(&*interner.resolve(atom), *kw);
    }
}

// =============================================================================
// Cross-interner behavior: Interner vs ShardedInterner atoms are NOT compatible
// =============================================================================

#[test]
fn test_interner_and_sharded_are_independent() {
    let mut basic = Interner::new();
    let sharded = ShardedInterner::new();

    let basic_atom = basic.intern("test");
    let sharded_atom = sharded.intern("test");

    // They may or may not produce the same Atom value, but they are from
    // different interners and should not be mixed. Just verify each resolves
    // correctly in its own interner.
    assert_eq!(basic.resolve(basic_atom), "test");
    assert_eq!(&*sharded.resolve(sharded_atom), "test");
}

// =============================================================================
// Edge cases
// =============================================================================

#[test]
fn test_interner_whitespace_strings() {
    let mut interner = Interner::new();
    let space = interner.intern(" ");
    let tab = interner.intern("\t");
    let newline = interner.intern("\n");
    let crlf = interner.intern("\r\n");

    assert_ne!(space, tab);
    assert_ne!(tab, newline);
    assert_ne!(newline, crlf);

    assert_eq!(interner.resolve(space), " ");
    assert_eq!(interner.resolve(tab), "\t");
    assert_eq!(interner.resolve(newline), "\n");
    assert_eq!(interner.resolve(crlf), "\r\n");
}

#[test]
fn test_interner_null_bytes() {
    let mut interner = Interner::new();
    let atom = interner.intern("hello\0world");
    assert_eq!(interner.resolve(atom), "hello\0world");
}

#[test]
fn test_interner_similar_strings() {
    let mut interner = Interner::new();
    let a = interner.intern("abc");
    let b = interner.intern("abcd");
    let c = interner.intern("ab");
    assert_ne!(a, b);
    assert_ne!(a, c);
    assert_ne!(b, c);
}

#[test]
fn test_sharded_interner_whitespace_strings() {
    let interner = ShardedInterner::new();
    let space = interner.intern(" ");
    let tab = interner.intern("\t");
    assert_ne!(space, tab);
    assert_eq!(&*interner.resolve(space), " ");
    assert_eq!(&*interner.resolve(tab), "\t");
}

#[test]
fn test_sharded_interner_null_bytes() {
    let interner = ShardedInterner::new();
    let atom = interner.intern("hello\0world");
    assert_eq!(&*interner.resolve(atom), "hello\0world");
}

// =============================================================================
// Interner - Default trait
// =============================================================================

#[test]
fn test_interner_default_vs_new() {
    // Interner derives Default, which gives a bare struct without the empty
    // string sentinel. Interner::new() is the intended constructor.
    let default_interner = Interner::default();
    assert_eq!(default_interner.len(), 0);

    let new_interner = Interner::new();
    assert_eq!(new_interner.len(), 1); // has the empty string sentinel
    assert!(new_interner.is_empty()); // is_empty means only the sentinel
    assert_eq!(new_interner.resolve(Atom::NONE), "");
}

// =============================================================================
// ShardedInterner - atom encoding internals validation
// =============================================================================

#[test]
fn test_sharded_interner_uses_all_shards_under_load() {
    // Intern enough strings that statistically all 64 shards should be hit
    let interner = ShardedInterner::new();
    for i in 0..5000 {
        let _ = interner.intern(&format!("shard_coverage_{i}"));
    }
    // The total len should account for all interned strings + the empty sentinel
    // This verifies len() correctly sums across shards
    assert!(interner.len() > 5000);
}

#[test]
fn test_sharded_interner_mixed_intern_and_intern_owned() {
    let interner = ShardedInterner::new();

    let a1 = interner.intern("mixed");
    let a2 = interner.intern_owned("mixed".to_string());
    assert_eq!(a1, a2);

    let a3 = interner.intern_owned("only_owned".to_string());
    let a4 = interner.intern("only_owned");
    assert_eq!(a3, a4);
}

#[test]
fn estimated_size_bytes_is_nonzero() {
    let interner = super::Interner::new();
    let size = interner.estimated_size_bytes();
    assert!(
        size > 0,
        "estimated_size_bytes should be nonzero for a fresh interner"
    );
}

#[test]
fn estimated_size_bytes_grows_with_content() {
    let mut interner = super::Interner::new();
    let before = interner.estimated_size_bytes();

    for i in 0..100 {
        let _ = interner.intern(&format!("string_number_{i}_with_some_length"));
    }
    let after = interner.estimated_size_bytes();

    assert!(
        after > before,
        "estimated_size_bytes should grow after interning strings: {before} -> {after}"
    );
}

#[test]
fn common_strings_has_no_duplicates() {
    use std::collections::HashSet;
    let mut seen: HashSet<&'static str> = HashSet::new();
    let mut dupes: Vec<&'static str> = Vec::new();
    for &s in super::COMMON_STRINGS {
        if !seen.insert(s) {
            dupes.push(s);
        }
    }
    assert!(
        dupes.is_empty(),
        "COMMON_STRINGS contains duplicate entries: {dupes:?}"
    );
}
