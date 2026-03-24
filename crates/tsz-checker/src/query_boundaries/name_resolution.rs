//! Name resolution boundary API.
//!
//! Provides a unified `NameResolutionRequest` → `NameResolutionResult` gateway
//! so that checker code does not open-code value/type/namespace/exported-member
//! lookup and suggestion logic.  The boundary owns:
//!
//! - Meaning classification (value vs type vs namespace vs exported member)
//! - Structured failure reasons (not found, wrong meaning, missing export, etc.)
//! - Spelling-suggestion collection (delegated to existing helpers)
//!
//! Diagnostic families covered: TS2304, TS2552, TS2694, TS2708, TS2305, TS2724.

use tsz_binder::SymbolId;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

// ---------------------------------------------------------------------------
// Request model
// ---------------------------------------------------------------------------

/// The semantic meaning the caller is looking for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NameLookupKind {
    /// Expression/value position (e.g., `let x = Foo`).
    Value,
    /// Type position (e.g., `let x: Foo`).
    Type,
    /// Namespace position (e.g., `Foo.Bar` where `Foo` is a namespace).
    Namespace,
    /// Exported member of an already-resolved module/namespace symbol.
    ExportedMember,
}

/// A structured request for name resolution.
#[derive(Debug, Clone)]
pub(crate) struct NameResolutionRequest<'n> {
    /// The identifier text to look up.
    pub name: &'n str,
    /// The AST node where the lookup originates (for scope + diagnostics).
    pub idx: NodeIndex,
    /// What kind of meaning the caller needs.
    pub kind: NameLookupKind,
    /// For `ExportedMember` lookups: the parent namespace/module symbol.
    pub parent_symbol: Option<SymbolId>,
    /// Optional list of export names (for spelling suggestions on exported members).
    pub export_candidates: Option<Vec<String>>,
}

impl<'n> NameResolutionRequest<'n> {
    /// Convenience: create a value-position lookup.
    pub const fn value(name: &'n str, idx: NodeIndex) -> Self {
        Self {
            name,
            idx,
            kind: NameLookupKind::Value,
            parent_symbol: None,
            export_candidates: None,
        }
    }

    /// Convenience: create a type-position lookup.
    pub const fn type_ref(name: &'n str, idx: NodeIndex) -> Self {
        Self {
            name,
            idx,
            kind: NameLookupKind::Type,
            parent_symbol: None,
            export_candidates: None,
        }
    }

    /// Convenience: create a namespace-position lookup.
    pub const fn namespace(name: &'n str, idx: NodeIndex) -> Self {
        Self {
            name,
            idx,
            kind: NameLookupKind::Namespace,
            parent_symbol: None,
            export_candidates: None,
        }
    }

    /// Convenience: create an exported-member lookup.
    pub const fn exported_member(
        name: &'n str,
        idx: NodeIndex,
        parent_symbol: SymbolId,
        export_candidates: Vec<String>,
    ) -> Self {
        Self {
            name,
            idx,
            kind: NameLookupKind::ExportedMember,
            parent_symbol: Some(parent_symbol),
            export_candidates: Some(export_candidates),
        }
    }
}

// ---------------------------------------------------------------------------
// Result model
// ---------------------------------------------------------------------------

/// Successful name resolution.
#[derive(Debug, Clone)]
pub(crate) struct ResolvedName {
    /// The symbol that was found.
    pub symbol_id: SymbolId,
    /// The symbol's binder flags (VALUE, TYPE, ALIAS, etc.).
    pub flags: u32,
    /// Whether the symbol is type-only (from `import type` / `export type`).
    pub is_type_only: bool,
}

/// Why a name lookup failed.
#[derive(Debug, Clone)]
pub(crate) enum ResolutionFailureKind {
    /// The name does not exist in any reachable scope.
    NotFound,
    /// The name exists but has the wrong meaning.
    /// For example, a type used as a value, or a namespace used as a value.
    WrongMeaning {
        /// The symbol that was found with the wrong meaning.
        found_symbol: SymbolId,
        /// What meaning the symbol actually has.
        actual_meaning: NameLookupKind,
    },
    /// The namespace/module does not export a member with this name.
    ExportedMemberMissing {
        /// The parent namespace/module symbol.
        parent_symbol: SymbolId,
        /// The parent namespace/module name (for diagnostics).
        parent_name: String,
    },
    /// The name is ambiguous or shadowed in a way that prevents resolution.
    Ambiguous {
        /// The competing symbols.
        candidates: Vec<SymbolId>,
    },
}

