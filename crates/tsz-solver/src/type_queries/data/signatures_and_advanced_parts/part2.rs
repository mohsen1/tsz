/// Find the private brand name for a type.
///
/// Private members in TypeScript classes use a "brand" property for nominal typing.
/// The brand is a property named like `__private_brand_#className`.
///
/// Returns the full brand property name (e.g., `"__private_brand_#Foo"`) if found,
/// or None if the type has no private brand.
pub fn get_private_brand_name(db: &dyn TypeDatabase, type_id: TypeId) -> Option<String> {
    // Fast path: intrinsics aren't `Object` / `ObjectWithIndex` / `Callable`.
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id)? {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = db.object_shape(shape_id);
            for prop in &shape.properties {
                let name = db.resolve_atom(prop.name);
                if name.starts_with("__private_brand_") {
                    return Some(name);
                }
            }
            None
        }
        TypeData::Callable(shape_id) => {
            let shape = db.callable_shape(shape_id);
            for prop in &shape.properties {
                let name = db.resolve_atom(prop.name);
                if name.starts_with("__private_brand_") {
                    return Some(name);
                }
            }
            None
        }
        _ => None,
    }
}

/// Find the private field name from a type's properties.
///
/// Given a type with private members, returns the name of the first private field
/// (a property starting with `#` that is not a brand marker).
///
/// Returns `Some(field_name)` (e.g., `"#foo"`) if found, None otherwise.
pub fn get_private_field_name(db: &dyn TypeDatabase, type_id: TypeId) -> Option<String> {
    // Fast path: intrinsics aren't `Object` / `ObjectWithIndex` / `Callable`.
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id)? {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = db.object_shape(shape_id);
            for prop in &shape.properties {
                let name = db.resolve_atom(prop.name);
                if name.starts_with('#') && !name.starts_with("__private_brand_") {
                    return Some(name);
                }
            }
            None
        }
        TypeData::Callable(shape_id) => {
            let shape = db.callable_shape(shape_id);
            for prop in &shape.properties {
                let name = db.resolve_atom(prop.name);
                if name.starts_with('#') && !name.starts_with("__private_brand_") {
                    return Some(name);
                }
            }
            None
        }
        _ => None,
    }
}

/// Get the symbol associated with a type's shape.
///
/// Checks object, object-with-index, and callable shapes for their `symbol` field.
/// Returns the first `SymbolId` found, or None if the type has no shape with a symbol.
pub fn get_type_shape_symbol(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_binder::SymbolId> {
    // Fast path: intrinsics aren't `Object` / `ObjectWithIndex` / `Callable`.
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id)? {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            db.object_shape(shape_id).symbol
        }
        TypeData::Callable(shape_id) => db.callable_shape(shape_id).symbol,
        _ => None,
    }
}

/// Get the `DefId` from an Enum type.
///
/// Returns None if the type is not an Enum type.
pub fn get_enum_def_id(db: &dyn TypeDatabase, type_id: TypeId) -> Option<crate::def::DefId> {
    // Fast path: intrinsics aren't `Enum(_)`.
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::Enum(def_id, _)) => Some(def_id),
        _ => None,
    }
}

/// Get the structural member type from an Enum type.
///
/// Returns None if the type is not an Enum type.
pub fn get_enum_member_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    // Fast path: intrinsics aren't `Enum(_)`.
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::Enum(_, member_type)) => Some(member_type),
        _ => None,
    }
}

/// Check if a type is a valid base type for a class `extends` clause.
///
/// In TypeScript, a valid base type must be:
/// - An object type (with properties/signatures) that is not a generic mapped type
/// - The `object` intrinsic (`NonPrimitive`)
/// - `any`
/// - An intersection where every member is a valid base type
/// - A union where every member is a valid base type (e.g. from overloaded constructors)
/// - A type parameter
///
/// Primitives, `never`, `void`, `undefined`, `null`, `unknown`, and literals
/// are NOT valid base types. Used for TS2509 checking.
pub fn is_valid_base_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Fast path: only `any` and `object` intrinsics are valid base types;
    // all other intrinsics (including `BOOLEAN_TRUE` / `BOOLEAN_FALSE`,
    // which lookup as `Literal(Boolean)` and don't match the `Literal` arm)
    // fall through to `_ => false`. Skip `lookup` for these.
    if type_id.is_intrinsic() {
        return type_id == TypeId::ANY
            || type_id == TypeId::OBJECT
            || type_id == TypeId::PROMISE_BASE;
    }
    match db.lookup(type_id) {
        // Object-like types, callables, arrays/tuples, type params, and
        // lazy/application/mapped refs are all valid class base types.
        Some(
            TypeData::Intrinsic(IntrinsicKind::Any | IntrinsicKind::Object)
            | TypeData::Object(_)
            | TypeData::ObjectWithIndex(_)
            | TypeData::Callable(_)
            | TypeData::Function(_)
            | TypeData::Array(_)
            | TypeData::Tuple(_)
            | TypeData::TypeParameter(_)
            | TypeData::Lazy(_)
            | TypeData::Application(_)
            | TypeData::Mapped(_),
        ) => true,
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().all(|&m| is_valid_base_type(db, m))
        }
        Some(TypeData::Union(list_id)) => {
            // Union can arise from construct-signature return-type merging
            // (get_construct_return_type_union). All members must be valid base types.
            let members = db.type_list(list_id);
            !members.is_empty() && members.iter().all(|&m| is_valid_base_type(db, m))
        }
        Some(TypeData::ReadonlyType(inner)) => is_valid_base_type(db, inner),
        // Intrinsics (never, void, null, etc.), literals, None => not valid base types
        _ => false,
    }
}

/// Check if a type is a valid base type for an interface `extends` clause.
///
/// Interface heritage is narrower than class heritage: the base must be an
/// object type or an intersection of object types with statically known
/// members. Unions and type parameters are rejected with TS2312.
pub fn is_valid_interface_base_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return type_id == TypeId::ANY || type_id == TypeId::OBJECT;
    }

    match db.lookup(type_id) {
        Some(
            TypeData::Intrinsic(IntrinsicKind::Any | IntrinsicKind::Object)
            | TypeData::Object(_)
            | TypeData::ObjectWithIndex(_)
            | TypeData::Callable(_)
            | TypeData::Function(_)
            | TypeData::Array(_)
            | TypeData::Tuple(_)
            | TypeData::Lazy(_)
            | TypeData::Application(_),
        ) => true,
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            !members.is_empty()
                && members
                    .iter()
                    .all(|&member| is_valid_interface_base_type(db, member))
        }
        Some(TypeData::Mapped(mapped_id)) => {
            let mapped = db.mapped_type(mapped_id);
            !contains_type_parameters_db(db, mapped.constraint)
                && !mapped
                    .name_type
                    .is_some_and(|name_type| contains_type_parameters_db(db, name_type))
        }
        Some(TypeData::ReadonlyType(inner)) => is_valid_interface_base_type(db, inner),
        _ => false,
    }
}
