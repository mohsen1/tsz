//! Core implementation for class instance type resolution.

include!("core_large_methods/get_class_instance_type_inner_13_0.rs");

use super::helpers::{
    AccessorAggregate, MethodAggregate, can_skip_base_instantiation,
    declaration_is_module_augmentation, exceeds_class_inheritance_depth_limit,
};
use crate::context::{EnclosingClassInfo, is_js_file_name};
use crate::query_boundaries::class_type::{callable_shape_for_type, object_shape_for_type};
use crate::query_boundaries::common::{ObjectFlags, TypeSubstitution, instantiate_type};
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;
use tsz_lowering::TypeLowering;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{
    CallSignature, CallableShape, IndexSignature, ObjectShape, PropertyInfo, TypeId, Visibility,
};

impl<'a> CheckerState<'a> {
    __tsz_split_core_get_class_instance_type_inner_13_0!();
}
