//! Order-independent seeding of value-binding names that an ES5 block-scoped
//! declaration nested in a sub-block must not capture when lowered to `var`.
//!
//! ## Why this exists
//!
//! tsc renames a block-scoped class lowered to ES5 (`var Foo = (function(){…})()`)
//! to `Foo_1` when its name collides with another *value* binding that is
//! visible throughout the enclosing function/script scope:
//!
//! ```ts
//! function f(b: boolean) {
//!     if (b) {
//!         class A { x: number }   // -> var A_1 = …
//!         let c = [new A()];      //    new A_1()
//!     }
//! }
//! class A {}                      // module-level value binding named `A`
//! ```
//!
//! tsc reaches this decision through its binder pre-pass, so the colliding
//! outer declaration may appear *textually after* the block-scoped class.
//! tsz's emitter is a single forward walk, so it must seed the enclosing
//! scope's visible value-binding names up front; otherwise the collision is
//! only detected when the outer declaration happens to precede the class.
//!
//! ## What is "visible throughout the scope"
//!
//! For a function/script scope, the names visible at any nested block are:
//!   * every declaration directly at the scope's own body level
//!     (`class` / `function` / `enum` / `namespace` / `var` / `let` / `const`), and
//!   * every `var` (function-scoped, therefore hoisted) declared anywhere in a
//!     nested block of the same function — but *not* block-scoped declarations
//!     (`class` / `let` / `const` / `function` / `enum` / `namespace`) sitting
//!     inside a sibling/nested block, which are invisible to other blocks.
//!
//! Descent stops at nested function/class/accessor/method boundaries because
//! those open their own scopes.

use super::{NodeIndex, Printer};
use tsz_parser::parser::NodeList;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext as sk;

impl<'a> Printer<'a> {
    /// Collect the value-binding names visible throughout the
    /// function/script scope whose body is `statements`, and seed them so a
    /// nested-block ES5 class/`let`/`const` lowering renames on collision
    /// regardless of declaration order.
    ///
    /// Must be called immediately after the scope's `enter_function_scope()`
    /// and before any of its statements are emitted.
    pub(in crate::emitter) fn seed_block_scope_value_binding_names(
        &mut self,
        statements: &NodeList,
    ) {
        let mut names: Vec<String> = Vec::new();
        for &stmt_idx in &statements.nodes {
            self.collect_body_level_binding_names(stmt_idx, &mut names);
        }
        for name in &names {
            self.ctx
                .block_scope_state
                .register_function_scope_shadowed_name(name);
        }
    }

