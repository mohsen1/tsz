use super::*;
use crate::SubtypeFailureReason;
use crate::caches::db::QueryDatabase;
use crate::computation::TypeEnvironment;
use crate::construction::TypeInterner;
use crate::def::DefId;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::{
    CallSignature, CallableShape, ConditionalType, FunctionShape, IndexSignature, MappedType,
    ObjectFlags, ObjectShape, ParamInfo, PropertyInfo, SymbolRef, TemplateSpan, TupleElement,
    TypeParamInfo, Visibility,
};

fn make_animal_dog(interner: &TypeInterner) -> (TypeId, TypeId) {
    let name = interner.intern_string("name");
    let breed = interner.intern_string("breed");

    let animal = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    let dog = interner.object(vec![
        PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
        PropertyInfo::new(breed, TypeId::STRING),
    ]);

    (animal, dog)
}

fn make_object_interface(interner: &TypeInterner) -> TypeId {
    let method = |return_type| FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };
    let method_with_any = |return_type| FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::ANY)],
        this_type: None,
        return_type,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let constructor = PropertyInfo::new(interner.intern_string("constructor"), TypeId::ANY);
    let to_string = PropertyInfo::method(
        interner.intern_string("toString"),
        interner.function(method(TypeId::STRING)),
    );
    let to_locale = PropertyInfo::method(
        interner.intern_string("toLocaleString"),
        interner.function(method(TypeId::STRING)),
    );
    let value_of = PropertyInfo::method(
        interner.intern_string("valueOf"),
        interner.function(method(TypeId::ANY)),
    );
    let has_own = PropertyInfo::method(
        interner.intern_string("hasOwnProperty"),
        interner.function(method_with_any(TypeId::BOOLEAN)),
    );
    let is_proto = PropertyInfo::method(
        interner.intern_string("isPrototypeOf"),
        interner.function(method_with_any(TypeId::BOOLEAN)),
    );
    let prop_enum = PropertyInfo::method(
        interner.intern_string("propertyIsEnumerable"),
        interner.function(method_with_any(TypeId::BOOLEAN)),
    );

    interner.object(vec![
        constructor,
        to_string,
        to_locale,
        value_of,
        has_own,
        is_proto,
        prop_enum,
    ])
}

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of compat_tests tests.
include!("compat_tests_parts/part_00.rs");
include!("compat_tests_parts/part_01.rs");
include!("compat_tests_parts/part_02.rs");
include!("compat_tests_parts/part_03.rs");
