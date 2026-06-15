//! Thread-local direct-mapped caches for type interning and lookup.
//!
//! This module is the interner's thread-local fast path: small direct-mapped
//! caches probed on every `lookup`, `intern`, and `intern_string` call. The
//! string cache (#13642) deliberately stays off the two existing pedantic
//! tripwires without a suppression attribute: its probe/insert wrappers use a
//! plain `#[inline]` hint (trivial bodies LLVM inlines anyway) so they do not
//! add to the `inline_always` ratchet, and its backing array is sized to keep
//! the per-thread allocation under the `large_stack_arrays` threshold.

use crate::types::{TypeData, TypeId};
use std::cell::Cell;
use tsz_common::interner::Atom;

// ---------------------------------------------------------------------------
// Thread-local direct-mapped lookup cache
// ---------------------------------------------------------------------------
// On single-threaded workloads (all benchmarks, CLI), every `lookup()` call
// goes through `RwLock::read()` which costs ~15-25 ns per call (atomic CAS on
// the reader count, memory fence, deref, fence, atomic decrement). A 1024-entry
// direct-mapped cache turns >90% of lookups into a single array index + compare
// (~1-2 ns). The cache is keyed by `TypeId.0` with the tag stored alongside
// the data, so collisions just evict (no correctness issue).

const LOOKUP_CACHE_BITS: u32 = 10;
const LOOKUP_CACHE_SIZE: usize = 1 << LOOKUP_CACHE_BITS; // 1024
#[allow(dead_code)]
const LOOKUP_CACHE_MASK: u32 = (LOOKUP_CACHE_SIZE as u32) - 1;

/// A single cache entry: (tag = TypeId raw value, cached TypeData, owning
/// interner `instance_id`).
///
/// `tag == 0` means empty (`TypeId::NONE` is never looked up for user types).
/// `instance_id` scopes the cache entry to the interner that inserted it, so
/// a stale entry from a previous `TypeInterner` on the same thread is
/// detected and treated as a miss -- even though the raw `tag` may collide
/// with a different type in the new interner. Without this, the thread-local
/// cache was disabled entirely, forcing every `lookup()` through a
/// `RwLock::read()` (~15-25 ns per call).
#[derive(Clone, Copy)]
struct LookupCacheEntry {
    tag: u32,
    instance_id: u32,
    data: TypeData,
}

// ---------------------------------------------------------------------------
// Thread-local combined cache for both lookup and intern
// ---------------------------------------------------------------------------
// Combines both caches into a single struct to reduce thread_local! accesses.
// On macOS, each thread_local! access goes through __tls_get_addr (~10-15ns).
// By combining into one TLS access, we halve the overhead.

const INTERN_CACHE_BITS: u32 = 9;
const INTERN_CACHE_SIZE: usize = 1 << INTERN_CACHE_BITS; // 512
#[allow(dead_code)]
const INTERN_CACHE_MASK: u64 = (INTERN_CACHE_SIZE as u64) - 1;

#[derive(Clone, Copy)]
struct InternCacheEntry {
    /// `FxHash` of the TypeData, used as tag
    hash: u64,
    /// Owning interner `instance_id` for cross-interner safety.
    instance_id: u32,
    /// The TypeData that was interned
    key: TypeData,
    /// The resulting TypeId
    result: TypeId,
}

// ---------------------------------------------------------------------------
// Thread-local direct-mapped cache for string interning (`intern_string`)
// ---------------------------------------------------------------------------
// `intern_string` is a top self-time leaf on type-heavy workloads: hot property
// names and type-parameter names (`"T"`, `"length"`, `"[Symbol.iterator]"`,
// `"value"`, ...) are re-interned tens of thousands of times. Each call hashes
// the string twice (shard select + map probe) and takes the shard `RwLock`.
// A small direct-mapped thread-local cache turns repeats into a single hash +
// array index + byte compare, scoped by the owning interner's `instance_id`.
//
// The key string is stored inline (`Copy`, no allocation) so the array-of-`Cell`
// layout matches the lookup/intern caches. Strings longer than
// `STRING_KEY_INLINE_CAP` bypass the cache and fall through to the interner; the
// cache only ever returns an `Atom` the interner already minted, so identity and
// determinism are preserved (same string -> same `Atom`). A full byte/length
// compare on hit makes hash collisions a miss, never a wrong `Atom`.

