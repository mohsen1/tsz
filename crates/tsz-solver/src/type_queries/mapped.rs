//! Mapped-Type Source Classification, Property Resolution, and Expansion Helpers
//!
//! This module centralizes mapped-type-specific logic that was previously in `data.rs`:
//! - Source classification (array/tuple/object preservation)
//! - Identity mapped type detection and passthrough
//! - Mapped property key remapping and value specialization
//! - Finite key collection and property type resolution
//! - Modifier computation and property expansion

use crate::TypeDatabase;
use crate::types::{MappedModifier, PropertyInfo, TypeData, TypeId};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::Atom;

// =============================================================================
// Mapped Property Key Remapping and Value Specialization
// =============================================================================

fn remap_mapped_property_key(
    db: &dyn TypeDatabase,
    mapped: &crate::types::MappedType,
    source_key: TypeId,
) -> TypeId {
    let Some(name_type) = mapped.name_type else {
        return source_key;
    };

    use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};

    let mut subst = TypeSubstitution::new();
    subst.insert(mapped.type_param.name, source_key);
    crate::evaluation::evaluate::evaluate_type(db, instantiate_type(db, name_type, &subst))
}

fn add_mapped_property_optional_undefined(
    db: &dyn TypeDatabase,
    mapped: &crate::types::MappedType,
    value_type: TypeId,
) -> TypeId {
    if mapped.optional_modifier == Some(MappedModifier::Add) {
        db.union2(value_type, TypeId::UNDEFINED)
    } else {
        value_type
    }
}

fn specialize_mapped_property_value_type_for_key(
    db: &dyn TypeDatabase,
    value_type: TypeId,
    key_literal: TypeId,
) -> TypeId {
    let value_type = crate::evaluation::evaluate::evaluate_type(db, value_type);
    match db.lookup(value_type) {
        Some(TypeData::Application(app_id)) => {
            let app = db.type_application(app_id);
            let args: Vec<_> = app
                .args
                .iter()
                .map(|&arg| specialize_mapped_property_value_type_for_key(db, arg, key_literal))
                .collect();
            if args == app.args {
                value_type
            } else {
                db.application(app.base, args)
            }
        }
        Some(TypeData::Function(shape_id)) => {
            let shape = db.function_shape(shape_id);
            let params: Vec<_> = shape
                .params
                .iter()
                .map(|param| crate::ParamInfo {
                    type_id: specialize_mapped_property_value_type_for_key(
                        db,
                        param.type_id,
                        key_literal,
                    ),
                    ..*param
                })
                .collect();
            let return_type =
                specialize_mapped_property_value_type_for_key(db, shape.return_type, key_literal);
            if params.iter().zip(shape.params.iter()).all(|(a, b)| a == b)
                && return_type == shape.return_type
            {
                value_type
            } else {
                db.function(crate::FunctionShape {
                    type_params: shape.type_params.clone(),
                    params,
                    this_type: shape.this_type,
                    return_type,
                    type_predicate: shape.type_predicate,
                    is_constructor: shape.is_constructor,
                    is_method: shape.is_method,
                })
            }
        }
        Some(TypeData::Union(_)) => {
            if let Some(narrowed) =
                narrow_union_by_literal_discriminant_property(db, value_type, key_literal)
            {
                return narrowed;
            }
            value_type
        }
        _ => value_type,
    }
}

fn narrow_union_by_literal_discriminant_property(
    db: &dyn TypeDatabase,
    union_type: TypeId,
    key_literal: TypeId,
) -> Option<TypeId> {
    let TypeData::Union(list_id) = db.lookup(union_type)? else {
        return None;
    };
    let members = db.type_list(list_id);
    let mut candidate_props = FxHashSet::default();

    for &member in members.iter() {
        let Some(shape) = super::data::get_object_shape(db, member) else {
            continue;
        };
        for prop in &shape.properties {
            if prop.type_id == key_literal {
                candidate_props.insert(prop.name);
            }
        }
    }

    for prop_name in candidate_props {
        let retained: Vec<_> = members
            .iter()
            .copied()
            .filter(|member| {
                super::data::get_object_shape(db, *member).is_some_and(|shape| {
                    shape
                        .properties
                        .iter()
                        .find(|prop| prop.name == prop_name)
                        .is_some_and(|prop| prop.type_id == key_literal)
                })
            })
            .collect();
        if retained.is_empty() || retained.len() == members.len() {
            continue;
        }
        return Some(if retained.len() == 1 {
            retained[0]
        } else {
            db.union_preserve_members(retained)
        });
    }

    None
}

