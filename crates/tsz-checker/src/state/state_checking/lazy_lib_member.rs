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
use tsz_parser::parser::NodeIndex;
use tsz_solver::{DefId, TypeId};

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

/// Kill-switch for preserving a bare-`Lazy` lib-interface receiver through the
/// known-global value-type override in property-access resolution. Set
/// `TSZ_DISABLE_GLOBAL_LAZY_RECV_PRESERVE=1` to force the legacy path that
/// always re-materializes the global value type, enabling byte-identical
/// diagnostic comparison.
///
/// Without this preservation, a global receiver like `document` (whose type is
/// already `Lazy(Document)`) is eagerly materialized to its full `Object` shape
/// — merging the entire heritage chain — even when only one own member is read,
/// defeating [`CheckerState::try_lazy_lib_member_property_access`].
pub(crate) fn global_lazy_receiver_preserve_disabled() -> bool {
    use std::sync::OnceLock;
    static DISABLED: OnceLock<bool> = OnceLock::new();
    *DISABLED.get_or_init(|| {
        std::env::var("TSZ_DISABLE_GLOBAL_LAZY_RECV_PRESERVE")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false)
    })
}

impl CheckerState<'_> {
    /// Compute the known-global value-type override for a property-access
    /// receiver identifier `ident_text`, given the receiver's `current_type`
    /// and its expression node `expr`. Returns `Some(value_type)` to override,
    /// or `None` to keep `current_type`.
    ///
    /// The override makes an unshadowed known-global value (e.g. `document`,
    /// `location`) authoritative over a stale/JS-inferred receiver type. But
    /// when `current_type` is *already* a bare `Lazy(DefId)` to an eligible
    /// simple lib interface, it IS the authoritative global type — overriding
    /// would only re-materialize the identical interface eagerly, defeating the
    /// lazy single-member fast path for global receivers like `document.title`
    /// (the receiver arrives lazy but is forced to a full `Object` shape,
    /// merging the whole heritage chain to read one member). In that case this
    /// returns `None` to preserve the lazy receiver; the fast path resolves the
    /// accessed own member, and on a miss the downstream materialization
    /// fallback produces the identical `Object` the override would have.
    /// Preservation is gated by [`global_lazy_receiver_preserve_disabled`].
    pub(crate) fn global_value_type_override(
        &mut self,
        ident_text: &str,
        current_type: TypeId,
        expr: NodeIndex,
    ) -> Option<TypeId> {
        if !self.is_known_global_value_name(ident_text)
            || self.known_global_value_has_local_shadow(expr, ident_text)
        {
            return None;
        }
        if !global_lazy_receiver_preserve_disabled()
            && self.lazy_lib_member_receiver_def_id(current_type).is_some()
        {
            return None;
        }
        let value_type = self.type_of_value_symbol_by_name(ident_text);
        (value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR).then_some(value_type)
    }

    pub(crate) fn global_value_type_override_for_property(
        &mut self,
        ident_text: &str,
        current_type: TypeId,
        expr: NodeIndex,
        prop_name: &str,
    ) -> Option<TypeId> {
        if !self.is_known_global_value_name(ident_text)
            || self.known_global_value_has_local_shadow(expr, ident_text)
        {
            return None;
        }
        if !global_lazy_receiver_preserve_disabled()
            && let Some(def_id) = self.lazy_lib_member_receiver_def_id(current_type)
            && let Some(sym_id) = self.ctx.def_to_symbol_id_with_fallback(def_id)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && self
                .resolve_simple_lib_interface_own_property(&symbol.escaped_name.clone(), prop_name)
                .is_some()
        {
            return None;
        }
        let value_type = self.type_of_value_symbol_by_name(ident_text);
        (value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR).then_some(value_type)
    }

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
    /// 6. The program has no global augmentations. Augmentation-aware programs
    ///    use full materialization so global interface merge state remains
    ///    authoritative across chained lib-interface reads.
    /// 7. The interface is not globally augmented (`declare global { interface
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
        if self.program_has_global_augmentations() {
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

    fn program_has_global_augmentations(&self) -> bool {
        !self.ctx.binder.global_augmentations.is_empty()
            || self
                .ctx
                .global_augmentation_targets_index
                .as_ref()
                .is_some_and(|index| !index.is_empty())
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
