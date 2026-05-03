//! Loop Capture IIFE Transform for ES5
//!
//! When targeting ES5 and loop variables (let/const) are captured by closures
//! inside the loop body, TypeScript transforms the loop into an IIFE pattern:
//!
//! ```typescript
//! for (let i = 0; i < 3; i++) {
//!     setTimeout(() => console.log(i), 100);
//! }
//! ```
//! Becomes:
//! ```javascript
//! var _loop_1 = function (i) {
//!     setTimeout(function () { return console.log(i); }, 100);
//! };
//! for (var i = 0; i < 3; i++) {
//!     _loop_1(i);
//! }
//! ```

use super::super::Printer;
use crate::transforms::block_scoping_es5::{LoopCaptureInfo, analyze_loop_capture};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{Node, NodeArena};
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext;

/// Information about variables in a loop body for the IIFE transform
#[derive(Debug, Default)]
pub(in crate::emitter) struct LoopBodyVarInfo {
    /// Block-scoped (let/const) variable names declared in the loop body
    pub block_scoped_vars: Vec<String>,

    /// Var-scoped variable names declared in the loop body
    /// These get hoisted before the loop as `var name1, name2;`
    pub var_decl_names: Vec<String>,

    /// Whether the loop body contains `continue` statements
    pub has_continue: bool,

    /// Whether the loop body contains `break` statements
    pub has_break: bool,

    /// Whether the loop body contains `return` statements
    pub has_return: bool,
}

/// Collect block-scoped and var-scoped variables from a loop body
pub(in crate::emitter) fn collect_loop_body_vars(
    arena: &NodeArena,
    body_idx: NodeIndex,
) -> LoopBodyVarInfo {
    let mut info = LoopBodyVarInfo::default();
    collect_vars_recursive(arena, body_idx, &mut info);
    info
}

