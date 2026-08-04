//! Literal and enum-member widening entry points on `CheckerState`.
//!
//! Widens fresh literal types to their primitives and enum-member literals to
//! their parent enum type for mutable bindings, display, and operator-error
//! messages. Backed by `query_boundaries::widening` and `query_boundaries::common`.
use crate::query_boundaries::enum_analysis as enum_query;
use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Read-time `unique symbol` widening for a variable binding, mirroring
    /// tsc's `widenTypeForVariableLikeDeclaration` — the unique-symbol arm of
    /// `getWidenedTypeForVariableLikeDeclaration`, applied inside
    /// `getTypeOfSymbol`. A bare `unique symbol` that is an *alias* of another
    /// symbol's unique identity and is bound by a variable declaration with no
    /// type annotation reads as `symbol` (`let p = cs` / `const p = cs` /
    /// `var p = cs`). tsc's exact guard is
    /// `(isBindingElement(decl) || !decl.type) && type.symbol !== getSymbolOfDeclaration(decl)`:
    /// a freshly minted `const s = Symbol()` — whose unique symbol's owning
    /// symbol *is* the declaration — and an explicit `typeof`/`unique symbol`
    /// annotation both keep the unique identity.
    ///
    /// This is the CHECK-side counterpart of the DTS emitter's own
    /// `getWidenedUniqueESSymbolType` read-widening
    /// (`symbol_has_unique_symbol_type` / `widen_unique_symbol_value_type_for_dts`).
    /// It transforms the *returned* declared type only and is never written back
    /// to `symbol_types`, so the emitter's cache-based factory detection — and
    /// every other consumer that reads the raw cache — is unaffected; only reads
    /// routed through `get_type_of_symbol` observe the widened `symbol`.
    ///
    /// A fresh object/array *literal* binding widens the same way in its mutable
    /// element positions (`const o = { m: cs }` reads `{ m: symbol }`,
    /// `const a = [cs]` reads `symbol[]`), because tsc's `getWidenedType` applies
    /// `getWidenedUniqueESSymbolType` recursively through the literal. This too is
    /// read-only, so the raw cache the emitter reads keeps `typeof cs` and its DTS
    /// output is byte-identical.
    pub(crate) fn widen_read_unique_symbol_binding(
        &self,
        sym_id: SymbolId,
        type_id: TypeId,
    ) -> TypeId {
        // Fast reject on the `get_type_of_symbol` hot path. Only a bare
        // `unique symbol` (single interner lookup, no `Lazy` resolution) or a
        // plain object/array literal shape can carry a widenable unique symbol.
        let is_bare =
            crate::query_boundaries::common::is_unique_symbol_type(self.ctx.types, type_id);
        let is_literal_shape = !is_bare
            && crate::query_boundaries::widening::is_plain_object_or_array_shape(
                self.ctx.types,
                type_id,
            );
        if !is_bare && !is_literal_shape {
            return type_id;
        }
        // tsc `type.symbol !== getSymbolOfDeclaration(decl)`: a bare mint site,
        // whose unique symbol's owning symbol is itself, keeps `unique symbol`.
        // (A nested element is never its binding's mint site, so this only gates
        // the bare case.)
        if is_bare {
            match crate::query_boundaries::common::unique_symbol_ref(self.ctx.types, type_id) {
                Some(sym_ref) if sym_ref.0 == sym_id.0 => return type_id,
                Some(_) => {}
                None => return type_id,
            }
        }
        // tsc `(isBindingElement(decl) || !decl.type)`: only an *inferred*
        // variable-declaration binding widens; an explicit annotation preserves
        // the identity. Scoping to same-file variable declarations excludes class
        // fields and destructuring elements (separate widening owners).
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return type_id;
        };
        let Some(node) = self.ctx.arena.get(symbol.value_declaration) else {
            return type_id;
        };
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) else {
            return type_id;
        };
        if var_decl.type_annotation.is_some() {
            return type_id;
        }
        if is_bare {
            return TypeId::SYMBOL;
        }
        // Fresh object/array literal: widen bare `unique symbol` aliases in the
        // mutable element positions (`readonly`/`as const` positions preserved).
        // A non-fresh compound initializer (a call result, a plain identifier
        // reference) keeps its literal element types.
        if self.is_fresh_literal_expression(var_decl.initializer) {
            return crate::query_boundaries::widening::widen_unique_symbol_literal_elements(
                self.ctx.types,
                type_id,
            );
        }
        type_id
    }

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
        self.widen_initializer_type_for_mutable_binding_impl(type_id, true)
    }

    /// Like [`Self::widen_initializer_type_for_mutable_binding`], but gates the
    /// non-strict nullish-to-`any` deep widen on `initializer`'s own widening
    /// provenance (#16384 leg B). tsc's nullish widening flavour belongs to the
    /// *expression* (`null`/`undefined` keyword, or the global `undefined`),
    /// not the type — `declare var q: undefined; var av = [q];` must keep
    /// `undefined[]`, not widen to `any[]`, because `q` is a declared value
    /// rather than a widening source. See
    /// [`Self::initializer_nullish_leaves_are_widening`] for the walk and its
    /// fail-closed policy on shapes it cannot account for.
    pub(crate) fn widen_initializer_type_for_mutable_binding_gated(
        &mut self,
        type_id: TypeId,
        initializer: tsz_parser::parser::NodeIndex,
    ) -> TypeId {
        let nullish_widening_allowed = self.initializer_nullish_leaves_are_widening(initializer);
        self.widen_initializer_type_for_mutable_binding_impl(type_id, nullish_widening_allowed)
    }

    fn widen_initializer_type_for_mutable_binding_impl(
        &mut self,
        type_id: TypeId,
        nullish_widening_allowed: bool,
    ) -> TypeId {
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
        if self.ctx.strict_null_checks() || !nullish_widening_allowed {
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
