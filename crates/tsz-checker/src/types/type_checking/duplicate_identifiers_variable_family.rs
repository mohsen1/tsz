//! Variable-redeclaration reporting for a symbol whose declarations are *all*
//! plain variable declarations (`var`/`let`/`const`).
//!
//! `tsc` reaches this family through **two independent passes**, so one
//! declaration can carry two codes at once and the two codes can cover
//! different subsets of the declaration list:
//!
//! 1. The binder's collision pass (`declareSymbol`). Each declaration is
//!    tested against the *surviving* symbol's accumulated flags. On a
//!    collision the message (TS2451 when the surviving symbol is already
//!    block-scoped, otherwise TS2300) is reported on every declaration
//!    recorded on the surviving symbol so far **and** on the colliding
//!    declaration — and then the colliding declaration is attached to a fresh
//!    throwaway symbol that never re-enters the symbol table. The surviving
//!    symbol therefore keeps its original flags and its declaration list only
//!    grows with declarations that did *not* collide.
//! 2. The exported-variable pass (TS2323), which reads the surviving symbol
//!    and reports on each of its declarations when it is exported and has
//!    more than one.
//!
//! The consequence is that `export var a; export let a; export var a;` reports
//! TS2300 on lines 1-2 (binder pass, whose surviving symbol at that point was
//! just line 1) and TS2323 on lines 1 and 3 (exported pass, whose surviving
//! symbol ends up holding both `var` declarations because the third one never
//! collided with the function-scoped survivor).
//!
//! A single-code-per-symbol `if`-chain cannot express either half: not the
//! co-emission, and not the differing footprints. This module models the two
//! passes directly for the sub-family where it is safe and self-contained —
//! every declaration local, a plain variable, part of the conflict set, and
//! uniformly exported or uniformly local. Anything else falls back to the
//! general selection chain in `duplicate_identifiers.rs`.
//!
//! Every expectation is pinned against `tsc` 7.0.2 with
//! `--noEmit --strict --pretty false --lib es2015 --target es2015`.

use super::duplicate_identifiers::DuplicateDeclarationOrigin;
use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;

/// One entry of the symbol's declaration list, in the shape
/// `check_duplicate_identifiers` collects it.
type DuplicateDeclaration = (NodeIndex, u32, bool, bool, DuplicateDeclarationOrigin);

