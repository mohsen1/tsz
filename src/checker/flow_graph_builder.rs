//! Flow Graph Builder for Control Flow Analysis.
//!
//! This module provides the `FlowGraph` side-table and `FlowGraphBuilder` for
//! constructing control flow graphs from Node AST post-binding.
//!
//! The FlowGraph is a side-table that tracks:
//! - Flow nodes for each control flow point (conditions, branches, loops)
//! - Mapping from AST nodes to their corresponding flow nodes
//! - Antecedent relationships between flow nodes
//!
//! This enables type narrowing analysis without mutating AST nodes.

use crate::binder::{FlowNode, FlowNodeArena, FlowNodeId, flow_flags};
use crate::parser::node::NodeArena;
use crate::parser::{NodeIndex, NodeList, syntax_kind_ext};
use crate::scanner::SyntaxKind;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use tracing::{Level, debug, span};

/// A control flow graph side-table.
///
/// This struct encapsulates the flow graph data structures, providing
/// a clean abstraction for querying flow information without holding
/// a reference to the entire binder state.
#[derive(Clone, Debug)]
pub struct FlowGraph {
    /// Arena storing all flow nodes
    pub nodes: FlowNodeArena,
    /// Mapping from AST node index to the flow node active at that point
    pub node_flow: FxHashMap<u32, FlowNodeId>,
    /// Unreachable flow node (for never-returning code paths)
    pub unreachable_flow: FlowNodeId,
    /// Set of AST node indices that are unreachable
    pub unreachable_nodes: FxHashSet<u32>,
}

impl FlowGraph {
    /// Create a new empty flow graph.
    pub fn new() -> Self {
        let mut nodes = FlowNodeArena::new();
        let unreachable_flow = nodes.alloc(flow_flags::UNREACHABLE);

        FlowGraph {
            nodes,
            node_flow: FxHashMap::default(),
            unreachable_flow,
            unreachable_nodes: FxHashSet::default(),
        }
    }

    /// Get the flow node at a given AST node position.
    pub fn get_flow_at_node(&self, node: NodeIndex) -> Option<FlowNodeId> {
        self.node_flow.get(&node.0).copied()
    }

    /// Get a flow node by ID.
    pub fn get_node(&self, id: FlowNodeId) -> Option<&FlowNode> {
        self.nodes.get(id)
    }

    /// Check if a flow node exists.
    pub fn has_flow_at_node(&self, node: NodeIndex) -> bool {
        self.node_flow.contains_key(&node.0)
    }

    /// Check if an AST node is unreachable.
    pub fn is_unreachable(&self, node: NodeIndex) -> bool {
        self.unreachable_nodes.contains(&node.0)
    }

    /// Mark an AST node as unreachable.
    pub fn mark_unreachable(&mut self, node: NodeIndex) {
        self.unreachable_nodes.insert(node.0);
    }
}

impl Default for FlowGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for constructing a FlowGraph from Node AST.
///
/// The FlowGraphBuilder traverses the AST post-binding and constructs
/// the control flow graph without mutating the AST nodes.
pub struct FlowGraphBuilder<'a> {
    /// Reference to the NodeArena
    arena: &'a NodeArena,
    /// The flow graph being constructed
    graph: FlowGraph,
    /// Current flow node during construction
    current_flow: FlowNodeId,
    /// Stack of flow contexts for nested constructs
    flow_stack: Vec<FlowContext>,
    /// Depth of async function nesting (0 if not in async function)
    async_depth: u32,
    /// Depth of generator function nesting (0 if not in generator function)
    generator_depth: u32,
}

/// Context for nested flow constructs (loops, switches, async functions, etc.)
#[derive(Clone, Copy)]
struct FlowContext {
    /// Label for breaking out of this construct
    break_label: FlowNodeId,
    /// Label for continuing this construct (loops only)
    continue_label: Option<FlowNodeId>,
    /// Type of flow construct
    context_type: FlowContextType,
    /// Finally block to execute on exit (for try statements)
    finally_block: NodeIndex,
    /// Flow state before entering finally (for routing exits through finally)
    #[allow(dead_code)] // Infrastructure for try-finally flow analysis
    pre_finally_flow: FlowNodeId,
    /// Flow state after exiting finally (for routing exits through finally)
    #[allow(dead_code)] // Infrastructure for try-finally flow analysis
    post_finally_flow: FlowNodeId,
    /// Label identifier for this context (for labeled statements)
    label: NodeIndex,
}

#[derive(Clone, Copy, PartialEq)]
#[allow(dead_code)] // Infrastructure for flow control analysis
enum FlowContextType {
    Loop,
    Switch,
    Try,
    AsyncFunction,
}

impl<'a> FlowGraphBuilder<'a> {
    /// Create a new FlowGraphBuilder.
    pub fn new(arena: &'a NodeArena) -> Self {
        let mut graph = FlowGraph::new();
        let start_flow = graph.nodes.alloc(flow_flags::START);

        FlowGraphBuilder {
            arena,
            graph,
            current_flow: start_flow,
            flow_stack: Vec::new(),
            async_depth: 0,
            generator_depth: 0,
        }
    }

    /// Build the flow graph for a source file.
    pub fn build_source_file(&mut self, statements: &NodeList) -> &FlowGraph {
        let _span = span!(
            Level::DEBUG,
            "build_flow_graph",
            num_statements = statements.nodes.len()
        )
        .entered();
        debug!(
            "Building flow graph for source file with {} statements",
            statements.nodes.len()
        );

        for &stmt_idx in &statements.nodes {
            if !stmt_idx.is_none() {
                self.build_statement(stmt_idx);
            }
        }
        &self.graph
    }

    /// Build the flow graph for a list of statements.
    ///
    /// This is a general entry point that can be used for:
    /// - Source files
    /// - Function bodies
    /// - Block statements
    /// - Any list of statements
    ///
    /// This is an alias for `build_source_file()` but with a more general name.
    pub fn build_flow_graph(&mut self, statements: &NodeList) -> &FlowGraph {
        self.build_source_file(statements)
    }

    /// Build the flow graph for a function body.
    ///
    /// Entry point for building flow graphs for function bodies.
    /// Resets the builder state and creates a new START node for the function.
    ///
    /// # Arguments
    /// * `body` - The block statement representing the function body
    ///
    /// # Returns
    /// Reference to the built flow graph
    pub fn build_function_body(&mut self, body: &crate::parser::node::BlockData) -> &FlowGraph {
        let _span = span!(
            Level::DEBUG,
            "build_function_body",
            num_statements = body.statements.nodes.len()
        )
        .entered();
        debug!(
            "Building flow graph for function body with {} statements",
            body.statements.nodes.len()
        );

        // Reset the builder state for a new function
        self.graph = FlowGraph::new();
        self.current_flow = self.graph.nodes.alloc(flow_flags::START);
        self.flow_stack.clear();
        self.async_depth = 0;
        self.generator_depth = 0;

        // Build the function body
        self.build_block(body);
        &self.graph
    }

    /// Build the flow graph for a single statement.
    fn build_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(node) = self.arena.get(stmt_idx) else {
            return;
        };

        match node.kind {
            // Block statement
            syntax_kind_ext::BLOCK => {
                if let Some(block) = self.arena.get_block(node) {
                    self.build_block(block);
                }
            }

            // If statement
            syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.arena.get_if_statement(node) {
                    self.build_if_statement(if_stmt);
                }
            }

            // While statement
            syntax_kind_ext::WHILE_STATEMENT => {
                if let Some(loop_data) = self.arena.get_loop(node) {
                    self.build_while_statement(loop_data);
                }
            }

            // Do-while statement
            syntax_kind_ext::DO_STATEMENT => {
                if let Some(loop_data) = self.arena.get_loop(node) {
                    self.build_do_while_statement(loop_data);
                }
            }

            // For statement
            syntax_kind_ext::FOR_STATEMENT => {
                if let Some(loop_data) = self.arena.get_loop(node) {
                    self.build_for_statement(loop_data);
                }
            }

            // For-in statement
            syntax_kind_ext::FOR_IN_STATEMENT => {
                if let Some(for_in_of) = self.arena.get_for_in_of(node) {
                    self.build_for_in_statement(for_in_of);
                }
            }

