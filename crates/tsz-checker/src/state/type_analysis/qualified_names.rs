//! Qualified-name type analysis and namespace/module member resolution.

use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use rustc_hash::FxHashSet;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Resolve a qualified name (A.B.C) to its type.
    ///
    /// This function handles qualified type names like `Namespace.SubType`, `Module.Interface`,
    /// or deeply nested names like `A.B.C`. It resolves each segment and looks up the final member.
    ///
    /// ## Resolution Strategy:
    /// 1. **Recursively resolve left side**: For `A.B.C`, first resolve `A.B`
    /// 2. **Get member type**: Look up rightmost member in left type's exports
    /// 3. **Handle symbol merging**: Supports merged class+namespace, enum+namespace, etc.
    ///
    /// ## Qualified Name Forms:
    /// - `Module.Type` - Type from module
    /// - `Namespace.Interface` - Interface from namespace
    /// - `A.B.C` - Deeply nested qualified name
    /// - `Class.StaticMember` - Static class member
    ///
    /// ## Symbol Resolution:
    /// - Checks exports of left side's symbol
    /// - Handles merged symbols (class+namespace, function+namespace)
    /// - Falls back to property access if not found in exports
    ///
    /// ## Error Reporting:
    /// - TS2694: Namespace has no exported member
    /// - Returns ERROR type if resolution fails
    ///
    /// ## Lib Binders:
    /// - Collects lib binders for cross-arena symbol lookup
    /// - Fixes TS2694 false positives for lib.d.ts types
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// // Module members
    /// namespace Utils {
    ///   export interface Helper {}
    /// }
    /// let h: Utils.Helper;  // resolve_qualified_name("Utils.Helper")
    ///
    /// // Deep nesting
    /// namespace A {
    ///   export namespace B {
    ///     export interface C {}
    ///   }
    /// }
    /// let x: A.B.C;  // resolve_qualified_name("A.B.C")
    ///
    /// // Static class members
    /// class Container {
    ///   static class Inner {}
    /// }
    /// let y: Container.Inner;  // resolve_qualified_name("Container.Inner")
    ///
    /// // Merged symbols
    /// function Model() {}
    /// namespace Model {
    ///   export interface Options {}
    /// }
    /// let opts: Model.Options;  // resolve_qualified_name("Model.Options")
    /// ```
    pub(crate) fn resolve_qualified_name(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(qn) = self.ctx.arena.get_qualified_name(node) else {
            return TypeId::ERROR; // Missing qualified name data - propagate error
        };

        let right_name = if let Some(right_node) = self.ctx.arena.get(qn.right) {
            if let Some(id) = self.ctx.arena.get_identifier(right_node) {
                id.escaped_text.to_string()
            } else {
                return TypeId::ERROR; // Missing identifier data - propagate error
            }
        } else {
            return TypeId::ERROR; // Missing right node - propagate error
        };

        // Resolve the left side (could be Identifier or another QualifiedName)
        let left_type = if let Some(left_node) = self.ctx.arena.get(qn.left) {
            let left_name = self.entity_name_text(qn.left).unwrap_or_default();

            let sym_res = if left_node.kind == syntax_kind_ext::QUALIFIED_NAME {
                self.resolve_qualified_symbol_in_type_position(qn.left)
            } else if left_node.kind == SyntaxKind::Identifier as u16 {
                let mut initial = self
                    .resolve_identifier_symbol_as_qualified_type_anchor(qn.left)
                    .map(TypeSymbolResolution::Type)
                    .unwrap_or_else(|| self.resolve_identifier_symbol_in_type_position(qn.left));

                // UMD `export as namespace Foo` makes Foo available as both a
                // value and a type-position namespace anchor. The type-position
                // resolver doesn't see UMD globals, so a bare `Foo.Thing`
                // reference would otherwise emit TS2503 ("Cannot find namespace")
                // even though the runtime/value side resolves fine. Promote to
                // Type(umd_sym_id) so the existing namespace-member machinery
                // downstream emits the correct TS2694 ("no exported member")
                // for missing members.
                if matches!(
                    initial,
                    TypeSymbolResolution::ValueOnly(_) | TypeSymbolResolution::NotFound
                ) && !left_name.is_empty()
                    && let Some(umd_sym_id) =
                        self.resolve_umd_global_symbol_by_name(left_name.as_str())
                {
                    initial = TypeSymbolResolution::Type(umd_sym_id);
                }

                // When the left side of a qualified name resolves to a type parameter,
                // it cannot serve as a namespace (no exports). In tsc, type parameters
                // do NOT shadow namespace imports in qualified name positions like
                // `E.Whatever` — the import `* as E` takes precedence over the type
                // parameter `E`.  Fall back to file_locals lookup which finds imports
                // and top-level declarations without type parameter shadowing.
                let lib_binders = self.get_lib_binders();
                if let TypeSymbolResolution::Type(sym_id) = initial {
                    if self
                        .ctx
                        .binder
                        .get_symbol_with_libs(sym_id, &lib_binders)
                        .is_some_and(|s| s.has_any_flags(symbol_flags::TYPE_PARAMETER))
                    {
                        if let Some(file_sym) = self.ctx.binder.file_locals.get(&left_name) {
                            let mut visited = AliasCycleTracker::new();
                            let resolved = self
                                .resolve_alias_symbol(file_sym, &mut visited)
                                .unwrap_or(file_sym);
                            if self
                                .ctx
                                .binder
                                .get_symbol_with_libs(resolved, &lib_binders)
                                .is_none_or(|s| !s.has_any_flags(symbol_flags::TYPE_PARAMETER))
                            {
                                TypeSymbolResolution::Type(resolved)
                            } else {
                                initial
                            }
                        } else {
                            initial
                        }
                    } else {
                        initial
                    }
                } else {
                    initial
                }
            } else {
                TypeSymbolResolution::NotFound
            };

            match sym_res {
                TypeSymbolResolution::Type(sym_id) => {
                    let lib_binders = self.get_lib_binders();
                    // Prefer the cross-file-registered owner when `sym_id` was
                    // pinned to a foreign file by an earlier resolution step
                    // (e.g. a cross-file alias target): per-file binders mint
                    // raw `SymbolId`s from zero, so reading `sym_id` from the
                    // *current* file's binder can silently collide with an
                    // unrelated local declaration at the same numeric id —
                    // see #16465. A `sym_id` never registered cross-file (the
                    // common, purely-local case) falls back to the plain
                    // local/lib read unchanged.
                    let symbol_lookup = self
                        .get_symbol_from_registered_file_target(sym_id)
                        .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders));
                    if let Some(symbol) = symbol_lookup {
                        // tsc's `SymbolFlags.Namespace` is `ValueModule |
                        // NamespaceModule | Enum` — a *class* is not in it. A
                        // class declaration alone therefore has no namespace
                        // meaning, and `C.m` in type position is
                        // TS2713/TS2702, not a member lookup. A class merged
                        // with a namespace declaration carries the module flags
                        // in the merged symbol, so the merge keeps working
                        // through those flags rather than through `CLASS`.
                        let valid_namespace_flags = symbol_flags::MODULE
                            | symbol_flags::NAMESPACE_MODULE
                            | symbol_flags::VALUE_MODULE
                            | symbol_flags::ENUM
                            | symbol_flags::REGULAR_ENUM
                            | symbol_flags::CONST_ENUM
                            | symbol_flags::ENUM_MEMBER;

                        // Skip TS2713/TS2702 for ALIAS symbols (imports) - a
                        // namespace import (`import * as X`) may target a
                        // namespace in another file with no single member to
                        // resolve here. Also skip when the resolved export has
                        // an alias_partner (TYPE_ALIAS+ALIAS merge), as the
                        // partner provides namespace access. Skip when parse
                        // errors exist, as the qualified name may be malformed.
                        let is_alias = symbol.has_any_flags(symbol_flags::ALIAS);
                        // A *named* or *default* import has one specific
                        // target, and it is the target's own namespace
                        // meaning — not "is this local binding an alias" —
                        // that decides TS2702/TS2713. `sym_id`/`symbol` above
                        // may already be misread: per-file binders mint raw
                        // `SymbolId`s from zero, so a cross-file-resolved id
                        // can collide with an unrelated local declaration when
                        // read back from the *current* file's binder (`self.
                        // ctx.binder`) — see #16465. Re-derive the alias from
                        // `name`'s own local binding in the current file (a
                        // same-file, unambiguous lookup) and resolve it
                        // through the owner-carrying cross-file path, which
                        // reads the target from *its own* declaring binder.
                        // Returns `None` for a namespace/require import
                        // (`import * as X`, no single named target) or when
                        // `name` isn't a local import alias at all — callers
                        // fall back to their own, more conservative answer.
                        // `export default <expr>` always synthesizes a fresh
                        // "default" symbol with hardcoded `ALIAS` flags — see
                        // `bind_export_declaration` in
                        // `crates/tsz-binder/src/modules/import_export.rs` —
                        // regardless of what the clause actually denotes.
                        // When the clause is an inline declaration (`export
                        // default class Decl {}`), that wrapper's meaning IS
                        // the declaration's own kind, and a class merged with
                        // a same-named namespace elsewhere in the file still
                        // reports TS2702 through `default` (tsc only folds the
                        // merge into the *named* binding, never the synthetic
                        // default slot) — see
                        // `default_imported_class_used_as_namespace_reports_type_used_as_namespace`.
                        // But when the clause is a bare identifier reference
                        // (`export default m;`, `m` declared elsewhere in the
                        // same file), the wrapper is a true alias and tsc asks
                        // the referenced declaration for its own (possibly
                        // merged) namespace meaning — #16486. Detect that case
                        // by reading the wrapper's own `value_declaration`
                        // from its own file's arena: an `Identifier` node
                        // means "alias to a same-file declaration", so resolve
                        // that identifier one hop further in the *target's own*
                        // file scope before giving up on it.
                        //
                        // One exception, oracle-verified against
                        // `typescript@7.0.2`: a resolved `Class` bit overrides
                        // an accompanying `NamespaceModule`/`Enum` bit here
                        // specifically. `class C {} namespace C {...}` keeps
                        // namespace meaning through this same bare-identifier
                        // hop when accessed via a *named* import or same-file
                        // reference (unaffected, already covered above and
                        // by `class_merged_with_namespace_resolves_its_exported_member`),
                        // but tsc's *default*-export slot still denies it
                        // (`TS2702`) for the identical merge even when the
                        // default clause is a bare reference to it rather
                        // than the inline declaration this hop already
                        // excludes — the merge with a `function` does not
                        // lose it the same way, so the override is keyed on
                        // `Class` specifically, not "any merge."
                        let default_export_alias_target_has_namespace_meaning =
                            |target_id: SymbolId, target: &tsz_binder::Symbol| -> Option<bool> {
                                if !target.has_any_flags(symbol_flags::ALIAS)
                                    || target.import_module().is_some()
                                {
                                    return None;
                                }
                                let file_idx = self.ctx.resolve_symbol_file_index(target_id)?;
                                let decl_node = self
                                    .ctx
                                    .get_arena_for_file(file_idx as u32)
                                    .get(target.value_declaration)?;
                                if decl_node.kind != SyntaxKind::Identifier as u16 {
                                    return None;
                                }
                                let ref_name = self
                                    .ctx
                                    .get_arena_for_file(file_idx as u32)
                                    .get_identifier(decl_node)?
                                    .escaped_text
                                    .as_str();
                                let target_binder = self.ctx.get_binder_for_file(file_idx)?;
                                let ref_id = target_binder.file_locals.get(ref_name)?;
                                let ref_symbol = target_binder.get_symbol(ref_id)?;
                                Some(
                                    !ref_symbol.has_any_flags(symbol_flags::CLASS)
                                        && ref_symbol.has_any_flags(valid_namespace_flags),
                                )
                            };
                        let resolve_import_target_has_namespace_meaning =
                            |name: &str| -> Option<bool> {
                                let local_id = self.ctx.binder.file_locals.get(name)?;
                                let local = self.ctx.binder.get_symbol(local_id)?;
                                if !local.has_any_flags(symbol_flags::ALIAS)
                                    || local.import_name().is_none()
                                {
                                    return None;
                                }
                                let target_id =
                                    self.ctx.resolve_import_alias_and_register(local_id)?;
                                let target =
                                    self.get_symbol_from_registered_file_target(target_id)?;
                                if target.has_any_flags(valid_namespace_flags) {
                                    return Some(true);
                                }
                                // A namespace-style alias (`export * as NS` /
                                // `import * as NS`) is a namespace anchor even
                                // without a `NAMESPACE_MODULE` flag: its members
                                // are the re-exported module's exports. A named
                                // import that reaches such an alias carries
                                // namespace meaning, so `NS.Member` resolves
                                // through it and a *missing* member is TS2694,
                                // not TS2702. (#17197)
                                if target.is_namespace_style_alias() {
                                    return Some(true);
                                }
                                if let Some(has_meaning) =
                                    default_export_alias_target_has_namespace_meaning(
                                        target_id, target,
                                    )
                                {
                                    return Some(has_meaning);
                                }
                                Some(false)
                            };
                        let alias_target_lacks_namespace_meaning = is_alias
                            && resolve_import_target_has_namespace_meaning(&left_name)
                                == Some(false);
                        let has_alias_partner =
                            self.ctx.alias_partners_contains(self.ctx.binder, sym_id)
                                || self.ctx.binder.resolve_import_symbol(sym_id).is_some_and(
                                    |resolved| {
                                        self.ctx.alias_partners_contains(self.ctx.binder, resolved)
                                    },
                                );
                        // tsc resolves the left name with `SymbolFlags.Namespace`
                        // as the requested meaning, so a nearer declaration that
                        // has no namespace meaning does not shadow an outer one
                        // that does — `var x = class C { prop: C.type }` finds the
                        // outer `namespace C`, not the class expression's own
                        // name. tsz resolves the name first and filters by meaning
                        // second, so re-ask the file scope for a namespace-meaning
                        // declaration before treating the qualifier as unusable.
                        // Same fallback shape as the type-parameter case above.
                        // When the outer declaration is itself a named/default
                        // import alias, ask its resolved target for namespace
                        // meaning (same rule as above) rather than assuming
                        // every ALIAS qualifies; fall back to the old
                        // conservative `true` when that can't be determined
                        // (a namespace import, or resolution failure).
                        let outer_namespace_meaning_exists = self
                            .ctx
                            .binder
                            .file_locals
                            .get(&left_name)
                            .map(|file_sym| {
                                let mut visited = AliasCycleTracker::new();
                                self.resolve_alias_symbol(file_sym, &mut visited)
                                    .unwrap_or(file_sym)
                            })
                            .and_then(|resolved| {
                                let lib_binders = self.get_lib_binders();
                                self.ctx
                                    .binder
                                    .get_symbol_with_libs(resolved, &lib_binders)
                                    .map(|outer| {
                                        outer.has_any_flags(valid_namespace_flags)
                                            || (outer.has_any_flags(symbol_flags::ALIAS)
                                                && resolve_import_target_has_namespace_meaning(
                                                    &left_name,
                                                )
                                                .unwrap_or(true))
                                    })
                            })
                            .unwrap_or(false);
                        if !symbol.has_any_flags(valid_namespace_flags)
                            && (!is_alias || alias_target_lacks_namespace_meaning)
                            && !has_alias_partner
                            && !outer_namespace_meaning_exists
                            && !self.ctx.has_parse_errors
                        {
                            let right_name = if let Some(right_node) = self.ctx.arena.get(qn.right)
                                && let Some(id) = self.ctx.arena.get_identifier(right_node)
                            {
                                id.escaped_text.to_string()
                            } else {
                                String::new()
                            };

                            // Get rightmost name of the left side
                            let left_rightmost_name = if left_node.kind
                                == syntax_kind_ext::QUALIFIED_NAME
                            {
                                if let Some(left_qn) = self.ctx.arena.get_qualified_name(left_node)
                                {
                                    if let Some(rn) = self.ctx.arena.get(left_qn.right)
                                        && let Some(id) = self.ctx.arena.get_identifier(rn)
                                    {
                                        id.escaped_text.to_string()
                                    } else {
                                        left_name.clone()
                                    }
                                } else {
                                    left_name.clone()
                                }
                            } else {
                                left_name.clone()
                            };

                            // Determine whether to emit TS2713 or TS2702.
                            // TS2713: the property exists on the type — suggest indexed access.
                            // TS2702: the property does NOT exist — generic "used as namespace" error.
                            //
                            // For type parameters, get_type_of_symbol may not return the
                            // TypeParameter type. Check the type_parameter_scope first.
                            //
                            // The question this branch asks is the one tsc asks in
                            // `getTypeFromTypeReference`: does the type *the left name
                            // denotes in type space* have this property? For a class
                            // that is the instance side, so the probe resolves the
                            // type-reference type rather than the symbol's value type
                            // — `class C { m: number }` makes `C.m` a TS2713
                            // indexed-access suggestion, while a static-only member
                            // stays TS2702 because the instance side does not carry it.
                            // The type-reference type of an interface is a
                            // `Lazy(DefId)` semantic ref, which carries no member
                            // surface of its own; evaluate before asking.
                            let left_type_id = self
                                .ctx
                                .type_parameter_scope
                                .get(&left_name)
                                .copied()
                                .unwrap_or_else(|| {
                                    let referenced = self.type_reference_symbol_type(sym_id);
                                    self.evaluate_type_with_env(referenced)
                                });
                            let prop_exists =
                                crate::query_boundaries::property_access::type_has_property(
                                    self.ctx.types,
                                    left_type_id,
                                    self.ctx.types.intern_string(&right_name),
                                );

                            use crate::diagnostics::diagnostic_codes;
                            if prop_exists {
                                self.error_at_node_msg(
                                    idx, // The entire qualified name node
                                    diagnostic_codes::CANNOT_ACCESS_BECAUSE_IS_A_TYPE_BUT_NOT_A_NAMESPACE_DID_YOU_MEAN_TO_RETRIEVE_THE,
                                    &[left_rightmost_name.as_str(), right_name.as_str()],
                                );
                            } else {
                                self.error_type_used_as_namespace_at(&left_rightmost_name, qn.left);
                            }
                            return TypeId::ERROR;
                        }
                    }
                    self.type_reference_symbol_type(sym_id)
                }
                TypeSymbolResolution::ValueOnly(_) | TypeSymbolResolution::NotFound => {
                    if left_node.kind == syntax_kind_ext::QUALIFIED_NAME {
                        self.resolve_qualified_name(qn.left)
                    } else if left_node.kind == SyntaxKind::Identifier as u16 {
                        // globalThis is a synthetic namespace in TSC (flags = ValueModule | NamespaceModule)
                        // with exports pointing to the global scope. Its members
                        // are the ambient global types, so `globalThis.X` in type
                        // position resolves `X` to the global type of that name
                        // (e.g. `globalThis.RegExp` -> the global `RegExp`
                        // interface). The left anchor has no user symbol to
                        // navigate, so it lands here as NotFound; recover the
                        // member from the global type table instead of collapsing
                        // to ERROR (which silently dropped the qualified name and
                        // could collapse, e.g., a type-predicate false branch).
                        // Guarded by `is_global_this_expression` so a same-file
                        // local named `globalThis` is not treated as the global.
                        if left_name == "globalThis" {
                            if self.is_global_this_expression(qn.left) {
                                if let Some(member_sym) =
                                    self.resolve_global_this_type_member_symbol(&right_name)
                                {
                                    if self.member_symbol_is_value_only_in_type_position(
                                        member_sym,
                                        &right_name,
                                    ) {
                                        self.report_wrong_meaning(
                                            &right_name,
                                            idx,
                                            member_sym,
                                            crate::query_boundaries::name_resolution::NameLookupKind::Value,
                                            crate::query_boundaries::name_resolution::NameLookupKind::Type,
                                        );
                                        return TypeId::ERROR;
                                    }
                                    return self.type_reference_symbol_type(member_sym);
                                }
                                // Negative control: `globalThis.NotAType` — tsc
                                // reports TS2694 ("Namespace 'globalThis' has no
                                // exported member 'X'").
                                if !self.ctx.has_parse_errors {
                                    self.error_namespace_no_export(
                                        "globalThis",
                                        &right_name,
                                        qn.right,
                                    );
                                }
                            }
                            return TypeId::ERROR;
                        }
                        if !self.is_unresolved_import_symbol(qn.left) && !left_name.is_empty() {
                            // TS1212/TS1213/TS1214: when the lead identifier of a
                            // qualified type name is a strict-mode reserved word
                            // (`public.bar`, `private.x`, `interface.B`, ...) and we
                            // are in strict mode, tsc emits the reserved-word error
                            // alongside TS2503. The general TypeReference path covers
                            // bare identifiers; qualified-name resolution lands here
                            // and was missing the check.
                            if crate::state_checking::is_strict_mode_reserved_name(
                                left_name.as_str(),
                            ) && self.is_strict_mode_for_node(qn.left)
                            {
                                self.emit_strict_mode_reserved_word_error_with_ast_walk(
                                    qn.left,
                                    left_name.as_str(),
                                );
                            }
                            // Route through boundary for TS2503/TS2552 with suggestions
                            let req = crate::query_boundaries::name_resolution::NameResolutionRequest::namespace(
                                left_name.as_str(),
                                qn.left,
                            );
                            match self.resolve_name_structured(&req) {
                                Err(failure) => {
                                    // The boundary precomputes suggestion eligibility for true
                                    // not-found failures. Wrong-meaning namespace anchors (for
                                    // example a value-only `m` near namespace `M`) still need the
                                    // namespace diagnostic owner to apply suppression eligibility
                                    // at emission time.
                                    if matches!(
                                        &failure.kind,
                                        crate::query_boundaries::name_resolution::ResolutionFailureKind::NotFound
                                    ) {
                                        self.error_cannot_find_namespace_with_precomputed_eligibility(
                                            left_name.as_str(),
                                            qn.left,
                                            failure.suggestions_eligible,
                                        );
                                    } else {
                                        self.error_cannot_find_namespace_with_suggestion(
                                            left_name.as_str(),
                                            qn.left,
                                        );
                                    }
                                }
                                Ok(_) => {
                                    // Shouldn't happen since resolve_qualified_symbol_in_type_position
                                    // already failed, but avoid false diagnostic
                                }
                            }
                        }
                        TypeId::ERROR
                    } else {
                        TypeId::ERROR
                    }
                }
            }
        } else {
            TypeId::ERROR // Missing left node - propagate error
        };

        if left_type == TypeId::ANY || left_type == TypeId::ERROR {
            return TypeId::ERROR; // Propagate error from left side
        }

        // Collect lib binders for cross-arena symbol lookup (fixes TS2694 false positives)
        let lib_binders = self.get_lib_binders();

        // First, try to resolve the left side as a symbol and check its exports.
        // This handles merged class+namespace, function+namespace, and enum+namespace symbols.
        let mut left_sym_for_missing = None;
        let mut left_module_specifier: Option<String> = None;
        let member_sym_id_from_symbol = if let Some(left_node) = self.ctx.arena.get(qn.left)
            && left_node.kind == SyntaxKind::Identifier as u16
        {
            if let Some(sym_id) = self.resolve_identifier_symbol_as_qualified_type_anchor(qn.left) {
                let anchor_symbol = self.qualified_anchor_symbol(sym_id, &lib_binders);
                if let Some(symbol) = anchor_symbol.as_deref() {
                    left_sym_for_missing = Some(sym_id);
                    left_module_specifier = symbol.import_module().map(str::to_string);
                    let mut result = self.resolve_symbol_export_for(
                        Some(sym_id),
                        symbol,
                        &right_name,
                        &lib_binders,
                    );
                    // TYPE_ALIAS+ALIAS merge: look up alias_partner and
                    // resolve the member through the ALIAS symbol's namespace
                    if result.is_none() {
                        let alias_id =
                            self.ctx
                                .alias_partner_for(self.ctx.binder, sym_id)
                                .or_else(|| {
                                    let resolved = self.ctx.binder.resolve_import_symbol(sym_id)?;
                                    self.ctx.alias_partner_for(self.ctx.binder, resolved)
                                });
                        if let Some(alias_id) = alias_id
                            && let Some(alias_sym) =
                                self.ctx.binder.get_symbol_with_libs(alias_id, &lib_binders)
                        {
                            result = alias_sym
                                .exports
                                .as_ref()
                                .and_then(|exports| exports.get(&right_name));
                            if result.is_none()
                                && let Some(module) = alias_sym.import_module()
                            {
                                // Resolve from ALIAS's source file, then
                                // fall back to current-file resolution.
                                result = self
                                    .ctx
                                    .resolve_alias_import_member(alias_id, module, &right_name)
                                    .or_else(|| {
                                        self.resolve_effective_module_exports(module)
                                            .and_then(|e| e.get(&right_name))
                                    })
                                    .or_else(|| {
                                        self.resolve_member_through_namespace_import_chain(
                                            alias_sym.import_module(),
                                            alias_sym.import_name(),
                                            alias_sym.escaped_name.as_str(),
                                            &right_name,
                                        )
                                    });
                            }
                        }
                    }
                    if result.is_none() && self.entity_name_text(qn.left).as_deref() == Some("JSX")
                    {
                        result = self.resolve_jsx_namespace_merged_export_symbol_id(&right_name);
                    }
                    if result.is_none() {
                        result = self.resolve_named_class_expression_namespace_member(
                            qn.left,
                            sym_id,
                            &right_name,
                        );
                    }
                    result
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            // When the left side is a QualifiedName (e.g., `ns.Root` in `ns.Root.Foo`),
            // extract the module specifier from the root identifier of the chain so that
            // module augmentation merging can be applied to nested members.
            if left_module_specifier.is_none()
                && let Some(left_node) = self.ctx.arena.get(qn.left)
                && left_node.kind == syntax_kind_ext::QUALIFIED_NAME
            {
                left_module_specifier = self.extract_root_module_specifier(qn.left, &lib_binders);
            }
            None
        };

        // If found via symbol resolution, use it
        if let Some(member_sym_id) = member_sym_id_from_symbol {
            if (self.alias_resolves_to_value_only(member_sym_id, Some(right_name.as_str()))
                || self.symbol_is_value_only(member_sym_id, Some(right_name.as_str())))
                && !self.symbol_is_type_only(member_sym_id, Some(right_name.as_str()))
            {
                let full_name = self
                    .entity_name_text(idx)
                    .unwrap_or_else(|| right_name.clone());
                self.report_wrong_meaning(
                    &full_name,
                    idx,
                    member_sym_id,
                    crate::query_boundaries::name_resolution::NameLookupKind::Value,
                    crate::query_boundaries::name_resolution::NameLookupKind::Type,
                );
                return TypeId::ERROR;
            }
            let mut member_type = self.type_reference_symbol_type(member_sym_id);
            if let Some(module_specifier) = left_module_specifier.as_deref() {
                member_type =
                    self.apply_module_augmentations(module_specifier, &right_name, member_type);
            }
            return member_type;
        }

        // Read the missing-member anchor from its owning binder (owned when
        // cross-file) so its reference survives the `&mut self` calls below —
        // same rule as the member-lookup site above (#16465/#16503).
        let missing_owner = left_sym_for_missing
            .and_then(|left_sym_id| self.qualified_anchor_symbol(left_sym_id, &lib_binders));
        if let Some(left_sym_id) = left_sym_for_missing
            && let Some(symbol) = missing_owner.as_deref()
            && (symbol.flags
                & (symbol_flags::MODULE
                    | symbol_flags::CLASS
                    | symbol_flags::REGULAR_ENUM
                    | symbol_flags::CONST_ENUM
                    | symbol_flags::INTERFACE)
                != 0
                // UMD `export as namespace Foo` produces an ALIAS symbol whose
                // import_module points at the source file. tsc treats Foo as a
                // namespace anchor in type position, so a missing member emits
                // TS2694 ("no exported member") rather than TS2503 ("cannot
                // find namespace"). Accept the UMD alias here so the
                // member-resolution failure below routes through the TS2694
                // branch.
                || symbol.is_umd_export
                // A named import bound to an `export * as NS` re-export is a
                // type-position namespace anchor too: a missing member is
                // TS2694, not TS2503.
                || self
                    .ctx
                    .namespace_reexport_anchor_backing_file(left_sym_id)
                    .is_some())
        {
            // If the left symbol is a pure interface (no namespace meaning) and a
            // local declaration shadows an outer namespace, the member might exist
            // on the outer namespace. In tsc, import-equals and qualified type names
            // prefer namespace meaning, so a local interface shouldn't cause TS2694
            // when an outer namespace with the same name has the member.
            let is_pure_interface = symbol.has_any_flags(symbol_flags::INTERFACE)
                && !symbol.has_any_flags(symbol_flags::MODULE)
                && !symbol.has_any_flags(symbol_flags::CLASS)
                && !symbol.has_any_flags(symbol_flags::REGULAR_ENUM)
                && !symbol.has_any_flags(symbol_flags::CONST_ENUM);
            if is_pure_interface {
                // Check if an outer namespace with this name has the member
                let left_name_str = self
                    .entity_name_text(qn.left)
                    .unwrap_or_else(|| symbol.escaped_name.clone());
                if self
                    .resolve_outer_namespace_member(qn.left, &left_name_str, &right_name)
                    .is_some()
                {
                    // The member exists on an outer namespace — don't emit TS2694.
                    // Return ERROR type since we can't resolve through the local interface,
                    // but avoid the misleading diagnostic.
                    return TypeId::ERROR;
                }
            }

            let export_names: Vec<String> = symbol
                .exports
                .as_ref()
                .map(|e| e.iter().map(|(name, _)| name.clone()).collect())
                .unwrap_or_default();
            let req =
                crate::query_boundaries::name_resolution::NameResolutionRequest::exported_member(
                    &right_name,
                    qn.right,
                    left_sym_id,
                    export_names,
                );
            let failure = match self.resolve_name_structured(&req) {
                Err(f) => f,
                Ok(_) => {
                    // Shouldn't happen since we already failed above, but be safe
                    return TypeId::ERROR;
                }
            };
            self.report_name_resolution_failure(&req, &failure);
            return TypeId::ERROR;
        }

        // Otherwise, fall back to type-based lookup for pure namespace/module types
        // Look up the member in the left side's exports
        // Supports both Lazy(DefId) and Enum types
        let fallback_sym_id = self.ctx.resolve_type_to_symbol_id(left_type);

        if let Some(fallback_sym) = fallback_sym_id
            && let Some(symbol) = self
                .ctx
                .binder
                .get_symbol_with_libs(fallback_sym, &lib_binders)
        {
            // Use the helper to resolve the member from exports, members, or re-exports
            if let Some(member_sym_id) = self.resolve_symbol_export_for(
                Some(fallback_sym),
                symbol,
                &right_name,
                &lib_binders,
            ) {
                // Check value-only, but skip for namespaces since they can be used
                // to navigate to types (e.g., Outer.Inner.Type)
                if let Some(member_symbol) = self
                    .ctx
                    .binder
                    .get_symbol_with_libs(member_sym_id, &lib_binders)
                {
                    let is_namespace = member_symbol.has_any_flags(symbol_flags::MODULE);
                    if !is_namespace
                        && (self
                            .alias_resolves_to_value_only(member_sym_id, Some(right_name.as_str()))
                            || self.symbol_is_value_only(member_sym_id, Some(right_name.as_str())))
                        && !self.symbol_is_type_only(member_sym_id, Some(right_name.as_str()))
                    {
                        let full_name = self
                            .entity_name_text(idx)
                            .unwrap_or_else(|| right_name.clone());
                        self.report_wrong_meaning(
                            &full_name,
                            idx,
                            member_sym_id,
                            crate::query_boundaries::name_resolution::NameLookupKind::Value,
                            crate::query_boundaries::name_resolution::NameLookupKind::Type,
                        );
                        return TypeId::ERROR;
                    }
                }
                let mut member_type = self.type_reference_symbol_type(member_sym_id);
                if let Some(module_specifier) = left_module_specifier.as_deref() {
                    member_type =
                        self.apply_module_augmentations(module_specifier, &right_name, member_type);
                }
                return member_type;
            }

            // If the symbol is an enum member, it cannot have exports.
            // Emit TS2749 (value used as type) instead of TS2694.
            if symbol.has_any_flags(symbol_flags::ENUM_MEMBER) {
                let full_name = self
                    .entity_name_text(idx)
                    .unwrap_or_else(|| right_name.clone());
                self.report_wrong_meaning(
                    &full_name,
                    idx,
                    fallback_sym,
                    crate::query_boundaries::name_resolution::NameLookupKind::Value,
                    crate::query_boundaries::name_resolution::NameLookupKind::Type,
                );
                return TypeId::ERROR;
            }

            // Not found - report TS2694 or TS2724 (with spelling suggestion)
            let export_names: Vec<String> = symbol
                .exports
                .as_ref()
                .map(|e| e.iter().map(|(name, _)| name.clone()).collect())
                .unwrap_or_default();
            let req =
                crate::query_boundaries::name_resolution::NameResolutionRequest::exported_member(
                    &right_name,
                    qn.right,
                    fallback_sym,
                    export_names,
                );
            let failure = match self.resolve_name_structured(&req) {
                Err(f) => f,
                Ok(_) => {
                    return TypeId::ERROR;
                }
            };
            self.report_name_resolution_failure(&req, &failure);
            return TypeId::ERROR;
        }

        // Left side wasn't a reference to a namespace/module
        // This is likely an error - the left side should resolve to a namespace
        // Emit an appropriate error for the unresolved qualified name
        // We don't emit TS2304 here because the left side might have already emitted an error
        // Returning ERROR prevents cascading errors while still indicating failure
        if let Some(left_node) = self.ctx.arena.get(qn.left)
            && left_node.kind == SyntaxKind::Identifier as u16
            && !self.is_unresolved_import_symbol(qn.left)
            && let Some(ident) = self.ctx.arena.get_identifier(left_node)
        {
            self.error_cannot_find_namespace_with_suggestion(ident.escaped_text.as_str(), qn.left);
        }
        TypeId::ERROR
    }

    /// Resolve `member_name` for a qualified-type-name anchor that is an import
    /// alias re-importing a namespace (`import * as Ns`), walking the import
    /// chain across module boundaries by `(file, name)` rather than by raw
    /// `SymbolId` (which is only unique within a single file's binder).
    ///
    /// `import { Ns } from "./reexporter"` where `./reexporter` has
    /// `import * as Ns from "./ns"; export { Ns }` makes `Ns` the namespace of
    /// `./ns`, so `Ns.Member` resolves `Member` from `./ns`'s exports even when a
    /// same-named local `type`/`interface` shadows `Ns` in type space. The walk
    /// is purely file-and-name based so it is immune to per-binder `SymbolId`
    /// collisions when the same numeric id denotes different symbols in two
    /// files.
    pub(crate) fn resolve_member_through_namespace_import_chain(
        &self,
        import_module: Option<&str>,
        import_name: Option<&str>,
        alias_escaped_name: &str,
        member_name: &str,
    ) -> Option<SymbolId> {
        // Bound the re-export walk; chains this long are pathological (mirrors
        // `namespace_anchor_alias_partner`'s guard).
        const MAX_REEXPORT_HOPS: usize = 32;

        let mut module = import_module?.to_string();
        let mut name = import_name.unwrap_or(alias_escaped_name).to_string();
        let mut cur_file = self.ctx.current_file_idx;
        let mut visited: FxHashSet<(usize, String)> = FxHashSet::default();
        for _ in 0..MAX_REEXPORT_HOPS {
            let target_file = self
                .ctx
                .resolve_import_target_from_file(cur_file, &module)?;
            let mut visited_exports = FxHashSet::default();
            if name == "*" {
                // The current binding is the whole namespace of `target_file`'s
                // module, so the member is a direct (re-)export of that module.
                return self.ctx.resolve_export_in_target_file(
                    target_file,
                    member_name,
                    &mut visited_exports,
                );
            }
            if !visited.insert((target_file, name.clone())) {
                return None;
            }
            let export_sym =
                self.ctx
                    .resolve_export_in_target_file(target_file, &name, &mut visited_exports)?;
            // Read the resolved export's import metadata from ITS OWN binder: the
            // symbol id is only meaningful within the file it was resolved in.
            let export_file = self
                .ctx
                .resolve_symbol_file_index(export_sym)
                .unwrap_or(target_file);
            let export_binder = self.ctx.get_binder_for_file(export_file)?;
            let export_symbol = export_binder.get_symbol(export_sym)?;
            // Only an import alias can carry the walk to a further module; a
            // concrete declaration is not a namespace anchor for `Member`.
            name = export_symbol
                .import_name()
                .unwrap_or(export_symbol.escaped_name.as_str())
                .to_string();
            module = export_symbol.import_module()?.to_string();
            cur_file = export_file;
        }
        None
    }

    /// Walk a qualified-name chain leftward to find the root identifier and return
    /// its `import_module` (module specifier), if any.  This is used to propagate
    /// module augmentation context through nested qualified names like `ns.Root.Foo`.
    pub(crate) fn extract_root_module_specifier(
        &self,
        mut idx: NodeIndex,
        lib_binders: &[std::sync::Arc<tsz_binder::BinderState>],
    ) -> Option<String> {
        loop {
            let node = self.ctx.arena.get(idx)?;
            if node.kind == syntax_kind_ext::QUALIFIED_NAME {
                let qn = self.ctx.arena.get_qualified_name(node)?;
                idx = qn.left;
            } else if node.kind == SyntaxKind::Identifier as u16 {
                let sym_id = self.resolve_identifier_symbol_as_qualified_type_anchor(idx)?;
                let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, lib_binders)?;
                return symbol.import_module().map(str::to_string);
            } else {
                return None;
            }
        }
    }

    /// When a named class expression shadows an outer namespace of the same name,
    /// qualified type names like `C.Member` still resolve through the namespace.
    fn resolve_named_class_expression_namespace_member(
        &self,
        node: NodeIndex,
        sym_id: SymbolId,
        member_name: &str,
    ) -> Option<SymbolId> {
        let lib_binders = self.get_lib_binders();
        let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)?;
        if !symbol.has_any_flags(symbol_flags::CLASS) {
            return None;
        }

        let decl_kind = self
            .ctx
            .arena
            .get(symbol.value_declaration)
            .map(|decl| decl.kind)?;
        if decl_kind != syntax_kind_ext::CLASS_EXPRESSION {
            return None;
        }

        self.resolve_outer_namespace_member(node, symbol.escaped_name.as_str(), member_name)
    }

    /// Resolve a member from an outer namespace with the same name.
    /// Used to avoid false TS2694 when a local declaration shadows an outer namespace.
    fn resolve_outer_namespace_member(
        &self,
        node: NodeIndex,
        namespace_name: &str,
        member_name: &str,
    ) -> Option<SymbolId> {
        let lib_binders = self.get_lib_binders();
        // Walk up scopes from the enclosing scope's parent
        let Some(scope_id) = self.ctx.binder.find_enclosing_scope(self.ctx.arena, node) else {
            return self.resolve_namespace_member_from_all_binders(namespace_name, member_name);
        };
        let Some(current_scope) = self.ctx.binder.scopes.get(scope_id.0 as usize) else {
            return self.resolve_namespace_member_from_all_binders(namespace_name, member_name);
        };
        let mut walk_id = current_scope.parent;

        while let Some(scope) = self.ctx.binder.scopes.get(walk_id.0 as usize) {
            if let Some(sym_id) = scope.table.get(namespace_name)
                && let Some(sym) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
                && sym.has_any_flags(symbol_flags::NAMESPACE)
            {
                // Found a namespace - check if it has the member
                if let Some(exports) = sym.exports.as_ref()
                    && let Some(member_id) = exports.get(member_name)
                {
                    return Some(member_id);
                }
            }
            if walk_id == scope.parent {
                break;
            }
            walk_id = scope.parent;
        }

        self.resolve_namespace_member_from_all_binders(namespace_name, member_name)
    }

    /// Read a qualified-name anchor `Symbol`, owning it only when it lives in
    /// another file.
    ///
    /// A cross-file-resolved `sym_id` (a re-anchored default-export
    /// namespace/enum, a named import's target) read bare from the *current*
    /// file's binder collides with an unrelated local at the same raw id —
    /// per-file binders mint `SymbolId`s from zero (#16465/#16503) — so it must
    /// come from its owning binder. That read borrows `self`, so it is cloned to
    /// outlive the later `&mut self` member-resolution calls. A purely-local
    /// anchor (the common case) takes the zero-clone `'a` borrow from the
    /// current binder instead of paying for a `Symbol` clone.
    fn qualified_anchor_symbol<'b>(
        &self,
        sym_id: SymbolId,
        lib_binders: &'b [std::sync::Arc<tsz_binder::BinderState>],
    ) -> Option<std::borrow::Cow<'b, tsz_binder::Symbol>>
    where
        'a: 'b,
    {
        if self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .is_some_and(|owner| owner != self.ctx.current_file_idx)
            && let Some(symbol) = self.get_symbol_from_registered_file_target(sym_id)
        {
            return Some(std::borrow::Cow::Owned(symbol.clone()));
        }
        self.ctx
            .binder
            .get_symbol_with_libs(sym_id, lib_binders)
            .map(std::borrow::Cow::Borrowed)
    }

    /// Resolve a member from a symbol's exports, members, or re-exports.
    ///
    /// This helper implements the common pattern of looking up a member in:
    /// 1. Direct exports
    /// 2. Members (for classes with static members)
    /// 3. Re-exports (for imported namespaces)
    ///
    /// Returns `Some(member_sym_id)` if found, `None` otherwise.
    fn resolve_symbol_export_for(
        &mut self,
        sym_id: Option<tsz_binder::SymbolId>,
        symbol: &tsz_binder::Symbol,
        member_name: &str,
        lib_binders: &[std::sync::Arc<tsz_binder::BinderState>],
    ) -> Option<tsz_binder::SymbolId> {
        // Try direct exports first
        if let Some(ref exports) = symbol.exports
            && let Some(member_id) = exports.get(member_name)
        {
            return Some(member_id);
        }

        // For classes, also check members (for static members in type queries)
        // This handles `typeof C.staticMember` where C is a class
        if symbol.has_any_flags(symbol_flags::CLASS)
            && let Some(ref members) = symbol.members
            && let Some(member_id) = members.get(member_name)
        {
            return Some(member_id);
        }

        if symbol.has_any_flags(symbol_flags::MODULE) {
            if let Some(member_id) =
                self.resolve_module_export_from_declarations(symbol, member_name)
            {
                return Some(member_id);
            }
            // Only the anonymous source-file module should see top-level file exports
            // through `file_locals`. Named namespaces/modules must resolve members
            // through their own declaration-local export tables; otherwise `X.bar`
            // can accidentally bind to an unrelated `export function bar()` from the
            // containing file and surface TS2749 instead of TS2694.
            let has_named_module_declaration = symbol.declarations.iter().any(|&decl_idx| {
                let Some(node) = self.ctx.arena.get(decl_idx) else {
                    return false;
                };
                if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                    return false;
                }
                self.ctx
                    .arena
                    .get_module(node)
                    .and_then(|module| self.ctx.arena.get(module.name))
                    .is_some_and(|name_node| name_node.kind == SyntaxKind::Identifier as u16)
            });
            if !has_named_module_declaration
                && let Some(local_sym_id) = self.ctx.binder.file_locals.get(member_name)
                && let Some(sym) = self.ctx.binder.get_symbol(local_sym_id)
                && sym.is_exported
            {
                return Some(local_sym_id);
            }
        }

        // If not found in direct exports, check for re-exports
        // The member might be re-exported from another module
        if let Some(module_specifier) = symbol.import_module() {
            if self
                .ctx
                .namespace_import_alias_has_local_namespace_conflict(symbol)
            {
                return None;
            }
            if symbol.has_any_flags(symbol_flags::ALIAS)
                && self
                    .ctx
                    .module_resolves_to_non_module_entity(module_specifier)
            {
                return None;
            }
            if let Some(reexported_sym_id) =
                self.resolve_reexported_member(module_specifier, member_name, lib_binders)
            {
                return Some(reexported_sym_id);
            }
            // Cross-file fallback: resolve the relative module specifier from
            // the ALIAS symbol's source file perspective.
            if let Some(alias_id) = sym_id
                && let Some(resolved) =
                    self.ctx
                        .resolve_alias_import_member(alias_id, module_specifier, member_name)
            {
                return Some(resolved);
            }
            // Named import bound to an `export * as NS` namespace re-export: the
            // member lives in the re-exported module, not in the importing
            // module's own export surface, so neither lookup above can find it.
            // Follow the named export to the namespace re-export and resolve the
            // member through its backing module.
            if let Some(alias_id) = sym_id
                && let Some(resolved) = self
                    .ctx
                    .resolve_member_via_namespace_reexport(alias_id, member_name)
            {
                return Some(resolved);
            }
        }

        None
    }

    fn resolve_module_export_from_declarations(
        &self,
        symbol: &tsz_binder::Symbol,
        member_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            let Some(module) = self.ctx.arena.get_module(node) else {
                continue;
            };
            if module.body.is_none() {
                continue;
            }
            if let Some(&scope_id) = self.ctx.binder.node_scope_ids.get(&module.body.0)
                && let Some(scope) = self.ctx.binder.scopes.get(scope_id.0 as usize)
                && let Some(sym_id) = scope.table.get(member_name)
                && let Some(sym) = self.ctx.binder.get_symbol(sym_id)
                && sym.is_exported
            {
                return Some(sym_id);
            }
            let Some(module_block) = self.ctx.arena.get_module_block_at(module.body) else {
                continue;
            };
            let Some(statements) = &module_block.statements else {
                continue;
            };

            for &stmt_idx in &statements.nodes {
                let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                    continue;
                };
                if (stmt_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || stmt_node.kind == syntax_kind_ext::INTERFACE_DECLARATION)
                    && let Some(name) = self.get_declaration_name_text(stmt_idx)
                    && name == member_name
                    && let Some(&sym_id) = self.ctx.binder.node_symbols.get(&stmt_idx.0)
                {
                    return Some(sym_id);
                }
                if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                    continue;
                }
                let Some(export_decl) = self.ctx.arena.get_export_decl(stmt_node) else {
                    continue;
                };
                if export_decl.export_clause.is_none() {
                    continue;
                }
                let Some(clause_node) = self.ctx.arena.get(export_decl.export_clause) else {
                    continue;
                };

                match clause_node.kind {
                    syntax_kind_ext::FUNCTION_DECLARATION
                    | syntax_kind_ext::CLASS_DECLARATION
                    | syntax_kind_ext::INTERFACE_DECLARATION
                    | syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    | syntax_kind_ext::ENUM_DECLARATION
                    | syntax_kind_ext::MODULE_DECLARATION => {
                        if let Some(name) =
                            self.get_declaration_name_text(export_decl.export_clause)
                            && name == member_name
                            && let Some(&sym_id) = self
                                .ctx
                                .binder
                                .node_symbols
                                .get(&export_decl.export_clause.0)
                        {
                            return Some(sym_id);
                        }
                    }
                    syntax_kind_ext::VARIABLE_STATEMENT => {
                        if let Some(var_stmt) = self.ctx.arena.get_variable(clause_node) {
                            // VariableStatement holds VariableDeclarationList nodes.
                            // Walk list -> declaration to recover exported namespace vars.
                            for &list_idx in &var_stmt.declarations.nodes {
                                let Some(list_node) = self.ctx.arena.get(list_idx) else {
                                    continue;
                                };
                                let Some(decl_list) = self.ctx.arena.get_variable(list_node) else {
                                    continue;
                                };
                                for &decl_idx in &decl_list.declarations.nodes {
                                    if let Some(name) = self.get_declaration_name_text(decl_idx)
                                        && name == member_name
                                        && let Some(&sym_id) =
                                            self.ctx.binder.node_symbols.get(&decl_idx.0)
                                    {
                                        return Some(sym_id);
                                    }
                                }
                            }
                        }
                    }
                    // Handle `export { A }` and `export { A as B }` — named export
                    // specifiers that re-export symbols from the enclosing scope.
                    syntax_kind_ext::NAMED_EXPORTS => {
                        let Some(named) = self.ctx.arena.get_named_imports(clause_node) else {
                            continue;
                        };
                        for &spec_idx in &named.elements.nodes {
                            let Some(spec_node) = self.ctx.arena.get(spec_idx) else {
                                continue;
                            };
                            let Some(spec) = self.ctx.arena.get_specifier(spec_node) else {
                                continue;
                            };
                            // The exported name: for `export { A }`, name is "A";
                            // for `export { A as B }`, name is "B".
                            let exported_name = self
                                .ctx
                                .arena
                                .get(spec.name)
                                .and_then(|n| self.ctx.arena.get_identifier(n))
                                .map(|id| id.escaped_text.as_str());
                            let Some(exported) = exported_name else {
                                continue;
                            };
                            if exported != member_name {
                                continue;
                            }
                            // The local name: for `export { A }`, local is "A";
                            // for `export { A as B }`, local is "A" (property_name).
                            let local_name_idx = if spec.property_name.is_some() {
                                spec.property_name
                            } else {
                                spec.name
                            };
                            let local_name = self
                                .ctx
                                .arena
                                .get(local_name_idx)
                                .and_then(|n| self.ctx.arena.get_identifier(n))
                                .map(|id| id.escaped_text.as_str());
                            let Some(local) = local_name else {
                                continue;
                            };
                            // Resolve the local name: try the module body scope first,
                            // then walk up to parent scopes and file locals.
                            if let Some(&scope_id) =
                                self.ctx.binder.node_scope_ids.get(&module.body.0)
                            {
                                // Check the module scope itself
                                if let Some(scope) = self.ctx.binder.scopes.get(scope_id.0 as usize)
                                    && let Some(sym_id) = scope.table.get(local)
                                {
                                    return Some(sym_id);
                                }
                                // Walk parent scopes
                                let mut current_scope_id = scope_id;
                                while let Some(scope) =
                                    self.ctx.binder.scopes.get(current_scope_id.0 as usize)
                                {
                                    if let Some(sym_id) = scope.table.get(local) {
                                        return Some(sym_id);
                                    }
                                    if current_scope_id == scope.parent {
                                        break; // root scope
                                    }
                                    current_scope_id = scope.parent;
                                }
                            }
                            // Fall back to file locals
                            if let Some(sym_id) = self.ctx.binder.file_locals.get(local) {
                                return Some(sym_id);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        None
    }
}