/// A structured failure from name resolution.
#[derive(Debug, Clone)]
pub(crate) struct ResolutionFailure {
    /// Why the lookup failed.
    pub kind: ResolutionFailureKind,
    /// Spelling-suggestion candidates (if any).
    pub suggestions: Vec<String>,
}

impl ResolutionFailure {
    /// Create a simple "not found" failure with no suggestions.
    pub const fn not_found() -> Self {
        Self {
            kind: ResolutionFailureKind::NotFound,
            suggestions: Vec::new(),
        }
    }

    /// Create a "not found" failure with spelling suggestions.
    pub const fn not_found_with_suggestions(suggestions: Vec<String>) -> Self {
        Self {
            kind: ResolutionFailureKind::NotFound,
            suggestions,
        }
    }

    /// Create a "wrong meaning" failure.
    pub const fn wrong_meaning(found_symbol: SymbolId, actual_meaning: NameLookupKind) -> Self {
        Self {
            kind: ResolutionFailureKind::WrongMeaning {
                found_symbol,
                actual_meaning,
            },
            suggestions: Vec::new(),
        }
    }

    /// Create an "exported member missing" failure.
    pub const fn exported_member_missing(parent_symbol: SymbolId, parent_name: String) -> Self {
        Self {
            kind: ResolutionFailureKind::ExportedMemberMissing {
                parent_symbol,
                parent_name,
            },
            suggestions: Vec::new(),
        }
    }

    /// Create an "exported member missing" failure with spelling suggestions.
    pub const fn exported_member_missing_with_suggestions(
        parent_symbol: SymbolId,
        parent_name: String,
        suggestions: Vec<String>,
    ) -> Self {
        Self {
            kind: ResolutionFailureKind::ExportedMemberMissing {
                parent_symbol,
                parent_name,
            },
            suggestions,
        }
    }

    /// Whether this failure has spelling suggestions.
    pub const fn has_suggestions(&self) -> bool {
        !self.suggestions.is_empty()
    }
}

/// The outcome of a name resolution attempt.
pub(crate) type NameResolutionResult = Result<ResolvedName, ResolutionFailure>;

// ---------------------------------------------------------------------------
// Resolution logic (on CheckerState)
// ---------------------------------------------------------------------------

use crate::state::CheckerState;
use tsz_binder::symbol_flags;

impl<'a> CheckerState<'a> {
    /// Unified name resolution gateway.
    ///
    /// Resolves a name according to the requested meaning (value/type/namespace/
    /// exported-member) and returns a structured result. The checker should use
    /// this instead of ad-hoc binder lookups + flag-checking for the diagnostic
    /// families TS2304/TS2552/TS2694/TS2708/TS2305/TS2724.
    pub(crate) fn resolve_name_structured(
        &self,
        request: &NameResolutionRequest<'_>,
    ) -> NameResolutionResult {
        match request.kind {
            NameLookupKind::ExportedMember => self.resolve_exported_member(request),
            NameLookupKind::Value | NameLookupKind::Type | NameLookupKind::Namespace => {
                self.resolve_scoped_name(request)
            }
        }
    }

