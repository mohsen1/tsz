/// Expand a mapped type with resolved finite keys into a list of `PropertyInfo`.
///
/// This takes:
/// - `db`: type database
/// - `mapped`: the mapped type definition
/// - `string_keys`: pre-collected finite key atoms (already resolved from constraint)
/// - `source_props`: optional map of source property info for homomorphic types
///   (maps key atom -> (optional, readonly, `declared_type`))
/// - `is_homomorphic`: whether this is a homomorphic mapped type (keyof T pattern)
///
/// Returns the expanded properties with correct modifiers and template instantiation.
/// Does NOT handle array/tuple preservation — callers should check `classify_mapped_source`
/// and use the solver's `evaluate_mapped_array`/`evaluate_mapped_tuple` for those cases.
pub fn expand_mapped_type_to_properties(
    db: &dyn TypeDatabase,
    mapped: &crate::types::MappedType,
    string_keys: &[Atom],
    source_props: &FxHashMap<Atom, (bool, bool, TypeId)>,
    is_homomorphic: bool,
) -> Vec<PropertyInfo> {
    use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};

    let is_remove_optional = mapped.optional_modifier == Some(MappedModifier::Remove);
    let mut properties = Vec::with_capacity(string_keys.len());
    let mut subst = TypeSubstitution::new();

    for &key_name in string_keys {
        let key_literal = db.literal_string_atom(key_name);

        // Handle name remapping
        let remapped = remap_mapped_property_key(db, mapped, key_literal);
        if remapped == TypeId::NEVER {
            continue;
        }

        // Extract property name(s) from remapped key
        let remapped_names: smallvec::SmallVec<[Atom; 1]> =
            if let Some(name) = crate::visitor::literal_string(db, remapped) {
                smallvec::smallvec![name]
            } else if let Some(TypeData::Union(list_id)) = db.lookup(remapped) {
                let members = db.type_list(list_id);
                let names: smallvec::SmallVec<[Atom; 1]> = members
                    .iter()
                    .filter_map(|&m| crate::visitor::literal_string(db, m))
                    .collect();
                if names.is_empty() {
                    continue;
                }
                names
            } else {
                // Can't resolve name — skip this key
                continue;
            };

        // Instantiate template with this key
        subst.clear();
        subst.insert(mapped.type_param.name, key_literal);
        let instantiated = instantiate_type(db, mapped.template, &subst);
        let mut property_type = crate::evaluation::evaluate::evaluate_type(db, instantiated);
        property_type = preserve_mapped_property_alias_provenance(db, instantiated, property_type);

        // Look up source property info for modifier computation
        let source_info = source_props.get(&key_name);
        let (source_optional, source_readonly) =
            source_info.map_or((false, false), |(opt, ro, _)| (*opt, *ro));

        let (optional, readonly) =
            compute_mapped_modifiers(mapped, is_homomorphic, source_optional, source_readonly);

        // For homomorphic mapped types with `-?` and optional source properties,
        // use the declared type (without implicit undefined from optionality).
        if is_homomorphic
            && is_remove_optional
            && source_optional
            && let Some((_, _, declared_type)) = source_info
        {
            property_type = *declared_type;
        } else if is_homomorphic
            && source_optional
            && let Some((_, _, declared_type)) = source_info
        {
            // For homomorphic types preserving optionality, use declared type
            // to avoid double-encoding undefined from indexed access.
            property_type = *declared_type;
        }

        for remapped_name in remapped_names {
            properties.push(PropertyInfo {
                name: remapped_name,
                type_id: property_type,
                write_type: property_type,
                optional,
                readonly,
                is_method: false,
                is_class_prototype: false,
                visibility: crate::types::Visibility::Public,
                parent_id: None,
                declaration_order: 0,
                is_string_named: false,
                is_symbol_named: false,
                single_quoted_name: false,
            });
        }
    }

    merge_colliding_mapped_properties(db, &mut properties);
    properties
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caches::db::QueryDatabase;
    use crate::construction::TypeInterner;
    use crate::types::TypeParamInfo;

    #[test]
    fn test_identity_mapped_passthrough_concrete_primitive() {
        use crate::types::MappedType;

        let interner = TypeInterner::new();

        // Build: { [K in keyof T]: T[K] } where T is a type parameter
        let t_name = interner.intern_string("T");
        let k_name = interner.intern_string("K");
        let t_param = interner.type_param(TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        });
        let k_param = interner.type_param(TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        });
        let constraint = interner.keyof(t_param);
        let template = interner.index_access(t_param, k_param);
        let mapped = MappedType {
            type_param: TypeParamInfo {
                name: k_name,
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint,
            name_type: None,
            template,
            readonly_modifier: None,
            optional_modifier: None,
        };
        let mapped_type = interner.mapped(mapped);
        let mapped_id =
            crate::mapped_type_id(&interner, mapped_type).expect("should be a mapped type");

        // Concrete primitives pass through
        assert_eq!(
            evaluate_identity_mapped_passthrough(&interner, mapped_id, TypeId::STRING),
            Some(TypeId::STRING)
        );
        assert_eq!(
            evaluate_identity_mapped_passthrough(&interner, mapped_id, TypeId::NUMBER),
            Some(TypeId::NUMBER)
        );
        assert_eq!(
            evaluate_identity_mapped_passthrough(&interner, mapped_id, TypeId::BOOLEAN),
            Some(TypeId::BOOLEAN)
        );
    }

    #[test]
    fn test_identity_mapped_passthrough_any_no_constraint() {
        use crate::types::MappedType;

        let interner = TypeInterner::new();

        // Build identity mapped type with unconstrained T
        let t_name = interner.intern_string("T");
        let k_name = interner.intern_string("K");
        let t_param = interner.type_param(TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        });
        let k_param = interner.type_param(TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        });
        let mapped = MappedType {
            type_param: TypeParamInfo {
                name: k_name,
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: interner.keyof(t_param),
            name_type: None,
            template: interner.index_access(t_param, k_param),
            readonly_modifier: None,
            optional_modifier: None,
        };
        let mapped_type = interner.mapped(mapped);
        let mapped_id =
            crate::mapped_type_id(&interner, mapped_type).expect("mapped type should have id");

        // `any` with no array constraint -> produces object with index signatures (not `any`)
        let result = evaluate_identity_mapped_passthrough(&interner, mapped_id, TypeId::ANY);
        assert!(result.is_some());
        let result = result.expect("result should be Some");
        assert_ne!(
            result,
            TypeId::ANY,
            "Objectish<any> should not passthrough to any"
        );

        // unknown with no array constraint -> no passthrough
        assert_eq!(
            evaluate_identity_mapped_passthrough(&interner, mapped_id, TypeId::UNKNOWN),
            None
        );
    }

    #[test]
    fn test_identity_mapped_passthrough_any_with_array_constraint() {
        use crate::types::MappedType;

        let interner = TypeInterner::new();

        // Build identity mapped type with T extends any[]
        let t_name = interner.intern_string("T");
        let k_name = interner.intern_string("K");
        let array_constraint = interner.factory().array(TypeId::ANY);
        let t_param = interner.type_param(TypeParamInfo {
            name: t_name,
            constraint: Some(array_constraint),
            default: None,
            is_const: false,
        });
        let k_param = interner.type_param(TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        });
        let mapped = MappedType {
            type_param: TypeParamInfo {
                name: k_name,
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: interner.keyof(t_param),
            name_type: None,
            template: interner.index_access(t_param, k_param),
            readonly_modifier: None,
            optional_modifier: None,
        };
        let mapped_type = interner.mapped(mapped);
        let mapped_id =
            crate::mapped_type_id(&interner, mapped_type).expect("mapped type should have id");

        // `any` with array constraint -> passthrough
        assert_eq!(
            evaluate_identity_mapped_passthrough(&interner, mapped_id, TypeId::ANY),
            Some(TypeId::ANY)
        );
    }

    #[test]
    fn test_identity_mapped_passthrough_non_identity() {
        use crate::types::MappedType;

        let interner = TypeInterner::new();

        // Build non-identity mapped type: { [K in keyof T]: string }
        let t_name = interner.intern_string("T");
        let k_name = interner.intern_string("K");
        let t_param = interner.type_param(TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        });
        let mapped = MappedType {
            type_param: TypeParamInfo {
                name: k_name,
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: interner.keyof(t_param),
            name_type: None,
            template: TypeId::STRING, // Non-identity: template is string, not T[K]
            readonly_modifier: None,
            optional_modifier: None,
        };
        let mapped_type = interner.mapped(mapped);
        let mapped_id =
            crate::mapped_type_id(&interner, mapped_type).expect("mapped type should have id");

        // Non-identity mapped type -> no passthrough
        assert_eq!(
            evaluate_identity_mapped_passthrough(&interner, mapped_id, TypeId::NUMBER),
            None
        );
    }

    #[test]
    fn finite_mapped_property_display_type_preserves_raw_index_access_surface() {
        use crate::types::MappedType;

        let interner = TypeInterner::new();

        let s_name = interner.intern_string("S");
        let t_name = interner.intern_string("T");
        let k_name = interner.intern_string("K");
        let a_name = interner.intern_string("a");

        let s_param = interner.type_param(TypeParamInfo {
            name: s_name,
            constraint: None,
            default: None,
            is_const: false,
        });
        let t_param = interner.type_param(TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        });
        let key_param = interner.type_param(TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        });

        let state = interner.object(vec![crate::types::PropertyInfo::opt(a_name, t_param)]);
        let source = interner.intersection(vec![s_param, state]);
        let mapped = MappedType {
            type_param: TypeParamInfo {
                name: k_name,
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: interner.literal_string("a"),
            name_type: None,
            template: interner.index_access(source, key_param),
            readonly_modifier: None,
            optional_modifier: None,
        };
        let mapped_type = interner.mapped(mapped);
        let mapped_id =
            crate::mapped_type_id(&interner, mapped_type).expect("mapped type should have id");

        let actual = get_finite_mapped_property_display_type(&interner, mapped_id, "a")
            .expect("display type should resolve");
        let expected = interner.union2(
            interner.index_access(source, interner.literal_string("a")),
            TypeId::UNDEFINED,
        );

        assert_eq!(actual, expected);
    }
}
