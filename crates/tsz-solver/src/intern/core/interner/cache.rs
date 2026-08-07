//! Thread-local direct-mapped caches for type interning and lookup.
//!
//! This module is the interner's thread-local fast path: small direct-mapped
//! caches probed on every `lookup`, `intern`, and `intern_string` call. The
//! string cache (#13642) deliberately stays off the two existing pedantic
//! tripwires without a suppression attribute: its probe/insert wrappers use a
//! plain `#[inline]` hint (trivial bodies LLVM inlines anyway) so they do not
//! add to the `inline_always` ratchet, and its backing array is sized to keep
//! the per-thread allocation under the `large_stack_arrays` threshold.

use super::UnionComplexityThreadState;
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
    /// Exceptional `TS2590` state for the small number of type universes used
    /// by this worker. Slots are assigned only when an event is produced and
    /// remain stable until the compilation-session cache is cleared, so a
    /// live checkpoint cannot lose its producer epoch through eviction.
    union_complexity: [Cell<UnionComplexityThreadState>; UNION_COMPLEXITY_SLOT_COUNT],
}

/// A CLI worker normally sees one `TypeInterner`; extra slots cover nested
/// test/project universes without allocating or hashing on the exceptional
/// signal path. If all slots are occupied, the owning interner provides a
/// bounded-lifetime overflow map keyed by worker thread.
const UNION_COMPLEXITY_SLOT_COUNT: usize = 8;

pub(super) enum UnionComplexityStateLookup {
    Found(UnionComplexityThreadState),
    /// At least one fixed slot is still empty, proving this worker has never
    /// overflowed for the requested interner.
    Absent,
    /// All fixed slots are occupied, so the interner-owned overflow map may
    /// contain this worker's state.
    OverflowPossible,
}

