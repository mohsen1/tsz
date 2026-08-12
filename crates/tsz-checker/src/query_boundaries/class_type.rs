use tsz_binder::SymbolId;
use tsz_common::interner::Atom;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::def::DefId;
use tsz_solver::{
    CallSignature, CallableShape, IndexSignature, ObjectShape, ParamInfo, PropertyInfo, TypeId,
    TypeParamInfo, TypePredicate, TypePredicateTarget, Visibility,
};

pub(crate) use super::common::{
    array_element_type, callable_shape_for_type, construct_signatures_for_type,
    contains_conditional_type, has_function_shape, intersection_members, is_generic_mapped_type,
    is_generic_type, object_shape_for_type,
};

pub(crate) fn function_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::FunctionShape>> {
    tsz_solver::type_queries::get_function_shape(db, type_id)
}

/// Boundary for [`tsz_solver::type_queries::callable_requires_explicit_receiver`].
/// See the solver query for the structural rule.
pub(crate) fn callable_requires_explicit_receiver(
    db: &dyn TypeDatabase,
    callee_type: TypeId,
) -> bool {
    tsz_solver::type_queries::callable_requires_explicit_receiver(db, callee_type)
}

pub(crate) fn type_includes_undefined(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::type_includes_undefined(db, type_id)
}

pub(crate) fn type_parameter_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_type_parameter_constraint(db, type_id)
}

/// Check if `undefined` is potentially assignable to the given type.
///
/// This mirrors tsc's `isTypeAssignableTo(undefinedType, type)` for the purposes
/// of TS2564 checking. In particular:
/// - `undefined` is assignable to `any`, `unknown`, `void`, `undefined`
/// - `undefined` is assignable to unions containing `undefined`
///
/// TypeScript does NOT suppress TS2564 for naked type parameters, even when their
/// constraint is `any`, `unknown`, or includes `undefined`. Only the declared
/// property type itself matters here, not what a future instantiation might allow.
pub(crate) fn undefined_is_assignable_to(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::ANY
        || type_id == TypeId::UNKNOWN
        || type_id == TypeId::UNDEFINED
        || type_id == TypeId::VOID
    {
        return true;
    }

    // Check if type directly includes undefined (e.g., string | undefined)
    if type_includes_undefined(db, type_id) {
        return true;
    }

    false
}

pub(crate) fn merged_static_late_bound_index_value_type(
    db: &dyn TypeDatabase,
    existing: TypeId,
    incoming: TypeId,
) -> TypeId {
    db.union2(existing, incoming)
}

pub(crate) const fn static_late_bound_index_signature(
    key_type: TypeId,
    value_type: TypeId,
) -> IndexSignature {
    IndexSignature {
        key_type,
        value_type,
        readonly: false,
        param_name: None,
    }
}

pub(crate) struct MergedClassInstanceInterfaceSurface {
    pub(crate) result_is_callable: bool,
    pub(crate) call_signatures: Vec<CallSignature>,
    pub(crate) construct_signatures: Vec<CallSignature>,
    pub(crate) properties: Vec<PropertyInfo>,
    pub(crate) string_index: Option<IndexSignature>,
    pub(crate) number_index: Option<IndexSignature>,
    pub(crate) symbol_index: Option<IndexSignature>,
    pub(crate) symbol: Option<SymbolId>,
    pub(crate) plain_object_without_indexes: bool,
}

pub(crate) fn merged_class_instance_interface_type(
    db: &dyn TypeDatabase,
    surface: MergedClassInstanceInterfaceSurface,
) -> TypeId {
    let MergedClassInstanceInterfaceSurface {
        result_is_callable,
        call_signatures,
        construct_signatures,
        properties,
        string_index,
        number_index,
        symbol_index,
        symbol,
        plain_object_without_indexes,
    } = surface;

    if result_is_callable {
        return db.callable(CallableShape {
            call_signatures,
            construct_signatures,
            properties,
            string_index,
            number_index,
            symbol,
            is_abstract: false,
        });
    }

    if plain_object_without_indexes {
        db.object(properties)
    } else {
        db.object_with_index(ObjectShape {
            properties,
            string_index,
            number_index,
            symbol_index,
            symbol,
            ..ObjectShape::default()
        })
    }
}

