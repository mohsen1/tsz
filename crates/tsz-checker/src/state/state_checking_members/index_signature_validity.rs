//! AST-based validity helpers for index-signature parameter type annotations.
//!
//! Extracted from `index_signature_checks.rs` to keep the parent module
//! under the 2000-LOC ceiling. Methods live on `CheckerState` so they can
//! reuse the type-position symbol resolver and the checker's type-parameter
//! stack.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Mirror tsc's `everyType(type, isValidIndexKeyType)` over the *resolved*
    /// index-signature key `TypeId`.
    ///
    /// tsc validates index-signature parameter types against the resolved type,
    /// not the syntactic spelling: an alias whose constituents are all subsets
    /// of `string | number | symbol` (or a template-literal/pattern type) is a
    /// valid key. The AST helper [`Self::is_valid_index_sig_param_type`] can only
    /// recurse alias bodies that live in the current file's arena, so it misses
    /// cross-file aliases — most notably the lib global `PropertyKey`
    /// (`string | number | symbol`). Resolving the key type first makes the
    /// decision independent of which source file declares the alias.
    ///
    /// `everyType` distributes the predicate over union constituents; a single
    /// constituent is valid when it is `string`/`number`/`symbol`, a
    /// template-literal type, or an intersection with some valid member. `any`,
    /// `unknown`, `boolean`, literals, and object types are not valid keys —
    /// matching tsc, and notably *not* an assignability test (`any` is
    /// assignable to `string | number | symbol` yet is still rejected).
    pub(crate) fn index_key_type_is_valid(&mut self, key_type: TypeId) -> bool {
        let resolved = self.evaluate_type_for_assignability(key_type);
        crate::query_boundaries::index_signature::resolved_index_key_type_is_valid(
            self.ctx.types.as_type_database(),
            resolved,
        )
    }

    /// True when the *resolved* index-signature key type is concrete (free of
    /// type parameters) and a structurally valid index key.
    ///
    /// Used by [`Self::classify_index_sig_param_type`] to override a spurious
    /// AST-level "generic" classification for instantiated generic-alias
    /// applications (e.g. `Brand<string, 'event'>`), while preserving TS1337 for
    /// still-generic keys such as `T & string` and literal keys such as
    /// `'a' | 'b'`. Mirrors tsc deciding TS1337 from the resolved type rather
    /// than the syntactic spelling.
    pub(crate) fn resolved_index_key_is_concrete_valid(&mut self, key_type: TypeId) -> bool {
        let resolved = self.evaluate_type_for_assignability(key_type);
        crate::query_boundaries::index_signature::resolved_index_key_is_concrete_valid(
            self.ctx.types.as_type_database(),
            resolved,
        )
    }

    /// Classify an index-signature parameter type against tsc's grammar rules,
    /// returning `(is_generic_or_literal, is_valid)`.
    ///
    /// tsc applies the TS1337 (literal/generic) condition *before* the
    /// `isValidIndexKeyType` (TS1268) check, so a generic key like `T & string`
    /// is never treated as valid even though one member is. `is_valid` combines
    /// the resolved-type structural check ([`Self::index_key_type_is_valid`],
    /// which accepts cross-file aliases such as the lib global `PropertyKey`)
    /// with the AST helper as a defensive fallback for local composite spellings.
    /// This is the single decision point shared by every index-signature site.
    pub(crate) fn classify_index_sig_param_type(
        &mut self,
        key_type: TypeId,
        type_annotation_idx: NodeIndex,
    ) -> (bool, bool) {
        let type_node_kind = self
            .ctx
            .arena
            .get(type_annotation_idx)
            .map_or(0, |n| n.kind);
        // Resolve the key once and derive both the TS1268 validity and the
        // TS1337 generic/literal classification from it.
        let resolved_key = self.evaluate_type_for_assignability(key_type);
        let db = self.ctx.types.as_type_database();
        let resolved_key_is_valid =
            crate::query_boundaries::index_signature::resolved_index_key_type_is_valid(
                db,
                resolved_key,
            );
        // The AST walk over-reports "generic" for an instantiated generic-alias
        // application (e.g. `Brand<string, 'event'>`); drop the spurious TS1337
        // when the resolved key is a concrete valid index key. See
        // `resolved_index_key_is_concrete_valid` for the full rationale.
        let resolved_key_is_concrete_valid = resolved_key_is_valid
            && !crate::query_boundaries::common::contains_free_type_parameters(db, resolved_key);
        let is_generic_or_literal = self
            .is_type_param_or_literal_in_index_sig(type_node_kind, type_annotation_idx)
            && !resolved_key_is_concrete_valid;
        let is_valid = !is_generic_or_literal
            && (resolved_key_is_valid
                || self.is_valid_index_sig_param_type(type_node_kind, type_annotation_idx));
        (is_generic_or_literal, is_valid)
    }

    /// Check if a type node represents a valid index signature parameter type.
    /// Valid types: string, number, symbol keywords, template literal types,
    /// type aliases that resolve to these, unions whose members are all valid,
    /// and non-generic intersections where some member is valid.
    pub(crate) fn is_valid_index_sig_param_type(
        &self,
        type_node_kind: u16,
        type_annotation_idx: NodeIndex,
    ) -> bool {
        use crate::symbol_resolver::TypeSymbolResolution;
        use tsz_scanner::SyntaxKind;

        match type_node_kind {
            k if k == SyntaxKind::StringKeyword as u16 => true,
            k if k == SyntaxKind::NumberKeyword as u16 => true,
            k if k == SyntaxKind::SymbolKeyword as u16 => true,
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => true,
            k if k == syntax_kind_ext::UNION_TYPE => self
                .ctx
                .arena
                .get(type_annotation_idx)
                .and_then(|n| self.ctx.arena.get_composite_type(n))
                .is_some_and(|composite| {
                    composite.types.nodes.iter().all(|&m| {
                        self.ctx
                            .arena
                            .get(m)
                            .is_some_and(|mn| self.is_valid_index_sig_param_type(mn.kind, m))
                    })
                }),
            k if k == syntax_kind_ext::INTERSECTION_TYPE => self
                .ctx
                .arena
                .get(type_annotation_idx)
                .and_then(|n| self.ctx.arena.get_composite_type(n))
                .is_some_and(|composite| {
                    // Accept the intersection only when at least one member is
                    // a structurally valid index-sig type AND no member contains
                    // a generic type parameter or literal type. The latter
                    // guard prevents `T & string` from sneaking past the
                    // TS1337 check at call sites that gate on validity.
                    let any_valid = composite.types.nodes.iter().any(|&m| {
                        self.ctx
                            .arena
                            .get(m)
                            .is_some_and(|mn| self.is_valid_index_sig_param_type(mn.kind, m))
                    });
                    let any_generic_or_literal = composite.types.nodes.iter().any(|&m| {
                        self.ctx.arena.get(m).is_some_and(|mn| {
                            self.is_type_param_or_literal_in_index_sig(mn.kind, m)
                        })
                    });
                    any_valid && !any_generic_or_literal
                }),
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let Some(type_node) = self.ctx.arena.get(type_annotation_idx) else {
                    return false;
                };
                let Some(type_ref) = self.ctx.arena.get_type_ref(type_node) else {
                    return false;
                };
                if let Some(name_node) = self.ctx.arena.get(type_ref.type_name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                {
                    let name = ident.escaped_text.as_str();
                    if matches!(name, "string" | "number" | "symbol") {
                        return true;
                    }
                }
                if let TypeSymbolResolution::Type(sym_id) =
                    self.resolve_identifier_symbol_in_type_position(type_ref.type_name)
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                    && (symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS) != 0
                    && let Some(&decl_idx) = symbol.declarations.first()
                    && let Some(decl_node) = self.ctx.arena.get(decl_idx)
                    && let Some(type_alias) = self.ctx.arena.get_type_alias(decl_node)
                    && let Some(alias_type_node) = self.ctx.arena.get(type_alias.type_node)
                {
                    return self
                        .is_valid_index_sig_param_type(alias_type_node.kind, type_alias.type_node);
                }
                false
            }
            _ => false,
        }
    }

    /// Check if the type annotation of an index signature parameter is a type
    /// parameter or a literal type (triggers TS1337 instead of TS1268).
    pub(crate) fn is_type_param_or_literal_in_index_sig(
        &self,
        type_node_kind: u16,
        type_annotation_idx: NodeIndex,
    ) -> bool {
        use crate::symbol_resolver::TypeSymbolResolution;
        use tsz_scanner::SyntaxKind;

        if type_node_kind == syntax_kind_ext::LITERAL_TYPE
            || type_node_kind == SyntaxKind::StringLiteral as u16
            || type_node_kind == SyntaxKind::NumericLiteral as u16
            || type_node_kind == SyntaxKind::TrueKeyword as u16
            || type_node_kind == SyntaxKind::FalseKeyword as u16
        {
            return true;
        }

        if type_node_kind == syntax_kind_ext::UNION_TYPE
            || type_node_kind == syntax_kind_ext::INTERSECTION_TYPE
        {
            return self
                .ctx
                .arena
                .get(type_annotation_idx)
                .and_then(|n| self.ctx.arena.get_composite_type(n))
                .is_some_and(|composite| {
                    composite.types.nodes.iter().any(|&m| {
                        self.ctx.arena.get(m).is_some_and(|mn| {
                            self.is_type_param_or_literal_in_index_sig(mn.kind, m)
                        })
                    })
                });
        }

        if type_node_kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_node) = self.ctx.arena.get(type_annotation_idx)
            && let Some(type_ref) = self.ctx.arena.get_type_ref(type_node)
        {
            if let TypeSymbolResolution::Type(sym_id) =
                self.resolve_identifier_symbol_in_type_position(type_ref.type_name)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            {
                if (symbol.flags & tsz_binder::symbol_flags::TYPE_PARAMETER) != 0 {
                    return true;
                }
                if (symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS) != 0
                    && let Some(&decl_idx) = symbol.declarations.first()
                    && let Some(decl_node) = self.ctx.arena.get(decl_idx)
                    && let Some(type_alias) = self.ctx.arena.get_type_alias(decl_node)
                    && let Some(alias_type_node) = self.ctx.arena.get(type_alias.type_node)
                {
                    return self.is_type_param_or_literal_in_index_sig(
                        alias_type_node.kind,
                        type_alias.type_node,
                    );
                }
            }
            // Fallback: checker's type parameter stack (covers type params from
            // type aliases/generics not registered in the binder symbol table).
            if let Some(name_node) = self.ctx.arena.get(type_ref.type_name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                && self
                    .lookup_type_parameter(ident.escaped_text.as_str())
                    .is_some()
            {
                return true;
            }
        }

        false
    }
}
