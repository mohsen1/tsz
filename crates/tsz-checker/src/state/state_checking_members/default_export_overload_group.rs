//! Merged `default`-export function group checks (TS2391/TS2394).
//!
//! tsc binds every `export default function` declaration — whatever its local
//! name — to the single `default` export symbol, then runs
//! `checkFunctionOrConstructorSymbol` over that merged declaration list in
//! addition to the per-local-name runs. Two behaviors only exist in the merged
//! run:
//!
//! - anonymous default-exported signatures (no local name, so the name-keyed
//!   statement walk cannot group them): a bodyless anonymous signature with no
//!   implementation gets `TS2391`, anchored at the whole statement; only the
//!   last of a consecutive bodyless run is reported;
//! - `TS2394`: every bodyless signature of the merged group is checked against
//!   the group's *first* implementation body, in declaration order, and only
//!   the first incompatible signature is reported (oracle-pinned against
//!   `typescript@7.0.2`: a cross-name signature is incompatible with another
//!   name's body, and a signature whose own name has a compatible body still
//!   reports against an earlier body of a different name).
//!
//! - `TS2391` on a body-carrying declaration whose group continues past a
//!   textual gap (`export default function a() {...}` / other statements /
//!   `export default function b(): void;` marks `a` too — oracle-pinned).
//!
//! Named signatures' terminal/gap `TS2391`/`TS2389` mostly coincide with what
//! the name-keyed walk in `check_function_implementations` reports (tsc emits
//! them from the per-local-name run as well and deduplicates); the checker's
//! diagnostic sink dedups by (start, code), so this pass mirrors tsc's walk
//! faithfully instead of special-casing the overlap.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

struct DefaultFunctionEntry {
    /// Position in the statement list, used for tsc's textual-adjacency test
    /// (`previousDeclaration.end !== node.pos`) at statement granularity.
    list_pos: usize,
    /// The statement node (export wrapper) — the anchor for anonymous errors.
    stmt_idx: NodeIndex,
    /// The function declaration node inside the wrapper.
    fn_idx: NodeIndex,
    has_body: bool,
}

impl<'a> CheckerState<'a> {
    /// Run the merged `default`-symbol half of function-implementation
    /// checking over a statement list. See the module docs for scope.
    pub(crate) fn check_default_export_function_group(&mut self, statements: &[NodeIndex]) {
        let mut entries: Vec<DefaultFunctionEntry> = Vec::new();
        for (list_pos, &stmt_idx) in statements.iter().enumerate() {
            let Some((fn_idx, true)) = self.statement_function_declaration_view(stmt_idx) else {
                continue;
            };
            let Some(fn_node) = self.ctx.arena.get(fn_idx) else {
                continue;
            };
            let Some(func) = self.ctx.arena.get_function(fn_node) else {
                continue;
            };
            // tsc resets its walk at ambient declarations; a declaration-file
            // or `declare module` body contributes nothing to this pass.
            if self.is_ambient_declaration(fn_idx) {
                continue;
            }
            entries.push(DefaultFunctionEntry {
                list_pos,
                stmt_idx,
                fn_idx,
                has_body: func.body.is_some(),
            });
        }
        if entries.is_empty() {
            return;
        }

        // tsc's walk: when a group member is not textually adjacent to its
        // predecessor, the predecessor gets an implementation-expected report
        // (unless the member is a duplicate implementation, which the
        // `TS2393` family in `module_exports.rs` owns); a bodyless final
        // member gets the terminal report. A consecutive bodyless run
        // therefore reports only its last member.
        let mut body_decl_seen = false;
        let mut prev: Option<usize> = None;
        for (pos, entry) in entries.iter().enumerate() {
            if entry.has_body && body_decl_seen {
                // Duplicate implementation: redeclare family, owned elsewhere.
            } else if let Some(prev_pos) = prev
                && entry.list_pos != entries[prev_pos].list_pos + 1
            {
                self.report_implementation_expected(&entries[prev_pos], statements);
            }
            if entry.has_body {
                body_decl_seen = true;
            }
            prev = Some(pos);
        }
        if let Some(last) = entries.last()
            && !last.has_body
        {
            self.report_implementation_expected(last, statements);
        }

        // TS2394 over the merged group: each bodyless signature against the
        // group's first implementation, first incompatible only.
        if !entries.iter().any(|e| !e.has_body) {
            return;
        }
        let Some(impl_entry_fn) = entries.iter().find(|e| e.has_body).map(|e| e.fn_idx) else {
            return;
        };
        let signature_entries: Vec<(NodeIndex, NodeIndex)> = entries
            .iter()
            .filter(|e| !e.has_body)
            .map(|e| (e.fn_idx, e.stmt_idx))
            .collect();
        self.report_first_incompatible_default_overload(impl_entry_fn, &signature_entries);
    }