fn collect_mapped_property_names_from_source_keys(
    db: &dyn TypeDatabase,
    mapped: &crate::types::MappedType,
    source_keys: FxHashSet<Atom>,
) -> Option<FxHashSet<Atom>> {
    let mut property_names = FxHashSet::default();

    for source_key in source_keys {
        let key_literal = property_key_atom_to_type(db, source_key);
        let mapped_key = remap_mapped_property_key(db, mapped, key_literal);
        let mapped_names = super::data::collect_exact_literal_property_keys(db, mapped_key)?;
        property_names.extend(mapped_names);
    }

    Some(property_names)
}

/// Collect exact property names for a mapped type when its key constraint can be reduced
/// to a finite set of literal property keys.
pub fn collect_finite_mapped_property_names(
    db: &dyn TypeDatabase,
    mapped_id: crate::types::MappedTypeId,
) -> Option<FxHashSet<Atom>> {
    let mapped = db.mapped_type(mapped_id);
    let source_keys = super::data::collect_exact_literal_property_keys(db, mapped.constraint)?;
    collect_mapped_property_names_from_source_keys(db, &mapped, source_keys)
}

/// Resolve the exact property type for a property on a mapped type when its key
/// constraint is a finite literal set.
pub fn get_finite_mapped_property_type(
    db: &dyn TypeDatabase,
    mapped_id: crate::types::MappedTypeId,
    property_name: &str,
) -> Option<TypeId> {
    let mapped = db.mapped_type(mapped_id);
    let source_keys = super::data::collect_exact_literal_property_keys(db, mapped.constraint)?;
    let target_atom = db.intern_string(property_name);
    let mut matches = Vec::new();

    for source_key in source_keys {
        let key_literal = property_key_atom_to_type(db, source_key);
        let remapped = remap_mapped_property_key(db, &mapped, key_literal);
        let remapped_keys = super::data::collect_exact_literal_property_keys(db, remapped)?;
        if !remapped_keys.contains(&target_atom) {
            continue;
        }

        let instantiated = super::data::instantiate_mapped_template_for_property(
            db,
            mapped.template,
            mapped.type_param.name,
            key_literal,
        );
        let value_type = specialize_mapped_property_value_type_for_key(
            db,
            crate::evaluation::evaluate::evaluate_type(db, instantiated),
            key_literal,
        );
        matches.push(add_mapped_property_optional_undefined(
            db, &mapped, value_type,
        ));
    }

    match matches.len() {
        0 => None,
        1 => Some(matches[0]),
        _ => Some(db.union_preserve_members(matches)),
    }
}

fn property_key_atom_to_type(db: &dyn TypeDatabase, key: Atom) -> TypeId {
    let key_str = db.resolve_atom(key);
    if let Some(symbol_ref) = key_str.strip_prefix("__unique_")
        && let Ok(id) = symbol_ref.parse::<u32>()
    {
        return db.unique_symbol(crate::types::SymbolRef(id));
    }
    db.literal_string(key_str.as_ref())
}

/// Collect exact property names for a deferred/remapped mapped type.
///
/// Backward-compatible alias that delegates to `collect_finite_mapped_property_names`.
pub fn collect_deferred_mapped_property_names(
    db: &dyn TypeDatabase,
    mapped_id: crate::types::MappedTypeId,
) -> Option<FxHashSet<Atom>> {
    collect_finite_mapped_property_names(db, mapped_id)
}

/// Backward-compatible alias for callers that only used this on deferred/remapped mapped types.
pub fn get_deferred_mapped_property_type(
    db: &dyn TypeDatabase,
    mapped_id: crate::types::MappedTypeId,
    property_name: &str,
) -> Option<TypeId> {
    get_finite_mapped_property_type(db, mapped_id, property_name)
}

