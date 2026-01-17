//! Tests for interner.rs

use crate::interner::*;

#[test]
fn test_intern_basic() {
    let mut interner = Interner::new();
    let a1 = interner.intern("hello");
    let a2 = interner.intern("hello");
    let a3 = interner.intern("world");

    assert_eq!(a1, a2, "Same string should return same atom");
    assert_ne!(a1, a3, "Different strings should return different atoms");
    assert_eq!(interner.resolve(a1), "hello");
    assert_eq!(interner.resolve(a3), "world");
}

#[test]
fn test_empty_string() {
    let mut interner = Interner::new();
    let empty = interner.intern("");
    assert_eq!(empty, Atom::NONE);
    assert!(empty.is_none());
    assert_eq!(interner.resolve(empty), "");
}

#[test]
fn test_intern_common() {
    let mut interner = Interner::new();
    interner.intern_common();

    // Common keywords should be interned
    let const_atom = interner.intern("const");
    let let_atom = interner.intern("let");
    assert_ne!(const_atom, let_atom);

    // Should be able to resolve them
    assert_eq!(interner.resolve(const_atom), "const");
    assert_eq!(interner.resolve(let_atom), "let");
}

#[test]
fn test_atom_copy() {
    let mut interner = Interner::new();
    let a1 = interner.intern("test");
    let a2 = a1; // Copy
    assert_eq!(a1, a2);
}

#[test]
fn test_sharded_interner_basic() {
    let interner = ShardedInterner::new();
    let a1 = interner.intern("hello");
    let a2 = interner.intern("hello");
    let a3 = interner.intern("world");

    assert_eq!(a1, a2, "Same string should return same atom");
    assert_ne!(a1, a3, "Different strings should return different atoms");
    assert_eq!(interner.resolve(a1).as_ref(), "hello");
    assert_eq!(interner.resolve(a3).as_ref(), "world");
}

#[test]
fn test_sharded_interner_empty_string() {
    let interner = ShardedInterner::new();
    let empty = interner.intern("");
    assert_eq!(empty, Atom::NONE);
    assert!(empty.is_none());
    assert_eq!(interner.resolve(empty).as_ref(), "");
    assert_eq!(interner.try_resolve(empty).as_deref(), Some(""));
}

#[test]
fn test_sharded_interner_concurrent() {
    use std::sync::Arc;
    use std::thread;

    let interner = Arc::new(ShardedInterner::new());
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let interner = Arc::clone(&interner);
            thread::spawn(move || interner.intern("parallel"))
        })
        .collect();

    let atoms: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().expect("thread failed"))
        .collect();

    assert!(!atoms.is_empty());
    assert!(atoms.iter().all(|&atom| atom == atoms[0]));
    assert_eq!(interner.resolve(atoms[0]).as_ref(), "parallel");
}

#[test]
fn test_sharded_interner_try_resolve_invalid() {
    let interner = ShardedInterner::new();
    assert_eq!(interner.try_resolve(Atom(u32::MAX)), None);
}
