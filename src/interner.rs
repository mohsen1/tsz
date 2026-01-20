//! String Interner for identifier deduplication.
//!
//! PERFORMANCE OPTIMIZATION: Intern strings into a global pool and pass around
//! u32 indices (Atoms). This eliminates duplicate string allocations for common
//! identifiers like "id", "value", "length", etc.
//!
//! Comparisons become integer comparisons (atom_a == atom_b) instead of string
//! comparisons, which is significantly faster.

use rustc_hash::{FxHashMap, FxHasher};
use serde::Serialize;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock};

/// An interned string identifier.
///
/// Atoms are cheap to copy (just a u32) and can be compared with == in O(1).
/// To get the actual string, use `Interner::resolve(atom)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Default, PartialOrd, Ord)]
pub struct Atom(pub u32);

impl Atom {
    /// A sentinel value representing no atom / empty string.
    pub const NONE: Atom = Atom(0);

    /// Check if this is the empty/none atom.
    #[inline]
    pub fn is_none(self) -> bool {
        self.0 == 0
    }

    /// Get the raw index value.
    #[inline]
    pub fn index(self) -> u32 {
        self.0
    }
}

const SHARD_BITS: u32 = 6;
const SHARD_COUNT: usize = 1 << SHARD_BITS;
const SHARD_MASK: u32 = (SHARD_COUNT as u32) - 1;
const COMMON_STRINGS: &[&str] = &[
    // Keywords
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
    "as",
    "implements",
    "interface",
    "let",
    "package",
    "private",
    "protected",
    "public",
    "static",
    "yield",
    "any",
    "boolean",
    "number",
    "string",
    "symbol",
    "type",
    "from",
    "of",
    "async",
    "await",
    // Common identifiers
    "id",
    "name",
    "value",
    "length",
    "key",
    "index",
    "item",
    "data",
    "error",
    "result",
    "response",
    "request",
    "options",
    "config",
    "props",
    "state",
    "children",
    "onClick",
    "onChange",
    "onSubmit",
    "constructor",
    "prototype",
    "toString",
    "valueOf",
    "hasOwnProperty",
    "Array",
    "Object",
    "String",
    "Number",
    "Boolean",
    "Function",
    "Promise",
    "Map",
    "Set",
    "Date",
    "RegExp",
    "Error",
    "Symbol",
    "console",
    "log",
    "warn",
    "error",
    "info",
    "debug",
    "document",
    "window",
    "global",
    "process",
    "module",
    "exports",
    "require",
    "define",
    "__dirname",
    "__filename",
];

/// String interner that deduplicates strings and returns Atom handles.
///
/// # Example
/// ```
/// use wasm::interner::Interner;
/// let mut interner = Interner::new();
/// let a1 = interner.intern("hello");
/// let a2 = interner.intern("hello");
/// assert_eq!(a1, a2); // Same atom for same string
/// assert_eq!(interner.resolve(a1), "hello");
/// ```
#[derive(Default)]
pub struct Interner {
    /// Map from string to atom index
    map: FxHashMap<Arc<str>, Atom>,
    /// Vector of all interned strings (index 0 is empty string)
    strings: Vec<Arc<str>>,
}

impl Interner {
    /// Create a new interner with the empty string pre-interned at index 0.
    pub fn new() -> Self {
        let mut interner = Interner {
            map: FxHashMap::default(),
            strings: Vec::with_capacity(1024), // Pre-allocate for common case
        };
        // Index 0 is reserved for empty/none
        let empty: Arc<str> = Arc::from("");
        interner.strings.push(empty.clone());
        interner.map.insert(empty, Atom::NONE);
        interner
    }

    /// Intern a string, returning its Atom handle.
    /// If the string was already interned, returns the existing Atom.
    #[inline]
    pub fn intern(&mut self, s: &str) -> Atom {
        if let Some(&atom) = self.map.get(s) {
            return atom;
        }
        let atom = Atom(self.strings.len() as u32);
        let owned: Arc<str> = Arc::from(s);
        self.strings.push(owned.clone());
        self.map.insert(owned, atom);
        atom
    }

    /// Intern an owned String, avoiding allocation if possible.
    #[inline]
    pub fn intern_owned(&mut self, s: String) -> Atom {
        if let Some(&atom) = self.map.get(s.as_str()) {
            return atom;
        }
        let atom = Atom(self.strings.len() as u32);
        let owned: Arc<str> = Arc::from(s.into_boxed_str());
        self.strings.push(owned.clone());
        self.map.insert(owned, atom);
        atom
    }

    /// Resolve an Atom back to its string value.
    /// Returns empty string if atom is out of bounds (safety for error recovery).
    #[inline]
    pub fn resolve(&self, atom: Atom) -> &str {
        self.strings
            .get(atom.0 as usize)
            .map(|s| s.as_ref())
            .unwrap_or("")
    }

    /// Try to resolve an Atom, returning None if invalid.
    #[inline]
    pub fn try_resolve(&self, atom: Atom) -> Option<&str> {
        self.strings.get(atom.0 as usize).map(|s| s.as_ref())
    }

    /// Get the number of interned strings.
    #[inline]
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// Check if the interner is empty (only has the empty string).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.strings.len() <= 1
    }

    /// Pre-intern common TypeScript keywords and identifiers.
    /// Call this after creating the interner for better cache locality.
    pub fn intern_common(&mut self) {
        for s in COMMON_STRINGS {
            self.intern(s);
        }
    }
}