pub(super) enum UnionComplexityPendingUpdate {
    Updated(u32),
    Absent,
    OverflowPossible,
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

const EMPTY_UNION_COMPLEXITY_STATE: UnionComplexityThreadState = UnionComplexityThreadState {
    instance_id: 0,
    produced_epoch: 0,
    pending_count: 0,
};

impl TypeInternerCache {
    #[allow(clippy::large_stack_arrays)]
    const fn new() -> Self {
        Self {
            lookup: [const { Cell::new(EMPTY_LOOKUP_ENTRY) }; LOOKUP_CACHE_SIZE],
            intern: [const { Cell::new(EMPTY_INTERN_ENTRY) }; INTERN_CACHE_SIZE],
            string: [const { Cell::new(EMPTY_STRING_ENTRY) }; STRING_CACHE_SIZE],
            union_complexity: [const { Cell::new(EMPTY_UNION_COMPLEXITY_STATE) };
                UNION_COMPLEXITY_SLOT_COUNT],
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

    /// Insert into the lookup cache, returning `true` when the slot already
    /// held a live entry (non-empty tag) for a *different* `TypeId` in this
    /// interner — a direct-mapped collision that evicts a still-useful entry.
    /// The boolean drives the `#13246` locality eviction counter; the insert
    /// itself is unconditional and correctness is unaffected (a stale slot is
    /// always re-validated on probe via tag + `instance_id`).
    #[inline(always)]
    fn lookup_insert(&self, id: TypeId, instance_id: u32, data: TypeData) -> bool {
        let idx = (id.0 & LOOKUP_CACHE_MASK) as usize;
        let prev = self.lookup[idx].get();
        let evicted = prev.tag != 0 && (prev.tag != id.0 || prev.instance_id != instance_id);
        self.lookup[idx].set(LookupCacheEntry {
            tag: id.0,
            instance_id,
            data,
        });
        evicted
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

    /// Insert into the intern cache, returning `true` when the slot already
    /// held a live entry (non-zero `result`) for a *different* hash/instance —
    /// a direct-mapped collision that evicts a still-useful entry. Drives the
    /// `#13246` intern-side eviction counter; correctness is unaffected
    /// (probe re-validates hash + key + `instance_id`).
    #[inline(always)]
    fn intern_insert(&self, hash: u64, instance_id: u32, key: TypeData, result: TypeId) -> bool {
        let idx = (hash & INTERN_CACHE_MASK) as usize;
        let prev = self.intern[idx].get();
        let evicted =
            prev.result != TypeId::NONE && (prev.hash != hash || prev.instance_id != instance_id);
        self.intern[idx].set(InternCacheEntry {
            hash,
            instance_id,
            key,
            result,
        });
        evicted
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

    /// Record a new event in this worker's fixed signal slots.
    ///
    /// Returns the previous pending state, or `None` when all slots belong to
    /// other live/checkpointed interner instances and the caller must use its
    /// interner-owned overflow map.
    fn mark_union_complexity(&self, instance_id: u32, produced_epoch: u64) -> Option<bool> {
        let mut empty_slot = None;
        for slot in &self.union_complexity {
            let state = slot.get();
            if state.instance_id == instance_id {
                let previous_pending_count = state.pending_count;
                slot.set(UnionComplexityThreadState {
                    produced_epoch,
                    pending_count: previous_pending_count.saturating_add(1),
                    ..state
                });
                return Some(previous_pending_count != 0);
            }
            if state.instance_id == 0 && empty_slot.is_none() {
                empty_slot = Some(slot);
            }
        }

        let slot = empty_slot?;
        slot.set(UnionComplexityThreadState {
            instance_id,
            produced_epoch,
            pending_count: 1,
        });
        Some(false)
    }

    fn union_complexity_state(&self, instance_id: u32) -> UnionComplexityStateLookup {
        let mut has_empty_slot = false;
        for slot in &self.union_complexity {
            let state = slot.get();
            if state.instance_id == instance_id {
                return UnionComplexityStateLookup::Found(state);
            }
            has_empty_slot |= state.instance_id == 0;
        }
        if has_empty_slot {
            UnionComplexityStateLookup::Absent
        } else {
            UnionComplexityStateLookup::OverflowPossible
        }
    }

    /// Change the pending-event count in an already assigned fixed slot,
    /// returning its previous value while distinguishing a proven absence
    /// from a possible overflow entry.
    fn set_union_complexity_pending_count(
        &self,
        instance_id: u32,
        pending_count: u32,
    ) -> UnionComplexityPendingUpdate {
        let mut has_empty_slot = false;
        for slot in &self.union_complexity {
            let mut state = slot.get();
            if state.instance_id == instance_id {
                let previous = state.pending_count;
                state.pending_count = pending_count;
                slot.set(state);
                return UnionComplexityPendingUpdate::Updated(previous);
            }
            has_empty_slot |= state.instance_id == 0;
        }
        if has_empty_slot {
            UnionComplexityPendingUpdate::Absent
        } else {
            UnionComplexityPendingUpdate::OverflowPossible
        }
    }
}

thread_local! {
    static TL_CACHE: TypeInternerCache = const { TypeInternerCache::new() };
}

/// Clear caches owned by the calling worker only.
///
/// Persistent Rayon workers can retain fixed union-complexity slots across
/// compilation sessions. Those slots are instance-tagged and never reused for
/// another universe; after all eight are occupied, a later signaled universe
/// takes its interner-owned overflow path instead.
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
        for cell in &cache.union_complexity {
            cell.set(EMPTY_UNION_COMPLEXITY_STATE);
        }
    });
}

#[inline]
pub(super) fn mark_union_complexity(instance_id: u32, produced_epoch: u64) -> Option<bool> {
    TL_CACHE.with(|cache| cache.mark_union_complexity(instance_id, produced_epoch))
}

#[inline]
pub(super) fn union_complexity_state(instance_id: u32) -> UnionComplexityStateLookup {
    TL_CACHE.with(|cache| cache.union_complexity_state(instance_id))
}

#[inline]
pub(super) fn set_union_complexity_pending_count(
    instance_id: u32,
    pending_count: u32,
) -> UnionComplexityPendingUpdate {
    TL_CACHE.with(|cache| cache.set_union_complexity_pending_count(instance_id, pending_count))
}

#[inline(always)]
pub(super) fn lookup_probe(id: TypeId, instance_id: u32) -> Option<TypeData> {
    TL_CACHE.with(|cache| cache.lookup_probe(id, instance_id))
}

/// Insert into the TLS lookup cache. Returns `true` when a live entry for a
/// different `TypeId` was evicted (a direct-mapped collision), which the
/// `lookup()` hot path forwards to the `#13246` eviction counter.
#[inline(always)]
pub(super) fn lookup_insert(id: TypeId, instance_id: u32, data: TypeData) -> bool {
    TL_CACHE.with(|cache| cache.lookup_insert(id, instance_id, data))
}

#[inline(always)]
pub(super) fn intern_probe(hash: u64, instance_id: u32, key: &TypeData) -> Option<TypeId> {
    TL_CACHE.with(|cache| cache.intern_probe(hash, instance_id, key))
}

/// Insert into the TLS intern cache. Returns `true` when a live entry for a
/// different hash/instance was evicted (a direct-mapped collision).
#[inline(always)]
pub(super) fn intern_insert(hash: u64, instance_id: u32, key: TypeData, result: TypeId) -> bool {
    TL_CACHE.with(|cache| cache.intern_insert(hash, instance_id, key, result))
}

#[inline]
pub(super) fn string_probe(hash: u64, instance_id: u32, s: &str) -> Option<Atom> {
    TL_CACHE.with(|cache| cache.string_probe(hash, instance_id, s))
}

#[inline]
pub(super) fn string_insert(hash: u64, instance_id: u32, s: &str, result: Atom) {
    TL_CACHE.with(|cache| cache.string_insert(hash, instance_id, s, result));
}

// ---------------------------------------------------------------------------
// Promoted global lookup tier (opt-in probe, `TSZ_PROMOTE_FIRST`, issue #13246)
// ---------------------------------------------------------------------------
// Measurement-only. The per-instance TLS lookup cache is small (1024 slots) and
// thrashes once a file's working set exceeds it, sending lookups to the cold
// sharded `RwLock<Vec<TypeData>>`. This promoted tier is a *much larger*
// process-global direct-mapped cache. When the probe is ON, every cold-Vec
// fallback also populates this tier, and the next time the same id misses the
// TLS cache the tier serves it before the cold shard. The fraction of TLS
// misses the tier catches (`interner_promote_tier_hits`) is a direct estimate
// of how much a bounded-partition / promoted hot-set interner would cut the
// cold-Vec fallback rate. It never changes the answer: the tier only ever holds
// `(instance_id, TypeId) -> TypeData` pairs the cold shard already returned, so
// a tier hit yields the identical `TypeData` the shard read would have.
//
// The tier is sized to comfortably hold a large file's working set (256K slots,
// ~12 bytes/slot under `Cell<u32 tag + u32 instance + TypeData>` ≈ a few MB) so
// it measures the "no-thrash" ceiling rather than a second small cache. It is
// only allocated when `TSZ_PROMOTE_FIRST` is set, so default runs never pay for
// it. A `RwLock` guards interior mutability for `Sync`; under the single-thread
// CLI/bench workload the read/write locks are uncontended.

const PROMOTE_TIER_BITS: u32 = 18;
const PROMOTE_TIER_SIZE: usize = 1 << PROMOTE_TIER_BITS; // 262_144
const PROMOTE_TIER_MASK: u32 = (PROMOTE_TIER_SIZE as u32) - 1;

#[derive(Clone, Copy)]
struct PromoteTierEntry {
    tag: u32,
    instance_id: u32,
    data: TypeData,
}

const EMPTY_PROMOTE_ENTRY: PromoteTierEntry = PromoteTierEntry {
    tag: 0,
    instance_id: 0,
    data: TypeData::Error,
};

static PROMOTE_TIER: std::sync::OnceLock<std::sync::RwLock<Vec<PromoteTierEntry>>> =
    std::sync::OnceLock::new();

#[inline]
fn promote_tier() -> &'static std::sync::RwLock<Vec<PromoteTierEntry>> {
    PROMOTE_TIER
        .get_or_init(|| std::sync::RwLock::new(vec![EMPTY_PROMOTE_ENTRY; PROMOTE_TIER_SIZE]))
}

/// Probe the promoted global tier for a `TypeId` after a TLS-cache miss.
/// Returns the cached `TypeData` when present for this interner. Only called
/// when `TSZ_PROMOTE_FIRST` is enabled; otherwise the tier is never touched.
#[inline]
pub(super) fn promote_tier_probe(id: TypeId, instance_id: u32) -> Option<TypeData> {
    let idx = (id.0 & PROMOTE_TIER_MASK) as usize;
    let tier = promote_tier().read().ok()?;
    let entry = tier[idx];
    if entry.tag == id.0 && entry.instance_id == instance_id {
        Some(entry.data)
    } else {
        None
    }
}

/// Populate the promoted global tier after a cold-Vec fallback resolved a
/// `TypeId`. Only called when `TSZ_PROMOTE_FIRST` is enabled.
#[inline]
pub(super) fn promote_tier_insert(id: TypeId, instance_id: u32, data: TypeData) {
    let idx = (id.0 & PROMOTE_TIER_MASK) as usize;
    if let Ok(mut tier) = promote_tier().write() {
        tier[idx] = PromoteTierEntry {
            tag: id.0,
            instance_id,
            data,
        };
    }
}
