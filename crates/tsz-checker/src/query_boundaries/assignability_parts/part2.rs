impl TargetPropertyIndex {
    fn insert(&mut self, prop: &PropertyInfo) {
        self.by_atom.entry(prop.name).or_insert(prop.type_id);
        self.fallback_order.push((prop.name, prop.type_id));
    }

    fn matching_type_for(
        &self,
        db: &dyn TypeDatabase,
        source_prop: &PropertyInfo,
    ) -> Option<TypeId> {
        if let Some(target_type) = self.by_atom.get(&source_prop.name).copied() {
            return Some(target_type);
        }

        tsz_common::perf_counters::record_property_classification_string_fallback_source_lookup();
        self.matching_type_by_resolved_name(db, source_prop.name)
    }

    fn matching_type_by_resolved_name(
        &self,
        db: &dyn TypeDatabase,
        source_name: Atom,
    ) -> Option<TypeId> {
        let source_text = db.resolve_atom_ref(source_name);
        self.fallback_order
            .iter()
            .find_map(|(target_name, target_type)| {
                tsz_common::perf_counters::record_property_classification_string_fallback_target_name();
                let target_text = db.resolve_atom_ref(*target_name);
                if target_text.as_ref() == source_text.as_ref() {
                    tsz_common::perf_counters::record_property_classification_string_fallback_target_type();
                    Some(*target_type)
                } else {
                    None
                }
            })
    }
}

/// Collect all property names and their types from a target type.
///
/// For unions, uses the type from the first member that has the property.
/// For intersections, uses the type from the first member that has the property.
fn collect_target_property_index(db: &dyn TypeDatabase, target: TypeId) -> TargetPropertyIndex {
    use super::common::{intersection_members, union_members};
    let mut props = TargetPropertyIndex::default();

    if let Some(shape) =
        crate::query_boundaries::common::get_merged_object_shape_for_type(db, target)
    {
        for prop in shape.properties.iter() {
            props.insert(prop);
        }
    }

    if let Some(members) = union_members(db, target) {
        for &member in &members {
            if let Some(shape) =
                crate::query_boundaries::common::get_merged_object_shape_for_type(db, member)
            {
                for prop in shape.properties.iter() {
                    props.insert(prop);
                }
            }
        }
    }

    if let Some(members) = intersection_members(db, target) {
        for &member in members.iter() {
            if let Some(shape) =
                crate::query_boundaries::common::get_merged_object_shape_for_type(db, member)
            {
                for prop in shape.properties.iter() {
                    props.insert(prop);
                }
            }
        }
    }

    props
}

/// Check if an object shape represents the global Object or Function interface.
///
/// These types have only inherited method properties and should suppress
/// excess property checking. This is the canonical boundary-level check,
/// replacing the checker-local `is_global_object_or_function_shape`.
///
/// Public boundary variant for checker code that needs to check a pre-resolved shape.
pub(crate) fn is_global_object_or_function_shape_boundary(
    db: &dyn TypeDatabase,
    shape: &tsz_solver::ObjectShape,
) -> bool {
    is_global_object_or_function_shape(db, shape)
}

fn is_global_object_or_function_shape(
    db: &dyn TypeDatabase,
    shape: &tsz_solver::ObjectShape,
) -> bool {
    static OBJECT_PROTO: &[&str] = &[
        "constructor",
        "toString",
        "toLocaleString",
        "valueOf",
        "hasOwnProperty",
        "isPrototypeOf",
        "propertyIsEnumerable",
    ];
    static FUNCTION_PROTO: &[&str] = &[
        "apply",
        "call",
        "bind",
        "toString",
        "length",
        "arguments",
        "caller",
        "prototype",
        "constructor",
        "toLocaleString",
        "valueOf",
        "hasOwnProperty",
        "isPrototypeOf",
        "propertyIsEnumerable",
    ];

    if shape.properties.is_empty() {
        return false;
    }

    shape.properties.iter().all(|prop| {
        let name = db.resolve_atom_ref(prop.name);
        OBJECT_PROTO.contains(&name.as_ref()) || FUNCTION_PROTO.contains(&name.as_ref())
    })
}

/// Explain a same-generic application failure (`C<A..>` vs `C<B..>`) via the
/// differing type arguments, mirroring tsc.
///
/// Must be called on the **raw** (unevaluated) operands so the application
/// structure survives; returns `None` unless the failure reliably reduces to a
/// concrete type argument, in which case the structural analysis should run.
pub(crate) fn same_generic_application_failure_reason<
    R: tsz_solver::relations::subtype::TypeResolver,
>(
    db: &dyn TypeDatabase,
    ctx: &crate::context::CheckerContext<'_>,
    resolver: &R,
    source: TypeId,
    target: TypeId,
) -> Option<SubtypeFailureReason> {
    tsz_solver::relations::relation_queries::explain_same_generic_application_with_resolver(
        db,
        resolver,
        source,
        target,
        |checker| ctx.configure_compat_checker(checker),
    )
}

/// Variance-aware Application-to-Application assignability check.
///
/// When both source and target are Applications with the same base type,
/// uses computed variance to check arguments without structural expansion.
/// Must be called BEFORE types are evaluated/expanded.
///
/// Returns `Some(true/false)` if conclusive, `None` to fall through.
pub(crate) fn check_application_variance_assignability<
    R: tsz_solver::relations::subtype::TypeResolver,
