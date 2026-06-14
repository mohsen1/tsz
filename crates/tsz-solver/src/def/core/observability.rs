//! Observability snapshot ([`StoreStatistics`]) for the definition store.
//!
//! Splits the size/composition reporting concern out of the storage core: the
//! `StoreStatistics` value type, its additive merge, `Display`, and the
//! `DefinitionStore` methods that compute a live snapshot
//! (`statistics`, `estimated_size_bytes`). Telemetry only -- no store mutation.

use super::{DefId, DefKind, DefinitionInfo, DefinitionStore, EnumMemberValue};
use crate::types::{ObjectShape, PropertyInfo, TypeId, TypeParamInfo};
use std::sync::atomic::Ordering;
use tsz_common::interner::Atom;

/// Snapshot of `DefinitionStore` sizes and composition.
///
/// Provides observability into the store's current state for performance
/// monitoring, capacity planning, and debugging. All counts are computed
/// at the time of the `statistics()` call and represent a consistent-ish
/// snapshot (individual `DashMap` reads are atomic but not globally synchronized).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoreStatistics {
    /// Total number of definitions.
    pub total_definitions: usize,

    /// Number of definitions by kind.
    pub type_aliases: usize,
    /// Number of interface definitions.
    pub interfaces: usize,
    /// Number of class definitions.
    pub classes: usize,
    /// Number of class constructor definitions.
    pub class_constructors: usize,
    /// Number of enum definitions.
    pub enums: usize,
    /// Number of namespace definitions.
    pub namespaces: usize,
    /// Number of function definitions.
    pub functions: usize,
    /// Number of variable definitions.
    pub variables: usize,

    /// Number of entries in the `TypeId` -> `DefId` reverse index.
    pub type_to_def_entries: usize,
    /// Number of entries in the `(SymbolId, file_idx)` -> `DefId` index.
    pub symbol_def_index_entries: usize,
    /// Number of entries in the `SymbolId` -> `DefId` (file-agnostic) index.
    pub symbol_only_index_entries: usize,
    /// Number of entries in the body `TypeId` -> `DefId` alias index.
    pub body_to_alias_entries: usize,
    /// Number of entries in the shape hash -> `DefId` index.
    pub shape_to_def_entries: usize,
    /// Number of entries in the class -> constructor companion index.
    pub class_to_constructor_entries: usize,
    /// Number of unique names in the name -> `DefId` index.
    pub name_to_defs_entries: usize,
    /// Number of files with registered definitions.
    pub file_count: usize,

    /// Next `DefId` value (high-water mark of allocation).
    pub next_def_id: u32,

    /// Estimated heap memory footprint of the store in bytes.
    ///
    /// Populated by `DefinitionStore::statistics()` using the live
    /// `estimated_size_bytes()` method. Zero when constructed via `Default`.
    pub estimated_size_bytes: usize,
}

impl StoreStatistics {
    /// Merge another `StoreStatistics` into this one (additive).
    ///
    /// Used to aggregate per-file statistics from parallel checking,
    /// where each checker has its own `DefinitionStore`.
    pub const fn merge(&mut self, other: &StoreStatistics) {
        self.total_definitions += other.total_definitions;
        self.type_aliases += other.type_aliases;
        self.interfaces += other.interfaces;
        self.classes += other.classes;
        self.class_constructors += other.class_constructors;
        self.enums += other.enums;
        self.namespaces += other.namespaces;
        self.functions += other.functions;
        self.variables += other.variables;
        self.type_to_def_entries += other.type_to_def_entries;
        self.symbol_def_index_entries += other.symbol_def_index_entries;
        self.symbol_only_index_entries += other.symbol_only_index_entries;
        self.body_to_alias_entries += other.body_to_alias_entries;
        self.shape_to_def_entries += other.shape_to_def_entries;
        self.class_to_constructor_entries += other.class_to_constructor_entries;
        self.name_to_defs_entries += other.name_to_defs_entries;
        self.file_count += other.file_count;
        // next_def_id: take the maximum (high-water mark)
        if other.next_def_id > self.next_def_id {
            self.next_def_id = other.next_def_id;
        }
        self.estimated_size_bytes += other.estimated_size_bytes;
    }
}

impl std::fmt::Display for StoreStatistics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "DefinitionStore statistics:")?;
        writeln!(f, "  definitions: {} total", self.total_definitions)?;
        writeln!(
            f,
            "    type_aliases={}, interfaces={}, classes={}, class_constructors={}",
            self.type_aliases, self.interfaces, self.classes, self.class_constructors
        )?;
        writeln!(
            f,
            "    enums={}, namespaces={}, functions={}, variables={}",
            self.enums, self.namespaces, self.functions, self.variables
        )?;
        writeln!(f, "  indices:")?;
        writeln!(f, "    type_to_def={}", self.type_to_def_entries)?;
        writeln!(f, "    symbol_def_index={}", self.symbol_def_index_entries)?;
        writeln!(
            f,
            "    symbol_only_index={}",
            self.symbol_only_index_entries
        )?;
        writeln!(f, "    body_to_alias={}", self.body_to_alias_entries)?;
        writeln!(f, "    shape_to_def={}", self.shape_to_def_entries)?;
        writeln!(
            f,
            "    class_to_constructor={}",
            self.class_to_constructor_entries
        )?;
        writeln!(f, "    name_to_defs={}", self.name_to_defs_entries)?;
        writeln!(f, "  files: {}", self.file_count)?;
        writeln!(f, "  next_def_id: {}", self.next_def_id)?;
        write!(
            f,
            "  estimated_size: {} bytes ({:.1} KB)",
            self.estimated_size_bytes,
            self.estimated_size_bytes as f64 / 1024.0,
        )
    }
}

