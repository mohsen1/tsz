//! Variable-redeclaration reporting for a symbol whose declarations are *all*
//! plain variable declarations (`var`/`let`/`const`).
//!
//! `tsc` reaches this family through **three independent passes** rooted in
//! two separate binder symbol tables (`declareModuleMember`,
//! `crates/tsz-checker` mirrors `container.locals` / `container.symbol.exports`
//! from `src/compiler/binder.ts`), so one declaration can carry several codes
//! at once and each pass can cover a different subset of the declaration
//! list:
//!
//! 1. The **locals-table** collision pass (`declareSymbol` over
//!    `container.locals`). Every declaration is tested against the
//!    *surviving* locals symbol's accumulated flags, but an **exported**
//!    declaration only ever contributes `EXPORT_VALUE` to that accumulated
//!    value — never `FUNCTION_SCOPED_VARIABLE`/`BLOCK_SCOPED_VARIABLE` — because
//!    `declareModuleMember` writes the export-shadow entry into `locals` with
//!    `exportKind`, not the real symbol flags. A collision reports TS2451
//!    (surviving flags already block-scoped) or TS2300 on every declaration
//!    recorded on the surviving symbol so far **and** on the colliding
//!    declaration; the colliding declaration then attaches to a fresh
//!    throwaway symbol that never re-enters the table, so later declarations
//!    keep comparing against the original surviving symbol.
//! 2. The **exports-table** collision pass (`declareSymbol` over
//!    `container.symbol.exports`), a *second*, independent walk using each
//!    declaration's *real* flags, over only the declarations whose export
//!    reaches the module's export table (external module scope, not a
//!    namespace-internal `export`, per #16158/#16161). Same collision
//!    mechanics as pass 1, an entirely separate surviving group, and can
//!    report its own TS2300/TS2451 on top of pass 1's.
//! 3. The **exported-variable** check (TS2323, `checkExternalModuleExports`),
//!    which reads pass 2's final surviving group and reports on every member
//!    when the group has more than one declaration.
//!
//! 4. `checkExportsOnMergedDeclarationsWorker` (TS2395) also reads a
//!    locals-table symbol — but the *pass 1* surviving group, not pass 2's —
//!    and reports on every member of that group when it mixes exported and
//!    non-exported declarations.
//!
//! For example, `export var a; export let a; export var a;` reports TS2300
//! on lines 1-2 (pass 1's surviving symbol at that point was just line 1) and
//! TS2323 on lines 1 and 3 (pass 2/3's surviving symbol ends up holding both
//! `var` declarations because the third one never collided with the
//! function-scoped survivor there, and pass 2 never sees line 2 at all since
//! it is not exported).
//!
//! A single-code-per-symbol `if`-chain cannot express any of this: not the
//! co-emission, not the differing footprints, and not the two independent
//! symbol tables mixed exportedness requires. This module models all three
//! (four, counting TS2395) passes directly for the sub-family where it is
//! safe and self-contained — every declaration local, a plain variable, and
//! part of the conflict set. Anything else falls back to the general
//! selection chain in `duplicate_identifiers.rs`.
//!
//! Every expectation is pinned against `tsc` 7.0.2 with
//! `--noEmit --strict --pretty false --lib es2015 --target es2015`.

use super::duplicate_identifiers::DuplicateDeclarationOrigin;
use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;

/// One entry of the symbol's declaration list, in the shape
/// `check_duplicate_identifiers` collects it.
type DuplicateDeclaration = (NodeIndex, u32, bool, bool, DuplicateDeclarationOrigin);

