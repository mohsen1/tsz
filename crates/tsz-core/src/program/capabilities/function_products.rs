use std::collections::BTreeSet;

use crate::source::NodeId;

use super::flow_containment::FunctionExpressionProducts;
use super::{
    ALL_TARGETS, CapabilityScope, CapabilityTarget, ProgramFile, ScopedNonclaims, SemanticGap,
    SyntaxGap, span_is_single_line, span_owns_comment,
};

pub(super) fn add_nonclaims(
    nonclaims: &mut ScopedNonclaims<'_>,
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
        let mut scoped = nonclaims.at(scope);
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
            scoped.semantic(&[target], gap);
        }
        if file.syntax.parser_recovery_facts.iter().any(|recovery| {
            function.span.start <= recovery.authored_span.start
                && recovery.authored_span.end <= function.span.end
        }) {
            scoped.javascript(SyntaxGap::FunctionExpressionRecovery);
        }
        if let Some(root) = file.syntax.statements.iter().find(|root| {
            root.span.start <= function.span.start && function.span.end <= root.span.end
        }) && file.syntax.comments().iter().any(|comment| {
            span_owns_comment(root.span, comment) && !span_owns_comment(function.span, comment)
        }) {
            let gap = SyntaxGap::FunctionExpressionOuterComments;
            scoped.node(id, root.id).javascript(gap);
        }
        if needs_printer_fence(&function) {
            scoped.javascript(SyntaxGap::FunctionLikePrinter);
        }
    }
    if !method_owners.is_empty() {
        nonclaims
            .at(CapabilityScope::Program)
            .semantic(&ALL_TARGETS[7..], SemanticGap::FunctionLikeService);
    }
    for owner in method_owners {
        nonclaims.node(id, owner).semantic(
            &[CapabilityTarget::DeclarationModel],
            SemanticGap::FunctionLikeService,
        );
    }
    for method in methods {
        if needs_printer_fence(&method) {
            nonclaims
                .node(id, method.owner)
                .javascript(SyntaxGap::FunctionLikePrinter);
        }
    }
}
