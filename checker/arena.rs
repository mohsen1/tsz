/// A simple arena allocator that owns values of type T.
/// Values are inserted and assigned a unique usize ID.
#[derive(Debug, Default)]
pub struct Arena<T> {
    items: Vec<T>,
}

impl<T> Arena<T> {
    /// Creates a new, empty arena.
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
        }
    }

    /// Inserts a value into the arena, returning its assigned ID.
    pub fn insert(&mut self, item: T) -> usize {
        let id = self.items.len();
        self.items.push(item);
        id
    }

    /// Attempts to get a reference to the item with the given ID.
    /// Returns None if the ID is invalid.
    pub fn get(&self, id: usize) -> Option<&T> {
        self.items.get(id)
    }

    /// Returns the number of items in the arena.
    pub fn len(&self) -> usize {
        self.items.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get() {
        let mut arena: Arena<String> = Arena::new();

        let id1 = arena.insert("first".to_string());
        assert_eq!(id1, 0);
        assert_eq!(arena.get(id1), Some(&"first".to_string()));

        let id2 = arena.insert("second".to_string());
        assert_eq!(id2, 1);
        assert_eq!(arena.get(id2), Some(&"second".to_string()));

        // Verify first item is still accessible
        assert_eq!(arena.get(id1), Some(&"first".to_string()));
    }

    #[test]
    fn test_get_invalid_id() {
        let arena: Arena<i32> = Arena::new();
        assert_eq!(arena.get(0), None);
        assert_eq!(arena.get(99), None);
    }
}
```

```rust
//