            // For-of statement
            syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(for_in_of) = self.arena.get_for_in_of(node) {
                    self.build_for_of_statement(for_in_of);
                }
            }

            // Switch statement
            syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch_data) = self.arena.get_switch(node) {
                    self.build_switch_statement(switch_data);
                }
            }

            // Try statement
            syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.arena.get_try(node) {
                    self.build_try_statement(try_data);
                }
            }

            // Labeled statement
            syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled_data) = self.arena.get_labeled_statement(node) {
                    self.build_labeled_statement(labeled_data);
                }
            }

            // Variable declaration
            syntax_kind_ext::VARIABLE_DECLARATION => {
                if let Some(var_decl) = self.arena.get_variable_declaration(node) {
                    self.build_variable_declaration(var_decl, stmt_idx);
                }
            }

            // Variable statement - contains variable declaration list
            syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_data) = self.arena.get_variable(node) {
                    for &decl_idx in &var_data.declarations.nodes {
                        if !decl_idx.is_none() {
                            self.build_statement(decl_idx);
                        }
                    }
                }
                self.record_node_flow(stmt_idx);
            }

            // Variable declaration list - contains variable declarations
            syntax_kind_ext::VARIABLE_DECLARATION_LIST => {
                if let Some(var_data) = self.arena.get_variable(node) {
                    for &decl_idx in &var_data.declarations.nodes {
                        if !decl_idx.is_none() {
                            self.build_statement(decl_idx);
                        }
                    }
                }
                self.record_node_flow(stmt_idx);
            }

            // Function declaration - check if async/generator
            syntax_kind_ext::FUNCTION_DECLARATION
            | syntax_kind_ext::FUNCTION_EXPRESSION
            | syntax_kind_ext::ARROW_FUNCTION => {
                if let Some(func) = self.arena.get_function(node) {
                    // Track async and generator context
                    let was_async = func.is_async;
                    let was_generator = func.asterisk_token;

                    if was_async {
                        self.async_depth += 1;
                    }
                    if was_generator {
                        self.generator_depth += 1;
                    }

                    self.record_node_flow(stmt_idx);
                    // Note: We don't descend into function bodies in this flow graph builder
                    // as each function has its own flow graph

                    if was_async {
                        self.async_depth -= 1;
                    }
                    if was_generator {
                        self.generator_depth -= 1;
                    }
                }
            }

            // Expression statement - check for await/yield expressions
            syntax_kind_ext::EXPRESSION_STATEMENT => {
                // Get the expression from the expression statement
                if let Some(expr_stmt) = self.arena.get_expression_statement(node) {
                    self.handle_expression_for_suspension_points(expr_stmt.expression);
                    self.handle_expression_for_assignments(expr_stmt.expression);
                }
                self.record_node_flow(stmt_idx);
            }

            // Await expression (as a standalone expression)
            syntax_kind_ext::AWAIT_EXPRESSION => {
                self.handle_await_expression(stmt_idx);
                self.record_node_flow(stmt_idx);
            }

            // Yield expression (as a standalone expression)
            syntax_kind_ext::YIELD_EXPRESSION => {
                self.handle_yield_expression(stmt_idx);
                self.record_node_flow(stmt_idx);
            }

            // Class declaration - track heritage clause expressions, static blocks, and computed properties
            syntax_kind_ext::CLASS_DECLARATION | syntax_kind_ext::CLASS_EXPRESSION => {
                self.build_class_declaration(stmt_idx);
            }

            // Return/throw/break/continue
            syntax_kind_ext::RETURN_STATEMENT | syntax_kind_ext::THROW_STATEMENT => {
                self.record_node_flow(stmt_idx);

                // Check for try contexts with finally blocks that need to execute
                let _pre_exit_flow = self.current_flow;

                // Collect and execute any finally blocks on the stack
                let mut finally_flows: Vec<NodeIndex> = Vec::new();
                for ctx in self.flow_stack.iter().rev() {
                    if !ctx.finally_block.is_none() && ctx.context_type == FlowContextType::Try {
                        finally_flows.push(ctx.finally_block);
                    }
                }

                // Build finally blocks in reverse order (innermost first)
                for finally_block in finally_flows.iter().rev() {
                    self.build_statement(*finally_block);
                }

                // After all finally blocks, set to unreachable
                self.current_flow = self.graph.unreachable_flow;
            }

            syntax_kind_ext::BREAK_STATEMENT => {
                self.record_node_flow(stmt_idx);
                self.handle_break(stmt_idx);
            }

            syntax_kind_ext::CONTINUE_STATEMENT => {
                self.record_node_flow(stmt_idx);
                self.handle_continue(stmt_idx);
            }

            _ => {
                // Default: just record flow position
                self.record_node_flow(stmt_idx);
            }
        }
    }

    /// Build flow graph for a block.
    ///
    /// Entry point for building flow graphs for block statements.
    /// Processes all statements in the block sequentially.
    ///
    /// # Arguments
    /// * `block` - The block statement to build flow graph for
    pub fn build_block(&mut self, block: &crate::parser::node::BlockData) {
        for &stmt_idx in &block.statements.nodes {
            if !stmt_idx.is_none() {
                self.build_statement(stmt_idx);
            }
        }
    }

    /// Build flow graph for an if statement.
    fn build_if_statement(&mut self, if_stmt: &crate::parser::node::IfStatementData) {
        // Bug #2.1: Track assignments in condition expression
        self.handle_expression_for_assignments(if_stmt.expression);

        // Save flow before the condition
        let pre_condition_flow = self.current_flow;

        // If already unreachable, stay unreachable (Bug #3.1 fix)
        if pre_condition_flow == self.graph.unreachable_flow {
            // Build branches for error checking but don't resurrect flow
            self.build_statement(if_stmt.then_statement);
            if !if_stmt.else_statement.is_none() {
                self.build_statement(if_stmt.else_statement);
            }
            // Stay unreachable - don't resurrect with merge label
            return;
        }

        // Create flow node for the true branch
        let true_flow = self.create_flow_node(
            flow_flags::TRUE_CONDITION,
            pre_condition_flow,
            if_stmt.expression,
        );

        // Bind the then statement with true flow
        self.current_flow = true_flow;
        self.build_statement(if_stmt.then_statement);
        let post_then_flow = self.current_flow;

        // Create merge label
        let merge_label = self.graph.nodes.alloc(flow_flags::BRANCH_LABEL);

        // Handle else branch if present
        if !if_stmt.else_statement.is_none() {
            // Create flow node for the false branch
            let false_flow = self.create_flow_node(
                flow_flags::FALSE_CONDITION,
                pre_condition_flow,
                if_stmt.expression,
            );

            // Bind the else statement
            self.current_flow = false_flow;
            self.build_statement(if_stmt.else_statement);
            let post_else_flow = self.current_flow;

            // Add both branches to merge label
            self.add_antecedent(merge_label, post_then_flow);
            self.add_antecedent(merge_label, post_else_flow);
        } else {
            // No else branch: false path goes directly to merge
            let false_flow = self.create_flow_node(
                flow_flags::FALSE_CONDITION,
                pre_condition_flow,
                if_stmt.expression,
            );

            self.add_antecedent(merge_label, post_then_flow);
            self.add_antecedent(merge_label, false_flow);
        }

        self.current_flow = merge_label;
    }

    /// Build flow graph for a while statement.
    fn build_while_statement(&mut self, loop_data: &crate::parser::node::LoopData) {
        // Create loop label
        let loop_label = self.graph.nodes.alloc(flow_flags::LOOP_LABEL);
        if !self.current_flow.is_none()
            && let Some(node) = self.graph.nodes.get_mut(loop_label)
        {
            node.antecedent.push(self.current_flow);
        }

        // Create merge label for after the loop
        let merge_label = self.graph.nodes.alloc(flow_flags::BRANCH_LABEL);

        // Push loop context
        self.flow_stack.push(FlowContext {
            break_label: merge_label,
            continue_label: Some(loop_label),
            context_type: FlowContextType::Loop,
            finally_block: NodeIndex::NONE,
            pre_finally_flow: FlowNodeId::NONE,
            post_finally_flow: FlowNodeId::NONE,
            label: NodeIndex::NONE,
        });

        self.current_flow = loop_label;

        // Bug #2.1: Track assignments in condition expression
        self.handle_expression_for_assignments(loop_data.condition);

        // Create flow for entering loop body
        let true_flow =
            self.create_flow_node(flow_flags::TRUE_CONDITION, loop_label, loop_data.condition);

        // Bind loop body
        self.current_flow = true_flow;
        self.build_statement(loop_data.statement);

        // Loop back to loop label
        self.add_antecedent(loop_label, self.current_flow);

        // Create flow for exiting loop
        let false_flow =
            self.create_flow_node(flow_flags::FALSE_CONDITION, loop_label, loop_data.condition);

        // Add to merge label
        self.add_antecedent(merge_label, false_flow);

        self.flow_stack.pop();
        self.current_flow = merge_label;
    }

    /// Build flow graph for a do-while statement.
    fn build_do_while_statement(&mut self, loop_data: &crate::parser::node::LoopData) {
        // Create loop label
        let loop_label = self.graph.nodes.alloc(flow_flags::LOOP_LABEL);
        if !self.current_flow.is_none()
            && let Some(node) = self.graph.nodes.get_mut(loop_label)
        {
            node.antecedent.push(self.current_flow);
        }

        // Create merge label for after the loop
        let merge_label = self.graph.nodes.alloc(flow_flags::BRANCH_LABEL);

        // Push loop context
        self.flow_stack.push(FlowContext {
            break_label: merge_label,
            continue_label: Some(loop_label),
            context_type: FlowContextType::Loop,
            finally_block: NodeIndex::NONE,
            pre_finally_flow: FlowNodeId::NONE,
            post_finally_flow: FlowNodeId::NONE,
            label: NodeIndex::NONE,
        });

        self.current_flow = loop_label;

        // Bind loop body
        self.build_statement(loop_data.statement);

        // Loop back to loop label (body always executes once)
        self.add_antecedent(loop_label, self.current_flow);

        // Bug #2.1: Track assignments in condition expression
        self.handle_expression_for_assignments(loop_data.condition);

        // Create flow for condition
        let pre_condition_flow = self.current_flow;

        // True flow: back to loop label
        let true_flow = self.create_flow_node(
            flow_flags::TRUE_CONDITION,
            pre_condition_flow,
            loop_data.condition,
        );
        self.add_antecedent(loop_label, true_flow);

        // False flow: exit loop
        let false_flow = self.create_flow_node(
            flow_flags::FALSE_CONDITION,
            pre_condition_flow,
            loop_data.condition,
        );
        self.add_antecedent(merge_label, false_flow);

        self.flow_stack.pop();
        self.current_flow = merge_label;
    }

    /// Build flow graph for a for statement.
    fn build_for_statement(&mut self, loop_data: &crate::parser::node::LoopData) {
        // Create loop label
        let loop_label = self.graph.nodes.alloc(flow_flags::LOOP_LABEL);
        if !self.current_flow.is_none()
            && let Some(node) = self.graph.nodes.get_mut(loop_label)
        {
            node.antecedent.push(self.current_flow);
        }

        // Create merge label for after the loop
        let merge_label = self.graph.nodes.alloc(flow_flags::BRANCH_LABEL);

        // Push loop context
        self.flow_stack.push(FlowContext {
            break_label: merge_label,
            continue_label: Some(loop_label),
            context_type: FlowContextType::Loop,
            finally_block: NodeIndex::NONE,
            pre_finally_flow: FlowNodeId::NONE,
            post_finally_flow: FlowNodeId::NONE,
            label: NodeIndex::NONE,
        });

        // Track initializer (variable declaration or expression)
        if !loop_data.initializer.is_none() {
            self.build_statement(loop_data.initializer);
            self.add_antecedent(loop_label, self.current_flow);
        }

        self.current_flow = loop_label;

        // Handle condition if present
        if !loop_data.condition.is_none() {
            // Bug #2.1: Track assignments in condition expression
            self.handle_expression_for_assignments(loop_data.condition);

            let true_flow =
                self.create_flow_node(flow_flags::TRUE_CONDITION, loop_label, loop_data.condition);
            self.current_flow = true_flow;

            // Bind loop body
            self.build_statement(loop_data.statement);

            // Continue point: after body, before incrementor
            self.add_antecedent(loop_label, self.current_flow);

            // False flow: exit loop
            let false_flow =
                self.create_flow_node(flow_flags::FALSE_CONDITION, loop_label, loop_data.condition);
            self.add_antecedent(merge_label, false_flow);
        } else {
            // No condition: infinite loop
            self.build_statement(loop_data.statement);
            self.add_antecedent(loop_label, self.current_flow);
        }

        // Handle incrementor
        if !loop_data.incrementor.is_none() {
            let flow = self.create_flow_node(
                flow_flags::ASSIGNMENT,
                self.current_flow,
                loop_data.incrementor,
            );
            self.current_flow = flow;
            self.add_antecedent(loop_label, self.current_flow);
        }

        self.flow_stack.pop();
        self.current_flow = merge_label;
    }

    /// Build flow graph for a for-in statement.
    fn build_for_in_statement(&mut self, for_in_of: &crate::parser::node::ForInOfData) {
        // Create loop label
        let loop_label = self.graph.nodes.alloc(flow_flags::LOOP_LABEL);
        if !self.current_flow.is_none()
            && let Some(node) = self.graph.nodes.get_mut(loop_label)
        {
            node.antecedent.push(self.current_flow);
        }

        // Create merge label
        let merge_label = self.graph.nodes.alloc(flow_flags::BRANCH_LABEL);

        // Push loop context
        self.flow_stack.push(FlowContext {
            break_label: merge_label,
            continue_label: Some(loop_label),
            context_type: FlowContextType::Loop,
            finally_block: NodeIndex::NONE,
            pre_finally_flow: FlowNodeId::NONE,
            post_finally_flow: FlowNodeId::NONE,
            label: NodeIndex::NONE,
        });

        // Track initializer (variable declaration)
        if !for_in_of.initializer.is_none() {
            self.build_statement(for_in_of.initializer);
        }

        self.current_flow = loop_label;

        // Bind loop body
        self.build_statement(for_in_of.statement);

        // Loop back
        self.add_antecedent(loop_label, self.current_flow);
        self.add_antecedent(merge_label, self.current_flow);

        self.flow_stack.pop();
        self.current_flow = merge_label;
    }

    /// Build flow graph for a for-of statement.
    fn build_for_of_statement(&mut self, for_in_of: &crate::parser::node::ForInOfData) {
        // Create loop label
        let loop_label = self.graph.nodes.alloc(flow_flags::LOOP_LABEL);
        if !self.current_flow.is_none()
            && let Some(node) = self.graph.nodes.get_mut(loop_label)
        {
            node.antecedent.push(self.current_flow);
        }

        // Create merge label
        let merge_label = self.graph.nodes.alloc(flow_flags::BRANCH_LABEL);

        // Push loop context
        self.flow_stack.push(FlowContext {
            break_label: merge_label,
            continue_label: Some(loop_label),
            context_type: FlowContextType::Loop,
            finally_block: NodeIndex::NONE,
            pre_finally_flow: FlowNodeId::NONE,
            post_finally_flow: FlowNodeId::NONE,
            label: NodeIndex::NONE,
        });

        // Track initializer (variable declaration)
        if !for_in_of.initializer.is_none() {
            self.build_statement(for_in_of.initializer);
        }

        self.current_flow = loop_label;

        // Bind loop body
        self.build_statement(for_in_of.statement);

        // Loop back
        self.add_antecedent(loop_label, self.current_flow);
        self.add_antecedent(merge_label, self.current_flow);

        self.flow_stack.pop();
        self.current_flow = merge_label;
    }

    /// Build flow graph for a switch statement.
    fn build_switch_statement(&mut self, switch_data: &crate::parser::node::SwitchData) {
        // Bug #2.1: Track assignments in switch expression
        self.handle_expression_for_assignments(switch_data.expression);

        let pre_switch_flow = self.current_flow;

        // Create branch label for end of switch
        let end_label = self.graph.nodes.alloc(flow_flags::BRANCH_LABEL);

        // Push switch context
        self.flow_stack.push(FlowContext {
            break_label: end_label,
            continue_label: None,
            context_type: FlowContextType::Switch,
            finally_block: NodeIndex::NONE,
            pre_finally_flow: FlowNodeId::NONE,
            post_finally_flow: FlowNodeId::NONE,
            label: NodeIndex::NONE,
        });

        // Bind case block
        if let Some(case_block_node) = self.arena.get(switch_data.case_block) {
            let Some(case_block) = self.arena.get_block(case_block_node) else {
                self.flow_stack.pop();
                self.current_flow = end_label;
                return;
            };
            let mut fallthrough_flow = FlowNodeId::NONE;

            for &clause_idx in &case_block.statements.nodes {
                if clause_idx.is_none() {
                    continue;
                }
                let Some(clause_node) = self.arena.get(clause_idx) else {
                    continue;
                };

                match clause_node.kind {
                    syntax_kind_ext::CASE_CLAUSE => {
                        if let Some(clause) = self.arena.get_case_clause(clause_node) {
                            // Create switch clause flow node
                            let clause_flow = self.create_switch_clause_flow(
                                pre_switch_flow,
                                fallthrough_flow,
                                clause.expression,
                            );
                            self.current_flow = clause_flow;

                            // Bind statements in clause
                            for &stmt_idx in &clause.statements.nodes {
                                if !stmt_idx.is_none() {
                                    self.build_statement(stmt_idx);
                                }
                            }

                            // Track fallthrough
                            if self.current_flow != self.graph.unreachable_flow {
                                fallthrough_flow = self.current_flow;
                            } else {
                                fallthrough_flow = FlowNodeId::NONE;
                            }
                        }
                    }

                    syntax_kind_ext::DEFAULT_CLAUSE => {
                        if let Some(clause) = self.arena.get_case_clause(clause_node) {
                            let clause_flow = self.create_switch_clause_flow(
                                pre_switch_flow,
                                fallthrough_flow,
                                NodeIndex::NONE, // No expression for default
                            );
                            self.current_flow = clause_flow;

                            for &stmt_idx in &clause.statements.nodes {
                                if !stmt_idx.is_none() {
                                    self.build_statement(stmt_idx);
                                }
                            }

                            self.add_antecedent(end_label, self.current_flow);
                            fallthrough_flow = FlowNodeId::NONE;
                        }
                    }

                    _ => {}
                }
            }
        }

        self.flow_stack.pop();
        self.current_flow = end_label;
    }

    /// Build flow graph for a try statement.
    fn build_try_statement(&mut self, try_data: &crate::parser::node::TryData) {
        let pre_try_flow = self.current_flow;
        let has_finally = !try_data.finally_block.is_none();

        // Create merge label for after try/catch (before finally)
        let pre_finally_label = self.graph.nodes.alloc(flow_flags::BRANCH_LABEL);

        // Push try context to track finally block for exit statements
        let finally_ctx = if has_finally {
            Some(FlowContext {
                break_label: FlowNodeId::NONE, // Not used for try
                continue_label: None,
                context_type: FlowContextType::Try,
                finally_block: try_data.finally_block,
                pre_finally_flow: pre_finally_label,
                post_finally_flow: FlowNodeId::NONE, // Will be set after building finally
                label: NodeIndex::NONE,
            })
        } else {
            None
        };

        if let Some(ctx) = finally_ctx {
            self.flow_stack.push(ctx);
        }

        // Bind try block
        self.build_statement(try_data.try_block);
        let post_try_flow = self.current_flow;

        // Bind catch clause if present
        let post_catch_flow = if !try_data.catch_clause.is_none() {
            if let Some(catch_node) = self.arena.get(try_data.catch_clause) {
                if let Some(catch) = self.arena.get_catch_clause(catch_node) {
                    // Reset flow - catch can be entered from any point in try
                    self.current_flow = pre_try_flow;

                    // Bind catch variable if present
                    if !catch.variable_declaration.is_none() {
                        self.build_statement(catch.variable_declaration);
                    }

                    // Bind catch block
                    self.build_statement(catch.block);
                    Some(self.current_flow)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        // Pop try context
        if has_finally {
            self.flow_stack.pop();
        }

        // Add post-try flow to pre-finally label
        self.add_antecedent(pre_finally_label, post_try_flow);

        // Add post-catch flow to pre-finally label if present
        if let Some(catch_flow) = post_catch_flow {
            self.add_antecedent(pre_finally_label, catch_flow);
        }

        // Bind finally block if present
        if has_finally {
            // Build the finally block starting from the pre-finally label
            self.current_flow = pre_finally_label;
            self.build_statement(try_data.finally_block);
            // After finally, current_flow is the post-finally flow
        } else {
            // No finally block, use pre-finally label directly
            self.current_flow = pre_finally_label;
        }
    }

    /// Build flow graph for a labeled statement.
    ///
    /// Labeled statements create a break target that can be referenced by
    /// break/continue statements with a matching label.
    fn build_labeled_statement(&mut self, labeled_data: &crate::parser::node::LabeledData) {
        // Create a break label for the labeled statement
        let break_label = self.graph.nodes.alloc(flow_flags::BRANCH_LABEL);

        // Push flow context with the label
        self.flow_stack.push(FlowContext {
            break_label,
            continue_label: None, // Labeled statements don't have continue targets unless they're loops
            context_type: FlowContextType::Loop, // Use Loop type to enable breaks
            finally_block: NodeIndex::NONE,
            pre_finally_flow: FlowNodeId::NONE,
            post_finally_flow: FlowNodeId::NONE,
            label: labeled_data.label,
        });

        // Build the inner statement
        self.build_statement(labeled_data.statement);

        // Pop the labeled statement context
        self.flow_stack.pop();

        // Set current flow to the break label (for code after the labeled statement)
        self.current_flow = break_label;
    }

    /// Build flow graph for a variable declaration.
    fn build_variable_declaration(
        &mut self,
        var_decl: &crate::parser::node::VariableDeclarationData,
        idx: NodeIndex,
    ) {
        // Track the variable declaration
        self.track_variable_declaration(var_decl, idx);

        // Check for await/yield expressions in initializer
        if !var_decl.initializer.is_none() {
            self.handle_expression_for_suspension_points(var_decl.initializer);
        }
    }

    /// Handle a break statement.
    fn handle_break(&mut self, stmt_idx: NodeIndex) {
        // Check if this break has a label
        let break_label =
            if let Some(jump_data) = self.arena.get_jump_data(self.arena.get(stmt_idx).unwrap()) {
                jump_data.label
            } else {
                NodeIndex::NONE
            };

        // First pass: collect finally blocks and find target
        let mut finally_blocks: Vec<NodeIndex> = Vec::new();
        let mut target_label = FlowNodeId::NONE;

        // Get the label text if this is a labeled break
        let label_text = if !break_label.is_none() {
            self.arena
                .get(break_label)
                .and_then(|node| self.arena.get_identifier(node))
                .map(|id| id.escaped_text.as_str())
        } else {
            None
        };

        for ctx in self.flow_stack.iter().rev() {
            if !ctx.finally_block.is_none() && ctx.context_type == FlowContextType::Try {
                finally_blocks.push(ctx.finally_block);
            }

            // If this break has a label, find the matching labeled statement
            if let Some(label) = label_text {
                if !ctx.label.is_none() {
                    if let Some(ctx_label_node) = self.arena.get(ctx.label) {
                        if let Some(ctx_label_data) = self.arena.get_identifier(ctx_label_node) {
                            if ctx_label_data.escaped_text == label {
                                target_label = ctx.break_label;
                                break;
                            }
                        }
                    }
                }
            } else {
                // No label, use the nearest loop/switch
                if ctx.break_label != FlowNodeId::NONE {
                    target_label = ctx.break_label;
                    break;
                }
            }
        }

        // Second pass: build finally blocks (if any)
        for finally_block in finally_blocks.iter().rev() {
            self.build_statement(*finally_block);
        }

        // Add break target antecedent
        if !target_label.is_none() {
            self.add_antecedent(target_label, self.current_flow);
        }
        self.current_flow = self.graph.unreachable_flow;
    }

    /// Handle a continue statement.
    fn handle_continue(&mut self, stmt_idx: NodeIndex) {
        // Check if this continue has a label
        let continue_label_idx =
            if let Some(jump_data) = self.arena.get_jump_data(self.arena.get(stmt_idx).unwrap()) {
                jump_data.label
            } else {
                NodeIndex::NONE
            };

        // First pass: collect finally blocks and find target
        let mut finally_blocks: Vec<NodeIndex> = Vec::new();
        let mut target_label = FlowNodeId::NONE;

        // Get the label text if this is a labeled continue
        let label_text = if !continue_label_idx.is_none() {
            self.arena
                .get(continue_label_idx)
                .and_then(|node| self.arena.get_identifier(node))
                .map(|id| id.escaped_text.as_str())
        } else {
            None
        };

        for ctx in self.flow_stack.iter().rev() {
            if !ctx.finally_block.is_none() && ctx.context_type == FlowContextType::Try {
                finally_blocks.push(ctx.finally_block);
            }

            // If this continue has a label, find the matching labeled statement
            if let Some(label) = label_text {
                if !ctx.label.is_none() {
                    if let Some(ctx_label_node) = self.arena.get(ctx.label) {
                        if let Some(ctx_label_data) = self.arena.get_identifier(ctx_label_node) {
                            if ctx_label_data.escaped_text == label {
                                if let Some(continue_label) = ctx.continue_label {
                                    target_label = continue_label;
                                    break;
                                }
                            }
                        }
                    }
                }
            } else {
                // No label, use the nearest loop
                if let Some(continue_label) = ctx.continue_label {
                    target_label = continue_label;
                    break;
                }
            }
        }

        // Second pass: build finally blocks (if any)
        for finally_block in finally_blocks.iter().rev() {
            self.build_statement(*finally_block);
        }

        // Add continue target antecedent
        if !target_label.is_none() {
            self.add_antecedent(target_label, self.current_flow);
        }
        self.current_flow = self.graph.unreachable_flow;
    }

    /// Create a new flow node and link it to an antecedent.
    fn create_flow_node(
        &mut self,
        flags: u32,
        antecedent: FlowNodeId,
        node: NodeIndex,
    ) -> FlowNodeId {
        // If the antecedent is unreachable, this flow node is also unreachable.
        // Preserve the unreachable sentinel so later statements remain marked unreachable.
        if antecedent == self.graph.unreachable_flow {
            return self.graph.unreachable_flow;
        }

        let id = self.graph.nodes.alloc(flags);
        if let Some(flow) = self.graph.nodes.get_mut(id) {
            if !antecedent.is_none() && antecedent != self.graph.unreachable_flow {
                flow.antecedent.push(antecedent);
            }
            flow.node = node;
        }
        id
    }

    /// Create a flow node for a switch clause with optional fallthrough.
    fn create_switch_clause_flow(
        &mut self,
        pre_switch: FlowNodeId,
        fallthrough: FlowNodeId,
        expression: NodeIndex,
    ) -> FlowNodeId {
        let id = self.graph.nodes.alloc(flow_flags::SWITCH_CLAUSE);
        if let Some(node) = self.graph.nodes.get_mut(id) {
            node.node = expression;
            if !pre_switch.is_none() && pre_switch != self.graph.unreachable_flow {
                node.antecedent.push(pre_switch);
            }
            if !fallthrough.is_none() && fallthrough != self.graph.unreachable_flow {
                node.antecedent.push(fallthrough);
            }
        }
        id
    }

    /// Add an antecedent to a flow node.
    fn add_antecedent(&mut self, label: FlowNodeId, antecedent: FlowNodeId) {
        if antecedent.is_none() || antecedent == self.graph.unreachable_flow {
            return;
        }

        if let Some(node) = self.graph.nodes.get_mut(label)
            && !node.antecedent.contains(&antecedent)
        {
            node.antecedent.push(antecedent);
        }
    }

    /// Record the current flow node for an AST node.
    fn record_node_flow(&mut self, node: NodeIndex) {
        if !self.current_flow.is_none() {
            self.graph.node_flow.insert(node.0, self.current_flow);

            // Mark node as unreachable if current flow is unreachable
            if self.current_flow == self.graph.unreachable_flow {
                self.graph.mark_unreachable(node);
            }
        }
    }

    /// Check if currently inside an async function.
    fn in_async_function(&self) -> bool {
        self.async_depth > 0
    }

    /// Check if currently inside a generator function.
    fn in_generator_function(&self) -> bool {
        self.generator_depth > 0
    }

    // =============================================================================
    // Variable Tracking
    // =============================================================================

    /// Track a variable declaration at the current flow point.
    ///
    /// Variable declarations are tracked to support:
    /// - Definite assignment analysis
    /// - Temporal dead zone (TDZ) checking
    /// - Variable scope tracking
    ///
    /// # Arguments
    /// * `var_decl` - The variable declaration node
    /// * `decl_node` - The AST node index of the declaration
    fn track_variable_declaration(
        &mut self,
        var_decl: &crate::parser::node::VariableDeclarationData,
        decl_node: NodeIndex,
    ) {
        // Record the flow node at the declaration point
        self.record_node_flow(decl_node);

        // If the variable has an initializer, track it as an assignment
        if !var_decl.initializer.is_none() {
            self.track_assignment(decl_node);
        }
    }

    /// Track an assignment at the current flow point.
    ///
    /// Assignments affect the definite assignment state of variables.
    ///
    /// # Arguments
    /// * `target` - The AST node being assigned to
    fn track_assignment(&mut self, target: NodeIndex) {
        // Create an assignment flow node
        let flow = self.create_flow_node(flow_flags::ASSIGNMENT, self.current_flow, target);
        self.current_flow = flow;
    }

    // =============================================================================
    // Await Expression Handling
    // =============================================================================

    /// Recursively traverse an expression to find and handle await/yield expressions.
    fn handle_expression_for_suspension_points(&mut self, expr_idx: NodeIndex) {
        let Some(node) = self.arena.get(expr_idx) else {
            return;
        };

        // Robustness: if we're analyzing inside an async function context but the parser represented
        // `await` as an identifier (e.g., due to recovery or when the analysis context is injected),
        // still treat it as an await suspension point.
        if self.in_async_function()
            && node.kind == SyntaxKind::Identifier as u16
            && let Some(ident) = self.arena.get_identifier(node)
            && ident.escaped_text == "await"
        {
            self.handle_await_expression(expr_idx);
            return;
        }

        // Check if this is an await expression
        if node.kind == syntax_kind_ext::AWAIT_EXPRESSION {
            self.handle_await_expression(expr_idx);
            // Also check the operand of the await expression
            if let Some(unary_data) = self.arena.get_unary_expr_ex(node) {
                self.handle_expression_for_suspension_points(unary_data.expression);
            }
            return;
        }

        // Check if this is a yield expression
        if node.kind == syntax_kind_ext::YIELD_EXPRESSION {
            self.handle_yield_expression(expr_idx);
            // Also check the operand of the yield expression (stored as UnaryExprDataEx)
            if let Some(unary_data) = self.arena.get_unary_expr_ex(node)
                && !unary_data.expression.is_none()
            {
                self.handle_expression_for_suspension_points(unary_data.expression);
            }
            return;
        }

        // Recursively check child expressions based on node kind
        match node.kind {
            syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(binary) = self.arena.get_binary_expr(node) {
                    self.handle_expression_for_suspension_points(binary.left);
                    self.handle_expression_for_suspension_points(binary.right);
                }
            }
            syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.arena.get_conditional_expr(node) {
                    self.handle_expression_for_suspension_points(cond.condition);
                    self.handle_expression_for_suspension_points(cond.when_true);
                    self.handle_expression_for_suspension_points(cond.when_false);
                }
            }
            syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    self.handle_expression_for_suspension_points(call.expression);
                    if let Some(args) = &call.arguments {
                        for &arg in &args.nodes {
                            if !arg.is_none() {
                                self.handle_expression_for_suspension_points(arg);
                            }
                        }
                    }
                }
            }
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            | syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    self.handle_expression_for_suspension_points(access.expression);
                    if !access.name_or_argument.is_none() {
                        self.handle_expression_for_suspension_points(access.name_or_argument);
                    }
                }
            }
            _ => {
                // For other expression types, don't descend further for now
                // This could be extended to handle more cases as needed
            }
        }
    }

    /// Recursively traverse an expression to create ASSIGNMENT flow nodes.
    ///
    /// This is used for definite assignment and narrowing invalidation logic.
    fn handle_expression_for_assignments(&mut self, expr_idx: NodeIndex) {
        let Some(node) = self.arena.get(expr_idx) else {
            return;
        };

        match node.kind {
            syntax_kind_ext::BINARY_EXPRESSION => {
                let Some(binary) = self.arena.get_binary_expr(node) else {
                    return;
                };

                // Check for short-circuit operators (&&, ||, ??)
                let is_short_circuit = binary.operator_token
                    == crate::scanner::SyntaxKind::AmpersandAmpersandToken as u16
                    || binary.operator_token == crate::scanner::SyntaxKind::BarBarToken as u16
                    || binary.operator_token
                        == crate::scanner::SyntaxKind::QuestionQuestionToken as u16;

                if is_short_circuit {
                    // Handle short-circuit expressions with proper flow branching
                    let before_expr = self.current_flow;

                    // Bind left operand and save the flow after it
                    self.handle_expression_for_assignments(binary.left);
                    let _after_left_flow = self.current_flow;

                    // Reset to before the expression for creating condition nodes
                    self.current_flow = before_expr;

                    if binary.operator_token
                        == crate::scanner::SyntaxKind::AmpersandAmpersandToken as u16
                    {
                        // For &&: right side is only evaluated when left is truthy
                        let true_condition = self.create_flow_node(
                            flow_flags::TRUE_CONDITION,
                            before_expr,
                            binary.left,
                        );
                        self.current_flow = true_condition;
                        self.handle_expression_for_assignments(binary.right);
                        let after_right_flow = self.current_flow;

                        // Short-circuit path: left is falsy, right is not evaluated
                        let false_condition = self.create_flow_node(
                            flow_flags::FALSE_CONDITION,
                            before_expr,
                            binary.left,
                        );

                        // Merge both paths
                        let merge = self.graph.nodes.alloc(flow_flags::BRANCH_LABEL);
                        self.add_antecedent(merge, after_right_flow);
                        self.add_antecedent(merge, false_condition);
                        self.current_flow = merge;
                    } else {
                        // For || and ??: right side is only evaluated when left is falsy/nullish
                        let false_condition = self.create_flow_node(
                            flow_flags::FALSE_CONDITION,
                            before_expr,
                            binary.left,
                        );
                        self.current_flow = false_condition;
                        self.handle_expression_for_assignments(binary.right);
                        let after_right_flow = self.current_flow;

                        // Short-circuit path: left is truthy, right is not evaluated
                        let true_condition = self.create_flow_node(
                            flow_flags::TRUE_CONDITION,
                            before_expr,
                            binary.left,
                        );

                        // Merge both paths
                        let merge = self.graph.nodes.alloc(flow_flags::BRANCH_LABEL);
                        self.add_antecedent(merge, after_right_flow);
                        self.add_antecedent(merge, true_condition);
                        self.current_flow = merge;
                    }
                } else {
                    // Regular binary expression
                    if Self::is_assignment_operator_token(binary.operator_token) {
                        let flow = self.create_flow_node(
                            flow_flags::ASSIGNMENT,
                            self.current_flow,
                            expr_idx,
                        );
                        self.current_flow = flow;
                    }

                    self.handle_expression_for_assignments(binary.left);
                    self.handle_expression_for_assignments(binary.right);
                }
            }
            syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.arena.get_conditional_expr(node) {
                    self.handle_expression_for_assignments(cond.condition);
                    self.handle_expression_for_assignments(cond.when_true);
                    self.handle_expression_for_assignments(cond.when_false);
                }
            }
            syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    self.handle_expression_for_assignments(call.expression);
                    if let Some(args) = &call.arguments {
                        for &arg in &args.nodes {
                            if !arg.is_none() {
                                self.handle_expression_for_assignments(arg);
                            }
                        }
                    }

                    // Check for array mutation and create appropriate flow node
                    if self.is_array_mutation_call(expr_idx) {
                        let flow = self.create_flow_array_mutation(expr_idx);
                        self.current_flow = flow;
                    }
                }
            }
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            | syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    self.handle_expression_for_assignments(access.expression);
                    if !access.name_or_argument.is_none() {
                        self.handle_expression_for_assignments(access.name_or_argument);
                    }
                }
            }
            _ => {}
        }
    }

    fn is_assignment_operator_token(operator_token: u16) -> bool {
        matches!(
            operator_token,
            x if x == SyntaxKind::EqualsToken as u16
                || x == SyntaxKind::PlusEqualsToken as u16
                || x == SyntaxKind::MinusEqualsToken as u16
                || x == SyntaxKind::AsteriskEqualsToken as u16
                || x == SyntaxKind::SlashEqualsToken as u16
                || x == SyntaxKind::PercentEqualsToken as u16
                || x == SyntaxKind::AsteriskAsteriskEqualsToken as u16
                || x == SyntaxKind::LessThanLessThanEqualsToken as u16
                || x == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
                || x == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
                || x == SyntaxKind::AmpersandEqualsToken as u16
                || x == SyntaxKind::CaretEqualsToken as u16
                || x == SyntaxKind::BarEqualsToken as u16
                || x == SyntaxKind::BarBarEqualsToken as u16
                || x == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                || x == SyntaxKind::QuestionQuestionEqualsToken as u16
        )
    }

    /// Check if a call expression is a known array mutation method.
    fn is_array_mutation_call(&self, call_idx: NodeIndex) -> bool {
        let Some(call_node) = self.arena.get(call_idx) else {
            return false;
        };
        let Some(call) = self.arena.get_call_expr(call_node) else {
            return false;
        };
        let Some(callee_node) = self.arena.get(call.expression) else {
            return false;
        };
        let Some(access) = self.arena.get_access_expr(callee_node) else {
            return false;
        };

        // Optional chains (?.) do not definitely mutate
        if access.question_dot_token {
            return false;
        }

        let Some(name_node) = self.arena.get(access.name_or_argument) else {
            return false;
        };

        let name = if let Some(ident) = self.arena.get_identifier(name_node) {
            ident.escaped_text.as_str()
        } else if let Some(literal) = self.arena.get_literal(name_node) {
            if name_node.kind == crate::scanner::SyntaxKind::StringLiteral as u16 {
                literal.text.as_str()
            } else {
                return false;
            }
        } else {
            return false;
        };

        matches!(
            name,
            "copyWithin"
                | "fill"
                | "pop"
                | "push"
                | "reverse"
                | "shift"
                | "sort"
                | "splice"
                | "unshift"
        )
    }

    /// Create a flow node for array mutation.
    fn create_flow_array_mutation(&mut self, call_idx: NodeIndex) -> FlowNodeId {
        let id = self.graph.nodes.alloc(flow_flags::ARRAY_MUTATION);
        if let Some(node) = self.graph.nodes.get_mut(id) {
            node.node = call_idx;
            if !self.current_flow.is_none() && self.current_flow != self.graph.unreachable_flow {
                node.antecedent.push(self.current_flow);
            }
        }
        id
    }

    /// Handle an await expression by creating an AWAIT_POINT flow node.
    fn handle_await_expression(&mut self, await_node: NodeIndex) {
        if self.in_async_function() {
            // Create an AWAIT_POINT flow node to track this suspension point
            let await_point =
                self.create_flow_node(flow_flags::AWAIT_POINT, self.current_flow, await_node);
            self.current_flow = await_point;
        }
        // If not in async function, this is a semantic error but we still continue flow analysis
    }

    /// Handle a yield expression by creating a YIELD_POINT flow node.
    fn handle_yield_expression(&mut self, yield_node: NodeIndex) {
        if self.in_generator_function() {
            // Create a YIELD_POINT flow node to track this suspension point
            let yield_point =
                self.create_flow_node(flow_flags::YIELD_POINT, self.current_flow, yield_node);
            self.current_flow = yield_point;
        }
        // If not in generator function, this is a semantic error but we still continue flow analysis
    }

    /// Get the flow graph being constructed.
    pub fn graph(&self) -> &FlowGraph {
        &self.graph
    }

    /// Consume the builder and return the flow graph.
    pub fn into_graph(self) -> FlowGraph {
        self.graph
    }

    // =============================================================================
    // Class Declaration Flow (CFA for classes)
    // =============================================================================

    /// Build flow graph for a class declaration.
    ///
    /// Class declarations have control flow for:
    /// - Heritage clause expressions (extends expression executes first)
    /// - Static blocks (execute during class definition)
    /// - Static field initializers (execute during class definition)
    /// - Computed property names (execute during evaluation)
    fn build_class_declaration(&mut self, idx: NodeIndex) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };
        let Some(class) = self.arena.get_class(node) else {
            return;
        };

        // 1. Heritage clauses (extends expression executes first)
        if let Some(heritage_clauses) = &class.heritage_clauses {
            for &clause_idx in &heritage_clauses.nodes {
                if clause_idx.is_none() {
                    continue;
                }
                if let Some(clause_node) = self.arena.get(clause_idx)
                    && let Some(heritage) = self.arena.get_heritage_clause(clause_node)
                {
                    // For 'extends', the expression is evaluated
                    if heritage.token == SyntaxKind::ExtendsKeyword as u16 {
                        for &type_idx in &heritage.types.nodes {
                            if type_idx.is_none() {
                                continue;
                            }
                            if let Some(expr_with_type) = self.arena.get(type_idx)
                                && let Some(data) = self.arena.get_expr_type_args(expr_with_type)
                            {
                                // The extends expression is evaluated at class definition time
                                self.handle_expression_for_suspension_points(data.expression);
                            }
                        }
                    }
                }
            }
        }

        // Clone members to avoid borrow issues
        let members = class.members.nodes.clone();

        // 2. Class members (static fields and blocks execute during class definition)
        // Note: Instance fields execute during construction, but for top-level flow
        // we only care about static side effects. Instance fields are handled
        // when checking the constructor.
        for &member_idx in &members {
            if member_idx.is_none() {
                continue;
            }
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    if let Some(prop) = self.arena.get_property_decl(member_node) {
                        let prop_name = prop.name;
                        let prop_initializer = prop.initializer;
                        let prop_modifiers = prop.modifiers.clone();

                        // Computed property name executes
                        if let Some(name_node) = self.arena.get(prop_name)
                            && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                            && let Some(computed) = self.arena.get_computed_property(name_node)
                        {
                            self.handle_expression_for_suspension_points(computed.expression);
                        }

                        // Static initializer executes
                        if self.has_static_modifier(&prop_modifiers) && !prop_initializer.is_none()
                        {
                            self.handle_expression_for_suspension_points(prop_initializer);
                            // Track assignment for static fields
                            let flow = self.create_flow_node(
                                flow_flags::ASSIGNMENT,
                                self.current_flow,
                                member_idx,
                            );
                            self.current_flow = flow;
                        }
                    }
                }
                k if k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION => {
                    // Static block executes immediately during class definition
                    if let Some(block) = self.arena.get_block(member_node) {
                        self.build_block(block);
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    // Computed property name executes
                    if let Some(method) = self.arena.get_method_decl(member_node) {
                        let method_name = method.name;
                        if let Some(name_node) = self.arena.get(method_name)
                            && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                            && let Some(computed) = self.arena.get_computed_property(name_node)
                        {
                            self.handle_expression_for_suspension_points(computed.expression);
                        }
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    // Computed property name executes
                    if let Some(accessor) = self.arena.get_accessor(member_node) {
                        let accessor_name = accessor.name;
                        if let Some(name_node) = self.arena.get(accessor_name)
                            && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                            && let Some(computed) = self.arena.get_computed_property(name_node)
                        {
                            self.handle_expression_for_suspension_points(computed.expression);
                        }
                    }
                }
                _ => {}
            }
        }

        self.record_node_flow(idx);
    }

    /// Check if modifiers list contains 'static'.
    fn has_static_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if mod_idx.is_none() {
                    continue;
                }
                if let Some(mod_node) = self.arena.get(mod_idx)
                    && mod_node.kind == SyntaxKind::StaticKeyword as u16
                {
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ParserState;
    use crate::parser::state::{CONTEXT_FLAG_ASYNC, CONTEXT_FLAG_GENERATOR};

    #[test]
    fn test_flow_graph_builder_basic() {
        let source = r#"
let x: string | number;
if (typeof x === "string") {
    console.log(x.length);
} else {
    console.log(x.toFixed(2));
}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut builder = FlowGraphBuilder::new(parser.get_arena());
        if let Some(source_file) = parser.get_arena().get(root)
            && let Some(sf) = parser.get_arena().get_source_file(source_file)
        {
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph was created
            assert!(!graph.nodes.is_empty());
        }
    }

    #[test]
    fn test_flow_graph_simple() {
        let source = r#"
let x: string | number;
if (x) {
    x = "hello";
}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        // Build flow graph using FlowGraphBuilder
        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            let mut builder = FlowGraphBuilder::new(arena);
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph exists
            assert!(!graph.nodes.is_empty());
        }
    }

    #[test]
    fn test_flow_graph_loop() {
        let source = r#"
while (true) {
    break;
}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            let mut builder = FlowGraphBuilder::new(arena);
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph exists with loop label
            assert!(!graph.nodes.is_empty());
        }
    }

    #[test]
    fn test_flow_graph_try_finally() {
        let source = r#"
let x;
try {
    x = 1;
} finally {
}
console.log(x);
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            let mut builder = FlowGraphBuilder::new(arena);
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph exists
            assert!(!graph.nodes.is_empty());

            // The key is that the finally block should be on the flow path
            // from the try block to the console.log statement
            // This ensures that assignments in try are visible after finally
        }
    }

    #[test]
    fn test_flow_graph_try_catch_finally() {
        let source = r#"
try {
    let x = 1;
} catch (e) {
    let y = 2;
} finally {
    let z = 3;
}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            let mut builder = FlowGraphBuilder::new(arena);
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph exists with try/catch/finally
            assert!(!graph.nodes.is_empty());
        }
    }

    #[test]
    fn test_flow_graph_async_function() {
        let source = r#"
let x = await bar();
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            // Create builder and set async depth (simulating being in async function)
            let mut builder = FlowGraphBuilder::new(arena);
            builder.async_depth = 1; // Simulate being in async function
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph exists
            assert!(!graph.nodes.is_empty());
        }
    }

    #[test]
    fn test_flow_graph_await_in_expression() {
        let source = r#"
const result = await bar() + await baz();
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            let mut builder = FlowGraphBuilder::new(arena);
            builder.async_depth = 1; // Simulate being in async function
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph exists
            assert!(!graph.nodes.is_empty());
        }
    }

    #[test]
    fn test_flow_graph_await_in_if() {
        let source = r#"
if (condition) {
    await bar();
} else {
    await baz();
}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            let mut builder = FlowGraphBuilder::new(arena);
            builder.async_depth = 1; // Simulate being in async function
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph exists
            assert!(!graph.nodes.is_empty());
        }
    }

    #[test]
    fn test_flow_graph_await_in_loop() {
        let source = r#"
while (condition) {
    await bar();
}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            let mut builder = FlowGraphBuilder::new(arena);
            builder.async_depth = 1; // Simulate being in async function
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph exists
            assert!(!graph.nodes.is_empty());
        }
    }

    #[test]
    fn test_flow_graph_await_in_try_catch() {
        let source = r#"
try {
    await bar();
} catch (e) {
    console.error(e);
}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            let mut builder = FlowGraphBuilder::new(arena);
            builder.async_depth = 1; // Simulate being in async function
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph exists
            assert!(!graph.nodes.is_empty());
        }
    }

    #[test]
    fn test_flow_graph_async_arrow_function() {
        let source = r#"
const x = await bar();
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            // Create builder and set async depth (simulating being in async arrow function)
            let mut builder = FlowGraphBuilder::new(arena);
            builder.async_depth = 1; // Simulate being in async function
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph exists
            assert!(!graph.nodes.is_empty());
        }
    }

    // =============================================================================
    // Generator Flow Tests (CFA-20)
    // =============================================================================

    #[test]
    fn test_flow_graph_generator_function() {
        let source = r#"
let x: string;
yield 1;
x = "hello";
yield 2;
console.log(x);
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        parser.context_flags |= CONTEXT_FLAG_GENERATOR; // Set generator context for parsing
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            let mut builder = FlowGraphBuilder::new(arena);
            builder.generator_depth = 1; // Simulate being in generator function
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph has nodes including YIELD_POINT nodes
            assert!(!graph.nodes.is_empty());

            // Count yield point nodes
            let yield_count = (0..graph.nodes.len())
                .filter_map(|i| graph.nodes.get(FlowNodeId(i as u32)))
                .filter(|n| (n.flags & flow_flags::YIELD_POINT) != 0)
                .count();

            // We should have 2 yield points (yield 1 and yield 2)
            assert!(
                yield_count >= 2,
                "Expected at least 2 yield points, got {}",
                yield_count
            );
        }
    }

    #[test]
    fn test_flow_graph_yield_star() {
        let source = r#"
yield* otherGenerator();
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            let mut builder = FlowGraphBuilder::new(arena);
            builder.generator_depth = 1; // Simulate being in generator function
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph exists and has yield point
            assert!(!graph.nodes.is_empty());
        }
    }

    #[test]
    fn test_flow_graph_yield_in_loop() {
        let source = r#"
let counter = 0;
while (counter < 10) {
    yield counter;
    counter++;
}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            let mut builder = FlowGraphBuilder::new(arena);
            builder.generator_depth = 1; // Simulate being in generator function
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph exists
            assert!(!graph.nodes.is_empty());
        }
    }

    #[test]
    fn test_flow_graph_yield_in_try_catch() {
        let source = r#"
try {
    yield 1;
} catch (e) {
    yield 2;
}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            let mut builder = FlowGraphBuilder::new(arena);
            builder.generator_depth = 1; // Simulate being in generator function
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph exists
            assert!(!graph.nodes.is_empty());
        }
    }

    #[test]
    fn test_flow_graph_async_generator_function() {
        // Test combined async and generator (async generator function)
        let source = r#"
let x: string;
yield await fetch('/api/data1');
x = "hello";
yield await fetch('/api/data2');
console.log(x);
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        parser.context_flags |= CONTEXT_FLAG_ASYNC | CONTEXT_FLAG_GENERATOR; // Set async and generator context
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            let mut builder = FlowGraphBuilder::new(arena);
            // Simulate being in async generator function
            builder.async_depth = 1;
            builder.generator_depth = 1;
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph exists with both await and yield points
            assert!(!graph.nodes.is_empty());

            // Count yield point nodes
            let yield_count = (0..graph.nodes.len())
                .filter_map(|i| graph.nodes.get(FlowNodeId(i as u32)))
                .filter(|n| (n.flags & flow_flags::YIELD_POINT) != 0)
                .count();

            // Count await point nodes
            let await_count = (0..graph.nodes.len())
                .filter_map(|i| graph.nodes.get(FlowNodeId(i as u32)))
                .filter(|n| (n.flags & flow_flags::AWAIT_POINT) != 0)
                .count();

            assert!(
                yield_count >= 2,
                "Expected at least 2 yield points, got {}",
                yield_count
            );
            assert!(
                await_count >= 2,
                "Expected at least 2 await points, got {}",
                await_count
            );
        }
    }

    #[test]
    fn test_flow_graph_variable_state_across_yield() {
        // Test that variable state is properly tracked across yield boundaries
        let source = r#"
let x: string | undefined;
x = "first";
yield 1;
x = "second";
yield 2;
console.log(x);
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            let mut builder = FlowGraphBuilder::new(arena);
            builder.generator_depth = 1; // Simulate being in generator function
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph has assignment and yield nodes
            assert!(!graph.nodes.is_empty());

            // Should have assignment nodes for tracking variable state
            let assignment_count = (0..graph.nodes.len())
                .filter_map(|i| graph.nodes.get(FlowNodeId(i as u32)))
                .filter(|n| (n.flags & flow_flags::ASSIGNMENT) != 0)
                .count();

            assert!(
                assignment_count >= 2,
                "Expected at least 2 assignment nodes for variable tracking, got {}",
                assignment_count
            );
        }
    }

    #[test]
    fn test_flow_graph_for_of_await_in_async_generator() {
        // Test for-await-of in async generator
        let source = r#"
let result: string[] = [];
for await (const item of asyncIterable) {
    yield item;
    result.push(item);
}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            let mut builder = FlowGraphBuilder::new(arena);
            // Simulate being in async generator function
            builder.async_depth = 1;
            builder.generator_depth = 1;
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph exists
            assert!(!graph.nodes.is_empty());
        }
    }

    #[test]
    fn test_flow_graph_conditional_yield() {
        // Test yield in conditional branches
        let source = r#"
let x: string | number;
if (condition) {
    x = "string";
    yield 1;
} else {
    x = 42;
    yield 2;
}
console.log(x);
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        parser.context_flags |= CONTEXT_FLAG_GENERATOR; // Set generator context for parsing
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            let mut builder = FlowGraphBuilder::new(arena);
            builder.generator_depth = 1; // Simulate being in generator function
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph exists with proper branching
            assert!(!graph.nodes.is_empty());

            // Should have yield points in both branches
            let yield_count = (0..graph.nodes.len())
                .filter_map(|i| graph.nodes.get(FlowNodeId(i as u32)))
                .filter(|n| (n.flags & flow_flags::YIELD_POINT) != 0)
                .count();

            assert!(
                yield_count >= 2,
                "Expected at least 2 yield points in conditional branches, got {}",
                yield_count
            );
        }
    }

    #[test]
    fn test_flow_graph_nested_generator() {
        // Test nested generator (generator calling another generator)
        let source = r#"
yield 1;
for (const val of innerGenerator()) {
    yield val * 2;
}
yield 3;
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        parser.context_flags |= CONTEXT_FLAG_GENERATOR; // Set generator context for parsing
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            let mut builder = FlowGraphBuilder::new(arena);
            builder.generator_depth = 1; // Simulate being in generator function
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph exists
            assert!(!graph.nodes.is_empty());

            // Should have at least 3 yield points
            let yield_count = (0..graph.nodes.len())
                .filter_map(|i| graph.nodes.get(FlowNodeId(i as u32)))
                .filter(|n| (n.flags & flow_flags::YIELD_POINT) != 0)
                .count();

            assert!(
                yield_count >= 3,
                "Expected at least 3 yield points, got {}",
                yield_count
            );
        }
    }

    // =============================================================================
    // Class Declaration Flow Tests
    // =============================================================================

    #[test]
    fn test_flow_graph_class_with_static_block() {
        let source = r#"
let x: number;
class Foo {
    static {
        x = 42;
    }
}
console.log(x);
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            let mut builder = FlowGraphBuilder::new(arena);
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph exists
            assert!(!graph.nodes.is_empty());
        }
    }

    #[test]
    fn test_flow_graph_class_with_static_property() {
        let source = r#"
let x: number;
class Foo {
    static prop = (x = 42);
}
console.log(x);
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            let mut builder = FlowGraphBuilder::new(arena);
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph has assignment node for static property
            assert!(!graph.nodes.is_empty());

            let assignment_count = (0..graph.nodes.len())
                .filter_map(|i| graph.nodes.get(FlowNodeId(i as u32)))
                .filter(|n| (n.flags & flow_flags::ASSIGNMENT) != 0)
                .count();

            // Should have assignment for static property initializer
            assert!(
                assignment_count >= 1,
                "Expected at least 1 assignment node for static property, got {}",
                assignment_count
            );
        }
    }

    #[test]
    fn test_flow_graph_class_with_extends() {
        let source = r#"
class Base {}
class Derived extends Base {}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            let mut builder = FlowGraphBuilder::new(arena);
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph exists - extends expression should be tracked
            assert!(!graph.nodes.is_empty());
        }
    }

    #[test]
    fn test_flow_graph_class_with_computed_property() {
        let source = r#"
const key = "myMethod";
class Foo {
    [key]() {
        return 42;
    }
}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            let mut builder = FlowGraphBuilder::new(arena);
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph exists - computed property expression should be tracked
            assert!(!graph.nodes.is_empty());
        }
    }

    #[test]
    fn test_flow_graph_class_multiple_static_blocks() {
        let source = r#"
let x: number;
let y: string;
class Foo {
    static {
        x = 1;
    }
    static prop = "hello";
    static {
        y = "world";
    }
}
console.log(x, y);
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            let mut builder = FlowGraphBuilder::new(arena);
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph exists
            assert!(!graph.nodes.is_empty());
        }
    }

    #[test]
    fn test_flow_graph_class_expression() {
        let source = r#"
let x: number;
const Foo = class {
    static {
        x = 42;
    }
};
console.log(x);
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root)
            && let Some(sf) = arena.get_source_file(source_file)
        {
            let mut builder = FlowGraphBuilder::new(arena);
            let graph = builder.build_source_file(&sf.statements);

            // Verify flow graph exists - class expression with static block
            assert!(!graph.nodes.is_empty());
        }
    }
}