fn collect_vars_recursive(arena: &NodeArena, idx: NodeIndex, info: &mut LoopBodyVarInfo) {
    let Some(node) = arena.get(idx) else {
        return;
    };

    match node.kind {
        // Function boundaries: don't recurse into functions for var/control flow collection
        k if k == syntax_kind_ext::FUNCTION_DECLARATION
            || k == syntax_kind_ext::FUNCTION_EXPRESSION
            || k == syntax_kind_ext::ARROW_FUNCTION => {}

        // Variable statement — contains one or more VARIABLE_DECLARATION_LIST nodes.
        // The LET/CONST flags are on the declaration LIST node, not the statement.
        k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
            if let Some(var_stmt) = arena.get_variable(node) {
                for &decl_list_idx in &var_stmt.declarations.nodes {
                    let Some(decl_list_node) = arena.get(decl_list_idx) else {
                        continue;
                    };
                    let flags = decl_list_node.flags as u32;
                    let is_block_scoped =
                        (flags & node_flags::LET != 0) || (flags & node_flags::CONST != 0);

                    if let Some(decl_list) = arena.get_variable(decl_list_node) {
                        for &decl_idx in &decl_list.declarations.nodes {
                            if let Some(decl_node) = arena.get(decl_idx)
                                && let Some(decl) = arena.get_variable_declaration(decl_node)
                            {
                                collect_binding_names(
                                    arena,
                                    decl.name,
                                    if is_block_scoped {
                                        &mut info.block_scoped_vars
                                    } else {
                                        &mut info.var_decl_names
                                    },
                                );
                            }
                        }
                    }
                }
            }
        }

        // Continue statement
        k if k == syntax_kind_ext::CONTINUE_STATEMENT => {
            info.has_continue = true;
        }

        // Break statement
        k if k == syntax_kind_ext::BREAK_STATEMENT => {
            info.has_break = true;
        }

        // Return statement
        k if k == syntax_kind_ext::RETURN_STATEMENT => {
            info.has_return = true;
        }

        // Block
        k if k == syntax_kind_ext::BLOCK => {
            if let Some(block) = arena.get_block(node) {
                for &stmt_idx in &block.statements.nodes {
                    collect_vars_recursive(arena, stmt_idx, info);
                }
            }
        }

        // If statement
        k if k == syntax_kind_ext::IF_STATEMENT => {
            if let Some(if_stmt) = arena.get_if_statement(node) {
                collect_vars_recursive(arena, if_stmt.then_statement, info);
                collect_vars_recursive(arena, if_stmt.else_statement, info);
            }
        }

        // Nested loops
        k if k == syntax_kind_ext::FOR_STATEMENT
            || k == syntax_kind_ext::WHILE_STATEMENT
            || k == syntax_kind_ext::DO_STATEMENT =>
        {
            if let Some(loop_data) = arena.get_loop(node) {
                collect_vars_recursive(arena, loop_data.initializer, info);
                collect_vars_recursive(arena, loop_data.statement, info);
            }
        }

        k if k == syntax_kind_ext::FOR_IN_STATEMENT || k == syntax_kind_ext::FOR_OF_STATEMENT => {
            if let Some(for_in_of) = arena.get_for_in_of(node) {
                collect_vars_recursive(arena, for_in_of.initializer, info);
                collect_vars_recursive(arena, for_in_of.statement, info);
            }
        }

        // Variable declaration list (inside for initializer)
        k if k == syntax_kind_ext::VARIABLE_DECLARATION_LIST => {
            if let Some(decl_list) = arena.get_variable(node) {
                let flags = node.flags as u32;
                let is_block_scoped =
                    (flags & node_flags::LET != 0) || (flags & node_flags::CONST != 0);

                for &decl_idx in &decl_list.declarations.nodes {
                    if let Some(decl_node) = arena.get(decl_idx)
                        && let Some(decl) = arena.get_variable_declaration(decl_node)
                    {
                        collect_binding_names(
                            arena,
                            decl.name,
                            if is_block_scoped {
                                &mut info.block_scoped_vars
                            } else {
                                &mut info.var_decl_names
                            },
                        );
                    }
                }
            }
        }

        // Switch
        k if k == syntax_kind_ext::SWITCH_STATEMENT => {
            if let Some(switch_stmt) = arena.get_switch(node) {
                // case_block is a CaseBlock node — its clauses are in arena.blocks
                if let Some(case_block_node) = arena.get(switch_stmt.case_block)
                    && let Some(block_data) = arena.blocks.get(case_block_node.data_index as usize)
                {
                    for &clause_idx in &block_data.statements.nodes {
                        collect_vars_recursive(arena, clause_idx, info);
                    }
                }
            }
        }

        k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
            if let Some(clause) = arena.get_case_clause(node) {
                for &stmt_idx in &clause.statements.nodes {
                    collect_vars_recursive(arena, stmt_idx, info);
                }
            }
        }

        // Try statement
        k if k == syntax_kind_ext::TRY_STATEMENT => {
            if let Some(try_stmt) = arena.get_try(node) {
                collect_vars_recursive(arena, try_stmt.try_block, info);
                collect_vars_recursive(arena, try_stmt.catch_clause, info);
                collect_vars_recursive(arena, try_stmt.finally_block, info);
            }
        }

        // Catch clause
        k if k == syntax_kind_ext::CATCH_CLAUSE => {
            if let Some(catch_clause) = arena.get_catch_clause(node) {
                collect_vars_recursive(arena, catch_clause.block, info);
            }
        }

        // Class declaration (don't recurse into methods)
        k if k == syntax_kind_ext::CLASS_DECLARATION || k == syntax_kind_ext::CLASS_EXPRESSION => {}

        _ => {}
    }
}

