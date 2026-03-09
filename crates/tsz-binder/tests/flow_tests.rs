//! Unit tests for the flow module.
//!
//! Tests for FlowNodeId, FlowNode, FlowNodeArena, and flow_flags.

use tsz_binder::{FlowNode, FlowNodeArena, FlowNodeId, flow_flags};

// =============================================================================
// FlowNodeId Tests
// =============================================================================

mod flow_node_id {
    use super::*;

    #[test]
    fn none_constant_is_max_u32() {
        assert_eq!(FlowNodeId::NONE.0, u32::MAX);
    }

    #[test]
    fn is_none_returns_true_for_none() {
        assert!(FlowNodeId::NONE.is_none());
    }

    #[test]
    fn is_none_returns_false_for_valid_id() {
        assert!(!FlowNodeId(0).is_none());
        assert!(!FlowNodeId(1).is_none());
        assert!(!FlowNodeId(1000).is_none());
    }

    #[test]
    fn is_some_returns_false_for_none() {
        assert!(!FlowNodeId::NONE.is_some());
    }

    #[test]
    fn is_some_returns_true_for_valid_id() {
        assert!(FlowNodeId(0).is_some());
        assert!(FlowNodeId(1).is_some());
        assert!(FlowNodeId(1000).is_some());
    }

    #[test]
    fn is_none_and_is_some_are_opposites() {
        for id in [
            FlowNodeId::NONE,
            FlowNodeId(0),
            FlowNodeId(42),
            FlowNodeId(u32::MAX - 1),
        ] {
            assert_eq!(id.is_none(), !id.is_some());
        }
    }
}

// =============================================================================
// Flow Flags Tests
// =============================================================================

mod flow_flags_tests {
    use super::*;

    #[test]
    fn individual_flags_are_powers_of_two() {
        // Each individual flag should be a single bit
        assert_eq!(flow_flags::UNREACHABLE.count_ones(), 1);
        assert_eq!(flow_flags::START.count_ones(), 1);
        assert_eq!(flow_flags::BRANCH_LABEL.count_ones(), 1);
        assert_eq!(flow_flags::LOOP_LABEL.count_ones(), 1);
        assert_eq!(flow_flags::ASSIGNMENT.count_ones(), 1);
        assert_eq!(flow_flags::TRUE_CONDITION.count_ones(), 1);
        assert_eq!(flow_flags::FALSE_CONDITION.count_ones(), 1);
        assert_eq!(flow_flags::SWITCH_CLAUSE.count_ones(), 1);
        assert_eq!(flow_flags::ARRAY_MUTATION.count_ones(), 1);
        assert_eq!(flow_flags::CALL.count_ones(), 1);
        assert_eq!(flow_flags::REDUCE_LABEL.count_ones(), 1);
        assert_eq!(flow_flags::REFERENCED.count_ones(), 1);
        assert_eq!(flow_flags::AWAIT_POINT.count_ones(), 1);
        assert_eq!(flow_flags::YIELD_POINT.count_ones(), 1);
    }

    #[test]
    fn label_composite_is_branch_or_loop() {
        assert_eq!(
            flow_flags::LABEL,
            flow_flags::BRANCH_LABEL | flow_flags::LOOP_LABEL
        );
    }

    #[test]
    fn condition_composite_is_true_or_false() {
        assert_eq!(
            flow_flags::CONDITION,
            flow_flags::TRUE_CONDITION | flow_flags::FALSE_CONDITION
        );
    }

    #[test]
    fn label_and_condition_share_no_bits() {
        // LABEL and CONDITION should have no overlapping bits
        assert_eq!(flow_flags::LABEL & flow_flags::CONDITION, 0);
    }

    #[test]
    fn start_and_unreachable_are_distinct() {
        // START and UNREACHABLE should be distinct flags
        assert_ne!(flow_flags::START, flow_flags::UNREACHABLE);
        assert_eq!(flow_flags::START & flow_flags::UNREACHABLE, 0);
    }
}

// =============================================================================
// FlowNode Tests
// =============================================================================

mod flow_node {
    use super::*;

