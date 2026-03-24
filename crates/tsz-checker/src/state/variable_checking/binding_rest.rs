//! Rest-binding type computation for destructuring patterns.
//!
//! Handles `{ a, ...rest } = obj` and `[a, ...rest] = arr` patterns,
//! computing the rest type by omitting named sibling properties.

use crate::query_boundaries::state::checking as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Compute the rest type for an object destructuring rest element.
    ///
    /// Given `{ a, b, ...rest } = expr`, computes the type of `rest` by removing
    /// the properties named `a` and `b` from the source type.
    ///
    /// For union types, the rest type is computed independently for each member
    /// and the results are unioned.
    ///
    /// For type parameters, the rest type is the type parameter itself. We cannot
    /// express `Omit<T, K>` directly, so we preserve the type parameter's identity.
    /// This ensures that when a generic function returns `{ ...rest, b: a }`, the
    /// return type contains `T` and is properly instantiated at call sites.
    pub(crate) fn compute_object_rest_type(
        &mut self,
        pattern_idx: NodeIndex,
        parent_type: TypeId,
    ) -> TypeId {
        // Collect the names of all non-rest sibling properties in this binding pattern.
        let excluded = self.collect_non_rest_property_names(pattern_idx);

        // For type parameters, preserve the generic identity.
        // The rest of `T extends { a, b }` with `a` excluded is `Omit<T, "a">`,
        // which we approximate as T itself. This preserves T in the function's
        // inferred return type so that instantiation at call sites works correctly.
        // Without this, rest resolves to a concrete object from the constraint,
        // losing generic properties that only appear when T is instantiated.
        let is_type_param = query::type_parameter_constraint(self.ctx.types, parent_type).is_some();
        if is_type_param {
            return parent_type;
        }

        // For union types, compute rest type for each member and union them.
        if let Some(members) = query::union_members(self.ctx.types, parent_type) {
            let rest_types: Vec<TypeId> = members
                .iter()
                .map(|&m| self.omit_properties_from_type(m, &excluded))
                .collect();
            return if rest_types.len() == 1 {
                rest_types[0]
            } else {
                self.ctx.types.factory().union(rest_types)
            };
        }

        self.omit_properties_from_type(parent_type, &excluded)
    }

    /// Collect static property names from all non-rest sibling elements in
    /// an object binding pattern.
    fn collect_non_rest_property_names(&self, pattern_idx: NodeIndex) -> Vec<String> {
        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return Vec::new();
        };
        let Some(pattern_data) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return Vec::new();
        };

        let mut names = Vec::new();
        for &element_idx in pattern_data.elements.nodes.iter() {
            if element_idx.is_none() {
                continue;
            }
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }
            let Some(element_data) = self.ctx.arena.get_binding_element(element_node) else {
                continue;
            };
            // Skip rest elements — they are the target, not excluded.
            if element_data.dot_dot_dot_token {
                continue;
            }
            // Extract the property name (same logic as the main property_name extraction).
            let prop_name = if element_data.property_name.is_some() {
                if let Some(prop_node) = self.ctx.arena.get(element_data.property_name) {
                    // Try identifier first
                    if let Some(ident) = self.ctx.arena.get_identifier(prop_node) {
                        Some(ident.escaped_text.clone())
                    } else if let Some(lit) = self.ctx.arena.get_literal(prop_node) {
                        // String literal property name: { 'b': renamed }
                        Some(lit.text.clone())
                    } else if let Some(computed) = self.ctx.arena.get_computed_property(prop_node) {
                        // Computed property with string literal: { ['b']: renamed }
                        self.ctx
                            .arena
                            .get(computed.expression)
                            .and_then(|expr| self.ctx.arena.get_literal(expr))
                            .map(|lit| lit.text.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                // Shorthand: { x } — the name itself is the property name.
                self.ctx
                    .arena
                    .get(element_data.name)
                    .and_then(|n| self.ctx.arena.get_identifier(n))
                    .map(|ident| ident.escaped_text.clone())
            };
            if let Some(name) = prop_name {
                names.push(name);
            }
        }
        names
    }

    /// Create a new object type from `type_id` with the given property names excluded.
    pub(crate) fn omit_properties_from_type(
        &mut self,
        type_id: TypeId,
        excluded: &[String],
    ) -> TypeId {
        if matches!(type_id, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN) {
            return type_id;
        }

        let constraint = query::type_parameter_constraint(self.ctx.types, type_id);
        let shape = query::object_shape(self.ctx.types, type_id).or_else(|| {
            // For type parameters, use the constraint's shape so that
            // `{ a, ...rest } = obj` where `obj: T extends { a, b }` produces
            // rest without the excluded properties.  Without this, `rest` would
            // keep all of T's constraint properties and trigger false TS2783.
            let constraint = constraint?;
            query::object_shape(self.ctx.types, constraint)
        });

        // Object rest follows the same property-collection rules as object spread:
        // drop readonly, prototype members, private/protected members, and
        // compiler-only private-brand properties before excluding named siblings.
        let mut remaining_props = self.collect_object_spread_properties(type_id);
        if remaining_props.is_empty()
            && query::object_shape(self.ctx.types, type_id).is_none()
            && let Some(constraint) = constraint
        {
            remaining_props = self.collect_object_spread_properties(constraint);
        }

        let remaining_props: Vec<_> = remaining_props
            .iter()
            .filter(|prop| {
                let name = self.ctx.types.resolve_atom_ref(prop.name);
                !excluded.iter().any(|ex| ex == name.as_ref())
            })
            .cloned()
            .collect();

        let Some(shape) = shape else {
            return if !remaining_props.is_empty()
                || query::is_object_like_type(self.ctx.types, type_id)
            {
                self.ctx.types.factory().object(remaining_props)
            } else {
                type_id
            };
        };

        // Preserve index signatures and object flags for object-rest types.
        // Rest results are structural copies, so they must not retain the
        // source type's nominal symbol (e.g. class identity).
        if shape.string_index.is_some() || shape.number_index.is_some() {
            let mut rest_shape = shape.as_ref().clone();
            rest_shape.properties = remaining_props;
            rest_shape.symbol = None;
            self.ctx.types.factory().object_with_index(rest_shape)
        } else {
            self.ctx.types.factory().object_with_flags_and_symbol(
                remaining_props,
                shape.flags,
                None,
            )
        }
    }

    /// Rest bindings from tuple members should produce an array type.
    /// Variadic tuple members can already carry array types (`...T[]`), so avoid
    /// wrapping those into nested arrays.
    pub(crate) fn rest_binding_array_type(&self, tuple_member_type: TypeId) -> TypeId {
        let tuple_member_type = query::unwrap_readonly_deep(self.ctx.types, tuple_member_type);
        if query::array_element_type(self.ctx.types, tuple_member_type).is_some() {
            tuple_member_type
        } else {
            self.ctx.types.factory().array(tuple_member_type)
        }
    }
}
