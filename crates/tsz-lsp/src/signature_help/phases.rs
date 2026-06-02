use super::{CallKind, CallSite, SignatureCandidate, SignatureHelpProvider, TypeId};
use crate::utils::find_node_at_or_before_offset;
use tsz_checker::state::CheckerState;
use tsz_common::position::Position;
use tsz_parser::{NodeIndex, syntax_kind_ext};

#[derive(Clone, Copy)]
pub(super) struct TypeArgumentContext {
    pub(super) active_parameter: u32,
    pub(super) span_start: u32,
    pub(super) span_length: u32,
}

#[derive(Clone, Copy)]
pub(super) struct SignatureHelpTriggerContext {
    pub(super) offset: u32,
    pub(super) leaf_node: NodeIndex,
}

pub(super) struct SignatureHelpDisplaySelection {
    pub(super) active_signature: u32,
    pub(super) active_parameter: u32,
    pub(super) argument_count: usize,
    pub(super) span_start: u32,
    pub(super) span_length: u32,
}

impl<'a> SignatureHelpProvider<'a> {
    pub(super) fn signature_help_trigger_context(
        &self,
        position: Position,
    ) -> Option<SignatureHelpTriggerContext> {
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;
        let leaf_node = find_node_at_or_before_offset(self.arena, offset, self.source_text);
        Some(SignatureHelpTriggerContext { offset, leaf_node })
    }

    pub(super) const fn call_site_has_explicit_type_args(call_site: &CallSite<'_>) -> bool {
        match call_site {
            CallSite::Regular(data) => data.type_arguments.is_some(),
            CallSite::TaggedTemplate(_) => false,
        }
    }

    pub(super) fn explicit_type_argument_texts(
        &self,
        call_site: &CallSite<'_>,
        has_explicit_type_args: bool,
    ) -> Vec<String> {
        let CallSite::Regular(data) = call_site else {
            return Vec::new();
        };
        let Some(type_args) = data
            .type_arguments
            .as_ref()
            .filter(|_| has_explicit_type_args)
        else {
            return Vec::new();
        };

        type_args
            .nodes
            .iter()
            .map(|&node_idx| {
                if let Some(node) = self.arena.get(node_idx) {
                    let start = node.pos as usize;
                    let end = (node.end as usize).min(self.source_text.len());
                    if start < end {
                        self.source_text[start..end].trim().to_string()
                    } else {
                        "unknown".to_string()
                    }
                } else {
                    "unknown".to_string()
                }
            })
            .collect()
    }

    pub(super) fn collect_signature_candidates_for_call(
        &self,
        callee_expr: NodeIndex,
        callee_type: TypeId,
        checker: &mut CheckerState,
        callee_name: &str,
        effective_call_kind: CallKind,
        has_explicit_type_args: bool,
        explicit_type_arg_texts: &[String],
    ) -> Vec<SignatureCandidate> {
        // For primitive intrinsic methods resolved via the no-lib fallback the
        // type system synthesizes `(...args: any[]) => ReturnType`. Try to
        // build directly from the intrinsic parameter table first so we never
        // pay the cost of `get_signatures_from_type` when the result would be
        // discarded.
        if let Some(sigs) = self.try_build_intrinsic_signatures(
            callee_expr,
            callee_type,
            checker,
            callee_name,
            has_explicit_type_args,
            explicit_type_arg_texts,
        ) {
            return sigs;
        }

        self.get_signatures_from_type(
            callee_type,
            checker,
            effective_call_kind,
            callee_name,
            has_explicit_type_args,
            explicit_type_arg_texts,
        )
    }

    pub(super) fn select_signature_help_display(
        &self,
        call_node_idx: NodeIndex,
        call_site: &CallSite<'_>,
        type_argument_context: Option<TypeArgumentContext>,
        offset: u32,
        signatures: &[SignatureCandidate],
        active_parameter: u32,
        supplied_argument_types: &[String],
    ) -> SignatureHelpDisplaySelection {
        let argument_count = if type_argument_context.is_some() {
            0
        } else {
            self.call_site_argument_count(call_site)
        };
        let active_signature = self.select_active_signature(
            signatures,
            argument_count,
            active_parameter,
            supplied_argument_types,
        );
        let active_parameter = self.clamp_active_parameter(
            signatures,
            active_signature,
            active_parameter,
            argument_count,
        );
        let (span_start, span_length) = self.signature_help_applicable_span(
            call_node_idx,
            call_site,
            type_argument_context,
            offset,
        );

        SignatureHelpDisplaySelection {
            active_signature,
            active_parameter,
            argument_count,
            span_start,
            span_length,
        }
    }

    fn call_site_argument_count(&self, call_site: &CallSite<'_>) -> usize {
        match call_site {
            CallSite::Regular(call_expr) => call_expr.arguments.as_ref().map_or(0, |args| {
                args.nodes
                    .iter()
                    .filter(|&&arg_idx| {
                        self.arena
                            .get(arg_idx)
                            .is_some_and(|node| node.kind != syntax_kind_ext::OMITTED_EXPRESSION)
                    })
                    .count()
            }),
            CallSite::TaggedTemplate(tagged) => {
                // For tagged templates, arg count = 1 (templateStrings) plus
                // the number of `${}` expressions.
                if let Some(tmpl_node) = self.arena.get(tagged.template) {
                    if let Some(tmpl_expr) = self.arena.get_template_expr(tmpl_node) {
                        1 + tmpl_expr.template_spans.nodes.len()
                    } else {
                        1
                    }
                } else {
                    1
                }
            }
        }
    }

    fn signature_help_applicable_span(
        &self,
        call_node_idx: NodeIndex,
        call_site: &CallSite<'_>,
        type_argument_context: Option<TypeArgumentContext>,
        offset: u32,
    ) -> (u32, u32) {
        if let Some(ctx) = type_argument_context {
            return (ctx.span_start, ctx.span_length);
        }

        match call_site {
            CallSite::Regular(call_expr) => self.compute_applicable_span(call_node_idx, call_expr),
            CallSite::TaggedTemplate(tagged) => {
                // For tagged templates, span covers the template.
                if let Some(tmpl_node) = self.arena.get(tagged.template) {
                    let tmpl_start = tmpl_node.pos as usize;
                    let tmpl_end = (tmpl_node.end as usize).min(self.source_text.len());
                    let tmpl_text = &self.source_text[tmpl_start..tmpl_end];
                    if let Some(bt) = tmpl_text.find('`') {
                        ((tmpl_start + bt + 1) as u32, 0)
                    } else {
                        (tmpl_node.pos, 0)
                    }
                } else {
                    (offset, 0)
                }
            }
        }
    }
}