    /// Classify whether an already-resolved symbol has the right meaning for
    /// the requested lookup kind.
    ///
    /// Use this when the checker already resolved a symbol (via binder lookup
    /// or `resolve_identifier_symbol_in_type_position`) and needs a structured
    /// meaning-mismatch result without re-resolving through binder scope chains.
    ///
    /// Returns `Ok(ResolvedName)` if the symbol matches, or `Err(ResolutionFailure)`
    /// with `WrongMeaning` if it doesn't.
    pub(crate) const fn classify_symbol_meaning(
        &self,
        sym_id: SymbolId,
        flags: u32,
        is_type_only: bool,
        desired: NameLookupKind,
    ) -> NameResolutionResult {
        match desired {
            NameLookupKind::Value => {
                let has_value = (flags & symbol_flags::VALUE) != 0;
                let is_type_alias = (flags & symbol_flags::TYPE_ALIAS) != 0;
                let has_type = (flags & symbol_flags::TYPE) != 0;
                let is_namespace = (flags & symbol_flags::NAMESPACE_MODULE) != 0;
                let value_flags_except_module = symbol_flags::VALUE & !symbol_flags::VALUE_MODULE;
                let has_other_value = (flags & value_flags_except_module) != 0;

                if is_type_alias && !has_value {
                    return Err(ResolutionFailure::wrong_meaning(
                        sym_id,
                        NameLookupKind::Type,
                    ));
                }
                if has_type && !has_value && (flags & symbol_flags::ALIAS) == 0 {
                    return Err(ResolutionFailure::wrong_meaning(
                        sym_id,
                        NameLookupKind::Type,
                    ));
                }
                if is_namespace && !has_other_value {
                    return Err(ResolutionFailure::wrong_meaning(
                        sym_id,
                        NameLookupKind::Namespace,
                    ));
                }
            }
            NameLookupKind::Type => {
                let has_type = (flags & symbol_flags::TYPE) != 0;
                let has_type_alias = (flags & symbol_flags::TYPE_ALIAS) != 0;
                if !has_type && !has_type_alias {
                    return Err(ResolutionFailure::wrong_meaning(
                        sym_id,
                        NameLookupKind::Value,
                    ));
                }
            }
            NameLookupKind::Namespace => {
                let is_namespace = (flags & symbol_flags::NAMESPACE_MODULE) != 0;
                let has_type = (flags & symbol_flags::TYPE) != 0;
                if !is_namespace && !has_type {
                    return Err(ResolutionFailure::wrong_meaning(
                        sym_id,
                        NameLookupKind::Value,
                    ));
                }
            }
            NameLookupKind::ExportedMember => {
                // ExportedMember classification is handled by resolve_exported_member
            }
        }

        Ok(ResolvedName {
            symbol_id: sym_id,
            flags,
            is_type_only,
        })
    }

