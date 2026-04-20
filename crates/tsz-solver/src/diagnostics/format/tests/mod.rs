use super::*;

use crate::TypeInterner;
use crate::caches::db::QueryDatabase;
use crate::types::{
    CallSignature, CallableShape, FunctionShape, MappedModifier, MappedType, ParamInfo,
    PropertyInfo, StringIntrinsicKind, TemplateSpan, TypeParamInfo,
};

mod basics;
mod aggregate_types;
mod composed_types;
mod advanced_types;
mod callable_types;
mod optional_and_special;