// 256 slots keep the per-thread `[StringCacheEntry; N]` allocation
// (~40 bytes/entry) under the `large_stack_arrays` 16384-byte threshold so the
// cache needs no lint suppression. The hot working set of interned names
// (type-parameter and property identifiers) is a few dozen distinct strings, so
// a 256-entry direct-mapped table still resolves the vast majority to one hit.
const STRING_CACHE_BITS: u32 = 8;
const STRING_CACHE_SIZE: usize = 1 << STRING_CACHE_BITS; // 256
const STRING_CACHE_MASK: u64 = (STRING_CACHE_SIZE as u64) - 1;
/// Inline capacity for the cached key bytes. Covers the hot short names
/// (`"[Symbol.iterator]"` is 17 bytes, `"removeEventListener"` is 19); longer
/// strings simply bypass the cache.
const STRING_KEY_INLINE_CAP: usize = 23;

#[derive(Clone, Copy)]
struct StringCacheEntry {
    /// `FxHash` of the key string, used as tag.
    hash: u64,
    /// Owning interner `instance_id` for cross-interner safety.
    instance_id: u32,
    /// Length of the cached key in `key_bytes` (`0` means empty/unused).
    len: u8,
    /// Inline key bytes (only `len` are meaningful).
    key_bytes: [u8; STRING_KEY_INLINE_CAP],
    /// The resulting `Atom`.
    result: Atom,
}

/// Combined thread-local cache for both `lookup()` and `intern()` directions.
///
/// Uses per-slot `Cell<T>` values for interior mutability. Both cache entry
/// types are `Copy`, so each probe/insert remains one direct slot `get`/`set`
/// with no `unsafe` and no manual `Send`/`Sync` impls. The cache is reached
/// only through `thread_local!`, which requires neither bound.
struct TypeInternerCache {
    lookup: [Cell<LookupCacheEntry>; LOOKUP_CACHE_SIZE],
    intern: [Cell<InternCacheEntry>; INTERN_CACHE_SIZE],
    string: [Cell<StringCacheEntry>; STRING_CACHE_SIZE],
}

const EMPTY_LOOKUP_ENTRY: LookupCacheEntry = LookupCacheEntry {
    tag: 0,
    instance_id: 0,
    data: TypeData::Error,
};

const EMPTY_INTERN_ENTRY: InternCacheEntry = InternCacheEntry {
    hash: 0,
    instance_id: 0,
    key: TypeData::Error,
    result: TypeId::NONE,
};

const EMPTY_STRING_ENTRY: StringCacheEntry = StringCacheEntry {
    hash: 0,
    instance_id: 0,
    len: 0,
    key_bytes: [0; STRING_KEY_INLINE_CAP],
    result: Atom::NONE,
};

#[allow(dead_code)]
impl TypeInternerCache {
    const fn new() -> Self {
        Self {
            lookup: [const { Cell::new(EMPTY_LOOKUP_ENTRY) }; LOOKUP_CACHE_SIZE],
            intern: [const { Cell::new(EMPTY_INTERN_ENTRY) }; INTERN_CACHE_SIZE],
            string: [const { Cell::new(EMPTY_STRING_ENTRY) }; STRING_CACHE_SIZE],
        }
    }

    #[inline(always)]
    const fn lookup_probe(&self, id: TypeId, instance_id: u32) -> Option<TypeData> {
        let idx = (id.0 & LOOKUP_CACHE_MASK) as usize;
        let entry = self.lookup[idx].get();
        if entry.tag == id.0 && entry.instance_id == instance_id {
            Some(entry.data)
        } else {
            None
        }
    }

    #[inline(always)]
    fn lookup_insert(&self, id: TypeId, instance_id: u32, data: TypeData) {
        let idx = (id.0 & LOOKUP_CACHE_MASK) as usize;
        self.lookup[idx].set(LookupCacheEntry {
            tag: id.0,
            instance_id,
            data,
        });
    }