impl CheckerState<'_> {
    /// Whether every *local* declaration in `declarations` is a plain
    /// variable declaration (`var`/`let`/`const`) from the symbol's own
    /// declaration list — the shape `try_emit_variable_redeclaration_family`
    /// owns end to end. The general selection chain's merge-visibility pass
    /// (TS2395/TS2652) and its `conflicts`-emptiness short-circuit both need
    /// this *before* either has run its own logic, so it is a cheap
    /// structural check over the raw tuples rather than the full validation
    /// `variable_family_declarations` does.
    pub(super) fn declarations_are_pure_variable_family(
        &self,
        declarations: &[DuplicateDeclaration],
    ) -> bool {
        let mut any_local = false;
        for (_, flags, is_local, _, origin) in declarations {
            if !*is_local {
                continue;
            }
            any_local = true;
            if *origin != DuplicateDeclarationOrigin::SymbolDeclaration {
                return false;
            }
            if (flags & !symbol_flags::VARIABLE) != 0 || (flags & symbol_flags::VARIABLE) == 0 {
                return false;
            }
        }
        any_local
    }

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
        name: &str,
        is_external_module: bool,
        suppressed_by_caller: bool,
    ) -> bool {
        if suppressed_by_caller || declarations.len() < 2 {
            return false;
        }

        let Some(ordered) = self.variable_family_declarations(declarations) else {
            return false;
        };

        // The exports-table *collision* walk (pass 2) runs for every exported
        // declaration regardless of container — a namespace has its own
        // `.exports` table too, and `declareModuleMember` writes into it the
        // same way a source-file module does. Only the later checker-side
        // TS2323 audit (`checkExternalModuleExports`, pass 3) is specific to a
        // genuine external module's export table; a namespace-internal
        // `export var` merge never reaches it (see #16158/#16161).
        let ts2323_applies = is_external_module
            && declarations.iter().all(|(decl_idx, _, _, is_exported, _)| {
                !*is_exported || self.get_enclosing_namespace(*decl_idx).is_none()
            });

        let reports = variable_family_reports(&ordered, ts2323_applies);
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
                diagnostic_codes::INDIVIDUAL_DECLARATIONS_IN_MERGED_DECLARATION_MUST_BE_ALL_EXPORTED_OR_ALL_LOCAL => {
                    format_message(
                        diagnostic_messages::INDIVIDUAL_DECLARATIONS_IN_MERGED_DECLARATION_MUST_BE_ALL_EXPORTED_OR_ALL_LOCAL,
                        &[name],
                    )
                }
                _ => format_message(diagnostic_messages::DUPLICATE_IDENTIFIER, &[name]),
            };
            let error_node = self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
            self.error_at_node(error_node, &message, code);
        }
        true
    }

    /// Validate that every declaration belongs to the modelled sub-family and
    /// return them ordered by source position as
    /// `(declaration, flags, is_exported)`.
    ///
    /// The family is deliberately narrow: a plain local variable declaration
    /// carrying no symbol flag other than `FUNCTION_SCOPED_VARIABLE` or
    /// `BLOCK_SCOPED_VARIABLE`, sourced from the symbol's own declaration list.
    /// Merged functions, classes, enums, aliases, namespaces, accessors and
    /// cross-file declarations all leave the family. Unlike the general
    /// selection chain this module replaces, membership does not also require
    /// the caller's own pairwise `conflicts` set: `tsc`'s two-table model
    /// (notably TS2395) applies to plain variables that never trigger a
    /// same-kind collision at all (`export var a; var a;` reports TS2395 with
    /// no TS2300/2451 anywhere), so this module runs its own complete
    /// algorithm rather than deferring to a pre-filter tuned for the
    /// single-code chain.
    fn variable_family_declarations(
        &self,
        declarations: &[DuplicateDeclaration],
    ) -> Option<Vec<(NodeIndex, u32, bool)>> {
        let mut ordered: Vec<(NodeIndex, u32, bool, u32)> = Vec::with_capacity(declarations.len());
        for (decl_idx, flags, is_local, is_exported, origin) in declarations {
            if !*is_local || *origin != DuplicateDeclarationOrigin::SymbolDeclaration {
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
            if ordered.iter().any(|(idx, _, _, _)| *idx == *decl_idx) {
                return None;
            }
            ordered.push((*decl_idx, *flags, *is_exported, pos));
        }
        // Two declarations at the same position mean the list is not a faithful
        // source-order view of the symbol, and the binder pass depends on order.
        ordered.sort_by_key(|(_, _, _, pos)| *pos);
        if ordered.windows(2).any(|w| w[0].3 == w[1].3) {
            return None;
        }
        Some(
            ordered
                .into_iter()
                .map(|(idx, flags, is_exported, _)| (idx, flags, is_exported))
                .collect(),
        )
    }
}

