//! Overload-group modifier flag agreement: `TS2383` (export) and `TS2384`
//! (ambient), mirroring tsc's `checkFunctionOrConstructorSymbol` /
//! `checkFlagAgreementBetweenOverloads` (#16742).

use super::{DuplicateDeclList, DuplicateDeclarationOrigin};
use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    /// Report export/ambient flag disagreements across one symbol's overload
    /// group and return the group's bodyless overload signatures (consumed by
    /// the `TS2385`/`TS2386` arms in the caller).
    ///
    /// Mismatch detection accumulates over the symbol's function-like
    /// declarations — implementation included, so a lone
    /// `export function f(sig);` above a non-exported implementation is a
    /// mismatch. Reporting then runs over every local declaration of the
    /// merged symbol (an exported `namespace f {}` merged into a mixed
    /// function group is blamed too), comparing each against the canonical
    /// declaration: the implementation when it shares a statement container
    /// with the first overload, otherwise the first overload. A declaration
    /// deviating on both axes reports only `TS2383` — tsc's else-if chain
    /// gives the export mismatch precedence over the ambient one.
    pub(super) fn check_overload_flag_agreement(
        &mut self,
        declarations: &DuplicateDeclList,
    ) -> Vec<NodeIndex> {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        // tsc's `getEffectiveDeclarationFlags` treats a member of an ambient
        // module/namespace body (`declare namespace M`, `declare module "m"`,
        // including namespaces nested inside one) as implicitly exported, so
        // mismatched `export` keywords there are not overload-consistency
        // errors (#16698). `declare global` is a global augmentation, not an
        // ambient module, and stays subject to the check, as does a bare
        // top-level `declare function`.
        let effective_exported = |state: &Self, decl_idx: NodeIndex, is_exported: bool| {
            is_exported || state.is_within_ambient_module_container(decl_idx)
        };
        let mut overload_signatures = Vec::new();
        let mut implementation: Option<NodeIndex> = None;
        let mut has_exported_func = false;
        let mut has_non_exported_func = false;
        let mut has_ambient_func = false;
        let mut has_non_ambient_func = false;
        for &(decl_idx, flags, is_local, is_exported, _) in declarations {
            if is_local && (flags & (symbol_flags::FUNCTION | symbol_flags::METHOD)) != 0 {
                if effective_exported(self, decl_idx, is_exported) {
                    has_exported_func = true;
                } else {
                    has_non_exported_func = true;
                }
                if self.is_ambient_declaration(decl_idx) {
                    has_ambient_func = true;
                } else {
                    has_non_ambient_func = true;
                }
                if self.function_has_body(decl_idx) {
                    if implementation.is_none() {
                        implementation = Some(decl_idx);
                    }
                } else {
                    overload_signatures.push(decl_idx);
                }
            }
        }
        let export_mismatch = has_exported_func && has_non_exported_func;
        let ambient_mismatch = has_ambient_func && has_non_ambient_func;
        // The flag-agreement check only runs when the group has at least one
        // bodyless overload signature: two duplicate implementations are
        // TS2393 territory and report no flag disagreement.
        if !overload_signatures.is_empty() && (export_mismatch || ambient_mismatch) {
            // tsc's `getCanonicalOverload` takes the canonical flags from the
            // *implementation* when it shares a container with the first
            // overload, and only otherwise from the first overload. The
            // container check is what keeps lib.d.ts overloads from being
            // blamed for a local implementation.
            // `effective_declaration_container` hoists through an
            // `EXPORT_DECLARATION` wrapper so `export function` declarations
            // compare equal to bare siblings in the same statement list.
            let first_overload = overload_signatures[0];
            let first_container = self.effective_declaration_container(first_overload);
            let canonical = implementation
                .filter(|&impl_idx| {
                    first_container.is_some()
                        && self.effective_declaration_container(impl_idx) == first_container
                })
                .unwrap_or(first_overload);
            let canonical_exported = declarations
                .iter()
                .find(|&&(di, _, _, _, _)| di == canonical)
                .is_some_and(|&(_, _, _, is_exported, _)| {
                    effective_exported(self, canonical, is_exported)
                });
            let canonical_ambient = self.is_ambient_declaration(canonical);
            for &(decl_idx, _, is_local, is_exported, origin) in declarations {
                if !is_local || origin != DuplicateDeclarationOrigin::SymbolDeclaration {
                    continue;
                }
                let error_node = self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                // Both deviation checks are ungated once the loop is entered
                // (tsc computes each declaration's deviation against the
                // canonical flags regardless of which axis tripped the
                // mismatch): an ambient-only mismatch among the function-likes
                // still blames a merged namespace whose export status
                // deviates.
                if effective_exported(self, decl_idx, is_exported) != canonical_exported {
                    self.error_at_node(
                        error_node,
                        diagnostic_messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_EXPORTED_OR_NON_EXPORTED,
                        diagnostic_codes::OVERLOAD_SIGNATURES_MUST_ALL_BE_EXPORTED_OR_NON_EXPORTED,
                    );
                } else if self.is_ambient_declaration(decl_idx) != canonical_ambient {
                    self.error_at_node(
                        error_node,
                        diagnostic_messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_AMBIENT_OR_NON_AMBIENT,
                        diagnostic_codes::OVERLOAD_SIGNATURES_MUST_ALL_BE_AMBIENT_OR_NON_AMBIENT,
                    );
                }
            }
        }
        overload_signatures
    }
}