    #[test]
    fn new_creates_node_with_flags() {
        let id = FlowNodeId(5);
        let node = FlowNode::new(id, flow_flags::START);

        assert_eq!(node.id, id);
        assert_eq!(node.flags, flow_flags::START);
    }

    #[test]
    fn new_creates_empty_antecedents() {
        let node = FlowNode::new(FlowNodeId(0), flow_flags::ASSIGNMENT);

        assert!(node.antecedent.is_empty());
    }

    #[test]
    fn has_flags_returns_true_when_all_flags_present() {
        let node = FlowNode::new(FlowNodeId(0), flow_flags::START | flow_flags::UNREACHABLE);

        assert!(node.has_flags(flow_flags::START));
        assert!(node.has_flags(flow_flags::UNREACHABLE));
        assert!(node.has_flags(flow_flags::START | flow_flags::UNREACHABLE));
    }

    #[test]
    fn has_flags_returns_false_when_any_flag_missing() {
        let node = FlowNode::new(FlowNodeId(0), flow_flags::START);

        assert!(!node.has_flags(flow_flags::UNREACHABLE));
        assert!(!node.has_flags(flow_flags::START | flow_flags::UNREACHABLE));
    }

    #[test]
    fn has_flags_returns_true_for_zero_flags() {
        // Checking for zero flags should always return true
        let node = FlowNode::new(FlowNodeId(0), 0);
        assert!(node.has_flags(0));

        let node_with_flags = FlowNode::new(FlowNodeId(1), flow_flags::START);
        assert!(node_with_flags.has_flags(0));
    }

    #[test]
    fn has_any_flags_returns_true_when_any_flag_present() {
        let node = FlowNode::new(FlowNodeId(0), flow_flags::START | flow_flags::UNREACHABLE);

        assert!(node.has_any_flags(flow_flags::START));
        assert!(node.has_any_flags(flow_flags::UNREACHABLE));
        assert!(node.has_any_flags(flow_flags::START | flow_flags::ASSIGNMENT));
    }

    #[test]
    fn has_any_flags_returns_false_when_no_flags_match() {
        let node = FlowNode::new(FlowNodeId(0), flow_flags::START);

        assert!(!node.has_any_flags(flow_flags::UNREACHABLE));
        assert!(!node.has_any_flags(flow_flags::ASSIGNMENT | flow_flags::CALL));
    }

    #[test]
    fn has_any_flags_returns_false_for_zero_check() {
        // Checking for zero flags should return false (no bits match)
        let node = FlowNode::new(FlowNodeId(0), flow_flags::START);
        assert!(!node.has_any_flags(0));

        let node_zero = FlowNode::new(FlowNodeId(1), 0);
        assert!(!node_zero.has_any_flags(0));
    }

    #[test]
    fn node_with_composite_flags() {
        // LABEL = BRANCH_LABEL | LOOP_LABEL
        // Creating a node with LABEL means both flags are set
        let node = FlowNode::new(FlowNodeId(0), flow_flags::LABEL);

        // Node should have both individual flags
        assert!(node.has_flags(flow_flags::BRANCH_LABEL));
        assert!(node.has_flags(flow_flags::LOOP_LABEL));
        // Node should have the composite (both flags together)
        assert!(node.has_flags(flow_flags::BRANCH_LABEL | flow_flags::LOOP_LABEL));

        // Creating with explicit OR gives the same result
        let label_node = FlowNode::new(
            FlowNodeId(1),
            flow_flags::BRANCH_LABEL | flow_flags::LOOP_LABEL,
        );
        assert!(label_node.has_flags(flow_flags::LABEL));
    }

    #[test]
    fn antecedent_can_be_modified() {
        let mut node = FlowNode::new(FlowNodeId(1), flow_flags::ASSIGNMENT);
        node.antecedent.push(FlowNodeId(0));

        assert_eq!(node.antecedent.len(), 1);
        assert_eq!(node.antecedent[0], FlowNodeId(0));
    }
}

// =============================================================================
// FlowNodeArena Tests
// =============================================================================

mod flow_node_arena {
    use super::*;

