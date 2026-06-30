use tsz_solver::TypeId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FlowCacheStability {
    Stable,
    Provisional,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct FlowCachePolicy {
    initial_type: TypeId,
    initial_has_type_params: bool,
    skip_cache_for_control_flow_typed_any: bool,
    stability: FlowCacheStability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FlowCacheBypass {
    explicit_unknown_switch: bool,
    exhaustive_unknown_typeof: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct FlowCacheRead {
    pub is_switch_clause: bool,
    pub is_loop_label_node: bool,
    pub bypass: FlowCacheBypass,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct FlowCacheWrite {
    pub is_loop_label_node: bool,
    pub bypass: FlowCacheBypass,
    pub final_type: TypeId,
    pub final_has_type_params: bool,
    pub unreachable_never: TypeId,
}

impl FlowCacheBypass {
    pub const fn new(explicit_unknown_switch: bool, exhaustive_unknown_typeof: bool) -> Self {
        Self {
            explicit_unknown_switch,
            exhaustive_unknown_typeof,
        }
    }

    pub const fn none() -> Self {
        Self::new(false, false)
    }

    const fn any(self) -> bool {
        self.explicit_unknown_switch || self.exhaustive_unknown_typeof
    }
}

impl FlowCachePolicy {
    pub const fn new(
        initial_type: TypeId,
        initial_has_type_params: bool,
        skip_cache_for_control_flow_typed_any: bool,
    ) -> Self {
        Self {
            initial_type,
            initial_has_type_params,
            skip_cache_for_control_flow_typed_any,
            stability: FlowCacheStability::Stable,
        }
    }

    pub const fn stability(self) -> FlowCacheStability {
        self.stability
    }

    pub const fn mark_provisional(&mut self) {
        self.stability = FlowCacheStability::Provisional;
    }

    pub const fn allows_read(self, read: FlowCacheRead) -> bool {
        !read.is_switch_clause
            && (!self.skip_cache_for_control_flow_typed_any || read.is_loop_label_node)
            && !read.bypass.any()
            && (!self.initial_has_type_params || read.is_loop_label_node)
    }

    pub fn allows_write(self, write: FlowCacheWrite) -> bool {
        write.final_type != write.unreachable_never
            && self.stability == FlowCacheStability::Stable
            && (!self.skip_cache_for_control_flow_typed_any || write.is_loop_label_node)
            && !write.bypass.any()
            && !self.initial_has_type_params
            && !write.final_has_type_params
    }

    pub fn allows_pending_writes(self) -> bool {
        self.stability == FlowCacheStability::Stable
    }

    pub fn allows_passthrough_chase(self) -> bool {
        !self.initial_has_type_params
            && !self.skip_cache_for_control_flow_typed_any
            && self.initial_type != TypeId::ANY
            && self.initial_type != TypeId::ERROR
            && self.initial_type != TypeId::UNKNOWN
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FlowCacheBypass, FlowCachePolicy, FlowCacheRead, FlowCacheStability, FlowCacheWrite,
    };
    use tsz_solver::TypeId;

    #[test]
    fn concrete_stable_flow_allows_cache_read_and_write() {
        let policy = FlowCachePolicy::new(TypeId::NUMBER, false, false);

        assert!(policy.allows_read(FlowCacheRead {
            is_switch_clause: false,
            is_loop_label_node: false,
            bypass: FlowCacheBypass::none(),
        }));
        assert!(policy.allows_write(FlowCacheWrite {
            is_loop_label_node: false,
            bypass: FlowCacheBypass::none(),
            final_type: TypeId::STRING,
            final_has_type_params: false,
            unreachable_never: TypeId::NEVER,
        }));
    }

    #[test]
    fn generic_initial_or_final_type_blocks_shared_writes() {
        let generic_initial = FlowCachePolicy::new(TypeId::NUMBER, true, false);
        let concrete_initial = FlowCachePolicy::new(TypeId::NUMBER, false, false);

        assert!(!generic_initial.allows_write(FlowCacheWrite {
            is_loop_label_node: true,
            bypass: FlowCacheBypass::none(),
            final_type: TypeId::STRING,
            final_has_type_params: false,
            unreachable_never: TypeId::NEVER,
        }));
        assert!(!concrete_initial.allows_write(FlowCacheWrite {
            is_loop_label_node: false,
            bypass: FlowCacheBypass::none(),
            final_type: TypeId::STRING,
            final_has_type_params: true,
            unreachable_never: TypeId::NEVER,
        }));
    }

    #[test]
    fn provisional_walk_blocks_pending_writes() {
        let mut policy = FlowCachePolicy::new(TypeId::NUMBER, false, false);

        policy.mark_provisional();

        assert_eq!(policy.stability(), FlowCacheStability::Provisional);
        assert!(!policy.allows_pending_writes());
        assert!(!policy.allows_write(FlowCacheWrite {
            is_loop_label_node: false,
            bypass: FlowCacheBypass::none(),
            final_type: TypeId::STRING,
            final_has_type_params: false,
            unreachable_never: TypeId::NEVER,
        }));
    }

    #[test]
    fn loop_label_can_read_recursion_guard_cache_for_generic_or_any_walks() {
        let generic_policy = FlowCachePolicy::new(TypeId::NUMBER, true, false);
        let control_flow_any_policy = FlowCachePolicy::new(TypeId::ANY, false, true);

        let loop_read = FlowCacheRead {
            is_switch_clause: false,
            is_loop_label_node: true,
            bypass: FlowCacheBypass::none(),
        };

        assert!(generic_policy.allows_read(loop_read));
        assert!(control_flow_any_policy.allows_read(loop_read));
    }

    #[test]
    fn explicit_unknown_paths_skip_cache_without_marking_walk_provisional() {
        let policy = FlowCachePolicy::new(TypeId::UNKNOWN, false, false);

        assert!(!policy.allows_read(FlowCacheRead {
            is_switch_clause: false,
            is_loop_label_node: false,
            bypass: FlowCacheBypass::new(true, false),
        }));
        assert!(!policy.allows_write(FlowCacheWrite {
            is_loop_label_node: false,
            bypass: FlowCacheBypass::new(false, true),
            final_type: TypeId::STRING,
            final_has_type_params: false,
            unreachable_never: TypeId::NEVER,
        }));
        assert_eq!(policy.stability(), FlowCacheStability::Stable);
    }
}
