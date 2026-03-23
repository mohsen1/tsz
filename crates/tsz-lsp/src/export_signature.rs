//! Position-independent export signature for smart cache invalidation.
//!
//! When a file changes, we reparse and rebind it (always necessary). But we only
//! need to invalidate *dependent* files' caches if the file's **public API** changed.
//! Most edits (function body changes, comments, whitespace) don't change the public
//! API, so dependents can keep their cached diagnostics.
//!
//! The `ExportSignature` is a deterministic hash of a file's exported symbols,
//! re-exports, wildcard re-exports, and augmentations. It is position-independent:
//! no `NodeIndex`, `SymbolId`, or byte offsets are included. Only names, kinds, and
//! structural relationships.
//!
//! # Unified semantics
//!
//! Both CLI (incremental compilation) and LSP (project updates) use
//! `ExportSignatureInput` → `ExportSignature::from_input` to ensure identical
//! hashing. The `ExportSignatureInput` is the shared contract: both systems
//! construct it from their respective data sources (binder state or merged program)
//! and get the same fingerprint for the same public API surface.
//!
//! # How it works
//!
//! After rebinding a file, we compute its new `ExportSignature` and compare it with
//! the previous one. If identical, dependent files' caches are NOT invalidated.
//! If different, we fall back to the current behavior (invalidate all dependents).

use std::hash::{Hash, Hasher};

use rustc_hash::FxHasher;
use tsz_binder::BinderState;

/// A 64-bit hash representing the position-independent public API of a file.
///
/// Two files with the same `ExportSignature` expose the same set of exported names,
/// with the same symbol kinds, re-export relationships, and augmentations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportSignature(pub u64);

/// Normalized, sorted representation of a file's public API surface.
///
/// This is the shared contract between CLI and LSP for export signature computation.
/// Both systems construct an `ExportSignatureInput` from their respective data
/// sources and pass it to `ExportSignature::from_input` to get identical hashes.
///
/// All fields must be pre-sorted by their primary key for deterministic hashing.
#[derive(Debug, Clone, Default)]
pub struct ExportSignatureInput {
    /// `(name, flags, is_type_only)` for direct module exports, sorted by name.
    pub exports: Vec<(String, u32, bool)>,
    /// `(export_name, source_module, original_name)` for named re-exports, sorted by `export_name`.
    pub named_reexports: Vec<(String, String, Option<String>)>,
    /// `(source_module, is_type_only)` for wildcard re-exports, sorted by `source_module`.
    pub wildcard_reexports: Vec<(String, bool)>,
    /// `(augmented_name, declaration_count)` for global augmentations, sorted by name.
    pub global_augmentations: Vec<(String, usize)>,
    /// `(module_name, sorted_augmentation_names)` for module augmentations, sorted by `module_name`.
    pub module_augmentations: Vec<(String, Vec<String>)>,
    /// `(name, flags, is_type_only)` for exported file-local symbols, sorted by name.
    pub exported_locals: Vec<(String, u32, bool)>,
}

/// Summary of what changed between two export signatures.
///
/// Useful for perf analysis: shows whether dependents were invalidated and why.
#[derive(Debug, Clone)]
pub struct InvalidationSummary {
    /// The file that was edited.
    pub file: String,
    /// Whether the public API changed (signature mismatch).
    pub api_changed: bool,
    /// Number of dependent files that were invalidated (0 if API unchanged).
    pub dependents_invalidated: usize,
    /// Old signature hash (None if file is new).
    pub old_signature: Option<u64>,
    /// New signature hash.
    pub new_signature: u64,
}

impl InvalidationSummary {
    /// Create a summary for a file whose API did not change.
    pub const fn unchanged(file: String, signature: u64) -> Self {
        Self {
            file,
            api_changed: false,
            dependents_invalidated: 0,
            old_signature: Some(signature),
            new_signature: signature,
        }
    }

