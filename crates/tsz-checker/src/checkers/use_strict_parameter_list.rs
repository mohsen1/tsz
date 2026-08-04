//! TS1346/TS1347: `"use strict"` directive with a non-simple parameter list.
//!
//! Mirrors tsc's `checkGrammarForUseStrictSimpleParameterList`, which only runs
//! at ES2016 or later: a function whose parameter list is non-simple (any
//! parameter with a default initializer, a rest element, or a binding pattern)
//! may not carry a `"use strict"` directive in its body prologue. TS1346 is
//! anchored on each offending parameter, TS1347 on the directive itself.
//!
//! Both halves carry cross-location related information, which is the whole
//! point of reporting the pair: TS1346 points forward at the directive with
//! TS1349, and TS1347 points back at every offending parameter with TS1348 for
//! the first and TS6204 (`and here.`) for each subsequent one.

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
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        // The directive anchor is shared: TS1347 is reported at it, and every
        // TS1346 points at it through a TS1349 related entry. Compute it once,
        // exactly as `error_at_node` would, so the primary diagnostic and the
        // related entries cannot drift onto different spans.
        let directive_span = self.get_node_span(directive_idx).map(|(start, end)| {
            self.normalized_anchor_span(directive_idx, start, end.saturating_sub(start))
        });

        // tsc anchors on the whole parameter node (`error(parameter, …)`),
        // which for a rest parameter starts at the `...` token. Emit at the
        // raw parameter span rather than through `error_at_node`, whose
        // parameter normalization narrows the anchor to the name and would
        // drop the leading `...`.
        let param_spans: Vec<(NodeIndex, u32, u32)> = non_simple
            .iter()
            .filter_map(|&param_idx| {
                self.get_node_span(param_idx)
                    .map(|(start, end)| (param_idx, start, end.saturating_sub(start)))
            })
            .collect();

        for &(_, start, length) in &param_spans {
            let related = directive_span
                .map(|(directive_start, directive_length)| {
                    vec![self.related_entry(
                        directive_start,
                        directive_length,
                        diagnostic_codes::USE_STRICT_DIRECTIVE_USED_HERE,
                        diagnostic_messages::USE_STRICT_DIRECTIVE_USED_HERE,
                    )]
                })
                .unwrap_or_default();
            self.error_at_span_with_related(
                start,
                length,
                diagnostic_messages::THIS_PARAMETER_IS_NOT_ALLOWED_WITH_USE_STRICT_DIRECTIVE,
                diagnostic_codes::THIS_PARAMETER_IS_NOT_ALLOWED_WITH_USE_STRICT_DIRECTIVE,
                related,
            );
        }

        // tsc names the first offending parameter and elides the rest as
        // `and here.`, in source order — the same shape as its other
        // multi-site grammar reports.
        let related: Vec<_> = param_spans
            .iter()
            .enumerate()
            .map(|(index, &(_, start, length))| {
                if index == 0 {
                    self.related_entry(
                        start,
                        length,
                        diagnostic_codes::NON_SIMPLE_PARAMETER_DECLARED_HERE,
                        diagnostic_messages::NON_SIMPLE_PARAMETER_DECLARED_HERE,
                    )
                } else {
                    self.related_entry(
                        start,
                        length,
                        diagnostic_codes::AND_HERE,
                        diagnostic_messages::AND_HERE,
                    )
                }
            })
            .collect();
        self.error_at_node_with_related(
            directive_idx,
            diagnostic_messages::USE_STRICT_DIRECTIVE_CANNOT_BE_USED_WITH_NON_SIMPLE_PARAMETER_LIST,
            diagnostic_codes::USE_STRICT_DIRECTIVE_CANNOT_BE_USED_WITH_NON_SIMPLE_PARAMETER_LIST,
            related,
        );
    }

    /// A cross-location related-information entry in the file under check.
    ///
    /// `depth` stays `0`: these are genuine "declared here" pointers, not links
    /// in an elaboration chain, so they must not pick up the progressive
    /// indentation the renderer applies to nested elaborations.
    fn related_entry(
        &self,
        start: u32,
        length: u32,
        code: u32,
        message: &str,
    ) -> crate::diagnostics::DiagnosticRelatedInformation {
        crate::diagnostics::DiagnosticRelatedInformation {
            category: tsz_common::diagnostics::DiagnosticCategory::Message,
            code,
            file: self.ctx.file_name.clone(),
            start,
            length,
            message_text: message.to_string(),
            depth: 0,
            kind: crate::diagnostics::RelatedInformationKind::LocationPointer,
        }
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

    fn checker_diagnostics_at_target(
        source: &str,
        target: tsz_common::common::ScriptTarget,
    ) -> Vec<crate::diagnostics::Diagnostic> {
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
        checker.ctx.diagnostics.clone()
    }

    fn checker_codes_at_target(source: &str, target: tsz_common::common::ScriptTarget) -> Vec<u32> {
        checker_diagnostics_at_target(source, target)
            .iter()
            .map(|diag| diag.code)
            .collect()
    }

    /// The `(code, message_text)` pairs of the related information attached to
    /// the first diagnostic with `code`, or `None` when no such diagnostic was
    /// reported at all — so a test cannot pass vacuously on a missing primary.
    fn related_of(
        source: &str,
        code: u32,
        target: tsz_common::common::ScriptTarget,
    ) -> Option<Vec<(u32, String)>> {
        checker_diagnostics_at_target(source, target)
            .into_iter()
            .find(|diag| diag.code == code)
            .map(|diag| {
                diag.related_information
                    .iter()
                    .map(|rel| (rel.code, rel.message_text.clone()))
                    .collect()
            })
    }

    /// The source offset a related entry of `related_code` points at, on the
    /// first diagnostic with `code`.
    fn related_start(
        source: &str,
        code: u32,
        related_code: u32,
        target: tsz_common::common::ScriptTarget,
    ) -> Option<u32> {
        checker_diagnostics_at_target(source, target)
            .into_iter()
            .find(|diag| diag.code == code)
            .and_then(|diag| {
                diag.related_information
                    .iter()
                    .find(|rel| rel.code == related_code)
                    .map(|rel| rel.start)
            })
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

    /// TS1346 points forward at the directive with a single TS1349 entry, for
    /// every non-simple parameter shape and every function-like carrier.
    #[test]
    fn ts1346_carries_one_ts1349_pointing_at_the_directive() {
        for source in [
            "function widget(size = 1) { \"use strict\"; }",
            "function collect(...items) { \"use strict\"; }",
            "function unpack({ label }) { \"use strict\"; }",
            "const handler = (opt = 2) => { \"use strict\"; };",
            "class Store { method(seed = 3) { \"use strict\"; } }",
            "class Widget { set label({ text }) { \"use strict\"; } }",
        ] {
            let related = related_of(source, 1346, tsz_common::common::ScriptTarget::ES2016)
                .unwrap_or_else(|| panic!("no TS1346 reported for `{source}`"));
            assert_eq!(
                related,
                vec![(1349, "'use strict' directive used here.".to_string())],
                "TS1346 related information for `{source}`"
            );
            // The related entry must land on the directive, not on the
            // parameter TS1346 is already anchored at.
            assert_eq!(
                related_start(source, 1346, 1349, tsz_common::common::ScriptTarget::ES2016),
                Some(
                    source
                        .find("\"use strict\"")
                        .expect("witness contains a \"use strict\" directive")
                        as u32
                ),
                "TS1349 anchor for `{source}`"
            );
        }
    }

    /// TS1347 points back at every offending parameter: TS1348 names the first,
    /// TS6204 (`and here.`) elides each subsequent one, in source order.
    #[test]
    fn ts1347_carries_ts1348_then_and_here_per_extra_parameter() {
        let single = "function widget(size = 1) { \"use strict\"; }";
        assert_eq!(
            related_of(single, 1347, tsz_common::common::ScriptTarget::ES2016),
            Some(vec![(
                1348,
                "Non-simple parameter declared here.".to_string()
            )]),
            "one non-simple parameter must produce exactly one TS1348 and no `and here.`"
        );

        // Three parameters, two of them non-simple, with a simple parameter
        // interleaved — the related list must skip the simple one and stay in
        // source order.
        let multi = "function render(size = 1, plain, ...rest) { \"use strict\"; }";
        assert_eq!(
            related_of(multi, 1347, tsz_common::common::ScriptTarget::ES2016),
            Some(vec![
                (1348, "Non-simple parameter declared here.".to_string()),
                (6204, "and here.".to_string()),
            ]),
            "TS1347 related information for `{multi}`"
        );
        assert_eq!(
            related_start(multi, 1347, 1348, tsz_common::common::ScriptTarget::ES2016),
            Some(
                multi
                    .find("size = 1")
                    .expect("witness contains the first non-simple parameter")
                    as u32
            ),
            "TS1348 must anchor on the first non-simple parameter"
        );
        assert_eq!(
            related_start(multi, 1347, 6204, tsz_common::common::ScriptTarget::ES2016),
            Some(
                multi
                    .find("...rest")
                    .expect("witness contains the rest parameter") as u32
            ),
            "`and here.` must anchor on the rest parameter including its `...`"
        );
    }

    /// The negative side: when the grammar check does not fire, neither related
    /// code may appear anywhere in the file's diagnostics.
    #[test]
    fn related_codes_do_not_leak_when_the_check_does_not_fire() {
        for (source, target) in [
            (
                "function plain(first, second) { \"use strict\"; }",
                tsz_common::common::ScriptTarget::ES2016,
            ),
            (
                "function widget(size = 1) { \"use strict\"; }",
                tsz_common::common::ScriptTarget::ES2015,
            ),
            (
                "function widget(size = 1) { const c = 1; \"use strict\"; }",
                tsz_common::common::ScriptTarget::ES2016,
            ),
        ] {
            let diagnostics = checker_diagnostics_at_target(source, target);
            for diag in &diagnostics {
                for rel in &diag.related_information {
                    assert!(
                        !matches!(rel.code, 1348 | 1349),
                        "TS{} leaked as related information on TS{} for `{source}`",
                        rel.code,
                        diag.code
                    );
                }
            }
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

    #[test]
    fn set_accessor_use_strict_non_simple_param_reports_ts1346_ts1347() {
        // Set-accessor parameters route through the accessor grammar path, not
        // the shared per-function-like param check; the use-strict check is
        // wired in there too. Vary binder names to keep the check structural.
        for source in [
            "class Store { set value(seed = 1) { \"use strict\"; } }",
            "class Widget { set label({ text }) { \"use strict\"; } }",
        ] {
            let codes = checker_codes_at_target(source, tsz_common::common::ScriptTarget::ES2016);
            assert!(
                codes.contains(&1346) && codes.contains(&1347),
                "expected TS1346+TS1347 for `{source}`: {codes:?}"
            );
        }
    }

    #[test]
    fn class_accessor_without_body_reports_ts1005() {
        // A non-ambient, non-abstract class accessor without a brace body is
        // TS1005, emitted at check time so it coexists with semantic diagnostics.
        for source in [
            "class Store { get value(): string; }",
            "class Widget { set label(v: string); }",
        ] {
            let codes = checker_codes_at_target(source, tsz_common::common::ScriptTarget::ES2016);
            assert!(
                codes.contains(&1005),
                "expected TS1005 for `{source}`: {codes:?}"
            );
        }
    }

    #[test]
    fn abstract_or_ambient_accessor_without_body_is_clean() {
        // Abstract and ambient (`declare class`) accessors are legitimately
        // body-less and must not report TS1005.
        for source in [
            "abstract class Store { abstract get value(): string; }",
            "declare class Widget { get label(): string; }",
        ] {
            let codes = checker_codes_at_target(source, tsz_common::common::ScriptTarget::ES2016);
            assert!(
                !codes.contains(&1005),
                "unexpected TS1005 for `{source}`: {codes:?}"
            );
        }
    }
}
