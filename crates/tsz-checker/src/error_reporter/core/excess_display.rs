//! Excess property diagnostic display helpers.

use crate::state::CheckerState;
use tsz_common::Atom;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter::core) fn is_generic_excess_union_member(
        &self,
        member: TypeId,
        evaluated: TypeId,
    ) -> bool {
        let alias = self.ctx.types.get_display_alias(member);
        crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, member)
            || crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, evaluated)
            || alias.is_some_and(|alias| {
                crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, alias)
                    || crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        alias,
                    )
            })
            || crate::query_boundaries::common::contains_type_parameters(self.ctx.types, evaluated)
            || crate::query_boundaries::common::contains_type_parameters(self.ctx.types, member)
            || self.intersection_contains_type_parameter_like(member)
            || self.intersection_contains_type_parameter_like(evaluated)
    }

    fn intersection_contains_type_parameter_like(&self, ty: TypeId) -> bool {
        crate::query_boundaries::common::intersection_members(self.ctx.types, ty).is_some_and(
            |members| {
                members.iter().any(|member| {
                    let evaluated =
                        crate::query_boundaries::common::evaluate_type(self.ctx.types, *member);
                    crate::query_boundaries::common::is_type_parameter_like(
                        self.ctx.types,
                        evaluated,
                    ) || crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        evaluated,
                    )
                })
            },
        )
    }

    pub(in crate::error_reporter::core) fn format_intersection_union_for_excess_display(
        &mut self,
        ty: TypeId,
    ) -> Option<String> {
        let members = crate::query_boundaries::common::union_members(self.ctx.types, ty)?;
        // Excess-property reporting on a union with both generic and concrete
        // members (e.g. `T & { prop: boolean } | { name: string }`) only applies
        // to the non-generic members in tsc — generic members can be
        // instantiated to anything, so an "excess" property might still be
        // valid there. Filter the generic members out before formatting so the
        // diagnostic message points at the concrete shape that actually
        // rejects the property.
        let display_members = members
            .iter()
            .copied()
            .filter(|member| {
                let evaluated =
                    crate::query_boundaries::common::evaluate_type(self.ctx.types, *member);
                !self.is_generic_excess_union_member(*member, evaluated)
            })
            .collect::<Vec<_>>();
        let members = if display_members.is_empty() {
            members.as_slice()
        } else {
            display_members.as_slice()
        };
        let member_displays = members
            .iter()
            .map(|&member| self.format_excess_union_member(member))
            .collect::<Vec<_>>();
        let concrete_displays = member_displays
            .iter()
            .filter(|display| !Self::display_looks_generic_excess_union_member_for_display(display))
            .cloned()
            .collect::<Vec<_>>();
        if !concrete_displays.is_empty() && concrete_displays.len() < member_displays.len() {
            return Some(Self::join_excess_union_member_displays(concrete_displays));
        }
        if !member_displays
            .iter()
            .any(|display| display.contains(" & "))
        {
            // Single concrete member after filtering — fall back to the
            // standard pipeline so the union-wrapper isn't preserved as
            // `{name:string;}` when only one member remains.
            if members.len() == 1 && member_displays.len() == 1 {
                return Some(
                    member_displays
                        .into_iter()
                        .next()
                        .expect("single display remains after filtering"),
                );
            }
            return None;
        }
        Some(Self::join_excess_union_member_displays(member_displays))
    }

    fn join_excess_union_member_displays(member_displays: Vec<String>) -> String {
        member_displays
            .into_iter()
            .map(|display| {
                if display.contains(" & ") {
                    format!("({display})")
                } else {
                    display
                }
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }

    fn display_looks_generic_excess_union_member_for_display(display: &str) -> bool {
        let head = display
            .split_once(" & ")
            .map_or(display, |(head, _)| head)
            .trim_matches(['(', ')', ' ']);
        !head.is_empty()
            && head.len() <= 2
            && head
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    }

    fn format_excess_union_member(&mut self, ty: TypeId) -> String {
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, ty)
        {
            return members
                .iter()
                .map(|&member| self.format_excess_union_member(member))
                .collect::<Vec<_>>()
                .join(" & ");
        }

        if crate::query_boundaries::common::is_lazy_type(self.ctx.types, ty) {
            return self.format_type_diagnostic_widened(ty);
        }

        if let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty)
            && let Some(def_id) = self
                .ctx
                .definition_store
                .find_def_for_type(ty)
                .or_else(|| self.ctx.definition_store.find_def_by_shape(&shape))
            && let Some(def) = self.ctx.definition_store.get(def_id)
        {
            return self.ctx.types.resolve_atom_ref(def.name).to_string();
        }

        self.format_type_diagnostic_widened(ty)
    }

    pub(in crate::error_reporter::core) fn normalize_excess_display_object_type(
        &self,
        ty: TypeId,
    ) -> Option<TypeId> {
        let shape = crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty)?;
        let mut normalized = shape.as_ref().clone();
        let mut changed = false;

        for prop in &mut normalized.properties {
            let read = self.normalize_excess_display_type(prop.type_id);
            let write = self.normalize_excess_display_type(prop.write_type);
            let prop_name = self.ctx.types.resolve_atom_ref(prop.name);
            let preserve_literal_property = prop_name.as_ref() == "type";
            let read = if prop.readonly || preserve_literal_property {
                read
            } else {
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, read)
            };
            let write = if prop.readonly || preserve_literal_property {
                write
            } else {
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, write)
            };
            changed |= read != prop.type_id || write != prop.write_type;
            prop.type_id = read;
            prop.write_type = write;
        }

        if let Some(index) = normalized.string_index.as_mut() {
            let value = self.normalize_excess_display_type(index.value_type);
            let value = if index.readonly {
                value
            } else {
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, value)
            };
            changed |= value != index.value_type;
            index.value_type = value;
        }

        if let Some(index) = normalized.number_index.as_mut() {
            let value = self.normalize_excess_display_type(index.value_type);
            let value = if index.readonly {
                value
            } else {
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, value)
            };
            changed |= value != index.value_type;
            index.value_type = value;
        }

        Some(if changed {
            self.ctx.types.factory().object_with_index(normalized)
        } else {
            ty
        })
    }

    pub(in crate::error_reporter::core) fn strip_top_level_readonly_for_excess_display(
        &self,
        ty: TypeId,
    ) -> TypeId {
        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty)
        else {
            return ty;
        };

        let has_readonly_property = shape.properties.iter().any(|prop| prop.readonly);
        let has_readonly_index = shape
            .string_index
            .as_ref()
            .is_some_and(|index| index.readonly)
            || shape
                .number_index
                .as_ref()
                .is_some_and(|index| index.readonly);
        let display_props = self.ctx.types.get_display_properties(ty);
        let has_readonly_display_property = display_props
            .as_ref()
            .is_some_and(|props| props.iter().any(|prop| prop.readonly));

        if !has_readonly_property && !has_readonly_index && !has_readonly_display_property {
            return ty;
        }

        let mut normalized = shape.as_ref().clone();
        for prop in &mut normalized.properties {
            prop.readonly = false;
        }
        if let Some(index) = normalized.string_index.as_mut() {
            index.readonly = false;
        }
        if let Some(index) = normalized.number_index.as_mut() {
            index.readonly = false;
        }

        let normalized_ty = self.ctx.types.factory().object_with_index(normalized);
        if let Some(props) = display_props {
            let mut props = props.as_ref().clone();
            for prop in &mut props {
                prop.readonly = false;
            }
            self.ctx
                .types
                .store_display_properties(normalized_ty, props);
        }
        if let Some(alias_origin) = self.ctx.types.get_display_alias(ty) {
            self.ctx
                .types
                .store_display_alias(normalized_ty, alias_origin);
        }
        if let Some(def_id) = self.ctx.definition_store.find_def_for_type(ty) {
            self.ctx
                .definition_store
                .register_type_to_def(normalized_ty, def_id);
        }
        normalized_ty
    }

    pub(in crate::error_reporter::core) fn normalize_nested_excess_display_type(
        &self,
        ty: TypeId,
    ) -> TypeId {
        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty)
        else {
            return ty;
        };

        let mut normalized = shape.as_ref().clone();
        let mut changed = false;
        for prop in &mut normalized.properties {
            let should_normalize_nested =
                self.should_normalize_nested_excess_display_property(prop.type_id);
            let mut read = if should_normalize_nested {
                self.normalize_excess_display_type(prop.type_id)
            } else {
                prop.type_id
            };
            let mut write = if should_normalize_nested {
                self.normalize_excess_display_type(prop.write_type)
            } else {
                prop.write_type
            };
            if self.should_strip_readonly_deep_for_nested_object_property(read) {
                read = self.strip_readonly_deep_for_excess_display(read);
                write = self.strip_readonly_deep_for_excess_display(write);
            }
            changed |= read != prop.type_id || write != prop.write_type;
            prop.type_id = read;
            prop.write_type = write;
        }
        if changed {
            self.ctx.types.factory().object_with_index(normalized)
        } else {
            ty
        }
    }

    fn should_normalize_nested_excess_display_property(&self, ty: TypeId) -> bool {
        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty)
        else {
            return false;
        };

        // Excess-property messages preserve literal types at the top level, but
        // TypeScript widens anonymous nested object-literal properties. Do not
        // apply that display normalization to named/interface/application types
        // or index-signature containers such as Record<K, T>.
        !shape.properties.is_empty()
            && shape.string_index.is_none()
            && shape.number_index.is_none()
            && shape.properties.iter().all(|prop| {
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, prop.type_id)
                    .is_none()
            })
            && self.ctx.types.get_display_alias(ty).is_none()
            && self.ctx.definition_store.find_def_for_type(ty).is_none()
            && crate::query_boundaries::common::type_application(self.ctx.types, ty).is_none()
    }

    /// Decide whether a property whose value type is `ty` should have its
    /// readonly modifiers stripped deeply for the excess-property display.
    ///
    /// The structural rule: tsc displays an asserted-type property (e.g.
    /// `types: {} as { actors: { ... } }`) without readonly modifiers, while
    /// a sibling property whose value is a flat anonymous object (e.g.
    /// `invoke: { src: "str" }`) retains readonly modifiers picked up from
    /// the surrounding reverse-mapped contextual type.
    ///
    /// We approximate that distinction structurally: when the property's
    /// value is an anonymous object whose own shape contains at least one
    /// nested object-typed property, strip readonly deeply. Flat anonymous
    /// objects (whose properties are all leaves) and named/aliased/indexed
    /// types are left alone, matching tsc's display.
    fn should_strip_readonly_deep_for_nested_object_property(&self, ty: TypeId) -> bool {
        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty)
        else {
            return false;
        };

        // Restrict to anonymous, non-aliased, non-applied object literals.
        if self.ctx.types.get_display_alias(ty).is_some()
            || self.ctx.definition_store.find_def_for_type(ty).is_some()
            || crate::query_boundaries::common::type_application(self.ctx.types, ty).is_some()
            || shape.string_index.is_some()
            || shape.number_index.is_some()
            || shape.properties.is_empty()
        {
            return false;
        }

        // Apply only when the value contains at least one nested object-typed
        // property — that's the structural shape produced by `value as { ... }`
        // assertions whose inner types are themselves objects.
        shape.properties.iter().any(|prop| {
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, prop.type_id)
                .is_some()
        })
    }

    fn strip_readonly_deep_for_excess_display(&self, ty: TypeId) -> TypeId {
        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty)
        else {
            return ty;
        };

        let mut normalized = shape.as_ref().clone();
        let mut changed = false;
        for prop in &mut normalized.properties {
            let read = self.strip_readonly_deep_for_excess_display(prop.type_id);
            let write = self.strip_readonly_deep_for_excess_display(prop.write_type);
            changed |= prop.readonly || read != prop.type_id || write != prop.write_type;
            prop.readonly = false;
            prop.type_id = read;
            prop.write_type = write;
        }
        if let Some(index) = normalized.string_index.as_mut() {
            let value = self.strip_readonly_deep_for_excess_display(index.value_type);
            changed |= index.readonly || value != index.value_type;
            index.readonly = false;
            index.value_type = value;
        }
        if let Some(index) = normalized.number_index.as_mut() {
            let value = self.strip_readonly_deep_for_excess_display(index.value_type);
            changed |= index.readonly || value != index.value_type;
            index.readonly = false;
            index.value_type = value;
        }

        if changed {
            self.ctx.types.factory().object_with_index(normalized)
        } else {
            ty
        }
    }

    pub(in crate::error_reporter::core) fn narrow_excess_function_param_by_property_key(
        &self,
        prop_name: Atom,
        ty: TypeId,
    ) -> Option<TypeId> {
        let key_type = self.ctx.types.literal_string_atom(prop_name);
        if let Some(app) = crate::query_boundaries::common::type_application(self.ctx.types, ty) {
            let mut changed = false;
            let args = app
                .args
                .iter()
                .map(|&arg| {
                    if let Some(narrowed) =
                        self.narrow_excess_function_param_by_property_key(prop_name, arg)
                    {
                        changed = true;
                        narrowed
                    } else {
                        arg
                    }
                })
                .collect::<Vec<_>>();
            return changed.then(|| self.ctx.types.factory().application(app.base, args));
        }

        let members = crate::query_boundaries::common::union_members(self.ctx.types, ty)?;
        let type_atom = self.ctx.types.intern_string("type");
        let narrowed = members
            .iter()
            .copied()
            .filter(|member| {
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, *member)
                    .is_some_and(|shape| {
                        shape.properties.iter().any(|prop| {
                            prop.name == type_atom
                                && crate::query_boundaries::common::evaluate_type(
                                    self.ctx.types,
                                    prop.type_id,
                                ) == key_type
                        })
                    })
            })
            .collect::<Vec<_>>();
        (!narrowed.is_empty() && narrowed.len() < members.len())
            .then(|| tsz_solver::utils::union_or_single(self.ctx.types, narrowed))
    }

    pub(in crate::error_reporter) fn normalize_assignability_union_display_order(
        &self,
        display: String,
    ) -> String {
        let members = display.split(" | ").collect::<Vec<_>>();
        let [first, second] = members.as_slice() else {
            return display;
        };

        if let Some(base) = first.strip_suffix("[]")
            && base == *second
            && Self::is_simple_assignability_type_name(base)
        {
            return format!("{base} | {base}[]");
        }

        if first.starts_with("NonNullable<")
            && first.ends_with('>')
            && Self::is_simple_assignability_type_name(second)
        {
            return format!("{second} | {first}");
        }

        display
    }

    fn is_simple_assignability_type_name(text: &str) -> bool {
        !text.is_empty()
            && text
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '.')
    }
}