    /// Create a summary for a file whose API changed.
    pub const fn changed(
        file: String,
        old_signature: Option<u64>,
        new_signature: u64,
        dependents_invalidated: usize,
    ) -> Self {
        Self {
            file,
            api_changed: true,
            dependents_invalidated,
            old_signature,
            new_signature,
        }
    }

    /// Create a summary for a new file.
    pub const fn new_file(file: String, signature: u64) -> Self {
        Self {
            file,
            api_changed: true,
            dependents_invalidated: 0,
            old_signature: None,
            new_signature: signature,
        }
    }
}

impl ExportSignatureInput {
    /// Construct from a `BinderState` (LSP path).
    ///
    /// Extracts exported names, flags, re-exports, and augmentations from the
    /// binder's per-file data structures.
    pub fn from_binder(binder: &BinderState, file_name: &str) -> Self {
        let mut input = Self::default();

        // 1. Direct exports from module_exports
        if let Some(exports) = binder.module_exports.get(file_name) {
            let mut entries: Vec<_> = exports.iter().collect();
            entries.sort_by_key(|(name, _)| *name);

            for (name, sym_id) in entries {
                if let Some(symbol) = binder.get_symbol(*sym_id) {
                    input
                        .exports
                        .push((name.clone(), symbol.flags, symbol.is_type_only));
                }
            }
        }

        // 2. Named re-exports
        if let Some(reexports) = binder.reexports.get(file_name) {
            let mut entries: Vec<_> = reexports.iter().collect();
            entries.sort_by_key(|(name, _)| *name);

            for (export_name, (source_module, original_name)) in entries {
                input.named_reexports.push((
                    export_name.clone(),
                    source_module.clone(),
                    original_name.clone(),
                ));
            }
        }

        // 3. Wildcard re-exports (with type_only provenance)
        if let Some(wildcards) = binder.wildcard_reexports.get(file_name) {
            let type_only_entries = binder.wildcard_reexports_type_only.get(file_name);
            let mut entries: Vec<(String, bool)> = wildcards
                .iter()
                .enumerate()
                .map(|(i, module)| {
                    let is_type_only = type_only_entries
                        .and_then(|v| v.get(i))
                        .is_some_and(|(_, to)| *to);
                    (module.clone(), is_type_only)
                })
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            input.wildcard_reexports = entries;
        }

        // 4. Global augmentations
        {
            let mut names: Vec<&String> = binder.global_augmentations.keys().collect();
            names.sort();
            for name in names {
                let count = binder
                    .global_augmentations
                    .get(name.as_str())
                    .map_or(0, Vec::len);
                input.global_augmentations.push((name.clone(), count));
            }
        }

        // 5. Module augmentations
        {
            let mut modules: Vec<&String> = binder.module_augmentations.keys().collect();
            modules.sort();
            for module in modules {
                let mut aug_names: Vec<String> = binder
                    .module_augmentations
                    .get(module.as_str())
                    .map(|augs| augs.iter().map(|a| a.name.clone()).collect())
                    .unwrap_or_default();
                aug_names.sort();
                input.module_augmentations.push((module.clone(), aug_names));
            }
        }

        // 6. Exported file-local symbols
        {
            let mut exported_locals: Vec<_> = binder
                .file_locals
                .iter()
                .filter(|(_, sym_id)| binder.get_symbol(**sym_id).is_some_and(|s| s.is_exported))
                .collect();
            exported_locals.sort_by_key(|(name, _)| *name);

            for (name, sym_id) in exported_locals {
                if let Some(symbol) = binder.get_symbol(*sym_id) {
                    input
                        .exported_locals
                        .push((name.clone(), symbol.flags, symbol.is_type_only));
                }
            }
        }

        input
    }