    #[test]
    fn new_creates_empty_arena() {
        let arena = FlowNodeArena::new();

        assert!(arena.is_empty());
        assert_eq!(arena.len(), 0);
    }

    #[test]
    fn default_creates_empty_arena() {
        let arena = FlowNodeArena::default();

        assert!(arena.is_empty());
        assert_eq!(arena.len(), 0);
    }

    #[test]
    fn alloc_returns_sequential_ids() {
        let mut arena = FlowNodeArena::new();

        let id0 = arena.alloc(flow_flags::START);
        let id1 = arena.alloc(flow_flags::ASSIGNMENT);
        let id2 = arena.alloc(flow_flags::TRUE_CONDITION);

        assert_eq!(id0, FlowNodeId(0));
        assert_eq!(id1, FlowNodeId(1));
        assert_eq!(id2, FlowNodeId(2));
    }

    #[test]
    fn alloc_increases_len() {
        let mut arena = FlowNodeArena::new();

        arena.alloc(flow_flags::START);
        assert_eq!(arena.len(), 1);

        arena.alloc(flow_flags::ASSIGNMENT);
        assert_eq!(arena.len(), 2);

        arena.alloc(flow_flags::TRUE_CONDITION);
        assert_eq!(arena.len(), 3);
    }

    #[test]
    fn get_returns_node_by_id() {
        let mut arena = FlowNodeArena::new();

        let id = arena.alloc(flow_flags::START | flow_flags::UNREACHABLE);
        let node = arena.get(id).expect("node should exist");

        assert_eq!(node.id, id);
        assert_eq!(node.flags, flow_flags::START | flow_flags::UNREACHABLE);
    }

    #[test]
    fn get_returns_none_for_invalid_id() {
        let arena = FlowNodeArena::new();

        // Arena is empty, any non-NONE id should return None
        assert!(arena.get(FlowNodeId(0)).is_none());
        assert!(arena.get(FlowNodeId(100)).is_none());
    }

    #[test]
    fn get_returns_none_for_none_id() {
        let mut arena = FlowNodeArena::new();
        arena.alloc(flow_flags::START);

        assert!(arena.get(FlowNodeId::NONE).is_none());
    }

    #[test]
    fn get_mut_returns_node_by_id() {
        let mut arena = FlowNodeArena::new();

        let id = arena.alloc(flow_flags::ASSIGNMENT);
        let node = arena.get_mut(id).expect("node should exist");

        assert_eq!(node.id, id);
        assert_eq!(node.flags, flow_flags::ASSIGNMENT);

        // Modify antecedent
        node.antecedent.push(FlowNodeId(0));
    }

    #[test]
    fn get_mut_allows_modification() {
        let mut arena = FlowNodeArena::new();

        let id0 = arena.alloc(flow_flags::START);
        let id1 = arena.alloc(flow_flags::ASSIGNMENT);

        // Modify id1's antecedent
        {
            let node = arena.get_mut(id1).expect("node should exist");
            node.antecedent.push(id0);
        }

        // Verify modification
        let node = arena.get(id1).expect("node should exist");
        assert_eq!(node.antecedent, vec![id0]);
    }

    #[test]
    fn get_mut_returns_none_for_none_id() {
        let mut arena = FlowNodeArena::new();
        arena.alloc(flow_flags::START);

        assert!(arena.get_mut(FlowNodeId::NONE).is_none());
    }

    #[test]
    fn clear_empties_arena() {
        let mut arena = FlowNodeArena::new();

        arena.alloc(flow_flags::START);
        arena.alloc(flow_flags::ASSIGNMENT);
        assert_eq!(arena.len(), 2);

        arena.clear();

        assert!(arena.is_empty());
        assert_eq!(arena.len(), 0);
    }

    #[test]
    fn find_unreachable_returns_none_when_no_unreachable_nodes() {
        let mut arena = FlowNodeArena::new();

        arena.alloc(flow_flags::START);
        arena.alloc(flow_flags::ASSIGNMENT);
        arena.alloc(flow_flags::TRUE_CONDITION);

        assert!(arena.find_unreachable().is_none());
    }