// =============================================================================
// Mapped-Type Source Classification and Expansion Helpers
// =============================================================================

/// Classification of a mapped type's source for structural preservation decisions.
///
/// When a homomorphic mapped type maps over `keyof T`, this classifies what `T`
/// resolves to, so callers can decide whether to preserve array/tuple identity
/// or expand to a plain object.
#[derive(Debug, Clone, PartialEq)]
pub enum MappedSourceKind {
    /// Source is an array type (`T[]`) — preserve as array after mapping.
    Array(TypeId),
    /// Source is a tuple type — preserve as tuple after mapping.
    Tuple(crate::types::TupleListId),
    /// Source is a readonly array (`ObjectWithIndex` with readonly number index).
    ReadonlyArray(TypeId),
    /// Source is a regular object or other non-array/tuple type.
    Object,
    /// Source is a type parameter with an array/tuple constraint.
    TypeParamWithArrayConstraint(TypeId),
}

/// Classify a resolved mapped-type source for array/tuple preservation.
///
/// Given the resolved source type from a homomorphic mapped type's `keyof T`
/// constraint, returns the structural kind. The checker/boundary can use this
/// to decide whether to delegate to the solver's tuple/array mapped evaluation
/// or use the standard object expansion path.
pub fn classify_mapped_source(db: &dyn TypeDatabase, source: TypeId) -> MappedSourceKind {
    let evaluated = crate::evaluation::evaluate::evaluate_type(db, source);
    classify_mapped_source_inner(db, evaluated)
}

fn classify_mapped_source_inner(db: &dyn TypeDatabase, source: TypeId) -> MappedSourceKind {
    match db.lookup(source) {
        Some(TypeData::Array(element_type)) => MappedSourceKind::Array(element_type),
        Some(TypeData::Tuple(tuple_id)) => MappedSourceKind::Tuple(tuple_id),
        Some(TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            if let Some(ref idx) = shape.number_index
                && idx.readonly
                && idx.key_type == TypeId::NUMBER
            {
                return MappedSourceKind::ReadonlyArray(idx.value_type);
            }
            MappedSourceKind::Object
        }
        Some(TypeData::TypeParameter(info)) => {
            if let Some(constraint) = info.constraint {
                let resolved = crate::evaluation::evaluate::evaluate_type(db, constraint);
                match classify_mapped_source_inner(db, resolved) {
                    MappedSourceKind::Object => MappedSourceKind::Object,
                    _ => MappedSourceKind::TypeParamWithArrayConstraint(constraint),
                }
            } else {
                MappedSourceKind::Object
            }
        }
        _ => MappedSourceKind::Object,
    }
}

/// Check if a mapped type's `as` clause is identity-preserving (no remapping).
///
/// Returns `true` when there's no `as` clause, or when the `as` clause maps
/// to the same type parameter (e.g., `{ [K in keyof T as K]: T[K] }`).
pub fn is_identity_name_mapping(db: &dyn TypeDatabase, mapped: &crate::types::MappedType) -> bool {
    match mapped.name_type {
        None => true,
        Some(nt) => matches!(
            db.lookup(nt),
            Some(TypeData::TypeParameter(param)) if param.name == mapped.type_param.name
        ),
    }
}

/// Info about an identity homomorphic mapped type `{ [K in keyof T]: T[K] }`.
///
/// Returned by [`classify_identity_mapped`] when a mapped type is confirmed to
/// be an identity mapping — constraint is `keyof T` where `T` is a type param,
/// and the template is `T[K]` where `K` is the mapped iteration variable.
#[derive(Clone, Debug)]
pub struct IdentityMappedInfo {
    /// Name of the source type parameter `T`.
    pub source_param_name: Atom,
    /// Constraint of the source type parameter (if any).
    pub source_constraint: Option<TypeId>,
}

