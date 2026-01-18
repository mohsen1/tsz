// This test file imports the Arena from the checker crate/module 
// to verify the behavior.

#[path = "../checker/arena.rs"]
mod arena;

#[cfg(test)]
mod tests {
    use super::arena::Arena;

    #[test]
    fn test_arena_basic_behavior() {
        // Initialize the Arena
        let mut arena = Arena::new();

        // Insert values and assert IDs are sequential
        let id_a = arena.insert(10);
        let id_b = arena.insert(20);
        let id_c = arena.insert(30);

        assert_eq!(id_a, 0);
        assert_eq!(id_b, 1);
        assert_eq!(id_c, 2);
        assert_eq!(arena.len(), 3);

        // Assert retrieved values match what was inserted
        assert_eq!(arena.get(id_a), Some(&10));
        assert_eq!(arena.get(id_b), Some(&20));
        assert_eq!(arena.get(id_c), Some(&30));

        // Assert behavior for out-of-bounds access
        assert_eq!(arena.get(999), None);
    }
}
```