    /// Collect binding names declared *directly* at the scope body level.
    /// `var` declarations also recurse into nested blocks (they hoist), while
    /// block-scoped declarations are only collected at this level.
    fn collect_body_level_binding_names(&self, idx: NodeIndex, out: &mut Vec<String>) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        match node.kind {
            k if k == sk::CLASS_DECLARATION
                || k == sk::FUNCTION_DECLARATION
                || k == sk::ENUM_DECLARATION
                || k == sk::MODULE_DECLARATION =>
            {
                self.push_decl_name(node, out);
            }
            k if k == sk::VARIABLE_STATEMENT => {
                // Body-level `var`/`let`/`const` are all visible to nested
                // blocks, so collect every declared name regardless of kind.
                self.collect_variable_statement_names(node, out, false);
            }
            // Body-level nested blocks/control flow: only hoisted `var`
            // declarations escape into the enclosing function scope.
            _ => {
                self.collect_hoisted_var_names(idx, out);
            }
        }
    }

    /// Collect every `var` name declared anywhere beneath `idx`, stopping at
    /// nested function/class/accessor/method scope boundaries. Block-scoped
    /// declarations are intentionally ignored — they are not visible across
    /// sibling blocks.
    fn collect_hoisted_var_names(&self, idx: NodeIndex, out: &mut Vec<String>) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        // Do not descend into constructs that open their own value scope; the
        // `var`s inside them belong to that scope, not this one.
        if matches!(
            node.kind,
            k if k == sk::FUNCTION_DECLARATION
                || k == sk::FUNCTION_EXPRESSION
                || k == sk::ARROW_FUNCTION
                || k == sk::METHOD_DECLARATION
                || k == sk::CONSTRUCTOR
                || k == sk::GET_ACCESSOR
                || k == sk::SET_ACCESSOR
                || k == sk::CLASS_DECLARATION
                || k == sk::CLASS_EXPRESSION
                || k == sk::MODULE_DECLARATION
        ) {
            return;
        }

        if node.kind == sk::VARIABLE_STATEMENT {
            // Inside a nested block only hoisted `var` bindings escape to the
            // enclosing function scope; block-scoped `let`/`const`/`using` are
            // invisible across blocks and must be skipped.
            self.collect_variable_statement_names(node, out, true);
            return;
        }

        for child in self.arena.get_children(idx) {
            self.collect_hoisted_var_names(child, out);
        }
    }

    /// Collect the binding names declared by a `VARIABLE_STATEMENT`.
    ///
    /// A `VARIABLE_STATEMENT` wraps one or more `VARIABLE_DECLARATION_LIST`
    /// nodes, and the `let`/`const`/`using` flags live on the *list* node, not
    /// the statement. When `hoisted_var_only` is set, lists declared with
    /// `let`/`const`/`using` are skipped (they do not hoist across blocks).
    fn collect_variable_statement_names(
        &self,
        node: &tsz_parser::parser::node::Node,
        out: &mut Vec<String>,
        hoisted_var_only: bool,
    ) {
        let Some(stmt) = self.arena.get_variable(node) else {
            return;
        };
        for &list_idx in &stmt.declarations.nodes {
            let Some(list_node) = self.arena.get(list_idx) else {
                continue;
            };
            if hoisted_var_only {
                let flags = list_node.flags as u32;
                if (flags & (node_flags::LET | node_flags::CONST | node_flags::USING)) != 0 {
                    continue;
                }
            }
            let Some(list) = self.arena.get_variable(list_node) else {
                continue;
            };
            for &decl_idx in &list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                self.collect_binding_name(decl.name, out);
            }
        }
    }

    fn push_decl_name(&self, node: &tsz_parser::parser::node::Node, out: &mut Vec<String>) {
        let name_idx = match node.kind {
            k if k == sk::CLASS_DECLARATION => self.arena.get_class(node).map(|c| c.name),
            k if k == sk::FUNCTION_DECLARATION => self.arena.get_function(node).map(|f| f.name),
            k if k == sk::ENUM_DECLARATION => self.arena.get_enum(node).map(|e| e.name),
            k if k == sk::MODULE_DECLARATION => self.arena.get_module(node).map(|m| m.name),
            _ => None,
        };
        if let Some(name_idx) = name_idx {
            self.collect_binding_name(name_idx, out);
        }
    }

    /// Push an identifier binding name (ignoring destructuring patterns, whose
    /// individual names are handled recursively).
    fn collect_binding_name(&self, name_idx: NodeIndex, out: &mut Vec<String>) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };
        if name_node.is_identifier()
            && let Some(ident) = self.arena.get_identifier(name_node)
        {
            out.push(ident.escaped_text.clone());
            return;
        }
        if matches!(
            name_node.kind,
            k if k == sk::ARRAY_BINDING_PATTERN || k == sk::OBJECT_BINDING_PATTERN
        ) && let Some(pattern) = self.arena.get_binding_pattern(name_node)
        {
            for &elem_idx in &pattern.elements.nodes {
                if let Some(elem_node) = self.arena.get(elem_idx)
                    && let Some(elem) = self.arena.get_binding_element(elem_node)
                {
                    self.collect_binding_name(elem.name, out);
                }
            }
        }
    }
}
