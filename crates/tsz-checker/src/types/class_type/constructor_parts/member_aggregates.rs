use tsz_parser::parser::NodeIndex;
use tsz_solver::{CallSignature, TypeId, Visibility};

pub(super) struct MethodAggregate {
    pub(super) overload_signatures: Vec<CallSignature>,
    pub(super) impl_signatures: Vec<CallSignature>,
    pub(super) overload_optional: bool,
    pub(super) impl_optional: bool,
    pub(super) visibility: Visibility,
    /// Node index of the implementation method (body present), used to cache
    /// the final callable type in `node_types` for declaration emit.
    pub(super) impl_member_idx: Option<NodeIndex>,
}

pub(super) struct AccessorAggregate {
    pub(super) getter: Option<TypeId>,
    pub(super) setter: Option<TypeId>,
    pub(super) visibility: Visibility,
}
