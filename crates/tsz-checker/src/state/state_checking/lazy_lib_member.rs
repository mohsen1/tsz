//! Lazy single-member resolution for simple lib-interface receivers.
//!
//! # Motivation
//!
//! Value-position property access on a global like `document.title` currently
//! materializes the **entire** `Document` structural shape (~200 members) and
//! its transitive `extends` heritage closure just to read one `string` member.
//! Measured on `const c = document.title;` with `ES2020 + DOM + DOM.Iterable`,
//! this interns ~9216 types vs ~716 for `const c = 1;`. The dominant cost is the
//! eager lowering of every own member's type annotation and method signature —
//! heritage merging is only ~16% of the gap; the rest is the full own-member
//! lowering and the referenced-interface cascade.
//!
//! tsc/tsgo resolve only the accessed member. This module is the value-position
//! counterpart to PR #8638 (`perf(checker): preserve lazy lib interface refs`),
//! which already keeps **type-position** annotations (`let d: Document`) lazy.
//!
//! # Structural rule
//!
//! > When resolving a property access `recv.p` whose receiver type is a
//! > `Lazy(DefId)` reference to a non-generic, unmerged, unaugmented,
//! > unshadowed lib interface, resolve only member `p` (including a
//! > heritage-inherited declaration of `p`) on demand, instead of materializing
//! > the receiver's entire structural object shape and transitive `extends`
//! > closure.
//!
//! # Soundness
//!
//! The eligibility predicate ([`CheckerState::lazy_lib_member_receiver_def_id`])
//! is intentionally conservative and mirrors the #8638 predicate
//! (`try_lower_simple_actual_lib_type_reference`): the receiver must be a bare
//! `Lazy(DefId)` for an actual/cloned-lib **interface** symbol that is
//! non-generic, not compiler-managed, not shadowed by a file-local type, and
//! not globally augmented. Any receiver that fails the predicate falls back to
//! the existing full-materialization path, so behavior is unchanged there.
//!
//! The fast path is additionally gated by the [`lazy_lib_member_access_disabled`]
//! kill-switch (`TSZ_DISABLE_LAZY_MEMBER_ACCESS`) so diagnostics can be compared
//! byte-for-byte with the fast path on vs off.

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_solver::DefId;

/// Kill-switch for the lazy single-member lib-interface property-access fast
/// path. Set `TSZ_DISABLE_LAZY_MEMBER_ACCESS=1` to force the legacy
/// full-materialization path, enabling byte-identical diagnostic comparison.
///
/// Cached in a `OnceLock` so the environment is read at most once per process.
pub(crate) fn lazy_lib_member_access_disabled() -> bool {
    use std::sync::OnceLock;
    static DISABLED: OnceLock<bool> = OnceLock::new();
    *DISABLED.get_or_init(|| {
        std::env::var("TSZ_DISABLE_LAZY_MEMBER_ACCESS")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false)
    })
}

impl CheckerState<'_> {
    /// Return the `DefId` of an eligible simple lib-interface receiver when
    /// `object_type` is a bare `Lazy(DefId)` reference to one, or `None`
    /// otherwise.
    ///
    /// Eligibility (same conservative shape as PR #8638's
    /// `try_lower_simple_actual_lib_type_reference`):
    /// 1. `object_type` is a bare `Lazy(DefId)` (not an `Application`).
    /// 2. The `DefId` maps to a symbol that is an `INTERFACE`.
    /// 3. The symbol is from the actual or cloned standard library.
    /// 4. The interface is **non-generic** (no type parameters) — generic
    ///    receivers need argument substitution that the single-member walk does
    ///    not perform.
    /// 5. The interface name is not compiler-managed and not shadowed by a
    ///    file-local type declaration.
    /// 6. The interface is not globally augmented (`declare global { interface
    ///    X { ... } }`), which could add the accessed member out of band.
    pub(crate) fn lazy_lib_member_receiver_def_id(
        &self,
        object_type: tsz_solver::TypeId,
    ) -> Option<DefId> {
        if lazy_lib_member_access_disabled() {
            return None;
        }

        let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, object_type)?;

        // Must resolve to a concrete lib interface symbol.
        let sym_id = self.ctx.def_to_symbol_id_with_fallback(def_id)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(symbol_flags::INTERFACE) {
            return None;
        }

        // Non-generic only: a generic interface body would need its receiver's
        // type arguments substituted into the member type, which the bare-Lazy
        // path cannot supply.
        if self
            .ctx
            .get_def_type_params(def_id)
            .is_some_and(|p| !p.is_empty())
        {
            return None;
        }

        let name = symbol.escaped_name.clone();
        if crate::query_boundaries::common::is_compiler_managed_type(&name) {
            return None;
        }
        if self.ctx.file_local_type_shadow_for_lib_name(&name) {
            return None;
        }

        // The symbol must come from the actual/cloned lib — user interfaces (even
        // sharing a lib name) take the normal path so augmentation/merging stays
        // correct.
        if !self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id) {
            return None;
        }

        // A globally-augmented interface may gain members from a separate
        // `declare global` block; fall back to full materialization so the
        // augmented members are visible.
        if self.lib_interface_is_globally_augmented(&name) {
            return None;
        }

        Some(def_id)
    }

    /// Whether a lib interface `name` has any global augmentation declarations
    /// recorded in the binder. Augmented interfaces must take the full
    /// materialization path so out-of-band members remain visible.
    fn lib_interface_is_globally_augmented(&self, name: &str) -> bool {
        self.ctx
            .binder
            .global_augmentations
            .get(name)
            .is_some_and(|decls| !decls.is_empty())
    }

    /// Look up `prop_name` as an **own** member symbol of an interface, returning
    /// its member `SymbolId` when present.
    ///
    /// This is the binder-table primitive the single-member lowering builds on:
    /// it answers "does this interface declare `prop_name` directly?" in O(1)
    /// without lowering any member types.
    pub(crate) fn lib_interface_own_member_symbol(
        &self,
        interface_sym_id: SymbolId,
        prop_name: &str,
    ) -> Option<SymbolId> {
        let symbol = self.ctx.binder.get_symbol(interface_sym_id)?;
        symbol.members.as_ref()?.get(prop_name)
    }
}
