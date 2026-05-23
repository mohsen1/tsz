//! Solver query helpers used by return-type inference.
//!
//! This module keeps return-type inference callers away from the broad
//! `common` quarantine while #8225 splits that surface into narrower request
//! boundaries.

pub(crate) use super::common::{
    application_info, array_element_type, contains_type_parameters, index_access_types,
    lazy_def_id, mapped_type_info, type_application, type_param_info, union_members,
};