pub(crate) struct ClassMemberProperty {
    name: Atom,
    type_id: TypeId,
    write_type: TypeId,
    flags: u8,
    visibility: Visibility,
    parent_id: Option<SymbolId>,
    declaration_order: u32,
}

impl ClassMemberProperty {
    const OPTIONAL: u8 = 1 << 0;
    const READONLY: u8 = 1 << 1;
    const IS_METHOD: u8 = 1 << 2;
    const IS_CLASS_PROTOTYPE: u8 = 1 << 3;
    const IS_SYMBOL_NAMED: u8 = 1 << 4;

    pub(crate) const fn new(name: Atom, type_id: TypeId) -> Self {
        Self {
            name,
            type_id,
            write_type: type_id,
            flags: 0,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
        }
    }

    const fn set_flag(&mut self, flag: u8, value: bool) {
        if value {
            self.flags |= flag;
        } else {
            self.flags &= !flag;
        }
    }

    const fn flag(&self, flag: u8) -> bool {
        self.flags & flag != 0
    }

    pub(crate) const fn with_write_type(mut self, write_type: TypeId) -> Self {
        self.write_type = write_type;
        self
    }

    pub(crate) const fn optional(mut self, optional: bool) -> Self {
        self.set_flag(Self::OPTIONAL, optional);
        self
    }

    pub(crate) const fn readonly(mut self, readonly: bool) -> Self {
        self.set_flag(Self::READONLY, readonly);
        self
    }

    pub(crate) const fn method(mut self, is_class_prototype: bool) -> Self {
        self.set_flag(Self::IS_METHOD, true);
        self.set_flag(Self::IS_CLASS_PROTOTYPE, is_class_prototype);
        self
    }

    /// Mark the member as a class prototype member without marking it a
    /// method. Used for instance accessors (getters/setters), which live on
    /// the prototype and are excluded from object-rest spreads
    /// (`Omit<T, K>` keys) like methods, but are not callable members.
    pub(crate) const fn class_prototype(mut self, is_class_prototype: bool) -> Self {
        self.set_flag(Self::IS_CLASS_PROTOTYPE, is_class_prototype);
        self
    }

    pub(crate) const fn visibility(mut self, visibility: Visibility) -> Self {
        self.visibility = visibility;
        self
    }

    pub(crate) const fn parent(mut self, parent_id: Option<SymbolId>) -> Self {
        self.parent_id = parent_id;
        self
    }

    pub(crate) const fn declaration_order(mut self, declaration_order: u32) -> Self {
        self.declaration_order = declaration_order;
        self
    }

    pub(crate) const fn symbol_named(mut self, is_symbol_named: bool) -> Self {
        self.set_flag(Self::IS_SYMBOL_NAMED, is_symbol_named);
        self
    }
}

pub(crate) const fn class_member_property(surface: ClassMemberProperty) -> PropertyInfo {
    PropertyInfo {
        name: surface.name,
        type_id: surface.type_id,
        write_type: surface.write_type,
        optional: surface.flag(ClassMemberProperty::OPTIONAL),
        readonly: surface.flag(ClassMemberProperty::READONLY),
        is_method: surface.flag(ClassMemberProperty::IS_METHOD),
        is_class_prototype: surface.flag(ClassMemberProperty::IS_CLASS_PROTOTYPE),
        visibility: surface.visibility,
        parent_id: surface.parent_id,
        declaration_order: surface.declaration_order,
        is_string_named: false,
        is_symbol_named: surface.flag(ClassMemberProperty::IS_SYMBOL_NAMED),
        single_quoted_name: false,
        non_widening: false,
    }
}

pub(crate) fn class_method_callable_type(
    db: &dyn TypeDatabase,
    signatures: Vec<CallSignature>,
) -> TypeId {
    db.callable(CallableShape {
        call_signatures: signatures,
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    })
}

pub(crate) fn optional_class_member_type(
    db: &dyn TypeDatabase,
    member_type: TypeId,
    optional: bool,
) -> TypeId {
    if optional {
        db.union2(member_type, TypeId::UNDEFINED)
    } else {
        member_type
    }
}

