//! TS1346/TS1347: `"use strict"` directive with a non-simple parameter list.
//!
//! Mirrors tsc's `checkGrammarForUseStrictSimpleParameterList`, which only runs
//! at ES2016 or later: a function whose parameter list is non-simple (any
//! parameter with a default initializer, a rest element, or a binding pattern)
//! may not carry a `"use strict"` directive in its body prologue. TS1346 is
//! anchored on each offending parameter, TS1347 on the directive itself.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    /// Reject a `"use strict"` directive prologue in a function whose parameter
    /// list is non-simple. Like the sibling `await`-parameter grammar check, tsc
    /// suppresses this program-wide once any file has a real parse error, so it
    /// is gated on `has_parse_errors`.
    pub(crate) fn check_use_strict_non_simple_parameter_list(
        &mut self,
        params: &[NodeIndex],
        func_idx: NodeIndex,
    ) {
        if self.ctx.has_parse_errors {
            return;
        }
        if (self.ctx.compiler_options.target as u32)
            < (tsz_common::common::ScriptTarget::ES2016 as u32)
        {
            return;
        }
        let Some(body_idx) = self.function_like_block_body(func_idx) else {
            return;
        };
        let Some(directive_idx) = self.use_strict_prologue_directive(body_idx) else {
            return;
        };
        let non_simple: Vec<NodeIndex> = params
            .iter()
            .copied()
            .filter(|&param_idx| self.parameter_is_non_simple(param_idx))
            .collect();
        if non_simple.is_empty() {
            return;
        }
        use crate::diagnostics::diagnostic_codes;
        for &param_idx in &non_simple {
            // tsc anchors on the whole parameter node (`error(parameter, …)`),
            // which for a rest parameter starts at the `...` token. Emit at the
            // raw parameter span rather than through `error_at_node`, whose
            // parameter normalization narrows the anchor to the name and would
            // drop the leading `...`.
            if let Some((start, end)) = self.get_node_span(param_idx) {
                self.error(
                    start,
                    end.saturating_sub(start),
                    "This parameter is not allowed with 'use strict' directive.".to_string(),
                    diagnostic_codes::THIS_PARAMETER_IS_NOT_ALLOWED_WITH_USE_STRICT_DIRECTIVE,
                );
            }
        }
        self.error_at_node(
            directive_idx,
            "'use strict' directive cannot be used with non-simple parameter list.",
            diagnostic_codes::USE_STRICT_DIRECTIVE_CANNOT_BE_USED_WITH_NON_SIMPLE_PARAMETER_LIST,
        );
    }

    /// The brace block body of a function-like declaration, if it has one. Only
    /// a block body can carry a directive prologue.
    fn function_like_block_body(&self, func_idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;
        let node = self.ctx.arena.get(func_idx)?;
        let body = match node.kind {
            syntax_kind_ext::FUNCTION_DECLARATION
            | syntax_kind_ext::FUNCTION_EXPRESSION
            | syntax_kind_ext::ARROW_FUNCTION => self.ctx.arena.get_function(node)?.body,
            syntax_kind_ext::METHOD_DECLARATION => self.ctx.arena.get_method_decl(node)?.body,
            syntax_kind_ext::CONSTRUCTOR => self.ctx.arena.get_constructor(node)?.body,
            syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                self.ctx.arena.get_accessor(node)?.body
            }
            _ => return None,
        };
        let body_node = self.ctx.arena.get(body)?;
        (body_node.kind == syntax_kind_ext::BLOCK).then_some(body)
    }

    /// The `"use strict"` directive statement in a block's directive prologue,
    /// if present. The prologue is the leading run of string-literal expression
    /// statements; it ends at the first statement that is not one — so a
    /// `"use strict"` appearing after any non-directive statement does not count.
    fn use_strict_prologue_directive(&self, block_idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;
        let block_node = self.ctx.arena.get(block_idx)?;
        let block = self.ctx.arena.get_block(block_node)?;
        for &stmt_idx in &block.statements.nodes {
            let stmt_node = self.ctx.arena.get(stmt_idx)?;
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                break;
            }
            let expr_stmt = self.ctx.arena.get_expression_statement(stmt_node)?;
            let expr_node = self.ctx.arena.get(expr_stmt.expression)?;
            if expr_node.kind != tsz_scanner::SyntaxKind::StringLiteral as u16 {
                break;
            }
            let lit = self.ctx.arena.get_literal(expr_node)?;
            if tsz_common::directives::is_use_strict_directive(lit.raw_text.as_deref(), &lit.text) {
                return Some(stmt_idx);
            }
        }
        None
    }

    /// A parameter is non-simple (ES2016 `IsSimpleParameterList`) when it has a
    /// default initializer, is a rest element, or binds a destructuring pattern.
    fn parameter_is_non_simple(&self, param_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let Some(param) = self.ctx.arena.get_parameter_at(param_idx) else {
            return false;
        };
        if param.dot_dot_dot_token || param.initializer.is_some() {
            return true;
        }
        self.ctx.arena.get(param.name).is_some_and(|name_node| {
            matches!(
                name_node.kind,
                syntax_kind_ext::OBJECT_BINDING_PATTERN | syntax_kind_ext::ARRAY_BINDING_PATTERN
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::context::CheckerOptions;
    use crate::query_boundaries::common::TypeInterner;
    use crate::state::CheckerState;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;

    fn checker_codes_at_target(source: &str, target: tsz_common::common::ScriptTarget) -> Vec<u32> {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let parse_diagnostics = parser.get_diagnostics().to_vec();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let options = CheckerOptions {
            target,
            ..CheckerOptions::default()
        };
        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            options,
        );
        checker.ctx.has_parse_errors = !parse_diagnostics.is_empty();
        checker.ctx.has_syntax_parse_errors = !parse_diagnostics.is_empty();
        checker.ctx.syntax_parse_error_positions =
            parse_diagnostics.iter().map(|diag| diag.start).collect();
        checker.ctx.all_parse_error_positions =
            parse_diagnostics.iter().map(|diag| diag.start).collect();

        checker.check_source_file(root);
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|diag| diag.code)
            .collect()
    }

    #[test]
    fn use_strict_non_simple_parameter_list_reports_ts1346_ts1347() {
        // A default initializer, a rest element, and a binding pattern each make
        // the parameter list non-simple. Vary binder names to keep the check
        // structural, not name-scoped.
        for source in [
            "function widget(size = 1) { \"use strict\"; }",
            "function collect(...items) { \"use strict\"; }",
            "function unpack({ label }) { \"use strict\"; }",
            "const handler = (opt = 2) => { \"use strict\"; };",
            "class Store { method(seed = 3) { \"use strict\"; } }",
        ] {
            let codes = checker_codes_at_target(source, tsz_common::common::ScriptTarget::ES2016);
            assert!(
                codes.contains(&1346) && codes.contains(&1347),
                "expected TS1346+TS1347 for `{source}`: {codes:?}"
            );
        }
    }

    #[test]
    fn use_strict_simple_parameter_list_is_clean() {
        let codes = checker_codes_at_target(
            "function plain(first, second) { \"use strict\"; }",
            tsz_common::common::ScriptTarget::ES2016,
        );
        assert!(
            !codes.contains(&1346) && !codes.contains(&1347),
            "simple parameter list must not report TS1346/TS1347: {codes:?}"
        );
    }

    #[test]
    fn use_strict_non_simple_parameter_list_gated_below_es2016() {
        // tsc's checkGrammarForUseStrictSimpleParameterList only runs at ES2016+.
        let codes = checker_codes_at_target(
            "function widget(size = 1) { \"use strict\"; }",
            tsz_common::common::ScriptTarget::ES2015,
        );
        assert!(
            !codes.contains(&1346) && !codes.contains(&1347),
            "below ES2016 must not report TS1346/TS1347: {codes:?}"
        );
    }

    #[test]
    fn use_strict_after_non_directive_statement_is_clean() {
        // `"use strict"` is only a directive in the leading prologue; once a
        // non-directive statement precedes it, it is an ordinary expression and
        // the grammar check does not apply.
        let codes = checker_codes_at_target(
            "function widget(size = 1) { const c = 1; \"use strict\"; }",
            tsz_common::common::ScriptTarget::ES2016,
        );
        assert!(
            !codes.contains(&1346) && !codes.contains(&1347),
            "non-prologue 'use strict' must not report TS1346/TS1347: {codes:?}"
        );
    }
}
