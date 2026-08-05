//! Apparent-`ObjectShape` builders for function-like sources relating to object
//! targets, extracted from [`super::core_dispatch`] so that shard stays under
//! the file-size ceiling (§19). `use super::*` re-exposes `SubtypeChecker` and
//! the parent module's imports, so the relocation is behavior-preserving.
//!
//! A function value relates to an object target through its *apparent type*
//! (`tsc`'s `getApparentType`): the call signatures plus the global `Function`
//! interface surface. Two shapes are distinguished:
//!
//! * the OWN surface ([`SubtypeChecker::function_apparent_object_shape`]) —
//!   used for the weak-type rule (TS2559), which scans a function's own
//!   properties and must NOT expand to the wider `Function` interface; and
//! * the FULL apparent surface
//!   ([`SubtypeChecker::function_apparent_full_object_shape`]) — used to satisfy
//!   a non-weak target's required properties.

use super::*;

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Build the OWN apparent `ObjectShape` of a bare function/constructor
    /// source. A function value has no user-declared members, but it exposes
    /// stable apparent properties: `call`/`apply` for a callable, `prototype`
    /// for a constructor. Modeling these as *required* properties keeps the
    /// source from being mistaken for a weak shape, so the weak-type rejection
    /// in `check_object_subtype` fires for a standalone or union-member
    /// all-optional target the function shares no name with — while an
    /// intersection-member target (weak rule suppressed) and an optional target
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

    /// Build the *full* apparent `ObjectShape` of a function-like source for
    /// relating against a non-weak object target: the source's OWN members
    /// (`own_shape`) unioned with the real global `Function` interface members,
    /// carrying their declared types (`length: number`, `name: string`, `bind`,
    /// `call`, `apply`, `arguments`, `caller`, `prototype`, ...). This mirrors
    /// `tsc`'s `getApparentType`, which relates a function value against the
    /// `Function` interface for member lookup. OWN members win over interface
    /// members of the same name. When no lib is loaded (the boxed-type registry
    /// is empty) the `Function` surface is unavailable and this degrades to the
    /// OWN stub, preserving `noLib` behavior.
    pub(crate) fn function_apparent_full_object_shape(
        &self,
        own_shape: &ObjectShape,
    ) -> ObjectShape {
        let mut properties = own_shape.properties.clone();
        if let Some(function_shape) = self.global_function_interface_shape() {
            // Union in each `Function` member the OWN set does not already
            // declare (OWN members win). Iterating the interned shape's slice
            // avoids cloning the whole member list up front — only the members
            // actually appended are cloned.
            for prop in &function_shape.properties {
                if !properties.iter().any(|existing| existing.name == prop.name) {
                    properties.push(prop.clone());
                }
            }
        }
        // `check_object_subtype`'s merge scan expects source properties sorted by
        // name (`Atom`).
        properties.sort_by_key(|p| p.name);
        ObjectShape {
            flags: ObjectFlags::empty(),
            properties,
            string_index: own_shape.string_index,
            number_index: own_shape.number_index,
            symbol_index: own_shape.symbol_index,
            symbol: own_shape.symbol,
        }
    }

    /// The interned `ObjectShape` of the real global `Function` interface, whose
    /// members carry their declared types. Resolved by identity through the
    /// boxed-type registry rather than by name, so it is not keyed on any user
    /// identifier. Returns `None` when no lib is loaded (the registry is empty,
    /// e.g. `noLib`) or the interface has no extractable, non-empty object shape
    /// — callers fall back to the apparent stub.
    fn global_function_interface_shape(&self) -> Option<std::sync::Arc<ObjectShape>> {
        let boxed = self
            .resolver
            .get_boxed_type(IntrinsicKind::Function)
            .or_else(|| self.interner.get_boxed_type(IntrinsicKind::Function))?;
        let mut extractor =
            crate::relations::compat::ShapeExtractor::new(self.interner, self.resolver);
        let shape_id = extractor.extract(boxed)?;
        let shape = self
            .interner
            .object_shape(crate::types::ObjectShapeId(shape_id));
        (!shape.properties.is_empty()).then_some(shape)
    }
}
