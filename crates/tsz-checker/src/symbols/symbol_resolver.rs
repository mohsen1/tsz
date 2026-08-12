//! Symbol resolution helpers (identifier lookup, qualified name resolution).
//! - Qualified name resolution
//! - Private identifier resolution
//! - Type parameter resolution
//! - Library type resolution
//! - Namespace member resolution
//!
//! This module extends `CheckerState` with additional methods for symbol-related
//! operations, providing cleaner APIs for common patterns.

use crate::query_boundaries::type_predicates::is_compiler_managed_type;
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use crate::symbols_domain::name_text::entity_name_text_in_arena;
use std::sync::Arc;
use tracing::trace;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeSymbolResolution {
    Type(SymbolId),
    ValueOnly(SymbolId),
    NotFound,
}

/// `true` when the type-position resolution memo
/// (`CheckerContext::type_position_resolution_cache`) is disabled via the
/// `TSZ_DISABLE_TYPE_POSITION_RESOLUTION_CACHE` environment variable (any
/// non-empty value other than `0`).
///
/// The memo is speed-only — it caches the alias/global/namespace resolution of
/// a type-position identifier, which is a pure function of `(arena, node)` for a
/// fixed binder + lib context. Disabling it must therefore leave diagnostics
/// byte-identical; the kill switch exists so that equivalence can be proven in
/// CI / A-B runs (issue #13987).
fn type_position_resolution_cache_disabled() -> bool {
    use std::sync::OnceLock;
    static DISABLED: OnceLock<bool> = OnceLock::new();
    *DISABLED.get_or_init(|| {
        std::env::var("TSZ_DISABLE_TYPE_POSITION_RESOLUTION_CACHE")
            .is_ok_and(|v| !v.is_empty() && v != "0")
    })
}