    #[test]
    fn find_unreachable_returns_id_of_unreachable_node() {
        let mut arena = FlowNodeArena::new();

        arena.alloc(flow_flags::START);
        let unreachable_id = arena.alloc(flow_flags::UNREACHABLE);
        arena.alloc(flow_flags::ASSIGNMENT);

        let found = arena.find_unreachable();
        assert_eq!(found, Some(unreachable_id));
    }

    #[test]
    fn find_unreachable_returns_first_unreachable() {
        let mut arena = FlowNodeArena::new();

        arena.alloc(flow_flags::START);
        let first_unreachable = arena.alloc(flow_flags::UNREACHABLE);
        arena.alloc(flow_flags::ASSIGNMENT);
        arena.alloc(flow_flags::UNREACHABLE | flow_flags::CALL); // Another unreachable

        // Should return the first one
        let found = arena.find_unreachable();
        assert_eq!(found, Some(first_unreachable));
    }

    #[test]
    fn find_unreachable_finds_node_with_unreachable_among_other_flags() {
        let mut arena = FlowNodeArena::new();

        arena.alloc(flow_flags::START);
        let id = arena.alloc(flow_flags::UNREACHABLE | flow_flags::ASSIGNMENT);

        let found = arena.find_unreachable();
        assert_eq!(found, Some(id));
    }

    #[test]
    fn is_empty_returns_false_after_alloc() {
        let mut arena = FlowNodeArena::new();
        assert!(arena.is_empty());

        arena.alloc(flow_flags::START);
        assert!(!arena.is_empty());
    }

    #[test]
    fn clone_creates_independent_copy() {
        let mut arena = FlowNodeArena::new();
        let id = arena.alloc(flow_flags::START);

        let cloned = arena.clone();

        // Both have the same content
        assert_eq!(arena.len(), cloned.len());
        assert_eq!(
            arena.get(id).map(|n| n.flags),
            cloned.get(id).map(|n| n.flags)
        );

        // Modifying original doesn't affect clone
        arena.alloc(flow_flags::ASSIGNMENT);
        assert_ne!(arena.len(), cloned.len());
    }
}

// =============================================================================
// Integration Tests
// =============================================================================

mod integration {
    use super::*;

    #[test]
    fn create_flow_graph_with_branches() {
        let mut arena = FlowNodeArena::new();

        // Create start node
        let start = arena.alloc(flow_flags::START);

        // Create branch nodes
        let true_branch = arena.alloc(flow_flags::TRUE_CONDITION);
        let false_branch = arena.alloc(flow_flags::FALSE_CONDITION);

        // Set up antecedents (both branches come from start)
        arena.get_mut(true_branch).unwrap().antecedent.push(start);
        arena.get_mut(false_branch).unwrap().antecedent.push(start);

        // Verify structure
        assert_eq!(arena.len(), 3);

        let true_node = arena.get(true_branch).unwrap();
        assert!(true_node.has_flags(flow_flags::TRUE_CONDITION));
        assert_eq!(true_node.antecedent, vec![start]);

        let false_node = arena.get(false_branch).unwrap();
        assert!(false_node.has_flags(flow_flags::FALSE_CONDITION));
        assert_eq!(false_node.antecedent, vec![start]);
    }

    #[test]
    fn create_loop_with_assignment() {
        let mut arena = FlowNodeArena::new();

        // Create loop structure
        let start = arena.alloc(flow_flags::START);
        let loop_label = arena.alloc(flow_flags::LOOP_LABEL);
        let assignment = arena.alloc(flow_flags::ASSIGNMENT);

        // Set up antecedents
        arena.get_mut(loop_label).unwrap().antecedent.push(start);
        arena
            .get_mut(assignment)
            .unwrap()
            .antecedent
            .push(loop_label);

        // Verify
        let assign_node = arena.get(assignment).unwrap();
        assert!(assign_node.has_flags(flow_flags::ASSIGNMENT));
        assert_eq!(assign_node.antecedent, vec![loop_label]);

        // Verify loop has label flag
        let loop_node = arena.get(loop_label).unwrap();
        assert!(loop_node.has_any_flags(flow_flags::LABEL)); // LABEL = BRANCH_LABEL | LOOP_LABEL
    }
}
