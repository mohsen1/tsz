use std::collections::BTreeSet;

use crate::source::NodeId;

use super::flow_containment::FunctionExpressionProducts;
use super::{
    ALL_TARGETS, CapabilityNonclaim, CapabilityScope, CapabilityTarget, ProgramFile, SemanticGap,
    SyntaxGap, add_javascript, add_semantic, span_is_single_line, span_owns_comment,
};

pub(super) fn add_nonclaims(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    file: &ProgramFile,
    functions: Vec<FunctionExpressionProducts>,
    method_owners: BTreeSet<NodeId>,
    methods: Vec<FunctionExpressionProducts>,
) {
    let id = file.source.id;
    let needs_printer_fence = |function: &FunctionExpressionProducts| {
        file.syntax
            .comments()
            .iter()
            .any(|comment| span_owns_comment(function.span, comment))
            || span_is_single_line(&file.source, function.body_span)
                && !function.inline_body_supported
    };
    for function in functions {
        let scope = CapabilityScope::node(id, function.owner);
        for (target, gap) in [
            (
                CapabilityTarget::Declaration,
                SemanticGap::DeclarationFunctionSummary,
            ),
            (
                CapabilityTarget::QuickInfo,
                SemanticGap::FunctionLikeService,
            ),
        ] {
            add_semantic(nonclaims, &[target], scope, gap);
        }
        if file.syntax.parser_recovery_facts().iter().any(|recovery| {
            function.span.start <= recovery.authored_span.start
                && recovery.authored_span.end <= function.span.end
        }) {
            add_javascript(nonclaims, scope, SyntaxGap::FunctionExpressionRecovery);
        }
        if let Some(root) = file.syntax.statements.iter().find(|root| {
            root.span.start <= function.span.start && function.span.end <= root.span.end
        }) && file.syntax.comments().iter().any(|comment| {
            span_owns_comment(root.span, comment) && !span_owns_comment(function.span, comment)
        }) {
            let gap = SyntaxGap::FunctionExpressionOuterComments;
            add_javascript(nonclaims, CapabilityScope::node(id, root.id), gap);
        }
        if needs_printer_fence(&function) {
            add_javascript(nonclaims, scope, SyntaxGap::FunctionLikePrinter);
        }
    }
    if !method_owners.is_empty() {
        add_semantic(
            nonclaims,
            &ALL_TARGETS[7..],
            CapabilityScope::Program,
            SemanticGap::FunctionLikeService,
        );
    }
    for owner in method_owners {
        add_semantic(
            nonclaims,
            &[CapabilityTarget::DeclarationModel],
            CapabilityScope::node(id, owner),
            SemanticGap::FunctionLikeService,
        );
    }
    for method in methods {
        if needs_printer_fence(&method) {
            add_javascript(
                nonclaims,
                CapabilityScope::node(id, method.owner),
                SyntaxGap::FunctionLikePrinter,
            );
        }
    }
}