// =============================================================================
// Symbol Resolution Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    pub(crate) fn resolve_enclosing_type_parameter_symbol(
        &self,
        idx: NodeIndex,
        name: &str,
    ) -> Option<SymbolId> {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = self.ctx.arena.parent_of(idx);
        // Track whether we've passed through a ComputedPropertyName. If so,
        // the enclosing class member's type parameters must be skipped because
        // computed property names are evaluated in the class scope, not the
        // method scope. In `[foo<T>(a)]<T>(a: T) {}`, `T` inside `[...]`
        // must NOT resolve to the method's own type parameter.
        let mut inside_computed_property_name = false;
        while let Some(parent_idx) = current {
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            if parent_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                inside_computed_property_name = true;
            }

            // Skip type parameters of class members when inside their computed property name
            let skip_type_params = inside_computed_property_name
                && matches!(
                    parent_node.kind,
                    k if k == syntax_kind_ext::METHOD_DECLARATION
                        || k == syntax_kind_ext::CONSTRUCTOR
                        || k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR
                );

            let type_params = if skip_type_params {
                // Clear the flag once we've skipped the class member
                inside_computed_property_name = false;
                None
            } else {
                self.ctx
                    .arena
                    .get_function(parent_node)
                    .and_then(|data| data.type_parameters.as_ref())
                    .or_else(|| {
                        self.ctx
                            .arena
                            .get_class(parent_node)
                            .and_then(|data| data.type_parameters.as_ref())
                    })
                    .or_else(|| {
                        self.ctx
                            .arena
                            .get_interface(parent_node)
                            .and_then(|data| data.type_parameters.as_ref())
                    })
                    .or_else(|| {
                        self.ctx
                            .arena
                            .get_type_alias(parent_node)
                            .and_then(|data| data.type_parameters.as_ref())
                    })
                    .or_else(|| {
                        self.ctx
                            .arena
                            .get_signature(parent_node)
                            .and_then(|data| data.type_parameters.as_ref())
                    })
                    .or_else(|| {
                        self.ctx
                            .arena
                            .get_method_decl(parent_node)
                            .and_then(|data| data.type_parameters.as_ref())
                    })
                    .or_else(|| {
                        self.ctx
                            .arena
                            .get_accessor(parent_node)
                            .and_then(|data| data.type_parameters.as_ref())
                    })
                    .or_else(|| {
                        self.ctx
                            .arena
                            .get_constructor(parent_node)
                            .and_then(|data| data.type_parameters.as_ref())
                    })
                    .or_else(|| {
                        self.ctx
                            .arena
                            .get_function_type(parent_node)
                            .and_then(|data| data.type_parameters.as_ref())
                    })
            };

            if let Some(type_params) = type_params {
                for &param_idx in &type_params.nodes {
                    let Some(param_node) = self.ctx.arena.get(param_idx) else {
                        continue;
                    };
                    let Some(param_data) = self.ctx.arena.get_type_parameter(param_node) else {
                        continue;
                    };
                    let Some(name_node) = self.ctx.arena.get(param_data.name) else {
                        continue;
                    };
                    let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                        continue;
                    };
                    if ident.escaped_text == name
                        && let Some(sym_id) = self.ctx.binder.get_node_symbol(param_idx)
                    {
                        return Some(sym_id);
                    }
                }
            }

            current = self
                .ctx
                .arena
                .get_extended(parent_idx)
                .map(|ext| ext.parent);
        }

        None
    }

    // =========================================================================
    // Symbol Type Resolution
    // =========================================================================

    // =========================================================================
    // Identifier Resolution
    // =========================================================================

    /// Collect lib binders from `lib_contexts` for cross-arena symbol lookup.
    /// This enables symbol resolution across lib.d.ts files when `lib_binders`
    /// is not populated in the binder (e.g., in the driver.rs path).
    ///
    /// Returns an `Arc`-wrapped vec for O(1) cloning. The `Arc<Vec<_>>` auto-derefs
    /// to `&[Arc<BinderState>]` so callers using `&lib_binders` work unchanged.
    pub(crate) fn get_lib_binders(&self) -> Arc<Vec<Arc<tsz_binder::BinderState>>> {
        // O(1) Arc::clone — the entire vec is shared, not individual elements.
        Arc::clone(&self.ctx.lib_binders_cached)
    }

    /// Check if a symbol represents a class member (property, method, accessor, or constructor).
    ///
    /// This filters out instance members that cannot be accessed as standalone values.
    /// However, static members and constructors should still be accessible.
    pub(crate) const fn is_class_member_symbol(flags: u32) -> bool {
        // Check if it's any kind of class member
        let is_member = (flags
            & (symbol_flags::PROPERTY
                | symbol_flags::METHOD
                | symbol_flags::GET_ACCESSOR
                | symbol_flags::SET_ACCESSOR
                | symbol_flags::CONSTRUCTOR))
            != 0;

        if !is_member {
            return false;
        }

        // Allow constructors - they represent the class itself
        if (flags & symbol_flags::CONSTRUCTOR) != 0 {
            return false;
        }

        // Allow static members - they're accessible via the class name
        if (flags & symbol_flags::STATIC) != 0 {
            return false;
        }

        // Filter out instance members (properties, methods, accessors without STATIC)
        true
    }

    /// Check if a symbol is a string-literal ambient module declaration
    /// (e.g., `declare module "foobar"`). These should not be accessible as bare
    /// identifiers — only namespace declarations with identifier names
    /// (e.g., `declare namespace Foo`) should resolve in expression context.
    fn is_string_literal_module_symbol(
        &self,
        sym_id: SymbolId,
        lib_binders: &[Arc<tsz_binder::BinderState>],
    ) -> bool {
        let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, lib_binders);
        let Some(symbol) = symbol else {
            return false;
        };
        // Only check symbols with MODULE flags
        if !symbol.has_any_flags(symbol_flags::MODULE) {
            return false;
        }
        // Check if ALL declarations are module declarations with string literal names
        if symbol.declarations.is_empty() {
            return false;
        }
        symbol.declarations.iter().all(|&decl_idx| {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                // Can't find node (possibly cross-file) — conservatively not a string module
                return false;
            };
            if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                return false;
            }
            let Some(module) = self.ctx.arena.get_module(node) else {
                return false;
            };
            // If the name node is a StringLiteral, this is a string-literal module
            self.ctx
                .arena
                .get(module.name)
                .is_some_and(|name_node| name_node.kind == SyntaxKind::StringLiteral as u16)
        })
    }

    /// Check if a symbol is an `import =` alias that can serve as the left-hand
    /// side of a qualified type name (e.g. `import b = require("m"); b.T`).
    ///
    /// These aliases are namespace-like anchors in qualified type positions even
    /// when the alias itself is not a type. Bare uses (`let x: b`) remain
    /// invalid; this only matters when the alias is followed by `.Member`.
    pub(crate) fn is_import_equals_type_anchor(
        &self,
        sym_id: SymbolId,
        lib_binders: &[Arc<tsz_binder::BinderState>],
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, lib_binders) else {
            return false;
        };
        if !symbol.has_any_flags(symbol_flags::ALIAS) {
            return false;
        }

        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .iter()
                .copied()
                .find(|idx| idx.is_some())
                .unwrap_or(NodeIndex::NONE)
        };

        decl_idx.is_some()
            && self.ctx.arena.get(decl_idx).is_some_and(|node| {
                // `import X = require(...)` or `import X = A.B.C`
                node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                    // `import * as X from "..."` — namespace import creates a
                    // namespace-like binding usable as a qualified type anchor
                    // (e.g., `X.SomeType`)
                    || node.kind == syntax_kind_ext::NAMESPACE_IMPORT
            })
    }

    fn enclosing_string_literal_module_augmentation_spec(&self, idx: NodeIndex) -> Option<String> {
        let mut current = idx;
        for _ in 0..128 {
            let ext = self.ctx.arena.get_extended(current)?;
            let parent = ext.parent;
            let parent_node = self.ctx.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::SOURCE_FILE {
                return None;
            }
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module_decl) = self.ctx.arena.get_module(parent_node)
                && self
                    .ctx
                    .arena
                    .has_modifier_ref(module_decl.modifiers.as_ref(), SyntaxKind::DeclareKeyword)
                && let Some(name_node) = self.ctx.arena.get(module_decl.name)
                && (name_node.kind == SyntaxKind::StringLiteral as u16
                    || name_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16)
                && let Some(literal) = self.ctx.arena.get_literal(name_node)
                && self
                    .ctx
                    .binder
                    .module_augmentations
                    .contains_key(&literal.text)
            {
                return Some(literal.text.clone());
            }
            current = parent;
        }
        None
    }

    fn module_augmentation_symbol_matches_spec(
        candidate_spec: &str,
        augmentation_spec: &str,
    ) -> bool {
        candidate_spec == augmentation_spec
            || crate::module_resolution::module_specifier_candidates(augmentation_spec)
                .iter()
                .any(|candidate| candidate == candidate_spec)
    }

    fn classify_type_position_symbol(&self, sym_id: SymbolId) -> TypeSymbolResolution {
        let lib_binders = self.get_lib_binders();
        let Some(symbol) = self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders))
        else {
            return TypeSymbolResolution::NotFound;
        };

        let is_namespace_or_module = symbol.has_any_flags(
            symbol_flags::MODULE | symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE,
        );
        let has_type = symbol.has_any_flags(symbol_flags::TYPE | symbol_flags::TYPE_ALIAS);
        let has_value = symbol.has_any_flags(symbol_flags::VALUE);

        if has_value && !has_type && !is_namespace_or_module {
            TypeSymbolResolution::ValueOnly(sym_id)
        } else {
            TypeSymbolResolution::Type(sym_id)
        }
    }

    fn resolve_module_augmentation_unqualified_type_symbol(
        &self,
        idx: NodeIndex,
        name: &str,
    ) -> Option<TypeSymbolResolution> {
        let module_spec = self.enclosing_string_literal_module_augmentation_spec(idx)?;

        for (&sym_id, candidate_spec) in self.ctx.binder.augmentation_target_modules.iter() {
            if !Self::module_augmentation_symbol_matches_spec(candidate_spec, &module_spec) {
                continue;
            }
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                continue;
            };
            if symbol.escaped_name != name {
                continue;
            }
            let result = self.classify_type_position_symbol(sym_id);
            if !matches!(result, TypeSymbolResolution::NotFound) {
                return Some(result);
            }
        }

        self.resolve_cross_file_export_from_file(
            &module_spec,
            name,
            Some(self.ctx.current_file_idx),
        )
        .map(|sym_id| self.classify_type_position_symbol(sym_id))
        .filter(|result| !matches!(result, TypeSymbolResolution::NotFound))
    }

    /// Resolve an identifier node to its symbol ID.
    ///
    /// This function walks the scope chain from the identifier's location upward,
    /// checking each scope's symbol table for the name. It also checks:
    /// - Module exports
    /// - Type parameter scope (for generic functions, classes, type aliases)
    /// - File locals (global scope from lib.d.ts)
    /// - Lib binders' `file_locals`
    ///
    /// Returns None if the identifier cannot be resolved to any symbol.
    pub(crate) fn resolve_identifier_symbol(&self, idx: NodeIndex) -> Option<SymbolId> {
        let result = self.resolve_identifier_symbol_inner(idx);
        if let Some(sym_id) = result {
            self.ctx.referenced_symbols.borrow_mut().insert(sym_id);
            trace!(sym_id = %sym_id.0, idx = %idx.0, "resolve_identifier_symbol: marked referenced");
        }
        result
    }

    /// Resolve an identifier without mutating unused-reference tracking.
    pub(crate) fn resolve_identifier_symbol_without_tracking(
        &self,
        idx: NodeIndex,
    ) -> Option<SymbolId> {
        self.resolve_identifier_symbol_inner(idx)
    }

    /// Resolve identifier for write context (assignment target).
    pub(crate) fn resolve_identifier_symbol_for_write(&self, idx: NodeIndex) -> Option<SymbolId> {
        let result = self.resolve_identifier_symbol_inner(idx);
        if let Some(sym_id) = result {
            self.ctx.written_symbols.borrow_mut().insert(sym_id);
        }
        result
    }

    fn resolve_identifier_symbol_inner(&self, idx: NodeIndex) -> Option<SymbolId> {
        if let Some(sym_id) = self.resolve_for_of_header_expression_symbol(idx) {
            return Some(sym_id);
        }

        let ignore_libs = !self.ctx.has_lib_loaded();
        let empty_binders: Arc<Vec<Arc<tsz_binder::BinderState>>> = Arc::new(Vec::new());
        let lib_binders = if ignore_libs {
            empty_binders
        } else {
            self.get_lib_binders()
        };
        let in_decorator_expr = self.is_in_decorator_expression(idx);
        let decorator_owner = in_decorator_expr
            .then(|| self.decorator_owner_declaration(idx))
            .flatten();
        let enclosing_decorator = in_decorator_expr
            .then(|| self.nearest_decorator_ancestor(idx))
            .flatten();
        let is_from_lib = |sym_id: SymbolId| self.ctx.symbol_is_from_lib(sym_id);
        let should_skip_lib_symbol = |sym_id: SymbolId| ignore_libs && is_from_lib(sym_id);

        // PERF: ident_name is only used by trace! calls which are compiled out
        // in release builds (release_max_level_warn). The to_string() allocation
        // is eliminated by the compiler since ident_name becomes dead code.
        let ident_name = self
            .ctx
            .arena
            .get_identifier_at(idx)
            .map(|i| i.escaped_text.as_str().to_string());

        trace!(
            ident_name = ?ident_name,
            idx = ?idx,
            ignore_libs = ignore_libs,
            "Resolving identifier symbol"
        );

        // First try the binder's resolver which checks scope chain and file_locals
        let identifier_is_type_position_for_first = self.is_identifier_in_type_position(idx);
        let result = self.ctx.binder.resolve_identifier_with_filter(
            self.ctx.arena,
            idx,
            &lib_binders,
            |sym_id| {
                if should_skip_lib_symbol(sym_id) {
                    return false;
                }
                if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) {
                    if let Some(owner_idx) = decorator_owner
                        && symbol.declarations.iter().any(|&decl_idx| {
                            // Filter decls nested in the owner, except the owner itself and `enclosing_decorator`.
                            decl_idx != owner_idx
                                && self.node_is_within_decorator_owner(decl_idx, owner_idx)
                                && !enclosing_decorator.is_some_and(|dec_idx| {
                                    self.node_is_within_decorator_owner(decl_idx, dec_idx)
                                })
                        })
                    {
                        return false;
                    }
                    // Reject cross-module values that the consuming file
                    // never imports — bare-identifier resolution must not
                    // pick up another module's exports as if they were
                    // globals (#3504). Class-member symbols continue to
                    // be filtered by the dedicated class-member branch
                    // below; type-position references are handled by the
                    // post-resolution filter further down.
                    let is_cross_module_private = !self.ctx.symbol_is_from_lib(sym_id)
                        && !symbol.is_umd_export
                        && symbol.decl_file_idx != u32::MAX
                        && symbol.decl_file_idx != self.ctx.current_file_idx as u32
                        && self
                            .ctx
                            .get_binder_for_file(symbol.decl_file_idx as usize)
                            .is_some_and(|binder| {
                                binder.is_external_module()
                                    && !binder
                                        .global_augmentations
                                        .contains_key(symbol.escaped_name.as_str())
                            })
                        && !self
                            .ctx
                            .get_arena_for_file(symbol.decl_file_idx)
                            .source_files
                            .first()
                            .is_some_and(|sf| sf.is_declaration_file);
                    if !identifier_is_type_position_for_first
                        && symbol.has_any_flags(symbol_flags::VALUE)
                        && is_cross_module_private
                        && !Self::is_class_member_symbol(symbol.flags)
                    {
                        return false;
                    }
                    let is_class_member = Self::is_class_member_symbol(symbol.flags);
                    if is_class_member {
                        if in_decorator_expr {
                            return false;
                        }
                        return is_from_lib(sym_id)
                            && symbol.has_any_flags(symbol_flags::EXPORT_VALUE);
                    }
                }
                true
            },
        );
        let result = {
            let expected_name = self
                .ctx
                .arena
                .get_identifier_at(idx)
                .map(|ident| ident.escaped_text.as_str());
            result.filter(|&sym_id| {
                let Some(expected_name) = expected_name else {
                    return false;
                };

                self.ctx
                    .binder
                    .get_symbol_with_libs(sym_id, &lib_binders)
                    .is_some_and(|symbol| symbol.escaped_name.as_str() == expected_name)
            })
        };

        // Filter out string-literal ambient module declarations (e.g. `declare module "foobar"`)
        // These should not resolve as bare identifiers — they are only reachable via import.
        let result =
            result.filter(|&sym_id| !self.is_string_literal_module_symbol(sym_id, &lib_binders));
        let identifier_is_type_position = self.is_identifier_in_type_position(idx);
        let result = result.filter(|&sym_id| {
            if !identifier_is_type_position {
                return true;
            }
            let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
                return true;
            };
            if !self.ctx.binder.is_external_module()
                || self.is_in_declare_namespace_or_module(idx)
                || self.ctx.symbol_is_from_lib(sym_id)
                || symbol.is_umd_export
                || symbol.decl_file_idx == u32::MAX
                || symbol.decl_file_idx == self.ctx.current_file_idx as u32
                || symbol.has_any_flags(symbol_flags::VALUE)
            {
                return true;
            }
            let Some(owner_binder) = self.ctx.get_binder_for_file(symbol.decl_file_idx as usize)
            else {
                return true;
            };
            let owner_is_declaration_file = self
                .ctx
                .get_arena_for_file(symbol.decl_file_idx)
                .source_files
                .first()
                .is_some_and(|sf| sf.is_declaration_file);
            owner_is_declaration_file
                || !owner_binder.is_external_module()
                || owner_binder
                    .global_augmentations
                    .contains_key(symbol.escaped_name.as_str())
        });

        trace!(
            ident_name = ?ident_name,
            binder_result = ?result,
            "Binder resolution result"
        );

        // IMPORTANT: If the binder didn't find the symbol, check lib_contexts directly as a fallback.
        //
        // # Why this fallback exists (and why it is NOT a bug)
        //
        // The binder's `resolve_identifier_with_filter` deliberately gates its
        // `lib_binders` traversal on `!self.lib_symbols_merged`. After
        // `merge_lib_contexts_into_binder` runs, the main binder's `file_locals`
        // is supposed to carry every globally-visible lib symbol — so the
        // binder skips re-walking `lib_binders` to avoid re-introducing the
        // symbols it just merged.
        //
        // Phase 3 of the merge intentionally EXCLUDES file_locals belonging to
        // external-module lib files unless the name appears in the lib's
        // `global_augmentations` map (`crates/tsz-binder/src/state/lib_merge.rs`,
        // around the `is_external_module && !global_augmentations.contains_key`
        // check). This prevents module-scoped names like the `class Iterator`
        // in `es2025.iterator.d.ts` from contaminating the global scope of
        // user code that doesn't explicitly augment.
        //
        // BUT: some lookups DO need access to those module-scoped lib symbols
        // (e.g. when generators.rs walks the iterator chain). The fallback
        // below queries `lib_contexts.file_locals` directly so those callers
        // can find the symbol. `should_skip_lib_symbol` filters the candidates
        // to keep the global pollution boundary intact.
        //
        // Robustness audit (PR #B, item 2 in
        // `docs/architecture/ROBUSTNESS_AUDIT_2026-04-26.md`): the audit's
        // initial framing ("the binder has a bug") was misleading — the
        // skip-after-merge is deliberate, and the divergence is a coordinated
        // policy. A future restructure should hoist the merge-phase filter and
        // the checker-side fallback into a single declarative resolver
        // boundary so the policy is co-located.
        if result.is_none() && !ignore_libs {
            // Get the identifier name
            let name = self.ctx.arena.get_identifier_at(idx)?.escaped_text.as_str();
            // Check lib_contexts directly for global symbols. Skip the whole
            // scan when the prebuilt index proves no lib declares `name`: the
            // loop body keys every probe on `file_locals.get(name)`, so an
            // absent name makes it a guaranteed no-op (byte-identical skip).
            if self.ctx.lib_name_possible(name) {
                for (lib_idx, lib_ctx) in self.ctx.lib_contexts.iter().enumerate() {
                    if let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name) {
                        trace!(
                            name = name,
                            lib_idx = lib_idx,
                            lib_sym_id = ?lib_sym_id,
                            "Found symbol in lib_context"
                        );
                        if !should_skip_lib_symbol(lib_sym_id) {
                            // Use file binder's sym_id for correct ID space after lib merge.
                            // Never return lib-context SymbolIds directly: they may collide with
                            // unrelated symbols in the current binder ID space.
                            //
                            // Cross-file lookup binders (cross-arena delegation) keep
                            // `file_locals` per-file and carry the hoisted lib-origin
                            // globals separately in `program_globals` (see
                            // `create_cross_file_lookup_binder_with_augmentations`).
                            // Consult it as the same program-ID-space mapping; otherwise a
                            // lib global (e.g. `Symbol` in a computed member name) silently
                            // fails to resolve during delegation and derived types depend
                            // on file check order. `program_globals` is empty on primary
                            // (lib-merged) binders, so this changes nothing there.
                            let Some(file_sym_id) = self
                                .ctx
                                .binder
                                .file_locals
                                .get(name)
                                .or_else(|| self.ctx.binder.program_globals.get(name))
                            else {
                                continue;
                            };
                            // Filter out string-literal ambient module symbols (e.g., `declare module "foobar"`)
                            // — they should not resolve as bare identifiers.
                            if self.is_string_literal_module_symbol(file_sym_id, &lib_binders) {
                                continue;
                            }
                            trace!(
                                name = name,
                                file_sym_id = ?file_sym_id,
                                lib_sym_id = ?lib_sym_id,
                                "Returning symbol from lib_contexts fallback"
                            );
                            return Some(file_sym_id);
                        }
                    }
                }
            }
        }

        trace!(
            ident_name = ?ident_name,
            final_result = ?result,
            "Symbol resolution final result"
        );

        if let Some(ident) = self.ctx.arena.get_identifier_at(idx)
            && let Some(found_sym_id) = result
            && self.ctx.binder.file_locals.get(ident.escaped_text.as_str()) == Some(found_sym_id)
            && let Some(ns_sym_id) = self
                .resolve_unqualified_name_in_enclosing_namespace(idx, ident.escaped_text.as_str())
            && ns_sym_id != found_sym_id
        {
            return Some(ns_sym_id);
        }

        if let Some(ident) = self.ctx.arena.get_identifier_at(idx)
            && result.is_none()
        {
            let name = ident.escaped_text.as_str();
            // These identifiers have intrinsic fallback semantics when unbound.
            // A same-file declaration may shadow them, but an export from another
            // external module must not become a bare lexical binding here.
            if matches!(name, "undefined" | "NaN" | "Infinity") {
                return None;
            }
            if let Some(sym_id) =
                self.resolve_identifier_symbol_from_all_binders(name, |sym_id, symbol| {
                    if should_skip_lib_symbol(sym_id) {
                        return false;
                    }
                    // A symbol declared in another external module is
                    // only reachable via explicit import. Reject cross-file
                    // fallback resolutions to such symbols, except where:
                    //  * the owning file is a declaration/script/global
                    //    augmentation source (legitimate global), or
                    //  * the symbol is exported from its module (downstream
                    //    diagnostic paths such as the class initializer
                    //    TS2663 detector rely on resolving these here so
                    //    they can emit a more specific diagnostic).
                    let is_cross_module_private = !self.ctx.symbol_is_from_lib(sym_id)
                        && !symbol.is_umd_export
                        && symbol.decl_file_idx != u32::MAX
                        && symbol.decl_file_idx != self.ctx.current_file_idx as u32
                        && self
                            .ctx
                            .get_binder_for_file(symbol.decl_file_idx as usize)
                            .is_some_and(|binder| {
                                binder.is_external_module()
                                    && !binder.global_augmentations.contains_key(name)
                            })
                        && !self
                            .ctx
                            .get_arena_for_file(symbol.decl_file_idx)
                            .source_files
                            .first()
                            .is_some_and(|sf| sf.is_declaration_file);
                    let is_private_external_module_type = identifier_is_type_position
                        && !symbol.has_any_flags(symbol_flags::VALUE)
                        && is_cross_module_private;
                    // Reject cross-module values that the consuming file
                    // never imports — exported or not. tsc emits TS2304
                    // for `leaked.toFixed()` in `b.ts` when `b.ts` does
                    // not import `leaked` from `a.ts`, regardless of
                    // whether `a.ts` exports `leaked` (#3504). The class
                    // member fallthrough below preserves the
                    // class-instance-member detector path (TS2663
                    // "Did you mean 'this.X'?") for inherited fields,
                    // because that branch resolves through the class
                    // hierarchy rather than this raw cross-file lookup.
                    let is_private_external_module_value = !identifier_is_type_position
                        && symbol.has_any_flags(symbol_flags::VALUE)
                        && is_cross_module_private;
                    if is_private_external_module_type || is_private_external_module_value {
                        return false;
                    }
                    // NOTE: We intentionally skip the decorator_owner check here.
                    // Cross-file symbols have NodeIndex values from different arenas,
                    // so `node_is_within_decorator_owner` would walk parent pointers
                    // in the wrong arena, causing false positives when indices
                    // coincidentally overlap with nodes inside the class declaration.
                    // Cross-file symbols can never be inside the current file's
                    // decorator owner, so this filter is unnecessary.

                    let is_class_member = Self::is_class_member_symbol(symbol.flags);
                    if is_class_member {
                        if in_decorator_expr {
                            return false;
                        }
                        return is_from_lib(sym_id)
                            && symbol.has_any_flags(symbol_flags::EXPORT_VALUE);
                    }
                    true
                })
            {
                // Filter out string-literal ambient module symbols (e.g., `declare module "foobar"`)
                // — they should not resolve as bare identifiers.
                if !self.is_string_literal_module_symbol(sym_id, &lib_binders) {
                    return Some(sym_id);
                }
            }

            // Cross-file namespace body fallback: if we're inside a namespace body
            // and the name wasn't found, check the merged namespace symbol's exports.
            // This handles e.g. `Point` in part2.ts referring to `Point` exported from
            // part1.ts's `namespace A`.
            if let Some(sym_id) = self.resolve_unqualified_name_in_enclosing_namespace(idx, name) {
                return Some(sym_id);
            }
        }

        trace!(
            ident_name = ?ident_name,
            final_result = ?result,
            "Symbol resolution final result"
        );

        if let Some(sym_id) = result
            && let Some(sym) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
        {
            trace!(
                ident_name = ?ident_name,
                sym_id = sym_id.0,
                sym_name = sym.escaped_name.as_str(),
                sym_flags = sym.flags,
                "Symbol resolution resolved metadata"
            );
        }
        result
    }

    /// Resolve an identifier symbol for type positions, skipping value-only symbols.
    pub(crate) fn resolve_identifier_symbol_in_type_position(
        &self,
        idx: NodeIndex,
    ) -> TypeSymbolResolution {
        let result = self.resolve_identifier_symbol_in_type_position_inner(idx);
        if let TypeSymbolResolution::Type(sym_id) = result {
            self.ctx.referenced_symbols.borrow_mut().insert(sym_id);
        }
        result
    }

    /// Resolve a type-position identifier without mutating unused-reference tracking.
    pub(crate) fn resolve_identifier_symbol_in_type_position_without_tracking(
        &self,
        idx: NodeIndex,
    ) -> TypeSymbolResolution {
        self.resolve_identifier_symbol_in_type_position_inner(idx)
    }

    /// Resolve an identifier when it appears as the left-hand side of a
    /// qualified type name (e.g. `Alias.Member`).
    ///
    /// This is slightly broader than ordinary type-position lookup because
    /// `import =` aliases act as namespace-like anchors even when the alias
    /// itself is value-only as a bare type.
    pub(crate) fn resolve_identifier_symbol_as_qualified_type_anchor(
        &self,
        idx: NodeIndex,
    ) -> Option<SymbolId> {
        let lib_binders = self.get_lib_binders();
        let name = self
            .ctx
            .arena
            .get_identifier_at(idx)
            .map(|ident| ident.escaped_text.as_str());
        match self.resolve_identifier_symbol_in_type_position(idx) {
            TypeSymbolResolution::Type(sym_id) => {
                if let Some(name) = name
                    && let Some(local_namespace_sym_id) = self
                        .ctx
                        .local_namespace_symbol_for_conflicted_namespace_import(
                            idx,
                            name,
                            sym_id,
                            &lib_binders,
                        )
                {
                    return Some(local_namespace_sym_id);
                }
                // A qualified-type-name LHS resolves with namespace meaning: when
                // a local `type`/`interface` shadows a same-named import alias
                // whose target is a namespace, anchor on the import.
                let anchor_src = self
                    .namespace_anchor_alias_partner(sym_id, &lib_binders)
                    .unwrap_or(sym_id);
                let mut visited_aliases = AliasCycleTracker::new();
                Some(self.qualified_type_anchor_symbol(anchor_src, &mut visited_aliases))
            }
            TypeSymbolResolution::ValueOnly(sym_id)
                if self.is_import_equals_type_anchor(sym_id, &lib_binders) =>
            {
                if let Some(name) = name
                    && let Some(local_namespace_sym_id) = self
                        .ctx
                        .local_namespace_symbol_for_conflicted_namespace_import(
                            idx,
                            name,
                            sym_id,
                            &lib_binders,
                        )
                {
                    return Some(local_namespace_sym_id);
                }
                self.ctx.referenced_symbols.borrow_mut().insert(sym_id);
                Some(sym_id)
            }
            TypeSymbolResolution::ValueOnly(_) | TypeSymbolResolution::NotFound => None,
        }
    }

    /// Resolve a type-position identifier, memoizing the context-free portion.
    ///
    /// The enclosing-type-parameter fast path is context-sensitive — the same
    /// lexical node binds to different type-parameter symbols across
    /// instantiation / return contexts (the
    /// `return_context_type_param_shadowing_tests` shape) — so it is resolved on
    /// every call and never cached. Everything past it (alias / global /
    /// namespace / module-augmentation resolution, the dominant scope-walk +
    /// by-name `HashMap` cost) is a pure function of `(arena, node)` for a fixed
    /// binder + lib context, so it is memoized under the same `(arena pointer,
    /// node index)` key the binder's own `resolve_identifier` cache uses. This
    /// collapses the per-recursion-level re-resolution that dominated recursive
    /// type evaluation (issue #13987).
    fn resolve_identifier_symbol_in_type_position_inner(
        &self,
        idx: NodeIndex,
    ) -> TypeSymbolResolution {
        // Context-sensitive fast path: an enclosing type parameter binds by
        // symbol, and the same lexical node can bind to different symbols across
        // instantiation / return contexts, so this is never cached. Reaching the
        // cache below therefore implies this node has *no* matching enclosing
        // type parameter — which is what makes the memoized resolution
        // independent of the dynamic type-parameter scope (and so sound),
        // regardless of how `_uncached` arrives at its answer.
        if let Some(node) = self.ctx.arena.get(idx)
            && let Some(ident) = self.ctx.arena.get_identifier(node)
            && let Some(sym_id) =
                self.resolve_enclosing_type_parameter_symbol(idx, ident.escaped_text.as_str())
        {
            return TypeSymbolResolution::Type(sym_id);
        }

        if type_position_resolution_cache_disabled() {
            return self.resolve_identifier_symbol_in_type_position_uncached(idx);
        }

        // Borrow in two phases (read-then-drop, compute, write) rather than an
        // `entry(..).or_insert_with(..)`: `_uncached` recursively resolves the
        // alias body's nested identifiers, re-entering this same `RefCell`, so
        // holding a borrow across the computation would panic.
        let key = (std::ptr::from_ref(self.ctx.arena) as usize, idx.0);
        if let Some(cached) = self
            .ctx
            .type_position_resolution_cache
            .borrow()
            .get(&key)
            .copied()
        {
            return cached;
        }
        let result = self.resolve_identifier_symbol_in_type_position_uncached(idx);
        self.ctx
            .type_position_resolution_cache
            .borrow_mut()
            .insert(key, result);
        result
    }

    /// The uncached body of [`Self::resolve_identifier_symbol_in_type_position_inner`].
    ///
    /// Only ever reached for identifiers that are *not* an enclosing type
    /// parameter (that fast path is handled, uncached, by the caller). The
    /// result is a pure function of `(arena, node)` for a fixed binder + lib
    /// context, which is what makes the caller's memo sound.
    fn resolve_identifier_symbol_in_type_position_uncached(
        &self,
        idx: NodeIndex,
    ) -> TypeSymbolResolution {
        let node = match self.ctx.arena.get(idx) {
            Some(node) => node,
            None => return TypeSymbolResolution::NotFound,
        };
        let ident = match self.ctx.arena.get_identifier(node) {
            Some(ident) => ident,
            None => return TypeSymbolResolution::NotFound,
        };
        let name = ident.escaped_text.as_str();

        if let Some(sym_id) =
            self.resolve_unqualified_name_in_enclosing_namespace_for_type_position(idx, name)
        {
            return TypeSymbolResolution::Type(sym_id);
        }

        if let Some(result) = self.resolve_module_augmentation_unqualified_type_symbol(idx, name) {
            return result;
        }

        let ignore_libs = !self.ctx.has_lib_loaded();
        // Collect lib binders for cross-arena symbol lookup
        let lib_binders = if ignore_libs {
            Arc::new(Vec::new())
        } else {
            self.get_lib_binders()
        };
        let should_skip_lib_symbol =
            |sym_id: SymbolId| ignore_libs && self.ctx.symbol_is_from_lib(sym_id);
        let value_only_candidate = std::cell::Cell::new(None::<SymbolId>);

        // Check if this name exists in a local scope (namespace/module) that would shadow
        // the global lib symbol. If so, we skip the early lib_contexts check and let the
        // binder's scope-based resolution find the local symbol first.
        // PERF: Use the cached resolve_identifier (which caches results per (arena, node_idx))
        // instead of resolve_identifier_with_filter which is uncached.
        let name_in_local_scope = if !ignore_libs {
            let scoped_type_shadow = self.ctx.binder.resolve_identifier_with_filter(
                self.ctx.arena,
                idx,
                &[],
                |candidate| {
                    let Some(symbol) = self.ctx.binder.get_symbol(candidate) else {
                        return false;
                    };
                    if symbol.escaped_name.as_str() != name {
                        return false;
                    }
                    let file_local = self.ctx.binder.file_locals.get(name) == Some(candidate);
                    let lib_like_file_local = file_local
                        && !symbol.has_any_flags(symbol_flags::ALIAS)
                        && (self.ctx.symbol_is_from_lib(candidate)
                            || symbol.decl_file_idx == u32::MAX);
                    if lib_like_file_local {
                        return false;
                    }
                    symbol.has_any_flags(
                        symbol_flags::TYPE
                            | symbol_flags::ALIAS
                            | symbol_flags::REGULAR_ENUM
                            | symbol_flags::CONST_ENUM
                            | symbol_flags::NAMESPACE_MODULE
                            | symbol_flags::VALUE_MODULE,
                    )
                },
            );
            scoped_type_shadow.is_some()
                || self
                    .ctx
                    .binder
                    .resolve_identifier(self.ctx.arena, idx)
                    .is_some_and(|found_sym_id| {
                        // Check if this symbol is different from the file_locals symbol.
                        // If it's different, it was found in a more local scope (namespace, etc.)
                        let found_in_file_locals =
                            self.ctx.binder.file_locals.get(name) == Some(found_sym_id);
                        !found_in_file_locals
                            || (self.ctx.binder.is_external_module()
                                && !self.ctx.symbol_is_from_lib(found_sym_id))
                    })
        } else {
            false
        };

        // IMPORTANT: Check lib_contexts directly BEFORE calling binder's resolve_identifier_with_filter.
        //
        // The binder's `resolve_identifier_with_filter` skips `lib_binders` when
        // `self.lib_symbols_merged == true`. The skip is deliberate (see the
        // long comment at `resolve_identifier_symbol` above for why), but the
        // merge phase intentionally excludes external-module lib file_locals
        // unless the name is in `global_augmentations`. For type-position
        // resolution we still need access to those module-scoped lib symbols
        // (e.g. lib types referenced from user augmentations), so we probe
        // `lib_contexts.file_locals` directly here.
        //
        // The `name_in_local_scope` short-circuit ensures local declarations
        // (namespaces, modules) shadow global lib types — without it, an
        // ambient `class Iterator` in a target lib would mask a user-defined
        // namespace-local `Iterator`.
        //
        // Robustness audit (PR #B, item 2): see the matching comment at
        // `resolve_identifier_symbol`. This is the type-position twin of
        // that bypass.
        if !ignore_libs && !name_in_local_scope && self.ctx.lib_name_possible(name) {
            for lib_ctx in self.ctx.lib_contexts.iter() {
                if let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name) {
                    // After lib merge, the file binder has the same symbols with
                    // potentially different IDs. Use file binder's ID for returns,
                    // and skip symbols not present in current binder ID space.
                    //
                    // Cross-file lookup binders (cross-arena delegation) keep
                    // `file_locals` per-file and carry the hoisted lib-origin globals
                    // separately in `program_globals`; consult it as the same
                    // program-ID-space mapping so type-position lib globals resolve
                    // independently of file check order (twin of the value-position
                    // fallback in `resolve_identifier_symbol`).
                    let Some(sym_id) = self
                        .ctx
                        .binder
                        .file_locals
                        .get(name)
                        .or_else(|| self.ctx.binder.program_globals.get(name))
                    else {
                        continue;
                    };
                    if !should_skip_lib_symbol(sym_id) {
                        // Check flags using lib binder (lib_sym_id is valid in lib binder)
                        let flags = lib_ctx.binder.get_symbol(lib_sym_id).map_or(0, |s| s.flags);

                        // Namespaces and modules are value-only but should be allowed in type position
                        let is_namespace_or_module = (flags
                            & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
                            != 0;

                        if is_namespace_or_module {
                            return TypeSymbolResolution::Type(sym_id);
                        }

                        // For ALIAS symbols, resolve to the target
                        if flags & symbol_flags::ALIAS != 0 {
                            let mut visited = AliasCycleTracker::new();
                            if let Some(target_sym_id) =
                                self.resolve_alias_symbol(sym_id, &mut visited)
                            {
                                // Check the target symbol's flags
                                let target_flags = self
                                    .ctx
                                    .binder
                                    .get_symbol_with_libs(target_sym_id, &lib_binders)
                                    .map_or(0, |s| s.flags);
                                if (target_flags
                                    & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
                                    != 0
                                {
                                    return TypeSymbolResolution::Type(target_sym_id);
                                }
                            }
                        }

                        // Check if this is a value-only symbol
                        let is_value_only = (self.alias_resolves_to_value_only(sym_id, None)
                            || self.symbol_is_value_only(sym_id, None))
                            && !self.symbol_is_type_only(sym_id, None);
                        if is_value_only {
                            if value_only_candidate.get().is_none() {
                                value_only_candidate.set(Some(sym_id));
                            }
                        } else {
                            // Valid type symbol found in lib
                            return TypeSymbolResolution::Type(sym_id);
                        }
                    }
                }
            }
        }

        let accept_type_symbol = |sym_id: SymbolId| -> bool {
            // Get symbol flags to check for special cases
            let flags = self
                .ctx
                .binder
                .get_symbol_with_libs(sym_id, &lib_binders)
                .map_or(0, |s| s.flags);

            // Namespaces and modules are value-only but should be allowed in type position
            // because they can contain types (e.g., MyNamespace.ValueInterface)
            let is_namespace_or_module =
                (flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)) != 0;

            if is_namespace_or_module {
                return true;
            }

            // For ALIAS symbols (import equals declarations), resolve to the target
            // and check if it's a namespace/module
            if flags & symbol_flags::ALIAS != 0 {
                let mut visited = AliasCycleTracker::new();
                if let Some(target_sym_id) = self.resolve_alias_symbol(sym_id, &mut visited) {
                    let target_flags = self
                        .ctx
                        .binder
                        .get_symbol_with_libs(target_sym_id, &lib_binders)
                        .map_or(0, |s| s.flags);
                    if (target_flags
                        & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
                        != 0
                    {
                        return true;
                    }
                }
            }

            // When a symbol is merged from an import alias and a local value declaration
            // (e.g., `import { FC } from "./types"; let FC: FC | null = null;`),
            // the type meaning comes from the alias chain. If the alias resolves to a
            // type (not value-only), accept the symbol in type position.
            let alias_is_type = (flags & symbol_flags::ALIAS) != 0
                && !self.alias_resolves_to_value_only(sym_id, None);
            if alias_is_type && (flags & symbol_flags::VALUE) != 0 {
                return true;
            }

            let is_value_only = (self.alias_resolves_to_value_only(sym_id, None)
                || self.symbol_is_value_only(sym_id, None))
                && !self.symbol_is_type_only(sym_id, None);
            if is_value_only {
                if value_only_candidate.get().is_none() {
                    value_only_candidate.set(Some(sym_id));
                }
                return false;
            }
            true
        };

        let should_preserve_alias_symbol_in_type_position = |sym_id: SymbolId| {
            let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
                return false;
            };
            if !symbol.has_any_flags(symbol_flags::ALIAS) {
                return false;
            }

            let has_local_type_meaning = self.symbol_has_declared_type_meaning(sym_id);
            let is_namespace_import_alias =
                symbol.import_module().is_some() && matches!(symbol.import_name(), Some("*"));

            has_local_type_meaning || is_namespace_import_alias
        };

        let is_private_external_module_type_symbol = |sym_id: SymbolId| -> bool {
            let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
                return false;
            };
            if !self.ctx.binder.is_external_module()
                || self.is_in_declare_namespace_or_module(idx)
                || self.ctx.symbol_is_from_lib(sym_id)
                || symbol.is_umd_export
                || symbol.decl_file_idx == u32::MAX
                || symbol.decl_file_idx == self.ctx.current_file_idx as u32
                || symbol.has_any_flags(symbol_flags::VALUE)
            {
                return false;
            }
            let Some(owner_binder) = self.ctx.get_binder_for_file(symbol.decl_file_idx as usize)
            else {
                return false;
            };
            let owner_is_declaration_file = self
                .ctx
                .get_arena_for_file(symbol.decl_file_idx)
                .source_files
                .first()
                .is_some_and(|sf| sf.is_declaration_file);
            if owner_is_declaration_file {
                return false;
            }
            owner_binder.is_external_module()
                && !owner_binder.global_augmentations.contains_key(name)
        };
        if let Some(local_sym_id) =
            self.ctx
                .binder
                .resolve_identifier_with_filter(self.ctx.arena, idx, &[], |sym_id| {
                    if self.ctx.symbol_is_from_lib(sym_id) {
                        return false;
                    }
                    if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                        let is_class_member = Self::is_class_member_symbol(symbol.flags);
                        if is_class_member {
                            return false;
                        }
                    }
                    accept_type_symbol(sym_id)
                })
            && !is_private_external_module_type_symbol(local_sym_id)
            && self
                .ctx
                .binder
                .get_symbol(local_sym_id)
                .is_some_and(|symbol| symbol.escaped_name.as_str() == name)
        {
            if let Some(symbol) = self.ctx.binder.get_symbol(local_sym_id)
                && symbol.has_any_flags(symbol_flags::ALIAS)
            {
                if let Some(local_namespace_sym_id) = self
                    .ctx
                    .local_namespace_symbol_for_conflicted_namespace_import(
                        idx,
                        name,
                        local_sym_id,
                        &lib_binders,
                    )
                {
                    return TypeSymbolResolution::Type(local_namespace_sym_id);
                }
                if let Some(type_alias_id) = self
                    .ctx
                    .alias_partner_reverse(self.ctx.binder, local_sym_id)
                {
                    return TypeSymbolResolution::Type(type_alias_id);
                }
                if should_preserve_alias_symbol_in_type_position(local_sym_id) {
                    return TypeSymbolResolution::Type(local_sym_id);
                }
                self.ctx
                    .referenced_symbols
                    .borrow_mut()
                    .insert(local_sym_id);
                if let Some(resolved) =
                    self.resolve_alias_type_position_result(local_sym_id, &lib_binders)
                {
                    return resolved;
                }
            }
            if self.ctx.binder.file_locals.get(name) == Some(local_sym_id)
                && let Some(ns_sym_id) = self
                    .resolve_unqualified_name_in_enclosing_namespace_for_type_position(idx, name)
                && ns_sym_id != local_sym_id
            {
                return TypeSymbolResolution::Type(ns_sym_id);
            }
            return TypeSymbolResolution::Type(local_sym_id);
        }

        if let Some(sym_id) =
            self.resolve_unqualified_name_in_enclosing_namespace_for_type_position(idx, name)
        {
            return TypeSymbolResolution::Type(sym_id);
        }

        let resolved = self
            .ctx
            .binder
            .resolve_identifier_with_filter(self.ctx.arena, idx, &lib_binders, |sym_id| {
                if should_skip_lib_symbol(sym_id) {
                    return false;
                }
                if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) {
                    let is_class_member = Self::is_class_member_symbol(symbol.flags);
                    if is_class_member {
                        return false;
                    }
                }
                accept_type_symbol(sym_id)
            })
            .filter(|&sym_id| !is_private_external_module_type_symbol(sym_id));
        let has_value_only = value_only_candidate.get().is_some();
        if resolved.is_none()
            && !has_value_only
            && let Some(sym_id) =
                self.resolve_identifier_symbol_from_all_binders(name, |sym_id, symbol| {
                    if should_skip_lib_symbol(sym_id) {
                        return false;
                    }
                    if is_private_external_module_type_symbol(sym_id) {
                        return false;
                    }

                    let is_class_member = Self::is_class_member_symbol(symbol.flags);
                    if is_class_member {
                        return false;
                    }
                    accept_type_symbol(sym_id)
                })
        {
            let is_value_only = (self.alias_resolves_to_value_only(sym_id, None)
                || self.symbol_is_value_only(sym_id, None))
                && !self.symbol_is_type_only(sym_id, None);
            if is_value_only {
                return TypeSymbolResolution::ValueOnly(sym_id);
            }
            return TypeSymbolResolution::Type(sym_id);
        }

        // Guard against SymbolId renumbering from lib merging: if the resolved
        // symbol's name doesn't match the requested name, the scope table has a
        // stale SymbolId. Reject it and fall through to value_only_candidate.
        let resolved = resolved.filter(|&sym_id| {
            self.ctx
                .binder
                .get_symbol_with_libs(sym_id, &lib_binders)
                .is_some_and(|s| s.escaped_name.as_str() == name)
        });
        if let Some(sym_id) = resolved {
            if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
                && symbol.has_any_flags(symbol_flags::ALIAS)
            {
                if let Some(local_namespace_sym_id) = self
                    .ctx
                    .local_namespace_symbol_for_conflicted_namespace_import(
                        idx,
                        name,
                        sym_id,
                        &lib_binders,
                    )
                {
                    return TypeSymbolResolution::Type(local_namespace_sym_id);
                }
                if should_preserve_alias_symbol_in_type_position(sym_id) {
                    return TypeSymbolResolution::Type(sym_id);
                }
                // Mark the local alias as referenced (for unused-import tracking).
                // When we follow the alias chain below, only the target gets returned
                // and inserted into referenced_symbols by the caller. Without this,
                // imports used only in type positions appear unused (false TS6133).
                self.ctx.referenced_symbols.borrow_mut().insert(sym_id);
                if let Some(resolved) =
                    self.resolve_alias_type_position_result(sym_id, &lib_binders)
                {
                    return resolved;
                }
            }
            return TypeSymbolResolution::Type(sym_id);
        }

        if let Some(value_only) = value_only_candidate.get() {
            // A VALUE-only local does not occupy the type namespace; fall back
            // to the lib TYPE symbol recorded during merge.
            if !ignore_libs
                && let Some(&lib_type_sym_id) = self.ctx.binder.lib_type_namespace.get(name)
            {
                return TypeSymbolResolution::Type(lib_type_sym_id);
            }
            return TypeSymbolResolution::ValueOnly(value_only);
        }

        // Last-resort fallback for `import X = require(...)` namespace
        // anchors in qualified-name type position.
        //
        // When this identifier is the left qualifier of a qualified name
        // (e.g. `server.IServer` where `server` comes from
        // `import server = require('./server')`), upstream filters can
        // reject the alias because cross-arena resolution intermittently
        // loses track of the import-equals target's module flags.  The
        // binder maps this node's identifier to a stable symbol via
        // `get_node_symbol`; fall back to that mapping only when the
        // resolved symbol is an IMPORT_EQUALS_DECLARATION (not a general
        // namespace import, which has its own value/type distinction that
        // must not be bypassed).
        if let Some(sym_id) = self
            .ctx
            .binder
            .get_node_symbol(idx)
            .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, idx))
            && let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
            && symbol.has_any_flags(symbol_flags::ALIAS)
            && symbol.declarations.iter().copied().any(|decl_idx| {
                self.ctx
                    .arena
                    .get(decl_idx)
                    .is_some_and(|node| node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION)
            })
        {
            return TypeSymbolResolution::Type(sym_id);
        }

        TypeSymbolResolution::NotFound
    }

    /// Resolve an import-alias symbol to its type-position target via cross-file
    /// export resolution.
    ///
    /// This mirrors how an unshadowed `import { X } from "./m"` is resolved when
    /// `X` is used in type position: it follows the alias to the exported target
    /// in the source module, classifies the target's meaning (namespace/module,
    /// type, or value-only), and follows synthetic default-export alias chains.
    /// Returns `None` when `sym_id` is not an import alias or its target cannot
    /// be resolved.
    pub(crate) fn resolve_alias_type_position_result(
        &self,
        sym_id: SymbolId,
        lib_binders: &[Arc<tsz_binder::BinderState>],
    ) -> Option<TypeSymbolResolution> {
        let classify_target_resolution = |target_sym_id: SymbolId| {
            let mut effective_target_id = target_sym_id;
            let target_symbol_has_declared_type_meaning = |sym_id: SymbolId| {
                let Some(symbol) = self
                    .get_cross_file_symbol(sym_id)
                    .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, lib_binders))
                else {
                    return false;
                };

                if !symbol.has_any_flags(symbol_flags::ALIAS)
                    && symbol.has_any_flags(symbol_flags::TYPE)
                {
                    return true;
                }

                symbol.declarations.iter().copied().any(|decl_idx| {
                    let arena = self
                        .ctx
                        .resolve_symbol_file_index(sym_id)
                        .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
                        .and_then(|binder| binder.get_arena_for_declaration(sym_id, decl_idx))
                        .or_else(|| self.ctx.binder.get_arena_for_declaration(sym_id, decl_idx))
                        .map_or(self.ctx.arena, |arena| arena.as_ref());

                    arena.get(decl_idx).is_some_and(|node| {
                        node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                            || node.kind == syntax_kind_ext::CLASS_DECLARATION
                            || node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                            || node.kind == syntax_kind_ext::ENUM_DECLARATION
                    })
                })
            };
            let mut target_flags = self
                .get_cross_file_symbol(effective_target_id)
                .or_else(|| {
                    self.ctx
                        .binder
                        .get_symbol_with_libs(effective_target_id, lib_binders)
                })
                .map_or(0, |s| s.flags);

            // Synthetic default-export symbols often exist as bare aliases
            // with no direct TYPE/VALUE flags. Follow the alias before
            // deciding whether the import is usable in type position.
            if (target_flags & symbol_flags::ALIAS) != 0 {
                if target_symbol_has_declared_type_meaning(effective_target_id) {
                    return TypeSymbolResolution::Type(effective_target_id);
                }
                let mut visited_target_aliases = AliasCycleTracker::new();
                if let Some(alias_target_id) =
                    self.resolve_alias_symbol(effective_target_id, &mut visited_target_aliases)
                    && alias_target_id != effective_target_id
                {
                    effective_target_id = alias_target_id;
                    target_flags = self
                        .get_cross_file_symbol(effective_target_id)
                        .or_else(|| {
                            self.ctx
                                .binder
                                .get_symbol_with_libs(effective_target_id, lib_binders)
                        })
                        .map_or(0, |s| s.flags);
                }
            }

            let target_is_namespace_module = (target_flags
                & (symbol_flags::MODULE
                    | symbol_flags::NAMESPACE_MODULE
                    | symbol_flags::VALUE_MODULE))
                != 0;
            let target_has_type =
                (target_flags & (symbol_flags::TYPE | symbol_flags::TYPE_ALIAS)) != 0;
            let target_has_value = (target_flags & symbol_flags::VALUE) != 0;
            let target_is_value_only =
                target_has_value && !target_has_type && !target_is_namespace_module;

            if target_is_value_only {
                TypeSymbolResolution::ValueOnly(effective_target_id)
            } else {
                TypeSymbolResolution::Type(effective_target_id)
            }
        };

        if let Some(alias_symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, lib_binders)
            && let Some(module_name) = alias_symbol.import_module()
            && alias_symbol.import_name().is_some()
        {
            let expected_name = alias_symbol
                .import_name()
                .unwrap_or(alias_symbol.escaped_name.as_str());
            let source_file_idx = self
                .ctx
                .resolve_symbol_file_index(sym_id)
                .unwrap_or(self.ctx.current_file_idx);
            if let Some(target_sym_id) = self.resolve_cross_file_export_from_file(
                module_name,
                expected_name,
                Some(source_file_idx),
            ) {
                let export_surface_meanings = (expected_name != "*")
                    .then(|| {
                        self.ctx
                            .resolve_import_target_from_file(source_file_idx, module_name)
                    })
                    .flatten()
                    .map(|target_file_idx| {
                        let declarations = self
                            .export_surface_declarations_in_file(target_file_idx, expected_name);
                        let has_type_position_meaning = declarations.iter().any(|(_, flags, _)| {
                            (*flags
                                & (symbol_flags::TYPE
                                    | symbol_flags::NAMESPACE_MODULE
                                    | symbol_flags::VALUE_MODULE))
                                != 0
                        });
                        let has_runtime_value = declarations
                            .iter()
                            .any(|(_, flags, _)| (*flags & symbol_flags::VALUE) != 0);
                        (has_type_position_meaning, has_runtime_value)
                    });
                if let Some((has_type_position_meaning, has_runtime_value)) =
                    export_surface_meanings
                {
                    if !has_type_position_meaning && has_runtime_value {
                        return Some(TypeSymbolResolution::ValueOnly(target_sym_id));
                    }
                    // The imported export surface positively declares a
                    // type-position meaning (interface, type alias, class,
                    // enum, or namespace/module). Trust it directly — in
                    // type position the type meaning always wins, even when
                    // the same name also carries a runtime value (tsc's dual
                    // meaning rule). Re-deriving through
                    // `classify_target_resolution` reads the target's flags
                    // via `get_cross_file_symbol`, which — when the cross-file
                    // target's raw `SymbolId` collides with a local import
                    // alias of the same id (per-file binders reuse ids) —
                    // short-circuits to the local alias and misreads a
                    // value-only merge: a false TS2749 on `import type { A }`
                    // + `const A: A` (the type-only import supplies the type,
                    // the local const the value).
                    if has_type_position_meaning {
                        self.record_cross_file_symbol_if_needed(
                            target_sym_id,
                            expected_name,
                            module_name,
                        );
                        return Some(TypeSymbolResolution::Type(target_sym_id));
                    }
                }
                // Use get_cross_file_symbol first, then fall back to
                // get_symbol_with_libs. When the target comes from a
                // different binder (ambient module, cross-file export),
                // SymbolId values can collide with the current binder's
                // symbols, causing incorrect flag lookups.
                self.record_cross_file_symbol_if_needed(target_sym_id, expected_name, module_name);
                return Some(classify_target_resolution(target_sym_id));
            }
        }
        let mut visited_aliases = AliasCycleTracker::new();
        self.resolve_alias_symbol(sym_id, &mut visited_aliases)
            .map(|target_sym_id| {
                if let Some(alias_symbol) =
                    self.ctx.binder.get_symbol_with_libs(sym_id, lib_binders)
                    && let Some(module_name) = alias_symbol.import_module()
                {
                    let expected_name = alias_symbol
                        .import_name()
                        .unwrap_or(alias_symbol.escaped_name.as_str());
                    self.record_cross_file_symbol_if_needed(
                        target_sym_id,
                        expected_name,
                        module_name,
                    );
                }
                classify_target_resolution(target_sym_id)
            })
    }

    // =========================================================================
    // Type Parameter Resolution
    // =========================================================================

    /// Look up a type parameter by name in the current type parameter scope.
    ///
    /// Type parameters are scoped to their declaring generic (function, class, interface, etc.).
    /// This function checks the current type parameter scope to resolve type parameter names.
    pub(crate) fn lookup_type_parameter(&self, name: &str) -> Option<TypeId> {
        self.ctx.type_parameter_scope.get(name).copied()
    }

    /// Get all type parameter bindings for passing to `TypeLowering`.
    ///
    /// Returns a vector of (name, `TypeId`) pairs for all type parameters in scope.
    pub(crate) fn get_type_param_bindings(&self) -> Vec<(tsz_common::interner::Atom, TypeId)> {
        self.ctx
            .type_parameter_scope
            .iter()
            .map(|(name, &type_id)| (self.ctx.types.intern_string(name), type_id))
            .collect()
    }

    // =========================================================================
    // Entity Name Resolution
    // =========================================================================

    /// Get the text representation of an entity name node.
    ///
    /// Get the text representation of an expression (simple chains only).
    /// Handles Identifiers and `PropertyAccessExpressions` (e.g., `a.b.c`).
    pub(crate) fn expression_text(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => self
                .ctx
                .arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.to_string()),
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                let left = self.expression_text(access.expression)?;
                let right = self.expression_text(access.name_or_argument)?;
                Some(format!("{left}.{right}"))
            }
            _ => None,
        }
    }

    /// Entity names can be simple identifiers or qualified names (e.g., `A.B.C`).
    /// This function recursively builds the full text representation.
    pub(crate) fn entity_name_text(&self, idx: NodeIndex) -> Option<String> {
        entity_name_text_in_arena(self.ctx.arena, idx)
    }

    pub(crate) fn resolve_global_augmentation_root_symbol(
        &self,
        name: &str,
        lib_binders: &[std::sync::Arc<tsz_binder::BinderState>],
    ) -> Option<tsz_binder::SymbolId> {
        let from_binder = |binder: &tsz_binder::BinderState,
                           file_idx: Option<usize>|
         -> Option<tsz_binder::SymbolId> {
            let augmentations = binder.global_augmentations.get(name)?;
            for augmentation in augmentations {
                if let Some(sym_id) = binder.node_symbols.get(&augmentation.node.0).copied() {
                    if let Some(file_idx) = file_idx {
                        self.ctx.register_symbol_file_target(sym_id, file_idx);
                    }
                    return Some(sym_id);
                }
            }
            None
        };

        if let Some(sym_id) = from_binder(
            self.ctx.binder,
            (self.ctx.current_file_idx != usize::MAX).then_some(self.ctx.current_file_idx),
        ) {
            return Some(sym_id);
        }

        if let Some(all_binders) = self.ctx.all_binders.as_ref() {
            for (file_idx, binder) in all_binders.iter().enumerate() {
                if let Some(sym_id) = from_binder(binder, Some(file_idx)) {
                    return Some(sym_id);
                }
            }
        }

        for binder in lib_binders {
            if let Some(sym_id) = from_binder(binder, None) {
                return Some(sym_id);
            }
        }

        None
    }

    // =========================================================================
    // Symbol Resolution for Lowering
    // =========================================================================

    /// Resolve a type symbol for type lowering.
    ///
    /// Returns the symbol ID if the resolved symbol has the TYPE flag set.
    /// Returns None for built-in types that have special handling in `TypeLowering`.
    pub(crate) fn resolve_type_symbol_for_lowering(&self, idx: NodeIndex) -> Option<u32> {
        // Skip built-in types that have special handling in TypeLowering
        // These types use built-in TypeData representations instead of Refs
        if let Some(node) = self.ctx.arena.get(idx)
            && let Some(ident) = self.ctx.arena.get_identifier(node)
        {
            let name = ident.escaped_text.as_str();
            if is_compiler_managed_type(name) {
                let scoped_shadow = self.ctx.binder.resolve_identifier_with_filter(
                    self.ctx.arena,
                    idx,
                    &[],
                    |candidate| {
                        let Some(symbol) = self.ctx.binder.get_symbol(candidate) else {
                            return false;
                        };
                        if symbol.escaped_name.as_str() != name {
                            return false;
                        }
                        let typeish = symbol.has_any_flags(
                            symbol_flags::TYPE
                                | symbol_flags::ALIAS
                                | symbol_flags::REGULAR_ENUM
                                | symbol_flags::CONST_ENUM,
                        );
                        if !typeish {
                            return false;
                        }
                        let file_local = self.ctx.binder.file_locals.get(name) == Some(candidate);
                        let lib_like_file_local = file_local
                            && !symbol.has_any_flags(symbol_flags::ALIAS)
                            && (self.ctx.symbol_is_from_lib(candidate)
                                || symbol.decl_file_idx == u32::MAX);
                        !lib_like_file_local
                    },
                );
                let shadows_compiler_managed_type = (matches!(name, "Array" | "ReadonlyArray")
                    && self.ctx.file_local_type_shadow_for_lib_name(name))
                    || scoped_shadow.is_some();
                if !shadows_compiler_managed_type {
                    return None;
                }
            }
            if node.kind == SyntaxKind::Identifier as u16
                && let TypeSymbolResolution::Type(sym_id) =
                    self.resolve_identifier_symbol_in_type_position(idx)
            {
                // An import alias from a module that never resolved (TS2307
                // already emitted) must not bind to a stable `DefId`: lowering
                // would build `Application(Lazy(alias_def), args)`, a non-error
                // shape that keeps its type arguments for structural comparison
                // (so two instantiations differing only in an argument fail to
                // relate). tsc poisons it to `any`; `None` routes lowering to the
                // error-like `UnresolvedTypeName`. See `is_unresolved_import_symbol_id`.
                if self.is_unresolved_import_symbol_id(sym_id) {
                    return None;
                }
                let lib_binders = self.get_lib_binders();
                if let Some(alias_symbol) =
                    self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
                    && alias_symbol.has_any_flags(symbol_flags::ALIAS)
                    && alias_symbol.is_type_only
                    && let Some(module_name) = alias_symbol.import_module()
                    && let Some(import_name) = alias_symbol.import_name()
                {
                    let source_file_idx = self
                        .ctx
                        .resolve_symbol_file_index(sym_id)
                        .unwrap_or(self.ctx.current_file_idx);
                    if let Some(target_sym_id) = self.resolve_cross_file_export_from_file(
                        module_name,
                        import_name,
                        Some(source_file_idx),
                    ) {
                        let target_has_type = self
                            .get_cross_file_symbol(target_sym_id)
                            .or_else(|| {
                                self.ctx
                                    .binder
                                    .get_symbol_with_libs(target_sym_id, &lib_binders)
                            })
                            .is_some_and(|target_symbol| {
                                target_symbol.has_any_flags(symbol_flags::TYPE)
                            });
                        if target_has_type {
                            return Some(target_sym_id.0);
                        }
                    }
                }
                if let Some(symbol) = self
                    .get_cross_file_symbol(sym_id)
                    .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders))
                {
                    if symbol.escaped_name != ident.escaped_text {
                        return self
                            .resolve_entity_name_text_to_def_id_for_lowering(
                                ident.escaped_text.as_str(),
                            )
                            .and_then(|def_id| {
                                self.ctx
                                    .def_symbol_identity(def_id)
                                    .map(|(sym_id, _)| sym_id.0)
                            });
                    }
                    if symbol.has_any_flags(symbol_flags::ALIAS) {
                        let mut visited_aliases = AliasCycleTracker::new();
                        if let Some(target_sym_id) =
                            self.resolve_alias_symbol(sym_id, &mut visited_aliases)
                            && target_sym_id != sym_id
                            && self
                                .get_cross_file_symbol(target_sym_id)
                                .or_else(|| {
                                    self.ctx
                                        .binder
                                        .get_symbol_with_libs(target_sym_id, &lib_binders)
                                })
                                .is_some_and(|target_symbol| {
                                    target_symbol.has_any_flags(symbol_flags::TYPE)
                                })
                        {
                            return Some(target_sym_id.0);
                        }
                    }
                    if symbol.has_any_flags(symbol_flags::TYPE) {
                        return Some(sym_id.0);
                    }
                }
            }
        }

        let mut sym_id = match self.resolve_qualified_symbol_in_type_position(idx) {
            TypeSymbolResolution::Type(sym_id) => sym_id,
            _ => return None,
        };
        // Use get_cross_file_symbol to avoid SymbolId collisions across binders.
        // When resolving qualified names like `server.IWorkspace`, the SymbolId
        // belongs to server.ts's binder, not the current file's binder. Without
        // this, we'd look up the SymbolId in the wrong binder and potentially
        // get a different symbol with a colliding ID.
        let lib_binders = self.get_lib_binders();
        let mut symbol = self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders))?;
        if symbol.has_any_flags(symbol_flags::ALIAS) {
            let mut visited_aliases = AliasCycleTracker::new();
            if let Some(target_sym_id) = self.resolve_alias_symbol(sym_id, &mut visited_aliases)
                && target_sym_id != sym_id
            {
                sym_id = target_sym_id;
                symbol = self
                    .get_cross_file_symbol(sym_id)
                    .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders))?;
            }
        }
        symbol.has_any_flags(symbol_flags::TYPE).then_some(sym_id.0)
    }

    /// Resolve a value symbol for type lowering.
    ///
    /// Returns the symbol ID if the resolved symbol has VALUE or ALIAS flags set.
    pub(crate) fn resolve_value_symbol_for_lowering(&self, idx: NodeIndex) -> Option<u32> {
        if let Some(node) = self.ctx.arena.get(idx) {
            if node.kind == SyntaxKind::Identifier as u16
                && let Some(sym_id) = self.resolve_identifier_symbol(idx)
                && self.alias_resolves_to_type_only(sym_id)
            {
                return None;
            }
            if node.kind == syntax_kind_ext::QUALIFIED_NAME {
                let mut current = idx;
                while let Some(node) = self.ctx.arena.get(current) {
                    if node.kind == SyntaxKind::Identifier as u16 {
                        if let Some(sym_id) = self.resolve_identifier_symbol(current)
                            && self.alias_resolves_to_type_only(sym_id)
                        {
                            return None;
                        }
                        break;
                    }
                    if node.kind != syntax_kind_ext::QUALIFIED_NAME {
                        break;
                    }
                    let Some(qn) = self.ctx.arena.get_qualified_name(node) else {
                        break;
                    };
                    current = qn.left;
                }
            }
        }
        let sym_id = self.resolve_qualified_symbol(idx)?;
        let lib_binders = self.get_lib_binders();
        let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)?;
        if symbol.is_type_only {
            return None;
        }
        // An uninstantiated namespace (`NAMESPACE_MODULE`, no `VALUE_MODULE`)
        // still occupies `typeof X`'s value namespace in tsc; excluding it makes
        // every caller here report a spurious TS2304 instead of TS2708.
        if (symbol.flags
            & (symbol_flags::VALUE | symbol_flags::ALIAS | symbol_flags::NAMESPACE_MODULE))
            != 0
        {
            return Some(sym_id.0);
        }

        // The initial resolution found a TYPE-only symbol (e.g., `interface Promise<T>`
        // from one lib file). But the VALUE declaration (`declare var Promise`) may
        // exist in a different lib file. Search all lib binders by name for a symbol
        // that has the VALUE flag. This handles declaration merging across lib files.
        let name = self
            .ctx
            .arena
            .get(idx)
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .map(|i| i.escaped_text.as_str());
        if let Some(name) = name {
            // Check file_locals first (may have merged value from lib)
            if let Some(val_sym_id) = self.ctx.binder.file_locals.get(name)
                && let Some(val_symbol) = self
                    .ctx
                    .binder
                    .get_symbol_with_libs(val_sym_id, &lib_binders)
                && (val_symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0
                && !val_symbol.is_type_only
            {
                return Some(val_sym_id.0);
            }
            // Search lib binders directly for a value declaration
            for lib_binder in lib_binders.iter() {
                if let Some(val_sym_id) = lib_binder.file_locals.get(name)
                    && let Some(val_symbol) = lib_binder.get_symbol(val_sym_id)
                    && (val_symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0
                    && !val_symbol.is_type_only
                {
                    return Some(val_sym_id.0);
                }
            }
        }

        None
    }
}
