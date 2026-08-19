//! Inherent constructors for [`CallSignature`], split out of `types.rs` to
//! keep that shard under the architecture size cap.

use super::{CallSignature, ParamInfo, TypeId};

impl CallSignature {
    /// Create a simple call signature with params and return type.
    pub const fn new(params: Vec<ParamInfo>, return_type: TypeId) -> Self {
        Self {
            type_params: Vec::new(),
            params,
            this_type: None,
            return_type,
            type_predicate: None,
            is_method: false,
            declaration_group: 0,
        }
    }
}