/// One symbol-table's-worth of `declareSymbol`'s collision walk: `surviving`
/// stays in the table and only grows with declarations that do not collide;
/// a colliding declaration is reported and dropped onto a throwaway symbol
/// that the table never sees again, so the *next* declaration keeps comparing
/// against the same accumulated flags. Each item is
/// `(declaration, real_flags, contributed_flags)`: `real_flags` (the
/// declaration's own var/let flag) picks the exclude set, while
/// `contributed_flags` is what gets OR'd into the accumulator on a
/// non-colliding merge. They differ for the locals table's export-shadow rule
/// (an exported declaration's `real_flags` is still its true var/let flag —
/// that is what a *later* declaration's excludes test against — but it
/// contributes only `EXPORT_VALUE`, mirroring `declareModuleMember` writing
/// the export-shadow entry into `container.locals` with `exportKind`). The
/// exports table's walk uses real flags for both.
fn collision_walk(
    ordered: impl IntoIterator<Item = (NodeIndex, u32, u32)>,
) -> (Vec<NodeIndex>, Vec<(NodeIndex, u32)>) {
    let mut iter = ordered.into_iter();
    let Some((first_idx, _, first_contributed)) = iter.next() else {
        return (Vec::new(), Vec::new());
    };
    let mut surviving: Vec<NodeIndex> = vec![first_idx];
    let mut surviving_flags = first_contributed;
    let mut reports: Vec<(NodeIndex, u32)> = Vec::new();

    for (decl_idx, real_flags, contributed_flags) in iter {
        let excludes = if (real_flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0 {
            symbol_flags::BLOCK_SCOPED_VARIABLE_EXCLUDES
        } else {
            symbol_flags::FUNCTION_SCOPED_VARIABLE_EXCLUDES
        };
        if (surviving_flags & excludes) == 0 {
            surviving.push(decl_idx);
            surviving_flags |= contributed_flags;
            continue;
        }
        let code = if (surviving_flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0 {
            diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE
        } else {
            diagnostic_codes::DUPLICATE_IDENTIFIER
        };
        reports.extend(surviving.iter().map(|idx| (*idx, code)));
        reports.push((decl_idx, code));
    }
    (surviving, reports)
}

/// Replay `tsc`'s locals-table pass, exports-table pass, and their two
/// checker-side follow-ups (TS2395, TS2323) over `ordered` (source order,
/// `(declaration, flags, is_exported)`), returning the `(declaration, code)`
/// pairs to report, deduplicated, in source order with ascending code per
/// declaration. `ts2323_applies` gates only the TS2323 checker-side audit —
/// the exports-table collision walk itself runs for every exported
/// declaration regardless of container, since a namespace has its own
/// `.exports` table too (see #16158/#16161).
fn variable_family_reports(
    ordered: &[(NodeIndex, u32, bool)],
    ts2323_applies: bool,
) -> Vec<(NodeIndex, u32)> {
    if ordered.is_empty() {
        return Vec::new();
    }

    // Pass 1: the locals-table collision walk. An exported declaration
    // contributes only `EXPORT_VALUE` to the accumulated flags — never its
    // real var/let flag — mirroring `declareModuleMember`'s export-shadow
    // entry into `container.locals`.
    let (locals_surviving, mut reports) =
        collision_walk(ordered.iter().map(|&(idx, flags, is_exported)| {
            let contributed = if is_exported {
                symbol_flags::EXPORT_VALUE
            } else {
                flags
            };
            (idx, flags, contributed)
        }));

    // TS2395: `checkExportsOnMergedDeclarationsWorker` reads the *same*
    // locals-table surviving group (a declaration that lost pass 1's
    // collision is attached to its own throwaway symbol and never rejoins
    // it), and reports on every member when the group mixes exported and
    // non-exported declarations.
    let is_exported_of: rustc_hash::FxHashMap<NodeIndex, bool> = ordered
        .iter()
        .map(|&(idx, _, is_exported)| (idx, is_exported))
        .collect();
    let surviving_has_exported = locals_surviving
        .iter()
        .any(|idx| is_exported_of.get(idx).copied().unwrap_or(false));
    let surviving_has_local = locals_surviving
        .iter()
        .any(|idx| !is_exported_of.get(idx).copied().unwrap_or(false));
    if surviving_has_exported && surviving_has_local {
        reports.extend(locals_surviving.iter().map(|idx| {
            (
                *idx,
                diagnostic_codes::INDIVIDUAL_DECLARATIONS_IN_MERGED_DECLARATION_MUST_BE_ALL_EXPORTED_OR_ALL_LOCAL,
            )
        }));
    }

    // Pass 2: the exports-table collision walk — real flags, every exported
    // declaration, independent of the locals-table walk above.
    let (exports_surviving, exports_reports) = collision_walk(
        ordered
            .iter()
            .filter(|&&(_, _, is_exported)| is_exported)
            .map(|&(idx, flags, _)| (idx, flags, flags)),
    );
    reports.extend(exports_reports);
    // Pass 3 (TS2323): the checker-side audit of pass 2's final surviving
    // group, restricted to a genuine external module's export table.
    if ts2323_applies && exports_surviving.len() > 1 {
        reports.extend(
            exports_surviving
                .iter()
                .map(|idx| (*idx, diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE)),
        );
    }

    let order: Vec<NodeIndex> = ordered.iter().map(|&(idx, _, _)| idx).collect();
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