    /// Construct from a precomputed `ExportSurface`.
    ///
    /// This is the preferred path when an `ExportSurface` has already been
    /// built — it avoids re-reading binder maps.
    pub fn from_surface(surface: &tsz_binder::ExportSurface) -> Self {
        let mut input = Self::default();

        // 1. Direct exports (sorted by name for deterministic hashing)
        let mut export_names: Vec<&str> =
            surface.exported_locals.keys().map(String::as_str).collect();
        export_names.sort();
        for name in &export_names {
            if let Some(entry) = surface.exported_locals.get(*name) {
                input
                    .exports
                    .push((name.to_string(), entry.flags, entry.is_type_only));
            }
        }

        // 2. Named re-exports (already sorted in ExportSurface)
        for re in &surface.named_reexports {
            input.named_reexports.push((
                re.export_name.clone(),
                re.source_module.clone(),
                re.original_name.clone(),
            ));
        }

        // 3. Wildcard re-exports (already sorted in ExportSurface)
        for wc in &surface.wildcard_reexports {
            input
                .wildcard_reexports
                .push((wc.source_module.clone(), wc.is_type_only));
        }

        // 4. Global augmentations
        input.global_augmentations = surface.global_augmentations.clone();

        // 5. Module augmentations
        input.module_augmentations = surface.module_augmentations.clone();

        // 6. Exported file-local symbols — in the surface these are merged
        //    into `exported_locals`, so `exported_locals` above already
        //    covers them.  We leave this section empty to preserve hash
        //    compatibility with the tuple-based format.  (The hash sections
        //    are tagged, so an empty section 5 is fine.)

        input
    }
}

impl ExportSignature {
    /// Compute the export signature from a normalized input.
    ///
    /// This is the single authoritative hashing implementation used by both
    /// CLI and LSP. Both systems construct an `ExportSignatureInput` from their
    /// respective data sources and call this method.
    pub fn from_input(input: &ExportSignatureInput) -> Self {
        let mut hasher = FxHasher::default();

        // Section 0: Direct exports
        0u8.hash(&mut hasher);
        for (name, flags, is_type_only) in &input.exports {
            name.hash(&mut hasher);
            flags.hash(&mut hasher);
            is_type_only.hash(&mut hasher);
        }

        // Section 1: Named re-exports
        1u8.hash(&mut hasher);
        for (export_name, source_module, original_name) in &input.named_reexports {
            export_name.hash(&mut hasher);
            source_module.hash(&mut hasher);
            original_name.hash(&mut hasher);
        }

        // Section 2: Wildcard re-exports
        2u8.hash(&mut hasher);
        for (module, is_type_only) in &input.wildcard_reexports {
            module.hash(&mut hasher);
            is_type_only.hash(&mut hasher);
        }

        // Section 3: Global augmentations
        3u8.hash(&mut hasher);
        for (name, count) in &input.global_augmentations {
            name.hash(&mut hasher);
            count.hash(&mut hasher);
        }

        // Section 4: Module augmentations
        4u8.hash(&mut hasher);
        for (module, aug_names) in &input.module_augmentations {
            module.hash(&mut hasher);
            for aug_name in aug_names {
                aug_name.hash(&mut hasher);
            }
        }

        // Section 5: Exported file-local symbols
        5u8.hash(&mut hasher);
        for (name, flags, is_type_only) in &input.exported_locals {
            name.hash(&mut hasher);
            flags.hash(&mut hasher);
            is_type_only.hash(&mut hasher);
        }

        Self(hasher.finish())
    }

    /// Compute the export signature for a file from its binder state.
    ///
    /// Convenience method that constructs an `ExportSignatureInput` from the
    /// binder and delegates to `from_input`. This ensures the LSP uses the
    /// same hashing logic as the CLI.
    pub fn compute(binder: &BinderState, file_name: &str) -> Self {
        let input = ExportSignatureInput::from_binder(binder, file_name);
        Self::from_input(&input)
    }
}

#[cfg(test)]
#[path = "../tests/export_signature_tests.rs"]
mod export_signature_tests;
