//! Apparent-`ObjectShape` construction and the global-`Function`-interface
//! "second opinion" for function-like sources relating to object targets,
//! extracted from [`super::core_dispatch`] so that shard stays under the
//! file-size ceiling (§19). `use super::*` re-exposes `SubtypeChecker` and the
//! parent module's imports, so the relocation is behavior-preserving.

use super::*;

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Second opinion for a function-like source that a synthesized apparent
    /// shape just rejected: relate the registered global `Function` interface to
    /// the same target.
    ///
    /// `tsc` compares a function value against its *apparent type* — the call
    /// and construct signatures plus every member of the global `Function`
    /// interface (`length`, `name`, `bind`, `call`, `apply`, `toString`,
    /// `arguments`, `caller`, `prototype`). The shapes built above carry only
    /// the source's own declared properties plus the two synthesized names the
    /// weak-type rule needs, so a target requiring any other `Function` member
    /// reads as unsatisfied even though every function provides it.
    ///
    /// Asking the boxed-type registry for the real interface (the same
    /// binder/global-builtin id `visitor.rs` uses for the intersection-member
    /// arm) keeps the member *types* honest too: `{ length: string }` still
    /// fails, because the interface declares `length: number`.
    ///
    /// This is purely a second opinion — it can only turn a rejection into
    /// acceptance:
    ///
    /// * A **weak** target (all-optional, no index signature) is left to the
    ///   verdict it already has. That verdict is owned by the weak-type rule in
    ///   `check_object_subtype`, which deliberately scans the synthesized names
    ///   and nothing else; widening the surface underneath it would silently
    ///   accept `{ length?: number }`, which `tsc` rejects with `TS2559`.
    /// * With no lib loaded the registry is empty and the synthesized verdict
    ///   stands unchanged.
    pub(crate) fn or_global_function_interface_surface(
        &mut self,
        target: TypeId,
        target_shape: &ObjectShape,
        result: SubtypeResult,
    ) -> SubtypeResult {
        if result.is_true() || Self::is_weak_type_shape(target_shape) {
            return result;
        }
        // The boxed `Function` surface must not satisfy an unwaived numeric index
        // the target requires — a concrete function value's apparent type carries
        // none. When the target *is* the (augmented) global `Function`,
        // `check_subtype(boxed_function, target)` would be identity-true and mask
        // that deficit, so keep the existing rejection (#16525).
        if self.function_target_has_unwaived_index(target) {
            return result;
        }
        let Some(boxed_function) = self
            .resolver
            .get_boxed_type(IntrinsicKind::Function)
            .or_else(|| self.interner.get_boxed_type(IntrinsicKind::Function))
        else {
            return result;
        };
        if self.check_subtype(boxed_function, target).is_true() {
            return SubtypeResult::True;
        }
        result
    }

    /// Build the apparent `ObjectShape` of a bare function/constructor source for
    /// structural object comparison. A function value has no user-declared
    /// members, but it exposes stable apparent properties: `call`/`apply` for a
    /// callable, `prototype` for a constructor. Modeling these as *required*
    /// properties keeps the source from being mistaken for a weak shape, so the
    /// weak-type rejection in `check_object_subtype` fires for a standalone or
    /// union-member all-optional target the function shares no name with — while
    /// an intersection-member target (weak rule suppressed) and an optional target
    /// that shares one of these names still succeed. Mirrors
    /// `CompatChecker::function_like_weak_type_properties`.
    pub(crate) fn function_apparent_object_shape(&self, source: TypeId) -> ObjectShape {
        let is_constructor = function_shape_id(self.interner, source)
            .map(|id| self.interner.function_shape(id).is_constructor)
            .unwrap_or(false);
        let mut properties = Vec::new();
        let mut push = |name: &str| {
            let atom = self.interner.intern_string(name);
            properties.push(PropertyInfo::new(atom, TypeId::ANY));
        };
        if is_constructor {
            push("prototype");
        } else {
            push("call");
            push("apply");
        }
        // `check_object_subtype`'s merge scan expects source properties sorted by
        // name (`Atom`), matching the callable-shape path above.
        properties.sort_by_key(|p| p.name);
        ObjectShape {
            flags: ObjectFlags::empty(),
            properties,
            string_index: None,
            number_index: None,
            symbol_index: None,
            symbol: None,
        }
    }
}
