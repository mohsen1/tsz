use super::*;
use crate::caches::db::QueryDatabase;
use crate::construction::TypeInterner;
use crate::types::{
    CallSignature, CallableShape, FunctionShape, MappedModifier, MappedType, ParamInfo,
    PropertyInfo, StringIntrinsicKind, TemplateSpan, TypeParamInfo,
};

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of tests tests.
include!("tests_parts/part_00.rs");
include!("tests_parts/part_01.rs");
include!("tests_parts/part_02.rs");