    #[inline(always)]
    fn intern_probe(&self, hash: u64, instance_id: u32, key: &TypeData) -> Option<TypeId> {
        let idx = (hash & INTERN_CACHE_MASK) as usize;
        let entry = self.intern[idx].get();
        if entry.hash == hash && entry.instance_id == instance_id && &entry.key == key {
            Some(entry.result)
        } else {
            None
        }
    }

    #[inline(always)]
    fn intern_insert(&self, hash: u64, instance_id: u32, key: TypeData, result: TypeId) {
        let idx = (hash & INTERN_CACHE_MASK) as usize;
        self.intern[idx].set(InternCacheEntry {
            hash,
            instance_id,
            key,
            result,
        });
    }

    #[inline]
    fn string_probe(&self, hash: u64, instance_id: u32, s: &str) -> Option<Atom> {
        // Strings longer than the inline cap are never inserted, so they can
        // never hit. Returning early also keeps the `key_bytes[..len]` slice
        // index below in bounds without relying on `&&` short-circuit order.
        let len = s.len();
        if len > STRING_KEY_INLINE_CAP {
            return None;
        }
        let idx = (hash & STRING_CACHE_MASK) as usize;
        let entry = self.string[idx].get();
        // A length/byte compare on top of the hash + instance_id makes a hash
        // collision a miss, never a wrong `Atom`.
        if entry.hash == hash
            && entry.instance_id == instance_id
            && entry.len as usize == len
            && entry.key_bytes[..len] == *s.as_bytes()
        {
            Some(entry.result)
        } else {
            None
        }
    }

    #[inline]
    fn string_insert(&self, hash: u64, instance_id: u32, s: &str, result: Atom) {
        // Only short strings fit inline; longer ones are not cached.
        let bytes = s.as_bytes();
        if bytes.len() > STRING_KEY_INLINE_CAP {
            return;
        }
        let idx = (hash & STRING_CACHE_MASK) as usize;
        let mut key_bytes = [0u8; STRING_KEY_INLINE_CAP];
        key_bytes[..bytes.len()].copy_from_slice(bytes);
        self.string[idx].set(StringCacheEntry {
            hash,
            instance_id,
            len: bytes.len() as u8,
            key_bytes,
            result,
        });
    }
}

thread_local! {
    static TL_CACHE: TypeInternerCache = const { TypeInternerCache::new() };
}

pub(super) fn clear_thread_local_cache() {
    TL_CACHE.with(|cache| {
        for cell in &cache.lookup {
            cell.set(EMPTY_LOOKUP_ENTRY);
        }
        for cell in &cache.intern {
            cell.set(EMPTY_INTERN_ENTRY);
        }
        for cell in &cache.string {
            cell.set(EMPTY_STRING_ENTRY);
        }
    });
}

#[inline(always)]
pub(super) fn lookup_probe(id: TypeId, instance_id: u32) -> Option<TypeData> {
    TL_CACHE.with(|cache| cache.lookup_probe(id, instance_id))
}

#[inline(always)]
pub(super) fn lookup_insert(id: TypeId, instance_id: u32, data: TypeData) {
    TL_CACHE.with(|cache| cache.lookup_insert(id, instance_id, data));
}

#[inline(always)]
pub(super) fn intern_probe(hash: u64, instance_id: u32, key: &TypeData) -> Option<TypeId> {
    TL_CACHE.with(|cache| cache.intern_probe(hash, instance_id, key))
}

#[inline(always)]
pub(super) fn intern_insert(hash: u64, instance_id: u32, key: TypeData, result: TypeId) {
    TL_CACHE.with(|cache| cache.intern_insert(hash, instance_id, key, result));
}

#[inline]
pub(super) fn string_probe(hash: u64, instance_id: u32, s: &str) -> Option<Atom> {
    TL_CACHE.with(|cache| cache.string_probe(hash, instance_id, s))
}

#[inline]
pub(super) fn string_insert(hash: u64, instance_id: u32, s: &str, result: Atom) {
    TL_CACHE.with(|cache| cache.string_insert(hash, instance_id, s, result));
}
