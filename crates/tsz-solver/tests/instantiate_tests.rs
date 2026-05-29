use super::*;
use crate::construction::TypeInterner;
use crate::def::DefId;
use crate::instantiation::instantiate::{
    MAX_INSTANTIATION_DEPTH, TypeSubstitution, instantiate_generic, instantiate_type,
};
use crate::relations::subtype::TypeEnvironment;
use crate::types::TypeData;

include!("instantiate_tests_parts/part_00.rs");
include!("instantiate_tests_parts/part_01.rs");
