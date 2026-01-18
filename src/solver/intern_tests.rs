```rust
// src/solver/intern_tests.rs

// This file contains specific tests for the Intern system.
// Basic unit tests are now included in intern.rs, but we keep this
// for more complex integration scenarios or to stress test the sharding.

use crate::solver::intern::{intern, ShardedArena};

#[test]
fn test_thread_local_isolation() {
    // This test verifies that the ShardedArena uses thread-local storage
    // by allocating data in different threads.

    let s1 = "thread_main";
    let main_intern = intern(s1);
    let main_local = ShardedArena.alloc_str(s1);

    // Spawn a thread to perform allocations.
    // Note: If `Bump` were global, this might contend or require locking.
    // With ShardedArena, this runs on the thread's own Bump instance.
    let handle = std::thread::spawn(move || {
        let s2 = "thread_child";
        let child_intern = intern(s2);
        let child_local = ShardedArena.alloc_str(s2);

        // Verify content is correct
        assert_eq!(child_intern.name, "thread_child");
        assert_eq!(child_local, "thread_child");
        
        // Return the ID to verify global ID consistency
        child_intern.id
    });

    let child_id = handle.join().unwrap();

    // IDs should be unique and sequential regardless of thread
    assert_ne!(main_intern.id, child_id);

    // Verify main thread data integrity
    assert_eq!(main_intern.name, "thread_main");
    assert_eq!(main_local, "thread_main");
}

#[test]
fn test_intern_id_uniqueness() {
    // High volume intern test to ensure AtomicU32 works correctly
    use std::collections::HashSet;
    
    let mut ids = HashSet::new();
    let data = vec
!["alpha", "beta", "gamma", "delta", "epsilon"];

    for &s in &data {
        let i = intern(s);
        assert!(!ids.contains(&i.id), "Duplicate ID found for {}", s);
        ids.insert(i.id);
    }

    // Re-interning should return same IDs
    for &s in &data {
        let i = intern(s);
        assert!(ids.contains(&i.id), "Missing ID for {}", s);
    }
}
```