    /// tsc's `reportImplementationExpectedError` for a merged-`default`-group
    /// member: peek at the textually adjacent next statement; a same-named
    /// function is a grouping question, not a missing implementation; an
    /// adjacent differently-named implementation gets `TS2389`; everything
    /// else is `TS2391` at the member's name, or at the whole export
    /// statement for an anonymous member (tsc anchors at the declaration
    /// node, whose span includes the `export default` modifiers).
    fn report_implementation_expected(
        &mut self,
        entry: &DefaultFunctionEntry,
        statements: &[NodeIndex],
    ) {
        // Mirror the name-keyed walk's parser-recovery suppression.
        if self.has_syntax_parse_errors()
            && let Some(stmt_node) = self.ctx.arena.get(entry.stmt_idx)
            && self
                .ctx
                .syntax_parse_error_positions
                .iter()
                .any(|&p| p >= stmt_node.pos && p <= stmt_node.end)
        {
            return;
        }
        let entry_name = self.get_function_name_from_node(entry.fn_idx);
        if let Some(&next_stmt) = statements.get(entry.list_pos + 1)
            && let Some((next_fn_idx, _)) = self.statement_function_declaration_view(next_stmt)
            && let Some(next_node) = self.ctx.arena.get(next_fn_idx)
            && let Some(next_func) = self.ctx.arena.get_function(next_node)
        {
            let next_name = self.get_function_name_from_node(next_fn_idx);
            if entry_name.is_some() && next_name.is_some() && entry_name == next_name {
                return;
            }
            if next_func.body.is_some()
                && let Some(expected) = entry_name.as_deref()
            {
                let impl_error_node = if next_func.name.is_some() {
                    next_func.name
                } else {
                    next_stmt
                };
                self.error_at_node(
                    impl_error_node,
                    &format!("Function implementation name must be '{expected}'."),
                    diagnostic_codes::FUNCTION_IMPLEMENTATION_NAME_MUST_BE,
                );
                return;
            }
        }
        let error_node = self
            .ctx
            .arena
            .get(entry.fn_idx)
            .and_then(|n| self.ctx.arena.get_function(n))
            .map(|f| f.name)
            .filter(|n| n.is_some())
            .unwrap_or(entry.stmt_idx);
        self.error_at_node(
            error_node,
            "Function implementation is missing or not immediately following the declaration.",
            diagnostic_codes::FUNCTION_IMPLEMENTATION_IS_MISSING_OR_NOT_IMMEDIATELY_FOLLOWING_THE_DECLARATION,
        );
    }

    /// Check each bodyless default-exported signature against the merged
    /// group's implementation and report `TS2394` at the first incompatible
    /// one. Anchors at the signature's name, or at the statement for an
    /// anonymous signature.
    fn report_first_incompatible_default_overload(
        &mut self,
        impl_fn_idx: NodeIndex,
        signature_entries: &[(NodeIndex, NodeIndex)],
    ) {
        use crate::query_boundaries::assignability::get_function_return_type;
        use crate::query_boundaries::assignability::replace_function_return_type;

        let type_resolver = |node_idx: NodeIndex| -> Option<u32> {
            self.ctx.binder.get_node_symbol(node_idx).map(|id| id.0)
        };
        let value_resolver = |node_idx: NodeIndex| -> Option<u32> {
            self.ctx.binder.get_node_symbol(node_idx).map(|id| id.0)
        };
        let lowering = tsz_lowering::TypeLowering::with_resolvers(
            self.ctx.arena,
            self.ctx.types,
            &type_resolver,
            &value_resolver,
        );

        // Implementation type: manual lowering, with the same inferred-return
        // fallbacks the symbol-driven `check_overload_compatibility` applies.
        let impl_return_override = self.get_impl_return_type_override(impl_fn_idx);
        let mut impl_type =
            lowering.lower_signature_from_declaration(impl_fn_idx, impl_return_override);
        if impl_type == tsz_solver::TypeId::ERROR {
            let node_type = self.get_type_of_node(impl_fn_idx);
            if node_type == tsz_solver::TypeId::ERROR {
                return;
            }
            impl_type = node_type;
        }
        if impl_return_override.is_some() {
            let inferred_type = self.get_type_of_node(impl_fn_idx);
            if inferred_type != tsz_solver::TypeId::ERROR
                && let Some(ret) = get_function_return_type(self.ctx.types, inferred_type)
                && ret != tsz_solver::TypeId::ERROR
            {
                impl_type = replace_function_return_type(self.ctx.types, impl_type, ret);
            }
        }
        impl_type = self.fix_error_params_in_function(impl_type);

        for &(sig_fn_idx, sig_stmt_idx) in signature_entries {
            let overload_return_override = self.get_overload_return_type_override(sig_fn_idx);
            let mut overload_type =
                lowering.lower_signature_from_declaration(sig_fn_idx, overload_return_override);
            if overload_type == tsz_solver::TypeId::ERROR {
                let node_type = self.get_type_of_node(sig_fn_idx);
                if node_type == tsz_solver::TypeId::ERROR {
                    continue;
                }
                overload_type = node_type;
            }
            overload_type = self.fix_error_params_in_function(overload_type);

            if !self.is_implementation_compatible_with_overload(impl_type, overload_type) {
                let error_node = self
                    .ctx
                    .arena
                    .get(sig_fn_idx)
                    .and_then(|n| self.ctx.arena.get_function(n))
                    .map(|f| f.name)
                    .filter(|n| n.is_some())
                    .unwrap_or(sig_stmt_idx);
                self.error_at_node(
                    error_node,
                    diagnostic_messages::THIS_OVERLOAD_SIGNATURE_IS_NOT_COMPATIBLE_WITH_ITS_IMPLEMENTATION_SIGNATURE,
                    diagnostic_codes::THIS_OVERLOAD_SIGNATURE_IS_NOT_COMPATIBLE_WITH_ITS_IMPLEMENTATION_SIGNATURE,
                );
                // tsc reports only the first incompatible overload per symbol.
                break;
            }
        }
    }
}