>(
    inputs: &AssignabilityQueryInputs<'_, R>,
) -> Option<bool> {
    let AssignabilityQueryInputs {
        db,
        resolver,
        source,
        target,
        flags,
        inheritance_graph,
        sound_mode,
    } = *inputs;
    let policy = relation_policy::from_checker_flags_u16(flags)
        .with_strict_subtype_checking(sound_mode)
        .with_strict_any_propagation(sound_mode);
    let context = tsz_solver::relations::relation_queries::RelationContext {
        query_db: Some(db),
        inheritance_graph: Some(inheritance_graph),
        class_check: None,
    };
    tsz_solver::relations::relation_queries::check_application_variance(
        db.as_type_database(),
        resolver,
        Some(db),
        source,
        target,
        policy,
        context,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_solver::construction::TypeInterner;
    use tsz_solver::def::DefId;
    use tsz_solver::{IndexSignature, MappedModifier, MappedType, TypeParamInfo};

    #[test]
    fn target_property_index_uses_first_atom_match() {
        let db = TypeInterner::new();
        let name = db.intern_string("renamed");
        let mut index = TargetPropertyIndex::default();

        index.insert(&PropertyInfo::new(name, TypeId::STRING));
        index.insert(&PropertyInfo::new(name, TypeId::NUMBER));

        let source = PropertyInfo::new(name, TypeId::BOOLEAN);
        assert_eq!(index.matching_type_for(&db, &source), Some(TypeId::STRING));
    }

    #[test]
    fn target_property_index_keeps_string_fallback() {
        let db = TypeInterner::new();
        let name = db.intern_string("fallbackName");
        let mut index = TargetPropertyIndex::default();

        index.fallback_order.push((name, TypeId::NUMBER));

        assert_eq!(
            index.matching_type_by_resolved_name(&db, name),
            Some(TypeId::NUMBER)
        );
    }

    #[test]
    fn symbol_named_source_property_is_accepted_by_property_key_index_signature() {
        let db = TypeInterner::new();
        let property_key = db.union3(TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL);
        let target = db.object_with_index(ObjectShape {
            string_index: Some(IndexSignature {
                key_type: property_key,
                value_type: TypeId::STRING,
                readonly: false,
                param_name: None,
            }),
            ..ObjectShape::default()
        });
        let mut source_prop =
            PropertyInfo::new(db.intern_string("[Symbol.iterator]"), TypeId::STRING);
        source_prop.is_symbol_named = true;
        let source = db.object(vec![source_prop]);

        let classification =
            classify_object_properties(&db, source, target).expect("object classification");

        assert!(classification.excess_properties.is_empty());
    }

    #[test]
    fn symbol_named_source_property_is_excess_for_plain_string_index_signature() {
        let db = TypeInterner::new();
        let target = db.object_with_index(ObjectShape {
            string_index: Some(IndexSignature {
                key_type: TypeId::STRING,
                value_type: TypeId::STRING,
                readonly: false,
                param_name: None,
            }),
            ..ObjectShape::default()
        });
        let mut source_prop =
            PropertyInfo::new(db.intern_string("[Symbol.iterator]"), TypeId::STRING);
        source_prop.is_symbol_named = true;
        let source = db.object(vec![source_prop]);

        let classification =
            classify_object_properties(&db, source, target).expect("object classification");

        assert_eq!(classification.excess_properties.len(), 1);
    }

    #[test]
    fn optional_mapped_implicit_undefined_is_structural_across_param_names() {
        let db = TypeInterner::new();

        for name in ["K", "Prop"] {
            let mapped = db.mapped(MappedType {
                type_param: TypeParamInfo::simple(db.intern_string(name)),
                constraint: TypeId::STRING,
                template: TypeId::NUMBER,
                name_type: None,
                readonly_modifier: None,
                optional_modifier: Some(MappedModifier::Add),
            });

            assert!(optional_mapped_type_adds_implicit_undefined(
                &db, &db, mapped
            ));
        }
    }

    #[test]
    fn optional_mapped_implicit_undefined_rejects_existing_undefined_template() {
        let db = TypeInterner::new();
        let template = db.union2(TypeId::NUMBER, TypeId::UNDEFINED);
        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo::simple(db.intern_string("K")),
            constraint: TypeId::STRING,
            template,
            name_type: None,
            readonly_modifier: None,
            optional_modifier: Some(MappedModifier::Add),
        });

        assert!(!optional_mapped_type_adds_implicit_undefined(
            &db, &db, mapped
        ));
    }

    #[test]
    fn optional_mapped_implicit_undefined_respects_display_alias_surface() {
        let db = TypeInterner::new();
        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo::simple(db.intern_string("K")),
            constraint: TypeId::STRING,
            template: TypeId::NUMBER,
            name_type: None,
            readonly_modifier: None,
            optional_modifier: Some(MappedModifier::Add),
        });
        let alias = db.application(db.lazy(DefId(1)), vec![TypeId::STRING]);
        db.store_display_alias(mapped, alias);

        assert!(!optional_mapped_type_adds_implicit_undefined(
            &db, &db, mapped
        ));
    }
}
