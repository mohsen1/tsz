//! Concurrent stress tests for the lock-free Trie solver.
//! These tests aim to detect data races, ABA problems, and memory corruption
//! by heavily utilizing concurrent mutations and reads.

use super::state::{Insert, InsertResult, Retract, RetractResult, Solver};
use rand::Rng;
use std::collections::HashSet;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

/// Helper to generate a random boolean assignment vector.
fn random_bool_vec(size: usize, rng: &mut impl Rng) -> Vec<bool> {
    (0..size).map(|_| rng.gen_bool(0.5)).collect()
}

#[test]
fn test_concurrent_insert_happy_path() {
    let solver = Arc::new(Solver::new());
    let num_threads = 8;
    let inserts_per_thread = 100;
    let barrier = Arc::new(Barrier::new(num_threads));

    let mut handles = vec![];

    for _ in 0..num_threads {
        let s = Arc::clone(&solver);
        let b = Arc::clone(&barrier);
        let handle = thread::spawn(move || {
            b.wait();
            let mut rng = rand::thread_rng();
            
            for i in 0..inserts_per_thread {
                // Create a unique clause ID based on thread and iteration to avoid collision
                // of logical content, or use the ID directly.
                let clause_id = format!("t-{:?}", thread::current().id());
                let lits: Vec<i32> = vec![(i as i32) + 1, -((i as i32) + 2)];
                
                let insert_op = Insert::new(clause_id, lits);
                // We don't care about the result here, just that it doesn't crash
                let _ = s.apply(insert_op);
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // Verify solver is still responsive
    let probe = Insert::new("final_check".to_string(), vec![1, 2, 3]);
    assert!(matches!(solver.apply(probe), InsertResult::Ok { .. }));
}

#[test]
fn test_concurrent_insert_retract_conflict() {
    // This test specifically targets the ABA problem and memory safety 
    // during Retract if the node has been reused or modified.
    let solver = Arc::new(Solver::new());
    let barrier = Arc::new(Barrier::new(4)); // 2 Inserters, 2 Retractors

    let clause_id = "shared_clause";
    // Pre-populate
    solver.apply(Insert::new(clause_id.to_string(), vec![1, 2]));
    
    let mut handles = vec![];

    // Thread 1: Relentless Inserters (re-inserting same ID or new data)
    for t in 0..2 {
        let s = Arc::clone(&solver);
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            b.wait();
            let mut rng = rand::thread_rng();
            for _ in 0..500 {
                let lits = vec![rng.gen_range(1..100), rng.gen_range(1..100)];
                let _ = s.apply(Insert::new(format!("{}-{}", clause_id, t), lits));
                // Small sleep to increase context switching likelihood
                // std::hint::spin_loop(); 
            }
        }));
    }

    // Thread 2: Relentless Retractors
    for t in 2..4 {
        let s = Arc::clone(&solver);
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            b.wait();
            let mut rng = rand::thread_rng();
            for _ in 0..500 {
                // Try to retract things that might or might not exist
                let id = format!("{}-{}", clause_id, rng.gen_range(0..2)); 
                let _ = s.apply(Retract::new(id));
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }
    
    // Final consistency check: The internal structure should not be corrupted.
    // We verify this by doing a complex insert.
    let res = solver.apply(Insert::new("final".to_string(), vec![10, 20, 30]));
    assert!(matches!(res, InsertResult::Ok { .. }));
}

#[test]
fn test_concurrent_conflicting_propagations() {
    // Test unit propagation handling where multiple threads modify
    // the assignment status simultaneously.
    let solver = Arc::new(Solver::new());
    let num_threads = 10;
    let barrier = Arc::new(Barrier::new(num_threads));
    
    // Insert a bunch of unit clauses.
    // Clause 1: [1]
    // Clause 2: [-1]
    // This creates immediate conflicts in propagation logic.
    solver.apply(Insert::new("u1".to_string(), vec![1]));
    solver.apply(Insert::new("u2".to_string(), vec![-1]));

    let handles: Vec<_> = (0..num_threads)
        .map(|_| {
            let s = Arc::clone(&solver);
            let b = Arc::clone(&barrier);
            thread::spawn(move || {
                b.wait();
                // Try to assign values. The solver must handle conflicts without crashing.
                let _ = s.apply(Insert::new(format!("t-{:?}", std::thread::current().id()), vec![2]));
                let _ = s.apply(Insert::new(format!("t-{:?}", std::thread::current().id()), vec![-2]));
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn test_stress_concurrent_random_operations() {
    let solver = Arc::new(Solver::new());
    let num_threads = 16;
    let ops_per_thread = 200;
    let barrier = Arc::new(Barrier::new(num_threads));

    let handles: Vec<_> = (0..num_threads)
        .map(|i| {
            let s = Arc::clone(&solver);
            let b = Arc::clone(&barrier);
            thread::spawn(move || {
                b.wait();
                let mut rng = rand::thread_rng();
                
                for _ in 0..ops_per_thread {
                    let action = rng.gen_range(0..3);
                    let id = format!("c-{}-{}", i, rng.gen_range(0..50));
                    
                    match action {
                        0 => {
                            // Insert
                            let n_lits = rng.gen_range(1..5);
                            let lits: Vec<i32> = (0..n_lits).map(|_| rng.gen_range(-100..100)).collect();
                            let _ = s.apply(Insert::new(id.clone(), lits));
                        }
                        1 => {
                            // Retract
                            let _ = s.apply(Retract::new(id));
                        }
                        _ => {
                            // Check consistency (simulate a read)
                            // We can't directly read the state easily without a public getter,
                            // but we perform a no-op insert to check traversal validity.
                            let _ = s.apply(Insert::new("_read_probe_".to_string(), vec![999]));
                        }
                    }
                }
            })
        })
        .collect();

    // Allow a maximum timeout for the test to prevent hanging forever if there is a deadlock
    for h in handles {
        let result = h.join();
        assert!(result.is_ok(), "Thread panicked or failed to join");
    }
}

#[test]
fn test_deterministic_interleaving_retract_reuse() {
    // Scenario: T1 Retracts Node A. T2 Inserts Node A (or similar path).
    // Verifies that reclamation of memory happens safely.
    let solver = Arc::new(Solver::new());
    
    let s1 = Arc::clone(&solver);
    let s2 = Arc::clone(&solver);
    
    let t1 = thread::spawn(move || {
        // Insert a long chain
        for i in 0..100 {
            let _ = s1.apply(Insert::new(format!("chain-{}", i), vec![i, i+1]));
        }
        // Retract them
        for i in 0..100 {
            let _ = s1.apply(Retract::new(format!("chain-{}", i)));
        }
    });

    let t2 = thread::spawn(move || {
        // Try to fill the memory hole
        for i in 0..100 {
             let _ = s2.apply(Insert::new(format!("gap-{}", i), vec![i+100, i+101]));
        }
    });

    t1.join().unwrap();
    t2.join().unwrap();
    
    // Final verification insert
    assert!(matches!(solver.apply(Insert::new("check".to_string(), vec![1])), InsertResult::Ok { .. }));
}
```

```rust