/// Check if a mapped type is an identity homomorphic mapped type.
///
/// An identity mapped type has the form `{ [K in keyof T]: T[K] }` where
/// `T` is a type parameter and the template is an indexed access of `T` by `K`.
/// This is the pattern where `Partial<number>` evaluates to `number`.
///
/// Returns [`IdentityMappedInfo`] with the source type parameter's name and
/// constraint, or `None` if the mapped type is not identity-homomorphic.
pub fn classify_identity_mapped(
    db: &dyn TypeDatabase,
    mapped_id: crate::types::MappedTypeId,
) -> Option<IdentityMappedInfo> {
    let mapped = db.mapped_type(mapped_id);
    let keyof_source = crate::keyof_inner_type(db, mapped.constraint)?;
    let tp = crate::type_param_info(db, keyof_source)?;
    let (obj, key) = crate::index_access_parts(db, mapped.template)?;
    if obj != keyof_source {
        return None;
    }
    let kp = crate::type_param_info(db, key)?;
    if kp.name != mapped.type_param.name {
        return None;
    }
    Some(IdentityMappedInfo {
        source_param_name: tp.name,
        source_constraint: tp.constraint,
    })
}

/// Evaluate identity mapped type passthrough for a given type argument.
///
/// For an identity homomorphic mapped type `{ [K in keyof T]: T[K] }`:
/// - Concrete primitives (string, number, boolean, etc.) pass through: returns `Some(arg)`.
/// - `any` with array/tuple constraint: passes through: returns `Some(any)`.
/// - `any` without array constraint: produces `{ [x: string]: any; [x: number]: any }`.
/// - `unknown`, `never`, `error` without array constraint: no passthrough: returns `None`.
/// - Non-identity mapped types: no passthrough: returns `None`.
///
/// This centralizes the passthrough logic that was previously split between
/// the checker (core.rs) and solver (evaluate.rs).
pub fn evaluate_identity_mapped_passthrough(
    db: &dyn TypeDatabase,
    mapped_id: crate::types::MappedTypeId,
    arg: TypeId,
) -> Option<TypeId> {
    let identity_info = classify_identity_mapped(db, mapped_id)?;

    // Handle any/unknown/never/error first — these are NOT considered "primitive"
    // by is_primitive_type, so they need separate handling.
    let is_any_like = arg == TypeId::ANY
        || arg == TypeId::UNKNOWN
        || arg == TypeId::NEVER
        || arg == TypeId::ERROR;
    if is_any_like {
        // Check if the type parameter has an array/tuple constraint
        let has_array_constraint = identity_info
            .source_constraint
            .is_some_and(|c| matches!(db.lookup(c), Some(TypeData::Array(_) | TypeData::Tuple(_))));
        if has_array_constraint {
            return Some(arg);
        }
        // Objectish<any>: produce { [x: string]: any; [x: number]: any }
        if arg == TypeId::ANY {
            use crate::types::{IndexSignature, ObjectShape};
            return Some(db.object_with_index(ObjectShape {
                flags: crate::types::ObjectFlags::empty(),
                properties: vec![],
                string_index: Some(IndexSignature {
                    key_type: TypeId::STRING,
                    value_type: TypeId::ANY,
                    readonly: false,
                    param_name: None,
                }),
                number_index: Some(IndexSignature {
                    key_type: TypeId::NUMBER,
                    value_type: TypeId::ANY,
                    readonly: false,
                    param_name: None,
                }),
                symbol: None,
            }));
        }
        // unknown/never/error without array constraint → no passthrough
        return None;
    }

    // Concrete primitives (string, number, boolean, etc.) pass through directly.
    if crate::is_primitive_type(db, arg) {
        return Some(arg);
    }
    None
}

/// Check if a mapped type's template is callable (has call/construct signatures).
///
/// This is used for TS2344 constraint checking: when an indexed access into a
/// mapped type (e.g., `{ [K in keyof T]: () => unknown }[keyof T]`) is checked
/// against a callable constraint, we need to know if the template type is callable.
pub fn is_mapped_template_callable(
    db: &dyn TypeDatabase,
    mapped_id: crate::types::MappedTypeId,
) -> bool {
    let mapped = db.mapped_type(mapped_id);
    super::is_callable_type(db, mapped.template)
        || super::data::get_callable_shape(db, mapped.template).is_some()
}