impl CheckerState<'_> {
    /// Emit the variable-redeclaration diagnostics for `declarations` the way
    /// `tsc`'s two passes do, returning `true` when this module owns the
    /// reporting for the symbol and the caller must not run its own selection
    /// chain.
    ///
    /// Returns `false` — leaving the existing behaviour untouched — whenever
    /// the symbol is outside the modelled sub-family, or when the model
    /// produces no diagnostics at all (`var a; var a;` is a legal
    /// redeclaration, and suppressing an unrelated pre-existing diagnostic is
    /// not this module's job).
    pub(super) fn try_emit_variable_redeclaration_family(
        &mut self,
        declarations: &[DuplicateDeclaration],
        conflicts: &FxHashSet<NodeIndex>,
        name: &str,
        is_external_module: bool,
        suppressed_by_caller: bool,
    ) -> bool {
        if suppressed_by_caller || conflicts.is_empty() || declarations.len() < 2 {
            return false;
        }

        let Some(ordered) = self.variable_family_declarations(declarations, conflicts) else {
            return false;
        };
        let all_exported = declarations
            .iter()
            .all(|(_, _, _, is_exported, _)| *is_exported);
        let none_exported = declarations
            .iter()
            .all(|(_, _, _, is_exported, _)| !*is_exported);
        // Mixed exportedness routes through `tsc`'s *separate* export table as
        // well as the local one, which gives the two passes different inputs
        // and pulls TS2395 in alongside. That is a different family; leave it.
        if !all_exported && !none_exported {
            return false;
        }
        // The exported-variable pass reads the *module's* export table, so a
        // namespace-internal `export var` merge never reaches it — tsc accepts
        // those (see #16158/#16161). The binder pass below still runs for them.
        let exported_pass_applies = all_exported
            && is_external_module
            && declarations
                .iter()
                .all(|(decl_idx, _, _, _, _)| self.get_enclosing_namespace(*decl_idx).is_none());

        let reports = variable_family_reports(&ordered, exported_pass_applies);
        if reports.is_empty() {
            return false;
        }

        for (decl_idx, code) in reports {
            let message = match code {
                diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE => format_message(
                    diagnostic_messages::CANNOT_REDECLARE_EXPORTED_VARIABLE,
                    &[name],
                ),
                diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE => format_message(
                    diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                    &[name],
                ),
                _ => format_message(diagnostic_messages::DUPLICATE_IDENTIFIER, &[name]),
            };
            let error_node = self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
            self.error_at_node(error_node, &message, code);
        }
        true
    }

    /// Validate that every declaration belongs to the modelled sub-family and
    /// return them ordered by source position as `(declaration, flags)`.
    ///
    /// The family is deliberately narrow: a plain local variable declaration
    /// carrying no symbol flag other than `FUNCTION_SCOPED_VARIABLE` or
    /// `BLOCK_SCOPED_VARIABLE`, sourced from the symbol's own declaration list,
    /// and part of the conflict set. Merged functions, classes, enums, aliases,
    /// namespaces, accessors and cross-file declarations all leave the family.
    fn variable_family_declarations(
        &self,
        declarations: &[DuplicateDeclaration],
        conflicts: &FxHashSet<NodeIndex>,
    ) -> Option<Vec<(NodeIndex, u32)>> {
        let mut ordered: Vec<(NodeIndex, u32, u32)> = Vec::with_capacity(declarations.len());
        for (decl_idx, flags, is_local, _, origin) in declarations {
            if !*is_local
                || *origin != DuplicateDeclarationOrigin::SymbolDeclaration
                || !conflicts.contains(decl_idx)
            {
                return None;
            }
            if (flags & !symbol_flags::VARIABLE) != 0 || (flags & symbol_flags::VARIABLE) == 0 {
                return None;
            }
            // `var` shadowing a block-scoped binding in the same scope is
            // TS2481's job, reported elsewhere; do not double-report it here.
            if self.is_var_shadowing_block_scoped_in_same_scope(*decl_idx) {
                return None;
            }
            let pos = self.ctx.arena.get(*decl_idx)?.pos;
            if ordered.iter().any(|(idx, _, _)| *idx == *decl_idx) {
                return None;
            }
            ordered.push((*decl_idx, *flags, pos));
        }
        // Two declarations at the same position mean the list is not a faithful
        // source-order view of the symbol, and the binder pass depends on order.
        ordered.sort_by_key(|(_, _, pos)| *pos);
        if ordered.windows(2).any(|w| w[0].2 == w[1].2) {
            return None;
        }
        Some(
            ordered
                .into_iter()
                .map(|(idx, flags, _)| (idx, flags))
                .collect(),
        )
    }
}

/// Replay `tsc`'s two passes over `ordered` (source order) and return the
/// `(declaration, code)` pairs to report, deduplicated, in source order with
/// ascending code per declaration.
fn variable_family_reports(
    ordered: &[(NodeIndex, u32)],
    exported_pass_applies: bool,
) -> Vec<(NodeIndex, u32)> {
    let Some(((first_idx, first_flags), rest)) = ordered.split_first() else {
        return Vec::new();
    };

    // Pass 1: the binder's collision walk. `surviving` is the symbol that stays
    // in the symbol table; colliding declarations are reported and dropped onto
    // a throwaway symbol, so they neither join the list nor widen the flags.
    let mut surviving: Vec<NodeIndex> = vec![*first_idx];
    let mut surviving_flags = *first_flags;
    let mut reports: Vec<(NodeIndex, u32)> = Vec::new();

    for (decl_idx, flags) in rest {
        let excludes = if (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0 {
            symbol_flags::BLOCK_SCOPED_VARIABLE_EXCLUDES
        } else {
            symbol_flags::FUNCTION_SCOPED_VARIABLE_EXCLUDES
        };
        if (surviving_flags & excludes) == 0 {
            surviving.push(*decl_idx);
            surviving_flags |= flags;
            continue;
        }
        let code = if (surviving_flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0 {
            diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE
        } else {
            diagnostic_codes::DUPLICATE_IDENTIFIER
        };
        reports.extend(surviving.iter().map(|idx| (*idx, code)));
        reports.push((*decl_idx, code));
    }

    // Pass 2: the exported-variable pass, reading the surviving symbol only.
    if exported_pass_applies && surviving.len() > 1 {
        reports.extend(
            surviving
                .iter()
                .map(|idx| (*idx, diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE)),
        );
    }

    let order: Vec<NodeIndex> = ordered.iter().map(|(idx, _)| *idx).collect();
    reports.sort_by_key(|(idx, code)| {
        (
            order
                .iter()
                .position(|candidate| candidate == idx)
                .unwrap_or(usize::MAX),
            *code,
        )
    });
    reports.dedup();
    reports
}