pub(crate) const fn class_rest_any_param() -> ParamInfo {
    class_construct_param(None, TypeId::ANY, false, true)
}

pub(crate) const fn class_method_call_signature(
    type_params: Vec<TypeParamInfo>,
    params: Vec<ParamInfo>,
    this_type: Option<TypeId>,
    return_type: TypeId,
    type_predicate: Option<TypePredicate>,
) -> CallSignature {
    class_construct_signature(
        type_params,
        params,
        this_type,
        return_type,
        type_predicate,
        true,
    )
}

pub(crate) const fn class_declared_index_signature(
    key_type: TypeId,
    value_type: TypeId,
    readonly: bool,
    param_name: Option<Atom>,
) -> IndexSignature {
    IndexSignature {
        key_type,
        value_type,
        readonly,
        param_name,
    }
}

pub(crate) fn class_member_object_type(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
) -> TypeId {
    db.object(properties)
}

pub(crate) fn class_member_object_with_indexes_type(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    string_index: Option<IndexSignature>,
    number_index: Option<IndexSignature>,
    symbol_index: Option<IndexSignature>,
    symbol: Option<SymbolId>,
) -> TypeId {
    db.object_with_index(ObjectShape {
        properties,
        string_index,
        number_index,
        symbol_index,
        symbol,
        ..ObjectShape::default()
    })
}

pub(crate) fn final_class_instance_type(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    string_index: Option<IndexSignature>,
    number_index: Option<IndexSignature>,
    symbol_index: Option<IndexSignature>,
    symbol: Option<SymbolId>,
    has_late_bound_members: bool,
    suppress_module_augmentation_lookup: bool,
) -> TypeId {
    let mut shape = ObjectShape {
        properties,
        string_index,
        number_index,
        symbol_index,
        symbol,
        ..ObjectShape::default()
    };
    if has_late_bound_members {
        shape.mark_has_late_bound_members();
    }
    if suppress_module_augmentation_lookup {
        shape.mark_no_module_augmentation_lookup();
    }
    db.object_with_index(shape)
}

pub(crate) fn class_member_partial_this_type(
    db: &dyn TypeDatabase,
    own_properties: Vec<PropertyInfo>,
    string_index: Option<IndexSignature>,
    number_index: Option<IndexSignature>,
    symbol_index: Option<IndexSignature>,
    symbol: Option<SymbolId>,
    prescan_this_type: Option<TypeId>,
) -> Option<TypeId> {
    if own_properties.is_empty() {
        return prescan_this_type;
    }

    let own_partial = class_member_object_with_indexes_type(
        db,
        own_properties,
        string_index,
        number_index,
        symbol_index,
        symbol,
    );
    Some(if let Some(prescan) = prescan_this_type {
        db.intersection(vec![own_partial, prescan])
    } else {
        own_partial
    })
}

pub(crate) fn rough_class_instance_return_type(
    db: &dyn TypeDatabase,
    self_ref: Option<TypeId>,
    rough_instance_return_type: TypeId,
) -> TypeId {
    match self_ref {
        Some(self_ref)
            if rough_instance_return_type != TypeId::ANY
                && rough_instance_return_type != TypeId::ERROR =>
        {
            db.intersection2(self_ref, rough_instance_return_type)
        }
        Some(self_ref) => self_ref,
        None => rough_instance_return_type,
    }
}

pub(crate) fn partial_static_method_type(
    db: &dyn TypeDatabase,
    signatures: &[CallSignature],
) -> TypeId {
    class_method_callable_type(db, signatures.to_vec())
}

pub(crate) const fn partial_static_method_property(
    name: Atom,
    type_id: TypeId,
    optional: bool,
    visibility: Visibility,
    parent_id: Option<SymbolId>,
) -> PropertyInfo {
    class_member_property(
        ClassMemberProperty::new(name, type_id)
            .optional(optional)
            .method(false)
            .visibility(visibility)
            .parent(parent_id),
    )
}

