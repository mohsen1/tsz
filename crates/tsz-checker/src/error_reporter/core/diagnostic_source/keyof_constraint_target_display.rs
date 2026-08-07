//! Target-side display for a call argument whose reported parameter type is a
//! type-parameter constraint written as the canonical primitive key union.
//!
//! `keyof any` and the longhand `string | number | symbol` both intern to the
//! one union `TypeId` that also carries `PropertyKey`'s registered `aliasSymbol`
//! (tsz keeps a single interned union for every spelling, where tsc keys
//! `getUnionType` on the member list *plus* the alias identity). So when a
//! generic call clamps an un-inferable type parameter to its constraint and
//! reports the argument against it (`TS2345`, "not assignable to parameter of
//! type `…`"), the general target-display path repaints that union as
//! `PropertyKey` regardless of what the constraint was written as.
//!
//! `tsc` strips the `aliasSymbol` here for a longhand / `keyof any` constraint
//! (which carries none) but keeps it for a constraint written as `PropertyKey`
//! (or a user alias). This is the target-side sibling of the source-side
//! `keyof any` handling (`#16748`) and the `TS2344` constraint handling
//! (`#16630`/`#16663`): the spelling written at the site decides. The written
//! constraint is recovered structurally from the callee's type-parameter
//! declaration, so a bare reference (`PropertyKey`, `Zed`) keeps its name and
//! only a longhand union or a `keyof any`/`keyof never` operand is force-expanded.

