//! Rest-binding type computation for destructuring patterns.
//!
//! Handles `{ a, ...rest } = obj` and `[a, ...rest] = arr` patterns,
//! computing the rest type by omitting named sibling properties.

use crate::query_boundaries::state::checking as query;
use crate::query_boundaries::type_checking_utilities;
use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::{SyntaxKind, keyword_to_text_static};
use tsz_solver::{TypeId, Visibility};

impl<'a> CheckerState<'a> {
    /// Compute the rest type for an object destructuring rest element.
    ///
    /// Given `{ a, b, ...rest } = expr`, computes the type of `rest` by removing
    /// the properties named `a` and `b` from the source type.
    ///
    /// Computed property keys (e.g. `{ [key]: _ }`) whose expressions do not
    /// resolve to a static string literal are handled separately: for generic
    /// sources the expression's TypeId is used directly as the second argument
    /// to `Omit<T, K>`; for concrete sources the expression's type is evaluated
    /// and, if it resolves to string literals, those names are added to the
    /// static exclusion set.
    ///
    /// For union types, the rest type is computed independently for each member
    /// and the results are unioned.
    ///
    /// For type parameters, `Omit<T, K>` is constructed when the lib alias is
    /// available and there is at least one key to omit. Otherwise `T` is
    /// returned unchanged so the function's inferred return type preserves `T`'s
    /// identity.
    pub(crate) fn compute_object_rest_type(
        &mut self,
        pattern_idx: NodeIndex,
        parent_type: TypeId,
    ) -> TypeId {
        let (excluded, computed_key_exprs) = self.collect_non_rest_key_info(pattern_idx);

        let mut computed_string_keys: Vec<String> = Vec::new();
        let mut computed_key_type_ids: Vec<TypeId> = Vec::new();
        for &expr_idx in &computed_key_exprs {
            let key_type = self.get_type_of_node(expr_idx);
            if matches!(key_type, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN) {
                continue;
            }
            let string_atoms = query::extract_string_literal_keys(self.ctx.types, key_type);
            if !string_atoms.is_empty() {
                for atom in string_atoms {
                    let s = self.ctx.types.resolve_atom(atom).to_string();
                    if !computed_string_keys.contains(&s) {
                        computed_string_keys.push(s);
                    }
                }
            } else if type_checking_utilities::get_invalid_index_type_member_strict(
                self.ctx.types,
                key_type,
            )
            .is_none()
            {
                computed_key_type_ids.push(key_type);
            }
        }

        // For type parameters, build `Omit<T, K>`. K is the union of static string-literal
        // types from explicit and computed siblings, non-literal TypeIds from computed key
        // expressions (e.g. `K` itself), and non-spreadable prototype member names from the
        // constraint. Fall back to returning `T` unchanged when the lib alias is unavailable
        // or there are no keys to omit.
        let is_type_param = query::is_type_parameter(self.ctx.types, parent_type);
        if is_type_param {
            let mut string_keys: Vec<String> = excluded;
            for name in &computed_string_keys {
                if !string_keys.iter().any(|k| k == name) {
                    string_keys.push(name.clone());
                }
            }
            let unspreadable = self.collect_unspreadable_prototype_names_from(parent_type);
            for name in unspreadable {
                if !string_keys.iter().any(|k| k == &name) {
                    string_keys.push(name);
                }
            }
            let local_omit_sym = self
                .ctx
                .global_file_locals_index
                .as_ref()
                .and_then(|idx| idx.get("Omit"))
                .and_then(|entries| {
                    entries.iter().find_map(|&(file_idx, sym_id)| {
                        if file_idx != self.ctx.current_file_idx
                            || self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
                        {
                            return None;
                        }
                        self.ctx
                            .get_binder_for_file(file_idx)
                            .or(Some(self.ctx.binder))
                            .and_then(|binder| binder.get_symbol(sym_id))
                            .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::TYPE_ALIAS))
                            .then_some(sym_id)
                    })
                })
                .or_else(|| {
                    self.ctx.binder.file_locals.get("Omit").and_then(|sym_id| {
                        (!self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
                            && self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                                symbol.has_any_flags(symbol_flags::TYPE_ALIAS)
                            }))
                        .then_some(sym_id)
                    })
                });
            let omit_type = self
                .resolve_lib_type_by_name("Omit")
                .or_else(|| local_omit_sym.map(|sym_id| self.type_reference_symbol_type(sym_id)));
            if (!string_keys.is_empty() || !computed_key_type_ids.is_empty())
                && let Some(omit_type) = omit_type
            {
                let factory = self.ctx.types.factory();
                let mut key_args: Vec<TypeId> = string_keys
                    .iter()
                    .map(|n| factory.literal_string(n))
                    .collect();
                key_args.extend_from_slice(&computed_key_type_ids);
                let key_arg = if key_args.len() == 1 {
                    key_args[0]
                } else {
                    factory.union(key_args)
                };
                return factory.application(omit_type, vec![parent_type, key_arg]);
            }
            return parent_type;
        }

        let mut all_excluded = excluded;
        for s in computed_string_keys {
            if !all_excluded.contains(&s) {
                all_excluded.push(s);
            }
        }

        if let Some(members) = query::union_members(self.ctx.types, parent_type) {
            let rest_types: Vec<TypeId> = members
                .iter()
                .map(|&m| self.omit_properties_from_type(m, &all_excluded))
                .collect();
            return if rest_types.len() == 1 {
                rest_types[0]
            } else {
                self.ctx.types.factory().union(rest_types)
            };
        }

        self.omit_properties_from_type(parent_type, &all_excluded)
    }

    /// Collect static property names from all non-rest sibling elements in
    /// an object binding pattern.
    pub(crate) fn collect_non_rest_property_names(&self, pattern_idx: NodeIndex) -> Vec<String> {
        self.collect_non_rest_key_info(pattern_idx).0
    }

    /// Single-pass traversal of an object binding pattern's non-rest sibling elements.
    ///
    /// Returns `(static_names, dynamic_computed_expr_indices)` where:
    /// - `static_names`: keys that resolve to a string at parse time (shorthand,
    ///   identifier, string literal, or computed string-literal).
    /// - `dynamic_computed_expr_indices`: expression `NodeIndex`es for computed keys
    ///   that could not be resolved to a static string — the caller must look up their
    ///   types to handle them.
    fn collect_non_rest_key_info(&self, pattern_idx: NodeIndex) -> (Vec<String>, Vec<NodeIndex>) {
        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return (Vec::new(), Vec::new());
        };
        let Some(pattern_data) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return (Vec::new(), Vec::new());
        };

        let mut names = Vec::new();
        let mut dynamic_exprs = Vec::new();

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
            if element_data.dot_dot_dot_token {
                continue;
            }

            if element_data.property_name.is_some() {
                let Some(prop_node) = self.ctx.arena.get(element_data.property_name) else {
                    continue;
                };
                if let Some(keyword) =
                    SyntaxKind::try_from_u16(prop_node.kind).and_then(keyword_to_text_static)
                {
                    names.push(keyword.to_string());
                } else if let Some(ident) = self.ctx.arena.get_identifier(prop_node) {
                    names.push(ident.escaped_text.clone());
                } else if let Some(lit) = self.ctx.arena.get_literal(prop_node) {
                    names.push(lit.text.clone());
                } else if let Some(computed) = self.ctx.arena.get_computed_property(prop_node) {
                    if let Some(lit) = self
                        .ctx
                        .arena
                        .get(computed.expression)
                        .and_then(|expr| self.ctx.arena.get_literal(expr))
                    {
                        // Computed key with string literal: `{ ['b']: v }`.
                        names.push(lit.text.clone());
                    } else {
                        // Dynamic computed key: `{ [key]: v }` — caller resolves the type.
                        dynamic_exprs.push(computed.expression);
                    }
                }
            } else if let Some(ident) = self
                .ctx
                .arena
                .get(element_data.name)
                .and_then(|n| self.ctx.arena.get_identifier(n))
            {
                names.push(ident.escaped_text.clone());
            }
        }

        (names, dynamic_exprs)
    }

    /// Collect the names of public prototype members (methods, getters,
    /// setters) reachable through `type_id`'s shape — or, when `type_id` is
    /// a type parameter without a direct shape, through its constraint.
    ///
    /// These are the keys tsc adds to `K` in `Omit<T, K>` for object-rest
    /// destructuring (see tsc's `isSpreadablePropertyOfClass`). Private and
    /// protected members are NOT included: they are filtered out by
    /// `keyof T` and so are already absent from the rest type.
    ///
    /// Order matches tsc's printed `K`: methods first, then accessors
    /// (getters/setters) in source declaration order. This keeps the
    /// `Omit<T, "method" | "getter" | "setter">` rendering identical between
    /// tsz and tsc.
    pub(crate) fn collect_unspreadable_prototype_names_from(&self, type_id: TypeId) -> Vec<String> {
        let shape = query::object_shape(self.ctx.types, type_id).or_else(|| {
            let constraint = query::type_parameter_constraint(self.ctx.types, type_id)?;
            query::object_shape(self.ctx.types, constraint)
        });
        let Some(shape) = shape else {
            return Vec::new();
        };
        let mut methods: Vec<(u32, String)> = Vec::new();
        let mut accessors: Vec<(u32, String)> = Vec::new();
        for prop in shape.properties.iter() {
            if !(prop.is_class_prototype && prop.visibility == Visibility::Public) {
                continue;
            }
            let name = self.ctx.types.resolve_atom(prop.name).to_string();
            if prop.is_method {
                methods.push((prop.declaration_order, name));
            } else {
                accessors.push((prop.declaration_order, name));
            }
        }
        methods.sort_by_key(|(o, _)| *o);
        accessors.sort_by_key(|(o, _)| *o);
        let mut out: Vec<String> = methods.into_iter().map(|(_, n)| n).collect();
        out.extend(accessors.into_iter().map(|(_, n)| n));
        out
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