pub(crate) const fn partial_static_accessor_property(
    name: Atom,
    read_type: TypeId,
    write_type: TypeId,
    readonly: bool,
    visibility: Visibility,
    parent_id: Option<SymbolId>,
) -> PropertyInfo {
    class_member_property(
        ClassMemberProperty::new(name, read_type)
            .with_write_type(write_type)
            .readonly(readonly)
            .visibility(visibility)
            .parent(parent_id),
    )
}

pub(crate) const fn partial_static_placeholder_property(
    name: Atom,
    parent_id: Option<SymbolId>,
) -> PropertyInfo {
    class_member_property(ClassMemberProperty::new(name, TypeId::ANY).parent(parent_id))
}

pub(crate) fn partial_static_constructor_callable_type(
    db: &dyn TypeDatabase,
    symbol: Option<SymbolId>,
    properties: Vec<PropertyInfo>,
    construct_signatures: &[CallSignature],
    string_index: Option<IndexSignature>,
    number_index: Option<IndexSignature>,
) -> TypeId {
    db.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: construct_signatures.to_vec(),
        properties,
        string_index,
        number_index,
        symbol,
        is_abstract: false,
    })
}

pub(crate) fn class_constructor_callable_type(
    db: &dyn TypeDatabase,
    symbol: Option<SymbolId>,
    properties: Vec<PropertyInfo>,
    construct_signatures: Vec<CallSignature>,
    string_index: Option<IndexSignature>,
    number_index: Option<IndexSignature>,
    is_abstract: bool,
) -> TypeId {
    db.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures,
        properties,
        string_index,
        number_index,
        symbol,
        is_abstract,
    })
}

pub(crate) fn class_constructor_callable_with_construct_signatures_replaced(
    db: &dyn TypeDatabase,
    base: &CallableShape,
    construct_signatures: Vec<CallSignature>,
) -> TypeId {
    db.callable(CallableShape {
        call_signatures: base.call_signatures.clone(),
        construct_signatures,
        properties: base.properties.clone(),
        string_index: base.string_index,
        number_index: base.number_index,
        symbol: base.symbol,
        is_abstract: base.is_abstract,
    })
}

pub(crate) fn class_constructor_mixin_intersection(
    db: &dyn TypeDatabase,
    base_type_param: TypeId,
    constructor_type: TypeId,
) -> TypeId {
    db.intersection2(base_type_param, constructor_type)
}

pub(crate) fn class_constructor_companion_lazy_type(
    db: &dyn TypeDatabase,
    def_id: DefId,
) -> TypeId {
    db.lazy(def_id)
}

pub(crate) fn rough_self_instance_lazy_type(db: &dyn TypeDatabase, def_id: DefId) -> TypeId {
    db.lazy(def_id)
}

pub(crate) fn rough_self_instance_application_type(
    db: &dyn TypeDatabase,
    lazy_ref: TypeId,
    args: Vec<TypeId>,
) -> TypeId {
    db.application(lazy_ref, args)
}

pub(crate) const fn class_construct_param(
    name: Option<Atom>,
    type_id: TypeId,
    optional: bool,
    rest: bool,
) -> ParamInfo {
    ParamInfo {
        name,
        type_id,
        optional,
        rest,
        arity_only_optional: false,
    }
}

pub(crate) const fn class_type_predicate(
    asserts: bool,
    target: TypePredicateTarget,
    type_id: Option<TypeId>,
    parameter_index: Option<usize>,
) -> TypePredicate {
    TypePredicate {
        asserts,
        target,
        type_id,
        parameter_index,
    }
}

pub(crate) const fn class_construct_signature(
    type_params: Vec<TypeParamInfo>,
    params: Vec<ParamInfo>,
    this_type: Option<TypeId>,
    return_type: TypeId,
    type_predicate: Option<TypePredicate>,
    is_method: bool,
) -> CallSignature {
    CallSignature {
        type_params,
        params,
        this_type,
        return_type,
        type_predicate,
        is_method,
    }
}

pub(crate) fn enclosing_function_type_param_type(db: &dyn TypeDatabase, name: Atom) -> TypeId {
    db.type_param(TypeParamInfo::simple(name))
}