/// Get the inner type of a `keyof T` type, delegated from the visitor layer.
///
/// Returns `Some(T)` if the type is `KeyOf(T)`, `None` otherwise.
/// This is the boundary-safe version of `crate::keyof_inner_type`.
pub fn keyof_inner_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    crate::keyof_inner_type(db, type_id)
}

/// Check if a type is an array or tuple type.
///
/// Used for constraint classification in mapped type passthrough decisions:
/// when a type parameter is constrained to array/tuple, `any` arguments
/// should pass through identity mapped types rather than expanding.
pub fn is_array_or_tuple_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    crate::visitors::visitor_predicates::is_array_type(db, type_id)
        || crate::visitors::visitor_predicates::is_tuple_type(db, type_id)
}

/// Reconstruct a mapped type with a new constraint, preserving all other fields.
///
/// Used when the checker evaluates a mapped type's constraint to concrete keys
/// and needs to create a new mapped type with the resolved constraint for
/// further evaluation (e.g., finite key collection).
///
/// Returns the `MappedTypeId` of the new (or interned-existing) mapped type.
pub fn reconstruct_mapped_with_constraint(
    db: &dyn TypeDatabase,
    mapped_id: crate::types::MappedTypeId,
    new_constraint: TypeId,
) -> crate::types::MappedTypeId {
    let mapped = db.mapped_type(mapped_id);
    if mapped.constraint == new_constraint {
        return mapped_id;
    }
    let new_mapped = crate::types::MappedType {
        type_param: mapped.type_param,
        constraint: new_constraint,
        name_type: mapped.name_type,
        template: mapped.template,
        readonly_modifier: mapped.readonly_modifier,
        optional_modifier: mapped.optional_modifier,
    };
    // Intern via the TypeDatabase factory and extract the MappedTypeId.
    let type_id = db.mapped(new_mapped);
    match crate::mapped_type_id(db, type_id) {
        Some(id) => id,
        None => crate::MappedTypeId(0),
    }
}

/// Compute modifier values for a mapped type property given the source property's
/// original modifiers and the mapped type's modifier directives.
///
/// This centralizes the `-?`, `+?`, `-readonly`, `+readonly` logic that was
/// previously duplicated between the solver's `evaluate_mapped` and the checker's
/// `evaluate_mapped_type_with_resolution_inner`.
pub const fn compute_mapped_modifiers(
    mapped: &crate::types::MappedType,
    is_homomorphic: bool,
    source_optional: bool,
    source_readonly: bool,
) -> (bool, bool) {
    let optional = match mapped.optional_modifier {
        Some(MappedModifier::Add) => true,
        Some(MappedModifier::Remove) => false,
        None => {
            if is_homomorphic {
                source_optional
            } else {
                false
            }
        }
    };
    let readonly = match mapped.readonly_modifier {
        Some(MappedModifier::Add) => true,
        Some(MappedModifier::Remove) => false,
        None => {
            if is_homomorphic {
                source_readonly
            } else {
                false
            }
        }
    };
    (optional, readonly)
}

/// Collect source property info from a homomorphic mapped type's source object.
///
/// For a mapped type `{ [K in keyof T]: ... }`, this resolves `T` and collects
/// its properties into a map of `(optional, readonly, declared_type)` tuples.
/// This is used by `expand_mapped_type_to_properties` to compute modifiers and
/// for `-?` to preserve the distinction between implicit and explicit undefined.
pub fn collect_homomorphic_source_properties(
    db: &dyn TypeDatabase,
    source: TypeId,
) -> FxHashMap<Atom, (bool, bool, TypeId)> {
    let evaluated = crate::evaluation::evaluate::evaluate_type(db, source);
    let mut props = FxHashMap::default();
    match db.lookup(evaluated) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            props.reserve(shape.properties.len());
            for prop in &shape.properties {
                props.insert(prop.name, (prop.optional, prop.readonly, prop.type_id));
            }
        }
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            props.reserve(shape.properties.len());
            for prop in &shape.properties {
                props.insert(prop.name, (prop.optional, prop.readonly, prop.type_id));
            }
        }
        _ => {}
    }
    props
}

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
            });
        }
    }

    properties
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TypeInterner;
    use crate::caches::db::QueryDatabase;
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
}