impl DefinitionStore {
    /// Compute a snapshot of store sizes and composition.
    ///
    /// This iterates all definitions once to count by `DefKind`, plus reads
    /// the length of each reverse index. Suitable for periodic logging or
    /// on-demand diagnostics; avoid calling on every type check.
    pub fn statistics(&self) -> StoreStatistics {
        let mut stats = StoreStatistics {
            total_definitions: self.definitions.len(),
            type_to_def_entries: self.type_to_def.len(),
            symbol_def_index_entries: self.symbol_def_index.len(),
            symbol_only_index_entries: self.symbol_only_index.len(),
            body_to_alias_entries: self.body_to_alias.len(),
            shape_to_def_entries: self.shape_to_def.len(),
            class_to_constructor_entries: self.class_to_constructor.len(),
            name_to_defs_entries: self.name_to_defs.len(),
            file_count: self.file_to_defs.len(),
            next_def_id: self.next_id.load(Ordering::Relaxed),
            ..Default::default()
        };

        for entry in &self.definitions {
            match entry.value().kind {
                DefKind::TypeAlias => stats.type_aliases += 1,
                DefKind::Interface => stats.interfaces += 1,
                DefKind::Class => stats.classes += 1,
                DefKind::ClassConstructor => stats.class_constructors += 1,
                DefKind::Enum => stats.enums += 1,
                DefKind::Namespace => stats.namespaces += 1,
                DefKind::Function => stats.functions += 1,
                DefKind::Variable => stats.variables += 1,
            }
        }

        stats.estimated_size_bytes = self.estimated_size_bytes();
        stats
    }

    /// Estimate the heap memory footprint of the store in bytes.
    ///
    /// Accounts for the `DashMap` overhead of each index and the `Vec`-backed
    /// fields inside `DefinitionInfo`. The result is a rough lower bound —
    /// `DashMap` shard overhead, alignment padding, and allocator metadata are
    /// not included. Useful for memory pressure tracking and telemetry.
    #[must_use]
    pub fn estimated_size_bytes(&self) -> usize {
        let mut size = std::mem::size_of::<Self>();

        // Per-entry overhead for DashMap: key + value + ~64 bytes bucket/shard overhead.
        const DASHMAP_ENTRY_OVERHEAD: usize = 64;

        // definitions: DefId -> DefinitionInfo
        for entry in &self.definitions {
            let info = entry.value();
            size += std::mem::size_of::<DefId>() + std::mem::size_of::<DefinitionInfo>();
            size += DASHMAP_ENTRY_OVERHEAD;
            // Vec fields inside DefinitionInfo
            size += info.type_params.capacity() * std::mem::size_of::<TypeParamInfo>();
            size += info.enum_members.capacity() * std::mem::size_of::<(Atom, EnumMemberValue)>();
            size += info.implements.capacity() * std::mem::size_of::<DefId>();
            size += info.exports.capacity() * std::mem::size_of::<(Atom, DefId)>();
            // Arc<ObjectShape> — count the shape itself (shared, but we include it here)
            if let Some(ref shape) = info.instance_shape {
                size += std::mem::size_of::<ObjectShape>();
                size += shape.properties.capacity() * std::mem::size_of::<PropertyInfo>();
            }
            if let Some(ref shape) = info.static_shape {
                size += std::mem::size_of::<ObjectShape>();
                size += shape.properties.capacity() * std::mem::size_of::<PropertyInfo>();
            }
        }

        // type_to_def: TypeId -> DefId
        size += self.type_to_def.len()
            * (std::mem::size_of::<TypeId>()
                + std::mem::size_of::<DefId>()
                + DASHMAP_ENTRY_OVERHEAD);

        // symbol_def_index: (u32, u32) -> DefId
        size += self.symbol_def_index.len()
            * (std::mem::size_of::<(u32, u32)>()
                + std::mem::size_of::<DefId>()
                + DASHMAP_ENTRY_OVERHEAD);

        // symbol_only_index: u32 -> DefId
        size += self.symbol_only_index.len()
            * (std::mem::size_of::<u32>() + std::mem::size_of::<DefId>() + DASHMAP_ENTRY_OVERHEAD);

        // body_to_alias: TypeId -> DefId
        size += self.body_to_alias.len()
            * (std::mem::size_of::<TypeId>()
                + std::mem::size_of::<DefId>()
                + DASHMAP_ENTRY_OVERHEAD);

        // shape_to_def: u64 -> DefId
        size += self.shape_to_def.len()
            * (std::mem::size_of::<u64>() + std::mem::size_of::<DefId>() + DASHMAP_ENTRY_OVERHEAD);

        // class_to_constructor: DefId -> DefId
        size += self.class_to_constructor.len()
            * (std::mem::size_of::<DefId>()
                + std::mem::size_of::<DefId>()
                + DASHMAP_ENTRY_OVERHEAD);

        // class_to_instance: DefId -> TypeId
        size += self.class_to_instance.len()
            * (std::mem::size_of::<DefId>()
                + std::mem::size_of::<TypeId>()
                + DASHMAP_ENTRY_OVERHEAD);

        // file_to_defs: u32 -> Vec<DefId>
        for entry in &self.file_to_defs {
            size += std::mem::size_of::<u32>() + DASHMAP_ENTRY_OVERHEAD;
            size += entry.value().capacity() * std::mem::size_of::<DefId>();
        }

        // name_to_defs: Atom -> Vec<DefId>
        for entry in &self.name_to_defs {
            size += std::mem::size_of::<Atom>() + DASHMAP_ENTRY_OVERHEAD;
            size += entry.value().capacity() * std::mem::size_of::<DefId>();
        }

        // Cross-file query cache (entries + their Arc'd type-param vecs).
        size += self.cross_file_cache.estimated_size_bytes();

        size
    }
}
