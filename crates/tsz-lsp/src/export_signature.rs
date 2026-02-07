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

impl ExportSignature {
    /// Compute the export signature for a file from its binder state.
    ///
    /// The signature captures:
    /// - Direct exports: `export function foo()`, `export class Bar`, etc.
    /// - Named re-exports: `export { foo } from './module'`
    /// - Wildcard re-exports: `export * from './module'`
    /// - Global augmentations: `declare global { ... }`
    /// - Module augmentations: `declare module 'x' { ... }`
    ///
    /// The signature is deterministic (sorted keys) and position-independent
    /// (no NodeIndex, SymbolId, or byte offsets).
    pub fn compute(binder: &BinderState, file_name: &str) -> Self {
        let mut hasher = FxHasher::default();

        // Marker to distinguish sections
        0u8.hash(&mut hasher);

        // 1. Direct exports from module_exports
        if let Some(exports) = binder.module_exports.get(file_name) {
            let mut entries: Vec<(&String, &tsz_binder::SymbolId)> = exports.iter().collect();
            entries.sort_by_key(|(name, _)| *name);

            for (name, sym_id) in &entries {
                name.hash(&mut hasher);
                // Hash the symbol's flags (kind: function, class, interface, etc.)
                // and is_exported/is_type_only status — NOT the SymbolId value itself
                if let Some(symbol) = binder.get_symbol(**sym_id) {
                    symbol.flags.hash(&mut hasher);
                    symbol.is_type_only.hash(&mut hasher);
                }
            }
        }

        // 2. Named re-exports: export { X } from './module'
        1u8.hash(&mut hasher);
        if let Some(reexports) = binder.reexports.get(file_name) {
            let mut entries: Vec<_> = reexports.iter().collect();
            entries.sort_by_key(|(name, _)| *name);

            for (export_name, (source_module, original_name)) in entries {
                export_name.hash(&mut hasher);
                source_module.hash(&mut hasher);
                original_name.hash(&mut hasher);
            }
        }

        // 3. Wildcard re-exports: export * from './module'
        2u8.hash(&mut hasher);
        if let Some(wildcards) = binder.wildcard_reexports.get(file_name) {
            let mut sorted: Vec<&String> = wildcards.iter().collect();
            sorted.sort();
            for module in sorted {
                module.hash(&mut hasher);
            }
        }

        // 4. Global augmentations: declare global { ... }
        3u8.hash(&mut hasher);
        {
            let mut names: Vec<&String> = binder.global_augmentations.keys().collect();
            names.sort();
            for name in names {
                name.hash(&mut hasher);
                // Hash the count of augmentation declarations (structural change indicator)
                if let Some(decls) = binder.global_augmentations.get(name.as_str()) {
                    decls.len().hash(&mut hasher);
                }
            }
        }

        // 5. Module augmentations: declare module 'x' { ... }
        4u8.hash(&mut hasher);
        {
            let mut modules: Vec<&String> = binder.module_augmentations.keys().collect();
            modules.sort();
            for module in modules {
                module.hash(&mut hasher);
                if let Some(augmentations) = binder.module_augmentations.get(module.as_str()) {
                    // Hash augmentation names (position-independent)
                    let mut aug_names: Vec<&String> =
                        augmentations.iter().map(|a| &a.name).collect();
                    aug_names.sort();
                    for aug_name in aug_names {
                        aug_name.hash(&mut hasher);
                    }
                }
            }
        }

        // 6. Also hash the file-level symbol flags for file_locals that are exported
        // This catches cases like changing `const x = 1` to `function x() {}` in exports
        5u8.hash(&mut hasher);
        {
            let mut exported_locals: Vec<(&String, &tsz_binder::SymbolId)> = binder
                .file_locals
                .iter()
                .filter(|(_, sym_id)| binder.get_symbol(**sym_id).is_some_and(|s| s.is_exported))
                .collect();
            exported_locals.sort_by_key(|(name, _)| *name);

            for (name, sym_id) in exported_locals {
                name.hash(&mut hasher);
                if let Some(symbol) = binder.get_symbol(*sym_id) {
                    symbol.flags.hash(&mut hasher);
                    symbol.is_type_only.hash(&mut hasher);
                }
            }
        }

        ExportSignature(hasher.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_body_edit_preserves_signature() {
        // Two files with the same exports but different function bodies
        // should produce the same export signature.
        let source_a = "export function foo() { return 1; }";
        let source_b = "export function foo() { return 2; }";

        let file_name = "test.ts";

        let mut parser_a =
            tsz_parser::ParserState::new(file_name.to_string(), source_a.to_string());
        let root_a = parser_a.parse_source_file();
        let mut binder_a = BinderState::new();
        binder_a.bind_source_file(parser_a.get_arena(), root_a);

        let mut parser_b =
            tsz_parser::ParserState::new(file_name.to_string(), source_b.to_string());
        let root_b = parser_b.parse_source_file();
        let mut binder_b = BinderState::new();
        binder_b.bind_source_file(parser_b.get_arena(), root_b);

        let sig_a = ExportSignature::compute(&binder_a, file_name);
        let sig_b = ExportSignature::compute(&binder_b, file_name);

        assert_eq!(
            sig_a, sig_b,
            "Body-only edit should not change export signature"
        );
    }

    #[test]
    fn test_adding_export_changes_signature() {
        let source_a = "export function foo() { return 1; }";
        let source_b = "export function foo() { return 1; }\nexport function bar() { return 2; }";

        let file_name = "test.ts";

        let mut parser_a =
            tsz_parser::ParserState::new(file_name.to_string(), source_a.to_string());
        let root_a = parser_a.parse_source_file();
        let mut binder_a = BinderState::new();
        binder_a.bind_source_file(parser_a.get_arena(), root_a);

        let mut parser_b =
            tsz_parser::ParserState::new(file_name.to_string(), source_b.to_string());
        let root_b = parser_b.parse_source_file();
        let mut binder_b = BinderState::new();
        binder_b.bind_source_file(parser_b.get_arena(), root_b);

        let sig_a = ExportSignature::compute(&binder_a, file_name);
        let sig_b = ExportSignature::compute(&binder_b, file_name);

        assert_ne!(sig_a, sig_b, "Adding an export should change the signature");
    }

    #[test]
    fn test_removing_export_changes_signature() {
        let source_a = "export function foo() {}\nexport function bar() {}";
        let source_b = "export function foo() {}\nfunction bar() {}";

        let file_name = "test.ts";

        let mut parser_a =
            tsz_parser::ParserState::new(file_name.to_string(), source_a.to_string());
        let root_a = parser_a.parse_source_file();
        let mut binder_a = BinderState::new();
        binder_a.bind_source_file(parser_a.get_arena(), root_a);

        let mut parser_b =
            tsz_parser::ParserState::new(file_name.to_string(), source_b.to_string());
        let root_b = parser_b.parse_source_file();
        let mut binder_b = BinderState::new();
        binder_b.bind_source_file(parser_b.get_arena(), root_b);

        let sig_a = ExportSignature::compute(&binder_a, file_name);
        let sig_b = ExportSignature::compute(&binder_b, file_name);

        assert_ne!(
            sig_a, sig_b,
            "Removing an export should change the signature"
        );
    }

    #[test]
    fn test_comment_edit_preserves_signature() {
        let source_a = "// version 1\nexport const x = 1;";
        let source_b = "// version 2\nexport const x = 1;";

        let file_name = "test.ts";

        let mut parser_a =
            tsz_parser::ParserState::new(file_name.to_string(), source_a.to_string());
        let root_a = parser_a.parse_source_file();
        let mut binder_a = BinderState::new();
        binder_a.bind_source_file(parser_a.get_arena(), root_a);

        let mut parser_b =
            tsz_parser::ParserState::new(file_name.to_string(), source_b.to_string());
        let root_b = parser_b.parse_source_file();
        let mut binder_b = BinderState::new();
        binder_b.bind_source_file(parser_b.get_arena(), root_b);

        let sig_a = ExportSignature::compute(&binder_a, file_name);
        let sig_b = ExportSignature::compute(&binder_b, file_name);

        assert_eq!(
            sig_a, sig_b,
            "Comment-only edit should not change export signature"
        );
    }

    #[test]
    fn test_private_addition_preserves_signature() {
        let source_a = "export function foo() {}";
        let source_b = "const helper = 42;\nexport function foo() {}";

        let file_name = "test.ts";

        let mut parser_a =
            tsz_parser::ParserState::new(file_name.to_string(), source_a.to_string());
        let root_a = parser_a.parse_source_file();
        let mut binder_a = BinderState::new();
        binder_a.bind_source_file(parser_a.get_arena(), root_a);

        let mut parser_b =
            tsz_parser::ParserState::new(file_name.to_string(), source_b.to_string());
        let root_b = parser_b.parse_source_file();
        let mut binder_b = BinderState::new();
        binder_b.bind_source_file(parser_b.get_arena(), root_b);

        let sig_a = ExportSignature::compute(&binder_a, file_name);
        let sig_b = ExportSignature::compute(&binder_b, file_name);

        assert_eq!(
            sig_a, sig_b,
            "Adding a private symbol should not change export signature"
        );
    }

    #[test]
    fn test_no_exports_consistent() {
        let source_a = "const x = 1;";
        let source_b = "const x = 2; const y = 3;";

        let file_name = "test.ts";

        let mut parser_a =
            tsz_parser::ParserState::new(file_name.to_string(), source_a.to_string());
        let root_a = parser_a.parse_source_file();
        let mut binder_a = BinderState::new();
        binder_a.bind_source_file(parser_a.get_arena(), root_a);

        let mut parser_b =
            tsz_parser::ParserState::new(file_name.to_string(), source_b.to_string());
        let root_b = parser_b.parse_source_file();
        let mut binder_b = BinderState::new();
        binder_b.bind_source_file(parser_b.get_arena(), root_b);

        let sig_a = ExportSignature::compute(&binder_a, file_name);
        let sig_b = ExportSignature::compute(&binder_b, file_name);

        assert_eq!(
            sig_a, sig_b,
            "Files with no exports should have the same signature"
        );
    }
}