/// Collect identifier names from a binding pattern or identifier
fn collect_binding_names(arena: &NodeArena, name_idx: NodeIndex, names: &mut Vec<String>) {
    let Some(name_node) = arena.get(name_idx) else {
        return;
    };

    if name_node.is_identifier() {
        if let Some(ident) = arena.get_identifier(name_node) {
            let text = arena.resolve_identifier_text(ident).to_string();
            if !names.contains(&text) {
                names.push(text);
            }
        }
    } else if (name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
        || name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN)
        && let Some(pattern) = arena.get_binding_pattern(name_node)
    {
        for &elem_idx in &pattern.elements.nodes {
            if let Some(elem_node) = arena.get(elem_idx)
                && let Some(elem) = arena.get_binding_element(elem_node)
            {
                collect_binding_names(arena, elem.name, names);
            }
        }
    }
}

/// Check if a loop needs the IIFE capture pattern.
pub(in crate::emitter) fn check_loop_needs_capture(
    arena: &NodeArena,
    body_idx: NodeIndex,
    initializer_vars: &[String],
    body_block_vars: &[String],
) -> Option<LoopCaptureInfo> {
    let all_vars: Vec<String> = initializer_vars
        .iter()
        .chain(body_block_vars.iter())
        .cloned()
        .collect();

    if all_vars.is_empty() {
        return None;
    }

    let info = analyze_loop_capture(arena, body_idx, &all_vars);
    if info.needs_capture { Some(info) } else { None }
}

impl<'a> Printer<'a> {
    /// Check if a for-statement initializer declares let/const variables
    pub(in crate::emitter) fn collect_for_initializer_let_const_vars(
        &self,
        initializer: NodeIndex,
    ) -> Vec<String> {
        let Some(node) = self.arena.get(initializer) else {
            return Vec::new();
        };

        if node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return Vec::new();
        }

        let flags = node.flags as u32;
        let is_block_scoped = (flags & node_flags::LET != 0) || (flags & node_flags::CONST != 0);

        if !is_block_scoped {
            return Vec::new();
        }

        let Some(decl_list) = self.arena.get_variable(node) else {
            return Vec::new();
        };