    /// Resolve a name through binder scope chains (value/type/namespace).
    fn resolve_scoped_name(&self, request: &NameResolutionRequest<'_>) -> NameResolutionResult {
        let sym_id = self.resolve_identifier_symbol(request.idx);
        let Some(sym_id) = sym_id else {
            // Not found — collect spelling suggestions via shared helper
            // which respects all tsc suppression rules.
            let suggestions = self.collect_spelling_suggestions(request.name, request.idx);
            return Err(if suggestions.is_empty() {
                ResolutionFailure::not_found()
            } else {
                ResolutionFailure::not_found_with_suggestions(suggestions)
            });
        };

        // Get symbol flags
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol(sym_id))
            .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders));
        let Some(symbol) = symbol else {
            return Err(ResolutionFailure::not_found());
        };

        let flags = symbol.flags;
        let is_type_only = symbol.is_type_only;

        // Delegate meaning classification to the shared helper
        self.classify_symbol_meaning(sym_id, flags, is_type_only, request.kind)
    }

    /// Resolve an exported member from a known namespace/module symbol.
    fn resolve_exported_member(&self, request: &NameResolutionRequest<'_>) -> NameResolutionResult {
        let parent_sym = request
            .parent_symbol
            .expect("ExportedMember lookup requires parent_symbol");

        let lib_binders = self.get_lib_binders();
        let parent_symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(parent_sym, &lib_binders);

        let parent_name = parent_symbol
            .map(|s| {
                // Build the fully qualified name by walking the parent chain.
                // This matches tsc behavior: when the parent is nested (e.g., foo.bar.baz),
                // display the full path rather than just the last segment.
                self.build_qualified_symbol_name(parent_sym, s)
            })
            .unwrap_or_default();

        // Look up the member in the parent's exports
        let member_sym: Option<SymbolId> = parent_symbol
            .and_then(|s| s.exports.as_ref())
            .and_then(|exports| exports.get(request.name));

        let Some(member_sym) = member_sym else {
            // Not found — try spelling suggestions from export candidates
            let export_candidates = request.export_candidates.as_deref().unwrap_or(&[]);

            let suggestions = if !export_candidates.is_empty() {
                Self::find_export_spelling_suggestion(request.name, export_candidates)
                    .into_iter()
                    .collect()
            } else {
                Vec::new()
            };

            return Err(if suggestions.is_empty() {
                ResolutionFailure::exported_member_missing(parent_sym, parent_name)
            } else {
                ResolutionFailure::exported_member_missing_with_suggestions(
                    parent_sym,
                    parent_name,
                    suggestions,
                )
            });
        };

        let member = self
            .ctx
            .binder
            .get_symbol_with_libs(member_sym, &lib_binders);
        let flags = member.map_or(0, |s| s.flags);
        let is_type_only = member.is_some_and(|s| s.is_type_only);

        Ok(ResolvedName {
            symbol_id: member_sym,
            flags,
            is_type_only,
        })
    }

    /// Build the fully qualified name for a symbol by walking its parent chain.
    ///
    /// For nested namespaces like `foo.bar.baz`, the symbol for `baz` has
    /// parent `bar`, which has parent `foo`. This method walks that chain to
    /// produce the full dotted name `foo.bar.baz`, matching tsc's display in
    /// TS2694 error messages.
    fn build_qualified_symbol_name(&self, sym_id: SymbolId, symbol: &tsz_binder::Symbol) -> String {
        let lib_binders = self.get_lib_binders();
        let mut parts = vec![symbol.escaped_name.clone()];
        let mut current = symbol.parent;

        // Walk up the parent chain, collecting names.
        // Stop at the source file / global scope (parent == NONE or parent is source file).
        let mut fuel = 20; // Prevent infinite loops
        while current != SymbolId::NONE && fuel > 0 {
            fuel -= 1;
            let parent = self.ctx.binder.get_symbol_with_libs(current, &lib_binders);
            match parent {
                Some(p) => {
                    // Stop if we reach a source-file-level symbol (no meaningful name)
                    if p.escaped_name.is_empty() || p.escaped_name == "__global" {
                        break;
                    }
                    parts.push(p.escaped_name.clone());
                    // Stop after adding this symbol if its parent is the root
                    if p.parent == SymbolId::NONE {
                        break;
                    }
                    current = p.parent;
                }
                None => break,
            }
        }

        // Only use the qualified name if there are multiple parts
        if parts.len() > 1 {
            parts.reverse();
            parts.join(".")
        } else {
            // Just the simple name - use the original symbol's escaped_name
            let _ = sym_id; // suppress unused warning
            symbol.escaped_name.clone()
        }
    }

    /// Report a wrong-meaning diagnostic for a symbol that was found but has
    /// the wrong meaning for the desired lookup kind.
    ///
    /// This is a convenience wrapper that constructs a `NameResolutionRequest`
    /// and `ResolutionFailure` from an already-classified symbol and emits the
    /// appropriate diagnostic (TS2693/TS2708/TS2749).
    pub(crate) fn report_wrong_meaning(
        &mut self,
        name: &str,
        idx: NodeIndex,
        sym_id: SymbolId,
        actual_meaning: NameLookupKind,
        desired: NameLookupKind,
    ) {
        let req = match desired {
            NameLookupKind::Value => NameResolutionRequest::value(name, idx),
            NameLookupKind::Type => NameResolutionRequest::type_ref(name, idx),
            NameLookupKind::Namespace => NameResolutionRequest::namespace(name, idx),
            NameLookupKind::ExportedMember => return, // not applicable
        };
        let failure = ResolutionFailure::wrong_meaning(sym_id, actual_meaning);
        self.report_name_resolution_failure(&req, &failure);
    }

    /// Resolve a type-position name through the boundary and report any failure.
    ///
    /// Returns `Ok(())` if the name resolves with type meaning, or emits the
    /// appropriate diagnostic (TS2304/TS2552/TS2749) and returns `Err(())`.
    ///
    /// Use this for type-position lookups that currently call
    /// `error_cannot_find_name_at` or `error_value_only_type_at` directly.
    pub(crate) fn resolve_type_name_or_report(
        &mut self,
        name: &str,
        idx: NodeIndex,
    ) -> Result<ResolvedName, ()> {
        let req = NameResolutionRequest::type_ref(name, idx);
        match self.resolve_name_structured(&req) {
            Ok(resolved) => Ok(resolved),
            Err(failure) => {
                self.report_name_resolution_failure(&req, &failure);
                Err(())
            }
        }
    }

    /// Emit the appropriate diagnostic for a name resolution failure.
    ///
    /// This is the single diagnostic-rendering gateway for name-resolution
    /// failures, replacing scattered `error_cannot_find_name_at` /
    /// `error_namespace_no_export` / etc. call sites.
    pub(crate) fn report_name_resolution_failure(
        &mut self,
        request: &NameResolutionRequest<'_>,
        failure: &ResolutionFailure,
    ) {
        // Suppress TS2304/TS2552 entirely for identifiers inside enum computed
        // property names. tsc only emits TS1164 for these and doesn't resolve
        // the expressions.
        if self.is_in_enum_computed_property(request.idx) {
            return;
        }

        // Suppress TS2304/TS2552 for expressions inside `export default` in a
        // namespace. TS1319 is the correct diagnostic; name resolution produces
        // false positives in this context.
        if self.is_in_namespace_export_default(request.idx) {
            return;
        }

        // TS1212: When `yield` is used as a type reference name inside a generator
        // function, emit TS1212 in addition to the normal TS2304. tsc emits both
        // diagnostics: TS1212 (reserved word) and TS2304 (cannot find name).
        if request.name == "yield"
            && matches!(failure.kind, ResolutionFailureKind::NotFound)
            && self.is_yield_in_generator_type_context(request.idx)
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let message = format_message(
                diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE,
                &[request.name],
            );
            self.error_at_node(
                request.idx,
                &message,
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE,
            );
            // Fall through to also emit the normal TS2304 diagnostic
        }

        // Suppress spelling suggestions (TS2552 → TS2304) in files with parse
        // errors. tsc keeps only primary diagnostics in these files and doesn't
        // offer "did you mean" suggestions.
        let suppress_suggestions = self.has_syntax_parse_errors();

        match &failure.kind {
            ResolutionFailureKind::NotFound => {
                if failure.has_suggestions() && !suppress_suggestions {
                    self.error_cannot_find_name_with_suggestions(
                        request.name,
                        &failure.suggestions,
                        request.idx,
                    );
                } else {
                    self.error_cannot_find_name_at(request.name, request.idx);
                }
            }
            ResolutionFailureKind::WrongMeaning { actual_meaning, .. } => {
                self.report_wrong_meaning_diagnostic(request.name, request.idx, *actual_meaning);
            }
            ResolutionFailureKind::ExportedMemberMissing { parent_name, .. } => {
                if failure.has_suggestions() {
                    // TS2724: did you mean ...?
                    let export_candidates = request.export_candidates.as_deref().unwrap_or(&[]);
                    self.error_namespace_no_export_with_exports(
                        parent_name,
                        request.name,
                        request.idx,
                        export_candidates,
                    );
                } else {
                    // TS2694: namespace has no exported member
                    self.error_namespace_no_export(parent_name, request.name, request.idx);
                }
            }
            ResolutionFailureKind::Ambiguous { .. } => {
                // For now, emit a generic "cannot find name" for ambiguous cases.
                self.error_cannot_find_name_at(request.name, request.idx);
            }
        }
    }

    /// Emit a wrong-meaning diagnostic based on the actual meaning of the symbol.
    ///
    /// This centralizes the mapping from `NameLookupKind` (what the symbol
    /// actually is) to the appropriate TS diagnostic:
    /// - Type → TS2693 (type used as value)
    /// - Namespace → TS2708 (namespace used as value)
    /// - Value → TS2749 (value used as type)
    pub(crate) fn report_wrong_meaning_diagnostic(
        &mut self,
        name: &str,
        idx: NodeIndex,
        actual_meaning: NameLookupKind,
    ) {
        match actual_meaning {
            NameLookupKind::Type => {
                // TS2693: only refers to a type, but is used as a value
                self.error_type_only_value_at(name, idx);
            }
            NameLookupKind::Namespace => {
                // TS2708: cannot use namespace as a value
                self.error_namespace_used_as_value_at(name, idx);
            }
            NameLookupKind::Value => {
                // TS2749: refers to a value, but is used as a type
                self.error_value_only_type_at(name, idx);
            }
            NameLookupKind::ExportedMember => {}
        }
    }

    /// Check if an identifier is inside an `export default` expression within
    /// a namespace/module declaration. tsc emits TS1319 for `export default`
    /// in a namespace but does not try to resolve the exported expression,
    /// so TS2304/TS2552 would be a false positive.
    fn is_in_namespace_export_default(&self, idx: NodeIndex) -> bool {
        let mut cur = idx;
        for _ in 0..8 {
            let node = match self.ctx.arena.get(cur) {
                Some(n) => n,
                None => return false,
            };
            // Heritage clauses should always emit TS2304 — stop walking.
            if node.kind == syntax_kind_ext::HERITAGE_CLAUSE {
                return false;
            }
            // Check for both ExportAssignment (`export default expr`) and
            // ExportDeclaration with default export (`export default expr`).
            // The parser may produce either node type depending on the context.
            let is_export_default = if node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT {
                // Only suppress for `export default`, not `export =`.
                !self
                    .ctx
                    .arena
                    .get_export_assignment(node)
                    .is_some_and(|data| data.is_export_equals)
            } else if node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                // ExportDeclaration with is_default_export flag
                self.ctx
                    .arena
                    .get_export_decl_at(cur)
                    .is_some_and(|data| data.is_default_export)
            } else {
                false
            };
            if is_export_default {
                // Check if this export is inside a namespace/module declaration.
                let mut ns = cur;
                for _ in 0..8 {
                    if let Some(nn) = self.ctx.arena.get(ns)
                        && nn.kind == syntax_kind_ext::MODULE_DECLARATION
                    {
                        return true;
                    }
                    match self.ctx.arena.get_extended(ns) {
                        Some(e) if e.parent.is_some() => ns = e.parent,
                        _ => return false,
                    }
                }
                return false;
            }
            match self.ctx.arena.get_extended(cur) {
                Some(e) if e.parent.is_some() => cur = e.parent,
                _ => return false,
            }
        }
        false
    }

    /// Check if an identifier is inside a computed property name within an
    /// enum member. tsc suppresses TS2304/TS2552 in this context because
    /// computed property names are not valid in enums (TS1164 is emitted
    /// instead by the parser).
    fn is_in_enum_computed_property(&self, idx: NodeIndex) -> bool {
        let ext = match self.ctx.arena.get_extended(idx) {
            Some(ext) => ext,
            None => return false,
        };
        let parent = match self.ctx.arena.get(ext.parent) {
            Some(p) => p,
            None => return false,
        };
        if parent.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        let gp_ext = match self.ctx.arena.get_extended(ext.parent) {
            Some(gp) => gp,
            None => return false,
        };
        let gp = match self.ctx.arena.get(gp_ext.parent) {
            Some(gp) => gp,
            None => return false,
        };
        gp.kind == syntax_kind_ext::ENUM_MEMBER
    }

    /// Check if a node is a `yield` identifier used as a type reference name
    /// inside a generator function. In generators, `yield` is a keyword and
    /// cannot be used as a type name.
    fn is_yield_in_generator_type_context(&self, idx: NodeIndex) -> bool {
        // Check if the identifier is inside a TYPE_REFERENCE
        let ext = match self.ctx.arena.get_extended(idx) {
            Some(ext) => ext,
            None => return false,
        };
        let parent = match self.ctx.arena.get(ext.parent) {
            Some(p) => p,
            None => return false,
        };
        if parent.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        // Walk up to find an enclosing generator function
        self.find_enclosing_function(idx).is_some_and(|fn_idx| {
            let Some(fn_node) = self.ctx.arena.get(fn_idx) else {
                return false;
            };
            if fn_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || fn_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            {
                self.ctx
                    .arena
                    .get_function(fn_node)
                    .is_some_and(|f| f.asterisk_token)
            } else if fn_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                self.ctx
                    .arena
                    .get_method_decl(fn_node)
                    .is_some_and(|m| m.asterisk_token)
            } else {
                false
            }
        })
    }

    /// Convenience: resolve a name and report any failure in one call.
    ///
    /// Returns `Some(ResolvedName)` on success, or `None` after emitting
    /// the appropriate diagnostic for the failure.
    pub(crate) fn resolve_and_report(
        &mut self,
        request: &NameResolutionRequest<'_>,
    ) -> Option<ResolvedName> {
        match self.resolve_name_structured(request) {
            Ok(resolved) => Some(resolved),
            Err(failure) => {
                self.report_name_resolution_failure(request, &failure);
                None
            }
        }
    }

    /// Resolve a value-position name through the boundary and report any failure.
    ///
    /// Returns `Ok(ResolvedName)` if the name resolves with value meaning,
    /// or emits the appropriate diagnostic (TS2304/TS2552/TS2693) and returns `Err(())`.
    ///
    /// Use this for value-position lookups that currently call
    /// `error_cannot_find_name_at` or `error_type_only_value_at` directly.
    pub(crate) fn resolve_value_name_or_report(
        &mut self,
        name: &str,
        idx: NodeIndex,
    ) -> Result<ResolvedName, ()> {
        let req = NameResolutionRequest::value(name, idx);
        match self.resolve_name_structured(&req) {
            Ok(resolved) => Ok(resolved),
            Err(failure) => {
                self.report_name_resolution_failure(&req, &failure);
                Err(())
            }
        }
    }

    /// Report a "not found" diagnostic with suggestion collection at the boundary.
    ///
    /// Collects spelling suggestions via `collect_spelling_suggestions` (which
    /// respects all tsc suppression rules: accessibility modifiers, spread
    /// elements, `arguments`, max-suggestion cap, parse-error suppression) and
    /// emits TS2304 or TS2552 accordingly.
    ///
    /// Use this when the caller already knows the name is not found (e.g.,
    /// after binder resolution failed) but wants suggestion-enriched diagnostics
    /// routed through the boundary.
    pub(crate) fn report_not_found_at_boundary(
        &mut self,
        name: &str,
        idx: NodeIndex,
        kind: NameLookupKind,
    ) {
        if matches!(kind, NameLookupKind::ExportedMember) {
            return;
        }

        // Delegate to the shared suggestion collector which applies all
        // suppression predicates (accessibility modifiers, spread elements,
        // arguments, max cap, parse errors).
        let suggestions = self.collect_spelling_suggestions(name, idx);

        let req = match kind {
            NameLookupKind::Value => NameResolutionRequest::value(name, idx),
            NameLookupKind::Type => NameResolutionRequest::type_ref(name, idx),
            NameLookupKind::Namespace => NameResolutionRequest::namespace(name, idx),
            NameLookupKind::ExportedMember => unreachable!(),
        };

        let failure = if suggestions.is_empty() {
            ResolutionFailure::not_found()
        } else {
            ResolutionFailure::not_found_with_suggestions(suggestions)
        };

        self.report_name_resolution_failure(&req, &failure);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::check_source_diagnostics;

    #[test]
    fn name_resolution_request_constructors() {
        let idx = NodeIndex::NONE;

        let req = NameResolutionRequest::value("foo", idx);
        assert_eq!(req.kind, NameLookupKind::Value);
        assert_eq!(req.name, "foo");
        assert!(req.parent_symbol.is_none());

        let req = NameResolutionRequest::type_ref("Bar", idx);
        assert_eq!(req.kind, NameLookupKind::Type);

        let req = NameResolutionRequest::namespace("ns", idx);
        assert_eq!(req.kind, NameLookupKind::Namespace);
    }

    #[test]
    fn resolution_failure_constructors() {
        let f = ResolutionFailure::not_found();
        assert!(!f.has_suggestions());
        assert!(matches!(f.kind, ResolutionFailureKind::NotFound));

        let f = ResolutionFailure::not_found_with_suggestions(vec!["bar".to_string()]);
        assert!(f.has_suggestions());
        assert_eq!(f.suggestions, vec!["bar"]);

        let sym = SymbolId(42);
        let f = ResolutionFailure::wrong_meaning(sym, NameLookupKind::Type);
        assert!(matches!(f.kind, ResolutionFailureKind::WrongMeaning { .. }));

        let f = ResolutionFailure::exported_member_missing(sym, "MyNs".to_string());
        assert!(matches!(
            f.kind,
            ResolutionFailureKind::ExportedMemberMissing { .. }
        ));

        let f = ResolutionFailure::exported_member_missing_with_suggestions(
            sym,
            "MyNs".to_string(),
            vec!["member1".to_string()],
        );
        assert!(f.has_suggestions());
    }

    // =========================================================================
    // Phase-2 regression tests: wrong-meaning, spelling, exported-member
    // =========================================================================

    /// TS2693: type used as value — routed through boundary
    #[test]
    fn type_used_as_value_emits_ts2693() {
        let diagnostics = check_source_diagnostics(
            r#"
interface Foo { x: number; }
const a = Foo;
"#,
        );
        assert!(
            diagnostics.iter().any(|d| d.code == 2693),
            "Expected TS2693 for interface used as value, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }

    /// TS2693: type alias used as value — routed through boundary
    #[test]
    fn type_alias_used_as_value_emits_ts2693() {
        let diagnostics = check_source_diagnostics(
            r#"
type MyType = string | number;
const a = MyType;
"#,
        );
        assert!(
            diagnostics.iter().any(|d| d.code == 2693),
            "Expected TS2693 for type alias used as value, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }

    /// TS2749: value used as type — routed through boundary
    #[test]
    fn value_used_as_type_emits_ts2749() {
        let diagnostics = check_source_diagnostics(
            r#"
const myValue = 42;
let x: myValue;
"#,
        );
        assert!(
            diagnostics.iter().any(|d| d.code == 2749),
            "Expected TS2749 for value used as type, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }

    /// TS2708: namespace used as value — routed through boundary
    #[test]
    fn namespace_used_as_value_emits_ts2708() {
        let diagnostics = check_source_diagnostics(
            r#"
namespace MyNs {
    export interface I { x: number; }
}
const a = MyNs;
"#,
        );
        assert!(
            diagnostics.iter().any(|d| d.code == 2708),
            "Expected TS2708 for namespace used as value, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }

    /// TS2693: primitive type keyword used as value — routed through boundary
    #[test]
    fn primitive_type_keyword_as_value_emits_ts2693() {
        let diagnostics = check_source_diagnostics("const a = number;");
        assert!(
            diagnostics.iter().any(|d| d.code == 2693),
            "Expected TS2693 for 'number' used as value, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }

    /// TS2749 in type-literal context: value symbol in type position
    #[test]
    fn value_used_as_type_in_type_literal_emits_ts2749() {
        let diagnostics = check_source_diagnostics(
            r#"
function myFunc() {}
type T = { x: myFunc };
"#,
        );
        assert!(
            diagnostics.iter().any(|d| d.code == 2749),
            "Expected TS2749 for function used as type, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }

    /// TS2693 in new expression: type used with `new`
    #[test]
    fn type_in_new_expression_emits_ts2693() {
        let diagnostics = check_source_diagnostics(
            r#"
interface Foo { x: number; }
const a = new Foo();
"#,
        );
        assert!(
            diagnostics.iter().any(|d| d.code == 2693),
            "Expected TS2693 for interface in new expression, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }

    /// TS2693 in assignment: type used in assignment target
    #[test]
    fn type_in_assignment_emits_ts2693() {
        let diagnostics = check_source_diagnostics(
            r#"
interface Foo { x: number; }
Foo = 42;
"#,
        );
        assert!(
            diagnostics.iter().any(|d| d.code == 2693),
            "Expected TS2693 for type in assignment, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }

    /// TS2304 in type position: unknown name in type reference
    #[test]
    fn unknown_name_in_type_position_emits_ts2304() {
        let diagnostics = check_source_diagnostics("let x: NonExistentType;");
        assert!(
            diagnostics.iter().any(|d| d.code == 2304),
            "Expected TS2304 for unknown type name, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }

    /// TS2708: namespace in extends clause — routed through boundary
    #[test]
    fn namespace_in_extends_clause_emits_ts2708() {
        let diagnostics = check_source_diagnostics(
            r#"
namespace NS {
    export interface I {}
}
class C extends NS {}
"#,
        );
        assert!(
            diagnostics.iter().any(|d| d.code == 2708),
            "Expected TS2708 for namespace in extends, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }
}