use crate::state::CheckerState;
use tsz_common::interner::AstAtom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Display override for a `TS2345` target that is a type-parameter
    /// constraint canonicalized to the primitive key union. Returns the
    /// structurally-expanded `string | number | symbol` when the written
    /// constraint at the call site was a longhand union or a `keyof any` /
    /// `keyof never` operand — neither of which `tsc` stamps with an
    /// `aliasSymbol` — and `None` (keep the ordinary alias display) otherwise,
    /// including the `K extends PropertyKey` control and every non-call or
    /// unresolvable site.
    pub(in crate::error_reporter) fn call_argument_key_union_constraint_target_display(
        &mut self,
        anchor_idx: NodeIndex,
        target: TypeId,
    ) -> Option<String> {
        // Only intervene when the reported parameter type is the canonical
        // primitive key union — the one shape that borrows `PropertyKey`'s
        // alias. Everything else renders through the ordinary path unchanged.
        let evaluated = self.evaluate_type_for_assignability(target);
        if !self.is_primitive_key_union_type(evaluated) {
            return None;
        }
        let constraint_idx =
            self.enclosing_call_argument_type_parameter_constraint_node(anchor_idx)?;
        // A constraint written as a bare reference (`PropertyKey`, a user alias)
        // keeps its written name through the ordinary alias display path. Only a
        // longhand primitive-keyword union or a `keyof any`/`keyof never` operand
        // — the spellings `tsc` renders structurally because neither carries an
        // `aliasSymbol` — is force-expanded here. Both predicates are the shared
        // "written structurally, aliasless" AST tests the source surface uses.
        let written_structurally =
            Self::annotation_is_longhand_primitive_keyword_union(self.ctx.arena, constraint_idx)
                || Self::annotation_is_keyof_over_degenerate_operand(
                    self.ctx.arena,
                    constraint_idx,
                );
        if !written_structurally {
            return None;
        }
        Some(self.format_type_diagnostic_constraint(evaluated))
    }

    /// Recover the *written* constraint clause of the type parameter standing in
    /// for the parameter slot that `anchor_idx` (a call argument) fills. Walks
    /// from the argument to its enclosing call, resolves the callee to a
    /// function-like declaration, matches the parameter at the argument's
    /// position to the declaration's type-parameter list by name, and returns
    /// that type parameter's constraint node.
    ///
    /// Read-only: it never lowers or mints a type. It returns `None` for any
    /// site it cannot resolve structurally (a method / overload callee, a
    /// parameter whose type is not a bare reference to a type parameter, a
    /// cross-file or non-function callee), so the ordinary display path stays in
    /// control there — the override is strictly additive.
    fn enclosing_call_argument_type_parameter_constraint_node(
        &self,
        anchor_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let (callee_expr, arg_pos) = self.enclosing_call_expression_and_arg_pos(anchor_idx)?;
        let sym_raw = self.resolve_value_symbol_for_lowering(callee_expr)?;
        // Only a callee declared in the current file is read here: its
        // declaration nodes and identifier atoms live in `self.ctx.arena`, so
        // the name-atom match below is valid. A cross-file callee's nodes belong
        // to another arena and are left to the ordinary display path (`None`).
        let symbol = self.ctx.binder.get_symbol(tsz_binder::SymbolId(sym_raw))?;

        for &decl_idx in &symbol.declarations {
            if let Some(constraint_idx) =
                self.declaration_arg_type_parameter_constraint_node(decl_idx, arg_pos)
            {
                return Some(constraint_idx);
            }
        }
        None
    }

    /// The callee expression node and the zero-based argument position of
    /// `anchor_idx` within the nearest enclosing call / new expression. Shared
    /// with `enclosing_call_arg_position`, which layers `get_type_of_node` on
    /// the callee node to return the callee *type* instead.
    pub(in crate::error_reporter) fn enclosing_call_expression_and_arg_pos(
        &self,
        anchor_idx: NodeIndex,
    ) -> Option<(NodeIndex, usize)> {
        let mut current = anchor_idx;
        loop {
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::CALL_EXPRESSION
                || node.kind == syntax_kind_ext::NEW_EXPRESSION
            {
                let call = self.ctx.arena.get_call_expr(node)?;
                let args = call.arguments.as_ref()?;
                let arg_pos = args.nodes.iter().position(|&a| a == anchor_idx)?;
                return Some((call.expression, arg_pos));
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
    }

    /// For a function-like declaration `decl_idx`, the constraint node of the
    /// type parameter named by the parameter at `arg_pos` — when that parameter
    /// is annotated as a bare (no type-argument) reference to one of the
    /// declaration's own type parameters. `None` for any other shape.
    fn declaration_arg_type_parameter_constraint_node(
        &self,
        decl_idx: NodeIndex,
        arg_pos: usize,
    ) -> Option<NodeIndex> {
        let func_idx = self.function_like_node_for_declaration(decl_idx)?;
        let func_node = self.ctx.arena.get(func_idx)?;
        let func = self.ctx.arena.get_function(func_node)?;
        let type_params = func.type_parameters.as_ref()?;

        let param_idx = *func.parameters.nodes.get(arg_pos)?;
        let param_node = self.ctx.arena.get(param_idx)?;
        let param = self.ctx.arena.get_parameter(param_node)?;
        if param.type_annotation == NodeIndex::NONE {
            return None;
        }
        let annotation_node = self.ctx.arena.get(param.type_annotation)?;
        if annotation_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let type_ref = self.ctx.arena.get_type_ref(annotation_node)?;
        if type_ref
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty())
        {
            return None;
        }
        let param_type_name = self.name_node_atom(type_ref.type_name)?;

        for &tp_idx in &type_params.nodes {
            let tp_node = self.ctx.arena.get(tp_idx)?;
            let Some(type_param) = self.ctx.arena.get_type_parameter(tp_node) else {
                continue;
            };
            if type_param.constraint == NodeIndex::NONE {
                continue;
            }
            if self
                .name_node_atom(type_param.name)
                .is_some_and(|name| name == param_type_name)
            {
                return Some(type_param.constraint);
            }
        }
        None
    }

    /// The function-like node backing a callee declaration: the declaration
    /// itself when it is a function declaration / expression / arrow, or the
    /// initializer of a `const f = <…>(…) => …` / `const f = function <…>(…)`
    /// variable declaration. `None` for any other declaration shape.
    fn function_like_node_for_declaration(&self, decl_idx: NodeIndex) -> Option<NodeIndex> {
        let decl_node = self.ctx.arena.get(decl_idx)?;
        if self.ctx.arena.get_function(decl_node).is_some() {
            return Some(decl_idx);
        }
        if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
            if var_decl.initializer == NodeIndex::NONE {
                return None;
            }
            let init_node = self.ctx.arena.get(var_decl.initializer)?;
            if self.ctx.arena.get_function(init_node).is_some() {
                return Some(var_decl.initializer);
            }
        }
        None
    }

    /// The interned identifier atom of a plain identifier name node, or `None`
    /// for any other node kind. Comparing atoms keeps the type-parameter/name
    /// match O(1) and allocation-free.
    fn name_node_atom(&self, name_idx: NodeIndex) -> Option<AstAtom> {
        let name_node = self.ctx.arena.get(name_idx)?;
        let ident = self.ctx.arena.get_identifier(name_node)?;
        Some(ident.atom)
    }
}