        let mut vars = Vec::new();
        for &decl_idx in &decl_list.declarations.nodes {
            if let Some(decl_node) = self.arena.get(decl_idx)
                && let Some(decl) = self.arena.get_variable_declaration(decl_node)
            {
                collect_binding_names(self.arena, decl.name, &mut vars);
            }
        }
        vars
    }

    /// Collect iteration variable names from a for-of/for-in initializer.
    pub(in crate::emitter) fn collect_for_of_iteration_var_names(
        &self,
        initializer: NodeIndex,
    ) -> Vec<String> {
        let Some(node) = self.arena.get(initializer) else {
            return Vec::new();
        };

        if node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return Vec::new();
        }

        let flags = node.flags as u32;
        let is_block_scoped = (flags & node_flags::LET != 0) || (flags & node_flags::CONST != 0);

        if !is_block_scoped {
            return Vec::new();
        }

        let Some(decl_list) = self.arena.get_variable(node) else {
            return Vec::new();
        };

        let mut vars = Vec::new();
        for &decl_idx in &decl_list.declarations.nodes {
            if let Some(decl_node) = self.arena.get(decl_idx)
                && let Some(decl) = self.arena.get_variable_declaration(decl_node)
            {
                collect_binding_names(self.arena, decl.name, &mut vars);
            }
        }
        vars
    }

    /// Emit a for-statement with the _`loop_N` IIFE capture pattern
    pub(in crate::emitter) fn emit_for_statement_with_capture(
        &mut self,
        _node: &Node,
        loop_stmt: &tsz_parser::parser::node::LoopData,
        capture_info: &LoopCaptureInfo,
        init_vars: &[String],
        body_info: &LoopBodyVarInfo,
    ) {
        let loop_fn_name = self.ctx.block_scope_state.next_loop_function_name();

        // Only initializer let/const vars are passed as IIFE parameters.
        // Body-scoped let/const vars get fresh scope inside the IIFE automatically.
        let init_var_set: std::collections::HashSet<&str> =
            init_vars.iter().map(String::as_str).collect();
        let param_vars: Vec<String> = capture_info
            .captured_vars
            .iter()
            .filter(|v| init_var_set.contains(v.as_str()))
            .cloned()
            .collect();

        // Emit: var _loop_1 = function (param1, param2) { ... };
        self.emit_loop_function(
            &loop_fn_name,
            &param_vars,
            loop_stmt.statement,
            body_info,
            init_vars,
        );
        self.write_line();

        // Emit hoisted var declarations for var-scoped variables in the body
        if !body_info.var_decl_names.is_empty() {
            self.write("var ");
            for (i, name) in body_info.var_decl_names.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.write(name);
            }
            self.write(";");
            self.write_line();
        }

        // Emit the for loop with _loop_1(args) as body
        self.write("for (");
        self.emit_for_initializer_as_var(loop_stmt.initializer);
        self.write(";");
        if loop_stmt.condition.is_some() {
            self.write(" ");
            self.emit(loop_stmt.condition);
        }
        self.write(";");
        if loop_stmt.incrementor.is_some() {
            self.write(" ");
            self.emit(loop_stmt.incrementor);
        }
        self.write(") {");
        self.write_line();
        self.increase_indent();

        self.emit_loop_call(&loop_fn_name, &param_vars, body_info);

        self.decrease_indent();
        self.write("}");
    }

    /// Emit a do-while statement with the _`loop_N` IIFE capture pattern
    pub(in crate::emitter) fn emit_do_statement_with_capture(
        &mut self,
        _node: &Node,
        loop_stmt: &tsz_parser::parser::node::LoopData,
        _capture_info: &LoopCaptureInfo,
        body_info: &LoopBodyVarInfo,
    ) {
        let loop_fn_name = self.ctx.block_scope_state.next_loop_function_name();

        // Do/while loops have no init vars — body vars get fresh scope inside IIFE
        let empty_params: Vec<String> = Vec::new();

        self.emit_loop_function(
            &loop_fn_name,
            &empty_params,
            loop_stmt.statement,
            body_info,
            &[],
        );
        self.write_line();

        // Emit hoisted var declarations
        if !body_info.var_decl_names.is_empty() {
            self.write("var ");
            for (i, name) in body_info.var_decl_names.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.write(name);
            }
            self.write(";");
            self.write_line();
        }

        // Emit: do { _loop_1(); } while (condition);
        self.write("do {");
        self.write_line();
        self.increase_indent();

        self.emit_loop_call(&loop_fn_name, &empty_params, body_info);

        self.decrease_indent();
        self.write("} while (");
        self.emit(loop_stmt.condition);
        self.write(");");
    }

    /// Emit a while statement with the _`loop_N` IIFE capture pattern
    pub(in crate::emitter) fn emit_while_statement_with_capture(
        &mut self,
        _node: &Node,
        loop_stmt: &tsz_parser::parser::node::LoopData,
        capture_info: &LoopCaptureInfo,
        body_info: &LoopBodyVarInfo,
    ) {
        let loop_fn_name = self.ctx.block_scope_state.next_loop_function_name();

        self.emit_loop_function(
            &loop_fn_name,
            &capture_info.captured_vars,
            loop_stmt.statement,
            body_info,
            &[],
        );
        self.write_line();

        // Emit hoisted var declarations
        if !body_info.var_decl_names.is_empty() {
            self.write("var ");
            for (i, name) in body_info.var_decl_names.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.write(name);
            }
            self.write(";");
            self.write_line();
        }

        // Emit: while (condition) { _loop_1(); }
        self.write("while (");
        self.emit(loop_stmt.condition);
        self.write(") {");
        self.write_line();
        self.increase_indent();

        self.emit_loop_call(&loop_fn_name, &capture_info.captured_vars, body_info);

        self.decrease_indent();
        self.write("}");
    }

    /// Emit the _`loop_N` function definition
    pub(in crate::emitter) fn emit_loop_function(
        &mut self,
        fn_name: &str,
        captured_vars: &[String],
        body_idx: NodeIndex,
        body_info: &LoopBodyVarInfo,
        _init_vars: &[String],
    ) {
        self.write("var ");
        self.write(fn_name);
        self.write(" = function (");

        // Parameters are the captured variables
        for (i, var) in captured_vars.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            // If the variable was renamed by block scoping, use the renamed version
            if let Some(emitted) = self.ctx.block_scope_state.get_emitted_name(var) {
                self.write(&emitted);
            } else {
                self.write(var);
            }
        }

        self.write(") {");
        self.write_line();
        self.increase_indent();

        // Emit the body statements inside the IIFE
        self.emit_loop_body_for_iife(body_idx, body_info, captured_vars, _init_vars);

        self.decrease_indent();
        self.write("};");
    }

    /// Emit loop body statements inside the IIFE function
    pub(in crate::emitter) fn emit_loop_body_for_iife(
        &mut self,
        body_idx: NodeIndex,
        body_info: &LoopBodyVarInfo,
        captured_vars: &[String],
        init_vars: &[String],
    ) {
        let Some(body_node) = self.arena.get(body_idx) else {
            return;
        };

        if body_node.kind == syntax_kind_ext::BLOCK {
            if let Some(block) = self.arena.get_block(body_node) {
                for &stmt_idx in &block.statements.nodes {
                    self.emit_statement_in_loop_iife(stmt_idx, body_info, captured_vars, init_vars);
                    self.write_line();
                }
            }
        } else {
            self.emit_statement_in_loop_iife(body_idx, body_info, captured_vars, init_vars);
            self.write_line();
        }
    }

    /// Emit a single statement inside the loop IIFE, handling transformations:
    /// - `var` declarations lose the `var` keyword (they're hoisted)
    /// - `continue` -> `return "continue"`
    /// - `break` -> `return "break"`
    fn emit_statement_in_loop_iife(
        &mut self,
        stmt_idx: NodeIndex,
        _body_info: &LoopBodyVarInfo,
        _captured_vars: &[String],
        _init_vars: &[String],
    ) {
        let Some(node) = self.arena.get(stmt_idx) else {
            return;
        };

        match node.kind {
            // Variable statement: check if it's var (needs hoisting transform)
            // Note: LET/CONST flags are on the VARIABLE_DECLARATION_LIST child, not the statement.
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = self.arena.get_variable(node) {
                    // Check the first declaration list child for LET/CONST flags
                    let is_var = var_stmt
                        .declarations
                        .nodes
                        .first()
                        .and_then(|&idx| self.arena.get(idx))
                        .map(|list_node| {
                            let flags = list_node.flags as u32;
                            (flags & node_flags::LET == 0) && (flags & node_flags::CONST == 0)
                        })
                        .unwrap_or(true);

                    if is_var {
                        // Var declarations: emit just the assignments without `var`
                        // var_stmt.declarations contains VARIABLE_DECLARATION_LIST nodes,
                        // each of which contains individual VARIABLE_DECLARATION nodes.
                        let mut all_decls = Vec::new();
                        for &list_idx in &var_stmt.declarations.nodes {
                            if let Some(list_node) = self.arena.get(list_idx)
                                && let Some(list_data) = self.arena.get_variable(list_node)
                            {
                                for &decl_idx in &list_data.declarations.nodes {
                                    all_decls.push(decl_idx);
                                }
                            }
                        }

                        let has_initializer = all_decls.iter().any(|&idx| {
                            self.arena
                                .get(idx)
                                .and_then(|n| self.arena.get_variable_declaration(n))
                                .is_some_and(|d| d.initializer.is_some())
                        });

                        if has_initializer {
                            let mut first = true;
                            for &decl_idx in &all_decls {
                                if let Some(decl_node) = self.arena.get(decl_idx)
                                    && let Some(decl) =
                                        self.arena.get_variable_declaration(decl_node)
                                    && decl.initializer.is_some()
                                {
                                    if !first {
                                        self.write(", ");
                                    }
                                    first = false;
                                    self.emit(decl.name);
                                    self.write(" = ");
                                    self.emit(decl.initializer);
                                }
                            }
                            self.write(";");
                        }
                        // If no initializers, skip entirely (vars are hoisted as bare declarations)
                    } else {
                        // let/const: emit normally (they become var inside the IIFE)
                        self.emit(stmt_idx);
                    }
                }
            }

            // Continue -> return "continue"
            k if k == syntax_kind_ext::CONTINUE_STATEMENT => {
                self.write("return \"continue\";");
            }

            // Break -> return "break"
            k if k == syntax_kind_ext::BREAK_STATEMENT => {
                self.write("return \"break\";");
            }

            // Return -> return { value: expr }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.arena.get_return_statement(node)
                    && ret.expression.is_some()
                {
                    self.write("return { value: ");
                    self.emit_expression(ret.expression);
                    self.write(" };");
                } else {
                    self.write("return { value: void 0 };");
                }
            }

            // Everything else: emit normally
            _ => {
                self.emit(stmt_idx);
            }
        }
    }

    /// Emit the loop call: _`loop_1(args)`;
    pub(in crate::emitter) fn emit_loop_call(
        &mut self,
        fn_name: &str,
        captured_vars: &[String],
        body_info: &LoopBodyVarInfo,
    ) {
        if body_info.has_continue || body_info.has_break || body_info.has_return {
            // Need to capture the return value
            if body_info.has_return {
                self.write("var _state = ");
                self.write(fn_name);
                self.write("(");
                self.emit_loop_call_args(captured_vars);
                self.write(");");
                self.write_line();

                if body_info.has_continue {
                    self.write("if (_state === \"continue\")");
                    self.write_line();
                    self.increase_indent();
                    self.write("continue;");
                    self.decrease_indent();
                    self.write_line();
                }
                if body_info.has_break {
                    self.write("if (_state === \"break\")");
                    self.write_line();
                    self.increase_indent();
                    self.write("break;");
                    self.decrease_indent();
                    self.write_line();
                }
                self.write("if (typeof _state === \"object\")");
                self.write_line();
                self.increase_indent();
                self.write("return _state.value;");
                self.decrease_indent();
            } else {
                self.write("var _state = ");
                self.write(fn_name);
                self.write("(");
                self.emit_loop_call_args(captured_vars);
                self.write(");");
                self.write_line();

                if body_info.has_continue {
                    self.write("if (_state === \"continue\")");
                    self.write_line();
                    self.increase_indent();
                    self.write("continue;");
                    self.decrease_indent();
                    if body_info.has_break {
                        self.write_line();
                    }
                }
                if body_info.has_break {
                    self.write("if (_state === \"break\")");
                    self.write_line();
                    self.increase_indent();
                    self.write("break;");
                    self.decrease_indent();
                }
            }
        } else {
            // Simple call: _loop_1(args);
            self.write(fn_name);
            self.write("(");
            self.emit_loop_call_args(captured_vars);
            self.write(");");
        }
        self.write_line();
    }

    /// Emit the arguments for the loop function call
    fn emit_loop_call_args(&mut self, captured_vars: &[String]) {
        for (i, var) in captured_vars.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.write(var);
        }
    }

    /// Emit a for-loop initializer, converting let/const to var
    fn emit_for_initializer_as_var(&mut self, initializer: NodeIndex) {
        let Some(node) = self.arena.get(initializer) else {
            return;
        };

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            let flags = node.flags as u32;
            let is_block_scoped =
                (flags & node_flags::LET != 0) || (flags & node_flags::CONST != 0);

            if is_block_scoped {
                self.write("var ");
                if let Some(decl_list) = self.arena.get_variable(node) {
                    let mut first = true;
                    for &decl_idx in &decl_list.declarations.nodes {
                        if !first {
                            self.write(", ");
                        }
                        first = false;
                        self.emit(decl_idx);
                    }
                }
            } else {
                self.emit(initializer);
            }
        } else {
            self.emit(initializer);
        }
    }
}