#[derive(Default)]
struct ShardState {
    map: FxHashMap<Arc<str>, Atom>,
    strings: Vec<Arc<str>>,
}

struct InternerShard {
    state: RwLock<ShardState>,
}

impl InternerShard {
    fn new() -> Self {
        InternerShard {
            state: RwLock::new(ShardState::default()),
        }
    }
}

/// Sharded string interner for concurrent use.
///
/// Uses fixed buckets to reduce lock contention while keeping Atom lookups O(1).
pub struct ShardedInterner {
    shards: [InternerShard; SHARD_COUNT],
}

impl ShardedInterner {
    /// Create a new sharded interner with the empty string pre-interned at index 0.
    pub fn new() -> Self {
        let shards = std::array::from_fn(|_| InternerShard::new());

        // Initialize empty string in shard 0 with safe lock handling
        if let Ok(mut state) = shards[0].state.write() {
            let empty: Arc<str> = Arc::from("");
            state.strings.push(empty.clone());
            state.map.insert(empty, Atom::NONE);
        }
        // Note: If lock is poisoned during initialization, we continue anyway
        // The empty string initialization is an optimization, not critical for correctness

        ShardedInterner { shards }
    }

    /// Intern a string, returning its Atom handle.
    /// If the string was already interned, returns the existing Atom.
    #[inline]
    pub fn intern(&self, s: &str) -> Atom {
        if s.is_empty() {
            return Atom::NONE;
        }

        let shard_idx = Self::shard_for(s);
        let shard = &self.shards[shard_idx];
        let Ok(mut state) = shard.state.write() else {
            // If lock is poisoned, return a fallback atom
            // This maintains availability even if internal state is corrupted
            return Atom::NONE;
        };

        if let Some(&atom) = state.map.get(s) {
            return atom;
        }

        let local_index = state.strings.len() as u32;
        if local_index > (u32::MAX >> SHARD_BITS) {
            // Return empty atom on overflow instead of panicking
            return Atom::NONE;
        }

        let atom = Self::make_atom(local_index, shard_idx as u32);
        let owned: Arc<str> = Arc::from(s);
        state.strings.push(owned.clone());
        state.map.insert(owned, atom);
        atom
    }

    /// Intern an owned String, avoiding allocation if possible.
    #[inline]
    pub fn intern_owned(&self, s: String) -> Atom {
        if s.is_empty() {
            return Atom::NONE;
        }

        let shard_idx = Self::shard_for(&s);
        let shard = &self.shards[shard_idx];
        let Ok(mut state) = shard.state.write() else {
            // If lock is poisoned, return a fallback atom
            return Atom::NONE;
        };

        if let Some(&atom) = state.map.get(s.as_str()) {
            return atom;
        }

        let local_index = state.strings.len() as u32;
        if local_index > (u32::MAX >> SHARD_BITS) {
            // Return empty atom on overflow instead of panicking
            return Atom::NONE;
        }

        let atom = Self::make_atom(local_index, shard_idx as u32);
        let owned: Arc<str> = Arc::from(s);
        state.strings.push(owned.clone());
        state.map.insert(owned, atom);
        atom
    }

    /// Resolve an Atom back to its string value.
    /// Returns empty string if atom is out of bounds (safety for error recovery).
    #[inline]
    pub fn resolve(&self, atom: Atom) -> Arc<str> {
        self.try_resolve(atom).unwrap_or_else(|| Arc::from(""))
    }

    /// Try to resolve an Atom, returning None if invalid.
    #[inline]
    pub fn try_resolve(&self, atom: Atom) -> Option<Arc<str>> {
        let (shard_idx, local_index) = Self::split_atom(atom)?;
        let shard = self.shards.get(shard_idx)?;
        let state = shard.state.read().ok()?; // Return None if lock is poisoned
        state.strings.get(local_index).cloned()
    }

    /// Get the number of interned strings.
    #[inline]
    pub fn len(&self) -> usize {
        self.shards
            .iter()
            .map(|shard| {
                // Handle lock poisoning gracefully by returning 0 for failed shards
                shard
                    .state
                    .read()
                    .map(|state| state.strings.len())
                    .unwrap_or(0)
            })
            .sum()
    }

    /// Check if the interner is empty (only has the empty string).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() <= 1
    }

    /// Pre-intern common TypeScript keywords and identifiers.
    /// Call this after creating the interner for better cache locality.
    pub fn intern_common(&self) {
        for s in COMMON_STRINGS {
            self.intern(s);
        }
    }

    #[inline]
    fn shard_for(s: &str) -> usize {
        let mut hasher = FxHasher::default();
        s.hash(&mut hasher);
        (hasher.finish() as usize) & (SHARD_COUNT - 1)
    }

    #[inline]
    fn make_atom(local_index: u32, shard_idx: u32) -> Atom {
        Atom((local_index << SHARD_BITS) | (shard_idx & SHARD_MASK))
    }

    #[inline]
    fn split_atom(atom: Atom) -> Option<(usize, usize)> {
        if atom == Atom::NONE {
            return Some((0, 0));
        }

        let raw = atom.0;
        let shard_idx = (raw & SHARD_MASK) as usize;
        let local_index = (raw >> SHARD_BITS) as usize;
        Some((shard_idx, local_index))
    }
}

impl Default for ShardedInterner {
    fn default() -> Self {
        Self::new()
    }
}
