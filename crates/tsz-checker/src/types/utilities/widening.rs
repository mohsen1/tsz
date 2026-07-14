//! Literal and enum-member widening entry points on `CheckerState`.
//!
//! Widens fresh literal types to their primitives and enum-member literals to
//! their parent enum type for mutable bindings, display, and operator-error
//! messages. Backed by `query_boundaries::widening` and `query_boundaries::common`.
use crate::query_boundaries::enum_analysis as enum_query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Widen a literal type to its primitive type.
    ///
    /// Converts literal types to their primitive types for widening (unannotated
    /// declarations, property assignments, return-type inference).
    ///
    /// ## Examples:
    /// ```typescript
    /// // Literal types are widened to primitives:
    /// let x = "hello";  // Type: string (not "hello")
    /// let y = 42;       // Type: number (not 42)
    /// let z = true;     // Type: boolean (not true)
    /// ```
    pub(crate) fn widen_literal_type(&self, type_id: TypeId) -> TypeId {
        crate::query_boundaries::common::widen_type(self.ctx.types, type_id)
    }

    /// Widen property types of object literal attrs for JSX generic inference.
    ///
    /// Only recurses into object types — top-level primitive literals
    /// (`"button"`, `12`, `true`) are preserved so string-literal JSX attrs
    /// keep precise types for conditional type matching.  Object property
    /// types are widened via `widen_literal_type` (e.g. `{ x: "y" }` becomes
    /// `{ x: string }`).
    pub(crate) fn widen_jsx_object_attr_type(&self, type_id: TypeId) -> TypeId {
        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)
        else {
            return type_id;
        };

        let mut new_props = Vec::with_capacity(shape.properties.len());
        let mut changed = false;
        for prop in &shape.properties {
            let widened_type = if prop.readonly {
                prop.type_id
            } else {
                crate::query_boundaries::common::widen_type(self.ctx.types, prop.type_id)
            };
            let widened_write_type = if prop.readonly {
                prop.write_type
            } else {
                crate::query_boundaries::common::widen_type(self.ctx.types, prop.write_type)
            };
            if widened_type != prop.type_id || widened_write_type != prop.write_type {
                changed = true;
            }
            let mut new_prop = prop.clone();
            new_prop.type_id = widened_type;
            new_prop.write_type = widened_write_type;
            new_props.push(new_prop);
        }

        if changed {
            self.ctx.types.factory().object(new_props)
        } else {
            type_id
        }
    }

    /// Widen a type for diagnostic display purposes.
    ///
    /// Like `widen_literal_type` but preserves boolean literal intrinsics
    /// (`true`/`false`), so narrowed types like `string | false` display
    /// correctly instead of being widened to `string | boolean`.
    pub(crate) fn widen_type_for_display(&self, type_id: TypeId) -> TypeId {
        crate::query_boundaries::common::widen_type_for_display(self.ctx.types, type_id)
    }

    /// The parent enum `DefId` for an enum-member type, or `None` when `type_id`
    /// is not an enum member. Both widening entry points below resolve this
    /// `DefId` to the enum *type* `E` — never `get_type_of_symbol(parent)`, which
    /// in value context yields the enum *object* type (`typeof E`). tsc's
    /// `getBaseTypeOfEnumLikeType` widens an enum-member literal to the enum
    /// type, never its static/object shape.
    fn enum_member_parent_def_id(&mut self, type_id: TypeId) -> Option<tsz_solver::DefId> {
        enum_query::enum_member_parent_def_id(&self.ctx, type_id)
    }

    /// The parent enum type `E` for an enum-member type as a **binding** type: a
    /// `Lazy(parent_def)` semantic ref (matching the solver's
    /// `common_parent_enum_type`). Kept as a `Lazy` because, as a variable's
    /// declared type, it must answer a `typeof` query with the enum type `E`
    /// (`typeof stage` → `Phase`); the resolved `Enum(def, structural)` form gets
    /// object-converted by the value-position `typeof` path.
    fn enum_member_widened_binding_type(&mut self, type_id: TypeId) -> Option<TypeId> {
        let parent = self.enum_member_parent_def_id(type_id)?;
        Some(self.ctx.types.factory().lazy(parent))
    }

    /// The parent enum type `E` for an enum-member type as a **display** type:
    /// the concrete `Enum(parent_def, structural)` body published on the parent
    /// def, so downstream display reductions (e.g. `Hue | 1` → `number` in a
    /// `TS2367` message) see the numeric structural. `get_def` returns the enum
    /// type, never the object type — that lives in a separate
    /// `enum_namespace_types` map. Falls back to a `Lazy` ref when the body has
    /// not been published yet.
    fn enum_member_widened_display_type(&mut self, type_id: TypeId) -> Option<TypeId> {
        let parent = self.enum_member_parent_def_id(type_id)?;
        let concrete = self
            .ctx
            .type_env
            .try_borrow()
            .ok()
            .and_then(|env| env.get_def(parent))
            .filter(|&body| crate::query_boundaries::common::is_enum_type(self.ctx.types, body));
        Some(concrete.unwrap_or_else(|| self.ctx.types.factory().lazy(parent)))
    }

    /// Widen a mutable binding initializer type (let/var semantics).
    ///
    /// In addition to primitive literal widening, TypeScript widens enum member
    /// initializers (`let x = E.A`) to the parent enum type (`E`), not the
    /// specific member.
    pub(crate) fn widen_initializer_type_for_mutable_binding(&mut self, type_id: TypeId) -> TypeId {
        // Enum member → parent enum type `E` (the union of member literals),
        // never the enum object type `typeof E`.
        if let Some(parent) = self.enum_member_widened_binding_type(type_id) {
            return parent;
        }
        // Use the mutable-binding widening entry so fresh array/object members
        // nested inside a top-level union widen too. tsc collapses a conditional
        // over array literals (`cond ? [1, 2, 3] : [4, 5]`) to `number[]`; the
        // plain literal-widening path would keep `(1 | 2 | 3)[] | (4 | 5)[]`,
        // whose later `.push` parameter contravariantly intersects to `never`.
        let widened = crate::query_boundaries::widening::widen_type_for_mutable_binding(
            self.ctx.types,
            type_id,
        );
        if self.ctx.strict_null_checks() {
            widened
        } else {
            // tsc widens null/undefined to `any` in inferred positions when
            // strictNullChecks is off (`var x = null` types as any; fresh
            // structure maps too: `[undefined, null]` → `[any, any]`).
            crate::query_boundaries::widening::widen_nullish_to_any_deep(self.ctx.types, widened)
        }
    }

    /// Widen only enum member types to their parent enum type.
    ///
    /// Unlike `widen_initializer_type_for_mutable_binding`, this does NOT widen
    /// literal types (e.g., `2` stays `2`, not `number`). This is used in operator
    /// error messages where tsc preserves literal types but widens enum members.
    pub(crate) fn widen_enum_member_type(&mut self, type_id: TypeId) -> TypeId {
        // Enum member → parent enum type `E`, never `typeof E`. Uses the concrete
        // display form so operator-error reductions see the enum's structural.
        self.enum_member_widened_display_type(type_id)
            // Do NOT widen literal types - return as-is
            .unwrap_or(type_id)
    }

    /// Whether `type_id` is a specific enum *member* (has a parent enum), as
    /// opposed to a whole enum type — the widening gate for inferred generator
    /// yield types (#15634).
    pub(crate) fn is_enum_member_type_for_widening(&self, type_id: TypeId) -> bool {
        if let Some(def_id) = crate::query_boundaries::common::enum_def_id(self.ctx.types, type_id)
        {
            // A member DefId has a parent enum; the whole enum type does not.
            return self
                .ctx
                .type_env
                .try_borrow()
                .ok()
                .is_some_and(|env| env.get_enum_parent(def_id).is_some());
        }
        false
    }
}
