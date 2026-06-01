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
use tsz_binder::symbol_flags;
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

/// Kill-switch for the lazy single-**method** lib-interface property-access
/// fast path (the overload-set + heritage-walk extension of the single-property
/// fast path). Set `TSZ_DISABLE_LAZY_METHOD=1` to force the legacy
/// full-materialization path for method members and inherited members, enabling
/// byte-identical diagnostic comparison of method/call resolution with the fast
/// path on vs off.
///
/// This is independent of [`lazy_lib_member_access_disabled`] so the
/// higher-risk method/overload/heritage path can be A/B compared in isolation
/// from the already-landed single-property path.
///
/// Cached in a `OnceLock` so the environment is read at most once per process.
pub(crate) fn lazy_lib_method_disabled() -> bool {
    use std::sync::OnceLock;
    static DISABLED: OnceLock<bool> = OnceLock::new();
    *DISABLED.get_or_init(|| {
        std::env::var("TSZ_DISABLE_LAZY_METHOD")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false)
    })
}

/// Kill-switch for keeping a global ambient-var value type lazy when its
/// annotation is a bare reference to a simple lib interface (e.g. the global
/// `declare var document: Document`). Set `TSZ_DISABLE_LAZY_GLOBAL_VAR=1` to
/// force the legacy eager `resolve_ref_type` materialization, enabling
/// byte-identical diagnostic comparison.
///
/// Cached in a `OnceLock` so the environment is read at most once per process.
pub(crate) fn lazy_global_var_disabled() -> bool {
    use std::sync::OnceLock;
    static DISABLED: OnceLock<bool> = OnceLock::new();
    *DISABLED.get_or_init(|| {
        std::env::var("TSZ_DISABLE_LAZY_GLOBAL_VAR")
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
        self.simple_lib_interface_lazy_ref_def_id(object_type)
    }

    /// Structural eligibility predicate shared by the property-access fast path
    /// ([`Self::lazy_lib_member_receiver_def_id`]) and the lazy global-var value
    /// type ([`Self::lazy_global_var_lib_interface_def_id`]).
    ///
    /// Returns the `DefId` when `object_type` is a bare `Lazy(DefId)` reference
    /// to a simple lib interface, or `None` otherwise. This does **not** check
    /// either kill-switch; callers gate independently so each lever can be A/B
    /// compared in isolation.
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
    pub(crate) fn simple_lib_interface_lazy_ref_def_id(
        &self,
        object_type: tsz_solver::TypeId,
    ) -> Option<DefId> {
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

    /// When `annotated_type` is the value type of a global/cross-file ambient var
    /// whose annotation is a bare reference to a simple lib interface, return the
    /// same `Lazy(DefId)` so the value type stays lazy instead of being eagerly
    /// materialized by `resolve_ref_type`.
    ///
    /// This is the value-position counterpart of the property-access fast path:
    /// keeping the global receiver lazy (e.g. `document: Document`) lets
    /// `try_lazy_lib_member_property_access` resolve only the accessed member on
    /// `document.title` / `document.querySelector(...)`, mirroring the already-lazy
    /// file-local `declare const d: Document` path.
    ///
    /// Returns `None` (caller keeps the eager `resolve_ref_type` result) when the
    /// kill-switch is set or the annotation is not an eligible bare lib-interface
    /// `Lazy(DefId)`.
    pub(crate) fn lazy_global_var_lib_interface_def_id(
        &self,
        annotated_type: tsz_solver::TypeId,
    ) -> Option<DefId> {
        if lazy_global_var_disabled() {
            return None;
        }
        self.simple_lib_interface_lazy_ref_def_id(annotated_type)
    }

    /// Build the `Lazy(DefId)` value type for a global ambient var whose
    /// annotation is a bare reference to the simple lib interface named
    /// `type_name`, or `None` when ineligible.
    ///
    /// Used by the lib-declaration resolution shortcut so an ambient lib var
    /// like `declare var document: Document` keeps a lazy value type instead of
    /// eagerly materializing the interface. The candidate lazy ref is validated
    /// through the same conservative eligibility predicate as the property-access
    /// fast path (simple, non-generic, actual-lib, unshadowed, unaugmented
    /// interface), and the whole lever is gated by `TSZ_DISABLE_LAZY_GLOBAL_VAR`.
    pub(crate) fn lazy_global_var_lib_interface_type(
        &self,
        type_name: &str,
    ) -> Option<tsz_solver::TypeId> {
        if lazy_global_var_disabled() {
            return None;
        }
        let def_id = self.ctx.actual_lib_def_id_for_bare_name(type_name)?;
        let lazy = self.ctx.types.lazy(def_id);
        // Re-validate the structural eligibility on the constructed lazy ref so a
        // generic, augmented, shadowed, or non-interface lib name falls back to
        // the eager materialization path below.
        self.simple_lib_interface_lazy_ref_def_id(lazy)
            .map(|_| lazy)
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

    /// Try to resolve `prop_name` on an eligible simple lib-interface receiver by
    /// lowering only that own property, returning a property-access `Success`
    /// without materializing the rest of the interface.
    ///
    /// Returns `None` (caller takes the full materialization path) when the
    /// receiver is not an eligible bare-`Lazy` lib interface, when the kill-switch
    /// is set, when the interface does not declare `prop_name` as an own plain
    /// property (including all heritage-inherited members), or when single-member
    /// lowering cannot prove the member shape.
    pub(crate) fn try_lazy_lib_member_property_access(
        &mut self,
        object_type: tsz_solver::TypeId,
        prop_name: &str,
    ) -> Option<tsz_solver::operations::property::PropertyAccessResult> {
        let def_id = self.lazy_lib_member_receiver_def_id(object_type)?;

        let sym_id = self.ctx.def_to_symbol_id_with_fallback(def_id)?;
        let name = self.ctx.binder.get_symbol(sym_id)?.escaped_name.clone();
        let member_type = self.resolve_simple_lib_interface_own_property(&name, prop_name)?;

        Some(tsz_solver::operations::property::PropertyAccessResult::simple(member_type))
    }

    /// Resolve a property-access receiver to its property-access-ready form,
    /// forcing full materialization of an eligible lib-interface `Lazy` that
    /// [`Self::resolve_type_for_property_access`] would otherwise leave lazy.
    ///
    /// Used on the property-read hot path when the single-member fast path
    /// missed (e.g. a heritage-inherited member): the structural member lookup
    /// that follows needs the full shape, so the bare `Lazy` is materialized
    /// here instead of falling back to `any`.
    pub(crate) fn resolve_property_access_base_materialized(
        &mut self,
        object_type: tsz_solver::TypeId,
    ) -> tsz_solver::TypeId {
        let resolved = self.resolve_type_for_property_access(object_type);
        if self.lazy_lib_member_receiver_def_id(resolved).is_some() {
            self.ensure_relation_input_ready(resolved);
            self.resolve_type_for_property_access_force(resolved)
        } else {
            resolved
        }
    }
}
