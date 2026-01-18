```rust
// src/solver/intern.rs
use bumpalo::Bump;
use once_cell::sync::Lazy;
use std::fmt;
use std::ops::Deref;

/// A wrapper around a static thread-local Bump allocator.
///
/// This replaces the global `RwLock<Bump>` strategy. Instead of synchronizing
/// access to a single memory arena, each thread gets its own arena, eliminating
/// lock contention during the solving phase.
#[derive(Debug, Default, Copy, Clone)]
pub struct ShardedArena;

// We define the arena as thread-local. This is static, so it is created once per thread
// and lives for the entire duration of the thread.
thread_local! {
    static ARENA: Bump = Bump::new();
}

impl ShardedArena {
    /// Allocates a string slice into the thread-local arena and returns a static reference.
    ///
    /// # Safety & Lifetime
    /// The returned reference is `&'static str`. This is safe because the thread-local
    /// `ARENA` is declared as `static` (via `thread_local!`), so it will never be moved
    /// or dropped while the thread is running. The reference remains valid for the
    /// entire execution of the current thread.
    #[inline]
    pub fn alloc_str(&self, s: &str) -> &'static str {
        ARENA.with(|arena| {
            // Bumpalo alloc_str returns a reference with the lifetime of the arena.
            // Since the arena is 'static (thread-local static), we can safely cast.
            unsafe {
                let ptr: &str = arena.alloc_str(s);
                &*(ptr as *const str)
            }
        })
    }

    /// Allocates a value into the thread-local arena.
    #[inline]
    pub fn alloc<T>(&self, val: T) -> &'static T {
        ARENA.with(|arena| {
            unsafe {
                let ptr: &T = arena.alloc(val);
                &*(ptr as *const T)
            }
        })
    }
}

// --- Intern Types ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Intern<'a> {
    /// The string content.
    pub name: &'a str,
    /// A unique identifier assigned during interning.
    pub id: u32,
}

impl<'a> Deref for Intern<'a> {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        self.name
    }
}

impl<'a> AsRef<str> for Intern<'a> {
    fn as_ref(&self) -> &str {
        self.name
    }
}

impl<'a> fmt::Display for Intern<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// Global registry to map string content to unique IDs.
/// We use `Lazy` to initialize the `RwLock` heap map only once.
static REGISTRY: Lazy<std::sync::RwLock<fxhash::FxHashMap<&'static str, u32>>> =
    Lazy::new(|| std::sync::RwLock::new(fxhash::FxHashMap::default()));

/// Atomic counter for assigning unique IDs.
static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

/// Interns a string slice.
///
/// Returns an `Intern<'static>` handle. The backing data is stored in the
/// thread-local arena, while the ID mapping is stored in a global registry.
pub fn intern(s: &str) -> Intern<'static> {
    let arena = ShardedArena;
    
    // 1. Try read-lock to see if it exists.
    // Since threads might race to insert the same string, we check the registry first.
    {
        let read = REGISTRY.read().unwrap();
        if let Some(&id) = read.get(s) {
            return Intern { name: arena.alloc_str(s), id };
        }
    }

    // 2. If not found, upgrade to write lock.
    // We must re-check in case another thread just inserted it.
    let mut write = REGISTRY.write().unwrap();
    if let Some(&id) = write.get(s) {
        // It was inserted while we waited for the lock.
        // Note: We still allocate in the local arena, which might result in
        // duplicates of the *string data* across thread-local arenas, but the *ID*
        // remains globally unique and consistent.
        return Intern { name: arena.alloc_str(s), id };
    }

    // 3. Insert new entry.
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    
    // We allocate in the local arena. The key stored in the global registry
    // must be 'static. Since ShardedArena allocates into thread-local static storage,
    // the resulting reference is valid for 'static.
    let name_static: &'static str = arena.alloc_str(s);
    
    write.insert(name_static, id);
    
    Intern { name: name_static, id }
}

/// Creates a pre-defined intern (usually for constants).
pub const fn intern_const(name: &'static str, id: u32) -> Intern<'static> {
    Intern { name, id }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intern_basic() {
        let a = intern("foo");
        let b = intern("foo");
        let c = intern("bar");

        assert_eq!(a.name, "foo");
        assert_eq!(a.id, b.id);
        assert_ne!(a.id, c.id);
    }

    #[test]
    fn test_sharded_arena_alloc() {
        let arena = ShardedArena;
        let s = arena.alloc_str("hello world");
        assert_eq!(s, "hello world");
        
        let val: &'static i32 = arena.alloc(42);
        assert_eq!(*val, 42);
    }
}
```

###
