use crate::bind::DeclarationKind::{FunctionExpression, JavaScriptPropertyAssignment};
use crate::bind::{BoundDeclaration, DeclarationKind, Meaning, ScopeId, TypeMemberSymbol};
use crate::config::CompilerOptionKey;
use crate::source::{DeclId, FileId, NodeId, SourceKind, Span};
use crate::syntax::{
    AuthoredLiteralKind, CommentTrivia, ExpressionKind, ExpressionRoot, ExpressionTraversal,
    FunctionLikeSyntax, Literal, LiteralSyntaxBoundary, SourceCheckDirectiveKind, SourceSyntaxFact,
    StatementKind, UnmodeledDeclarationHostKind, contains_matching_expression,
    for_each_statement_in,
};
use std::cmp::Reverse;
use std::collections::BTreeSet;
use std::path::Path;

#[cfg(test)]
use crate::syntax::ParserRecoveryKind;
#[cfg(test)]
use recovery::{RecoveryRole, recovery_nodes};
#[cfg(test)]
use std::collections::BTreeMap;

use super::{
    CompilerOptions, DeferredCompilerOption, DeferredOptionEffect, JavaScriptAssignments,
    ProgramFile, global_declarations,
};

pub(crate) use crate::source::is_declaration_source_path as is_declaration_source;

mod declaration_groups;
mod emit_targets;
mod flow_containment;
mod function_products;
mod inferred_products;
mod recovery;
use flow_containment::FileBoundary;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CapabilityTarget {
    SemanticCheck,
    DeclarationModel,
    DeclarationValue,
    RequiredType,
    SemanticDiagnostics,
    JavaScript,
    Declaration,
    QuickInfo,
    TypeDefinition,
    Definition,
    References,
    Highlights,
    Rename,
    SyntacticDiagnostics,
}

const ALL_TARGETS: [CapabilityTarget; 13] = [
    CapabilityTarget::DeclarationModel,
    CapabilityTarget::DeclarationValue,
    CapabilityTarget::SemanticCheck,
    CapabilityTarget::SemanticDiagnostics,
    CapabilityTarget::RequiredType,
    CapabilityTarget::Declaration,
    CapabilityTarget::JavaScript,
    CapabilityTarget::QuickInfo,
    CapabilityTarget::TypeDefinition,
    CapabilityTarget::Definition,
    CapabilityTarget::References,
    CapabilityTarget::Highlights,
    CapabilityTarget::Rename,
];

const SEMANTIC_TYPE_TARGETS: [CapabilityTarget; 8] = [
    CapabilityTarget::DeclarationModel,
    CapabilityTarget::DeclarationValue,
    CapabilityTarget::SemanticCheck,
    CapabilityTarget::SemanticDiagnostics,
    CapabilityTarget::RequiredType,
    CapabilityTarget::Declaration,
    CapabilityTarget::QuickInfo,
    CapabilityTarget::TypeDefinition,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CapabilityScope {
    Program,
    File(FileId),
    Node {
        file: FileId,
        owner: NodeId,
    },
    Span {
        file: FileId,
        start: u32,
        end: u32,
    },
    Dependency {
        file: FileId,
        owner: NodeId,
        kind: CapabilityDependency,
        identifiers: bool,
    },
    Position {
        file: FileId,
        owner: Option<NodeId>,
        offset: u32,
    },
}

impl CapabilityScope {
    pub(crate) const fn node(file: FileId, owner: NodeId) -> Self {
        Self::Node { file, owner }
    }

    pub(crate) const fn semantic_descendant(
        file: FileId,
        owner: NodeId,
        identifiers: bool,
    ) -> Self {
        Self::dependency(file, owner, CapabilityDependency::Semantic, identifiers)
    }

    pub(crate) const fn function_like_descendant(
        file: FileId,
        owner: NodeId,
        identifiers: bool,
    ) -> Self {
        Self::dependency(file, owner, CapabilityDependency::FunctionLike, identifiers)
    }

    pub(crate) const fn required_function_like(file: FileId, owner: NodeId) -> Self {
        Self::dependency(file, owner, CapabilityDependency::RequiredType, false)
    }

    const fn dependency(
        file: FileId,
        owner: NodeId,
        kind: CapabilityDependency,
        identifiers: bool,
    ) -> Self {
        Self::Dependency {
            file,
            owner,
            kind,
            identifiers,
        }
    }

    const fn span(span: Span) -> Self {
        Self::Span {
            file: span.file,
            start: span.start,
            end: span.end,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CapabilityDependency {
    Semantic,
    FunctionLike,
    RequiredType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SyntaxGap {
    Function,
    Class,
    Declaration,
    GeneratorFunctionLike,
    CommonJsClass,
    CommonJsNamespaceImportReexport,
    DeclarationHost,
    DefaultExportHost,
    Expression,
    ObjectMember,
    ForStatement,
    ComputedPropertyName,
    ClassExpression,
    ClassFieldTransform,
    PrivateIdentifierTransform,
    FunctionExpressionClassPropertyTransform,
    FunctionExpressionCommonJsTransform,
    AsyncFunctionTransform,
    FunctionExpressionModifier,
    FunctionLikeBindingPattern,
    FunctionExpressionOuterComments,
    FunctionExpressionRecovery,
    FunctionLikePrinter,
    AngleAssertion,
    RejectedGenericArrowPrefix,
    Template,
    RegularExpression,
    NumericRecovery,
    NumericSeparator,
    TypeRecovery,
    UnicodeLineCommentTerminator,
    JavaScriptModuleFormat,
    ModuleClauseComment,
    DeclarationOverloadSummary,
    UnsignedRightShiftAssignmentRecovery,
    UnsignedRightShiftOperandRecovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SemanticGap {
    FlowTypeOfReference,
    UnusedFunctionExpressionBindings,
    JavaScriptJSDocSignature,
    JavaScriptJSDocValue,
    JavaScriptPropertyNavigation,
    FunctionLikeTypeParameters,
    FunctionExpressionBindingName,
    DeclarationFunctionSummary,
    DeclarationAccessorSummary,
    /// Remove when class checking owns reserved names, constructor grammar,
    /// and class-member body diagnostics through binder symbol identity.
    ClassMemberSemantics,
    /// Remove when declaration emit consumes completed checker-owned inferred type summaries.
    DeclarationExpressionSummary,
    FunctionLikeService,
    ExplicitThisParameter,
    /// Remove with TS7.0.2 TS18046/TS2365/TS6807, folding, and checked summaries.
    UnsignedRightShift,
    /// Remove when the binder identifies every syntax producer navigation can answer.
    NavigationIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum NonclaimReason {
    Syntax(SyntaxGap),
    SyntaxAtSemanticOwner(SyntaxGap),
    Semantic(SemanticGap),
    MissingEssentialTypes,
    FatalCompilerOption,
    UnsupportedCompilerOption(CompilerOptionKey),
    DeferredCompilerOption(DeferredCompilerOption),
}

impl NonclaimReason {
    const DEEPEST: u16 = 1 << 0;
    const FLOW: u16 = 1 << 1;
    const REPRESENTATION: u16 = 1 << 2;
    const JSDOC: u16 = 1 << 3;
    const FUNCTION_LIKE: u16 = 1 << 4;
    const FUNCTION_LIKE_IDENTIFIER: u16 = 1 << 5;
    const SEMANTIC_DESCENDANT: u16 = 1 << 6;
    const IDENTIFIER_SYNTAX: u16 = 1 << 7;
    const IDENTIFIER_REPRESENTATION: u16 = 1 << 8;
    const GENERATOR: u16 = 1 << 9;

    const fn dependency_traits(self) -> u16 {
        match self {
            Self::SyntaxAtSemanticOwner(gap) => {
                Self::DEEPEST
                    | Self::SEMANTIC_DESCENDANT
                    | match gap {
                        SyntaxGap::GeneratorFunctionLike => Self::FUNCTION_LIKE | Self::GENERATOR,
                        SyntaxGap::RejectedGenericArrowPrefix | SyntaxGap::ForStatement => {
                            Self::IDENTIFIER_SYNTAX
                        }
                        SyntaxGap::Template
                        | SyntaxGap::RegularExpression
                        | SyntaxGap::NumericRecovery
                        | SyntaxGap::NumericSeparator => Self::IDENTIFIER_REPRESENTATION,
                        _ => 0,
                    }
            }
            Self::Syntax(
                gap @ (SyntaxGap::Declaration
                | SyntaxGap::Expression
                | SyntaxGap::Template
                | SyntaxGap::NumericRecovery
                | SyntaxGap::NumericSeparator
                | SyntaxGap::TypeRecovery),
            ) => {
                Self::REPRESENTATION
                    | Self::SEMANTIC_DESCENDANT
                    | match gap {
                        SyntaxGap::Template
                        | SyntaxGap::NumericRecovery
                        | SyntaxGap::NumericSeparator => Self::IDENTIFIER_REPRESENTATION,
                        _ => 0,
                    }
            }
            Self::Semantic(SemanticGap::FlowTypeOfReference) => {
                Self::FLOW
                    | Self::SEMANTIC_DESCENDANT
                    | Self::FUNCTION_LIKE
                    | Self::FUNCTION_LIKE_IDENTIFIER
            }
            Self::Semantic(SemanticGap::JavaScriptJSDocValue) => {
                Self::JSDOC
                    | Self::SEMANTIC_DESCENDANT
                    | Self::FUNCTION_LIKE
                    | Self::FUNCTION_LIKE_IDENTIFIER
            }
            Self::Semantic(SemanticGap::JavaScriptJSDocSignature) => {
                Self::FUNCTION_LIKE | Self::FUNCTION_LIKE_IDENTIFIER
            }
            Self::Semantic(
                SemanticGap::ExplicitThisParameter | SemanticGap::FunctionExpressionBindingName,
            ) => Self::FUNCTION_LIKE,
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CapabilityNonclaim {
    pub(crate) target: CapabilityTarget,
    pub(crate) scope: CapabilityScope,
    pub(crate) reason: NonclaimReason,
}

struct ScopedNonclaims<'a> {
    records: &'a mut Vec<CapabilityNonclaim>,
    scope: CapabilityScope,
}

macro_rules! capability_helpers {
    ($($name:ident => $targets:expr),+ $(,)?) => { $(
        fn $name(&mut self, gap: SyntaxGap) { self.syntax($targets, gap); }
    )+ };
}

impl<'a> ScopedNonclaims<'a> {
    const fn new(records: &'a mut Vec<CapabilityNonclaim>, scope: CapabilityScope) -> Self {
        Self { records, scope }
    }
    const fn at(&mut self, scope: CapabilityScope) -> ScopedNonclaims<'_> {
        ScopedNonclaims::new(self.records, scope)
    }
    const fn node(&mut self, file: FileId, owner: NodeId) -> ScopedNonclaims<'_> {
        self.at(CapabilityScope::node(file, owner))
    }
    fn add(&mut self, targets: &[CapabilityTarget], reason: NonclaimReason) {
        self.records
            .extend(targets.iter().copied().map(|target| CapabilityNonclaim {
                target,
                scope: self.scope,
                reason,
            }));
    }
    fn syntax(&mut self, targets: &[CapabilityTarget], gap: SyntaxGap) {
        self.syntax_owned_by(targets, gap, false);
    }
    fn syntax_owned_by(
        &mut self,
        targets: &[CapabilityTarget],
        gap: SyntaxGap,
        deepest_semantic_owner: bool,
    ) {
        let reason = if deepest_semantic_owner {
            NonclaimReason::SyntaxAtSemanticOwner(gap)
        } else {
            NonclaimReason::Syntax(gap)
        };
        self.add(targets, reason);
    }

    fn semantic(&mut self, targets: &[CapabilityTarget], gap: SemanticGap) {
        self.add(targets, NonclaimReason::Semantic(gap));
    }

    capability_helpers! {
        emit => &ALL_TARGETS[5..7],
        javascript => &[CapabilityTarget::JavaScript],
        declaration => &[CapabilityTarget::Declaration],
        semantic_diagnostics => &[CapabilityTarget::SemanticDiagnostics],
        syntactic_diagnostics => &[CapabilityTarget::SyntacticDiagnostics],
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CapabilityContext {
    pub(crate) has_fatal_option_error: bool,
    pub(crate) has_missing_essential_types: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileSemanticMode {
    Checked,
    UncheckedJavaScript,
    Unchecked,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CapabilityAnalysis {
    nonclaims: Box<[CapabilityNonclaim]>,
    file_semantic_modes: Box<[FileSemanticMode]>,
    function_like_owners: Box<[(FileId, NodeId)]>,
}

impl CapabilityAnalysis {
    #[cfg(test)]
    pub(crate) fn derive(
        files: &[ProgramFile],
        options: &CompilerOptions,
        context: CapabilityContext,
    ) -> Self {
        Self::derive_inner(files, options, context, None)
    }

    pub(crate) fn derive_with_javascript_assignments(
        files: &[ProgramFile],
        options: &CompilerOptions,
        context: CapabilityContext,
        javascript_assignments: &JavaScriptAssignments,
    ) -> Self {
        Self::derive_inner(files, options, context, Some(javascript_assignments))
    }

    fn derive_inner(
        files: &[ProgramFile],
        options: &CompilerOptions,
        context: CapabilityContext,
        javascript_assignments: Option<&JavaScriptAssignments>,
    ) -> Self {
        let mut nonclaims = Vec::new();
        let mut function_like_owners = Vec::new();
        let mut file_semantic_modes = vec![FileSemanticMode::Checked; files.len()];
        for file in files {
            let mode = match file.syntax.source_check_directive.map(|item| item.kind) {
                _ if options.skip_lib_check && is_declaration_source(&file.source.path) => {
                    FileSemanticMode::Unchecked
                }
                Some(SourceCheckDirectiveKind::NoCheck) => FileSemanticMode::Unchecked,
                None if matches!(
                    file.source.kind(),
                    SourceKind::JavaScript | SourceKind::JavaScriptJsx
                ) && options.check_js != Some(true) =>
                {
                    FileSemanticMode::UncheckedJavaScript
                }
                Some(SourceCheckDirectiveKind::Check) | None => FileSemanticMode::Checked,
            };
            if let Some(slot) = file_semantic_modes.get_mut(file.source.id.0 as usize) {
                *slot = mode;
            }
        }
        if context.has_missing_essential_types {
            // Complete TS2318 diagnostics coexist with nonclaimed semantic models.
            ScopedNonclaims::new(&mut nonclaims, CapabilityScope::Program)
                .add(&ALL_TARGETS[..3], NonclaimReason::MissingEssentialTypes);
        }
        if context.has_fatal_option_error {
            ScopedNonclaims::new(&mut nonclaims, CapabilityScope::Program)
                .add(&ALL_TARGETS, NonclaimReason::FatalCompilerOption);
        }
        for file in files {
            for owner in derive_file_nonclaims(&mut nonclaims, file, options) {
                function_like_owners.push((file.source.id, owner));
            }
        }
        if files.iter().any(|file| {
            file.syntax
                .has_source_syntax_fact(SourceSyntaxFact::TemplateExpressionIdentifier)
        }) {
            ScopedNonclaims::new(&mut nonclaims, CapabilityScope::Program)
                .syntax(&ALL_TARGETS[10..], SyntaxGap::Template);
        }
        derive_program_nonclaims(&mut nonclaims, files, options);

        close_declaration_groups(&mut nonclaims, files, &function_like_owners);
        if javascript_assignments.is_some() {
            add_javascript_property_navigation_nonclaims(&mut nonclaims, files);
        }
        add_navigation_identity_nonclaims(&mut nonclaims, files);

        nonclaims.sort_unstable();
        nonclaims.dedup();
        Self {
            nonclaims: nonclaims.into_boxed_slice(),
            file_semantic_modes: file_semantic_modes.into_boxed_slice(),
            function_like_owners: function_like_owners.into_boxed_slice(),
        }
    }

    pub(crate) fn claim(
        &self,
        target: CapabilityTarget,
        scope: CapabilityScope,
    ) -> CapabilityClaim<'_> {
        let range = self.target_range(target);
        let reasons = CapabilityReasons {
            nonclaims: &self.nonclaims[range],
            scope,
            index: 0,
        };
        if reasons.clone().next().is_none()
            || Self::dependency_is_claimed(target, scope, reasons.clone())
        {
            CapabilityClaim::Claimed
        } else {
            CapabilityClaim::Nonclaimed(reasons)
        }
    }

    pub(crate) fn navigation_query_is_claimed(
        &self,
        target: CapabilityTarget,
        file: &ProgramFile,
        offset: u32,
    ) -> bool {
        let owner = match file.capability_scope_at(offset) {
            Some(CapabilityScope::Node { owner, .. }) => Some(owner),
            _ => None,
        };
        self.claim(
            target,
            CapabilityScope::Position {
                file: file.source.id,
                owner,
                offset,
            },
        )
        .is_claimed()
    }

    pub(crate) const fn navigation_declaration_has_identity(
        declaration: &BoundDeclaration,
    ) -> bool {
        !matches!(
            declaration.kind,
            DeclarationKind::TypeMember | DeclarationKind::AnonymousSignature
        ) && !(matches!(declaration.kind, DeclarationKind::FunctionExpression)
            && declaration.name.is_empty())
    }

    fn target_range(&self, target: CapabilityTarget) -> std::ops::Range<usize> {
        self.nonclaims
            .partition_point(|record| record.target < target)
            ..self
                .nonclaims
                .partition_point(|record| record.target <= target)
    }

    pub(crate) fn semantic_check_node_is_claimed(&self, file: FileId, owner: NodeId) -> bool {
        self.claim(
            CapabilityTarget::SemanticCheck,
            CapabilityScope::node(file, owner),
        )
        .is_claimed()
    }

    pub(crate) fn semantic_check_file_is_enabled(&self, file: FileId) -> bool {
        self.file_semantic_modes.get(file.0 as usize) == Some(&FileSemanticMode::Checked)
    }

    pub(crate) fn has_claimed_function_like(&self, file: FileId) -> bool {
        self.function_like_owners
            .iter()
            .any(|&(owner_file, owner)| {
                owner_file == file && self.semantic_check_node_is_claimed(file, owner)
            })
    }

    fn dependency_is_claimed(
        target: CapabilityTarget,
        scope: CapabilityScope,
        reasons: CapabilityReasons<'_>,
    ) -> bool {
        let CapabilityScope::Dependency {
            file,
            owner,
            kind,
            identifiers,
        } = scope
        else {
            return false;
        };
        let exact = CapabilityScope::node(file, owner);
        let mut every = u16::MAX;
        let mut any = 0;
        for record in reasons {
            if record.scope != exact {
                return false;
            }
            let traits = record.reason.dependency_traits();
            every &= traits;
            any |= traits;
        }
        let has = |bits, flag| bits & flag != 0;
        if kind == CapabilityDependency::RequiredType {
            return target == CapabilityTarget::RequiredType && has(every, NonclaimReason::DEEPEST);
        }
        if target != CapabilityTarget::SemanticCheck {
            return false;
        }
        match kind {
            CapabilityDependency::FunctionLike => {
                has(every, NonclaimReason::FUNCTION_LIKE)
                    && (!identifiers || has(every, NonclaimReason::FUNCTION_LIKE_IDENTIFIER))
            }
            CapabilityDependency::Semantic => {
                has(every, NonclaimReason::SEMANTIC_DESCENDANT)
                    && (has(any, NonclaimReason::DEEPEST | NonclaimReason::JSDOC)
                        || has(any, NonclaimReason::FLOW)
                            && has(any, NonclaimReason::REPRESENTATION))
                    && (!identifiers
                        || !has(any, NonclaimReason::GENERATOR)
                            && (has(
                                any,
                                NonclaimReason::IDENTIFIER_SYNTAX | NonclaimReason::JSDOC,
                            ) || has(any, NonclaimReason::FLOW)
                                && has(any, NonclaimReason::IDENTIFIER_REPRESENTATION)))
            }
            CapabilityDependency::RequiredType => unreachable!(),
        }
    }

    pub(crate) fn semantic_declaration_is_claimed(
        &self,
        files: &[ProgramFile],
        declaration: DeclId,
    ) -> bool {
        self.declaration_is_claimed(files, CapabilityTarget::DeclarationValue, declaration)
    }

    pub(crate) fn declaration_is_claimed(
        &self,
        files: &[ProgramFile],
        target: CapabilityTarget,
        declaration: DeclId,
    ) -> bool {
        scope_for_declaration(files, declaration, &self.function_like_owners)
            .is_some_and(|scope| self.claim(target, scope).is_claimed())
    }

    pub(crate) fn semantic_diagnostics_file_is_claimed(&self, file: FileId) -> bool {
        let CapabilityClaim::Nonclaimed(mut reasons) = self.claim(
            CapabilityTarget::SemanticDiagnostics,
            CapabilityScope::File(file),
        ) else {
            return true;
        };
        !reasons.any(|reason| {
            matches!(reason.scope, CapabilityScope::Program)
                || self.semantic_check_file_is_enabled(file)
        })
    }

    pub(crate) fn semantic_diagnostics_are_claimed(&self, options: &CompilerOptions) -> bool {
        if options.no_check {
            return true;
        }
        let CapabilityClaim::Nonclaimed(mut reasons) = self.claim(
            CapabilityTarget::SemanticDiagnostics,
            CapabilityScope::Program,
        ) else {
            return true;
        };
        !reasons.any(|reason| match reason.scope {
            CapabilityScope::Program => true,
            CapabilityScope::File(file) | CapabilityScope::Node { file, .. } => {
                self.semantic_check_file_is_enabled(file)
            }
            CapabilityScope::Span { .. }
            | CapabilityScope::Dependency { .. }
            | CapabilityScope::Position { .. } => false,
        })
    }

    pub(crate) fn syntactic_diagnostics_are_claimed(&self) -> bool {
        self.claim(
            CapabilityTarget::SyntacticDiagnostics,
            CapabilityScope::Program,
        )
        .is_claimed()
    }

    pub(crate) fn syntactic_diagnostics_file_is_claimed(&self, file: FileId) -> bool {
        self.claim(
            CapabilityTarget::SyntacticDiagnostics,
            CapabilityScope::File(file),
        )
        .is_claimed()
    }

    pub(crate) fn requested_emit_is_claimed(
        &self,
        files: &[ProgramFile],
        options: &CompilerOptions,
    ) -> bool {
        options.no_emit
            || files
                .iter()
                .filter(|file| !is_declaration_source(&file.source.path))
                .all(|file| {
                    let scope = CapabilityScope::File(file.source.id);
                    std::iter::once(CapabilityTarget::JavaScript)
                        .chain(options.declaration.then_some(CapabilityTarget::Declaration))
                        .all(|target| self.product_is_claimed(target, scope, options))
                })
    }

    pub(crate) fn product_is_claimed(
        &self,
        target: CapabilityTarget,
        scope: CapabilityScope,
        _options: &CompilerOptions,
    ) -> bool {
        self.claim(target, scope).is_claimed()
    }
}

fn add_navigation_identity_nonclaims(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    files: &[ProgramFile],
) {
    for file in files {
        let modeled = file
            .bindings
            .declarations
            .iter()
            .filter(|declaration| {
                CapabilityAnalysis::navigation_declaration_has_identity(declaration)
            })
            .map(|declaration| (declaration.name_span.start, declaration.name_span.end))
            .chain(
                file.bindings
                    .reference_facts()
                    .iter()
                    .map(|reference| (reference.span.start, reference.span.end)),
            )
            .collect::<BTreeSet<_>>();
        let mut record = |span: Span| {
            if !modeled.contains(&(span.start, span.end)) {
                ScopedNonclaims::new(nonclaims, CapabilityScope::span(span))
                    .semantic(&ALL_TARGETS[7..], SemanticGap::NavigationIdentity);
            }
        };
        file.syntax
            .identifier_token_spans
            .iter()
            .copied()
            .for_each(&mut record);
        file.bindings
            .type_members
            .values()
            .filter(|member| matches!(&member.symbol, Some(TypeMemberSymbol::Named(_))))
            .filter_map(|member| file.bindings.declaration(member.declaration))
            .map(|declaration| declaration.name_span)
            .for_each(&mut record);
        for_each_statement_in(&file.syntax.statements, &mut |statement| {
            if let StatementKind::Class(class) = &statement.kind {
                class
                    .members
                    .iter()
                    .map(|member| member.name_span)
                    .for_each(&mut record);
            }
            contains_matching_expression(
                ExpressionRoot::Statement(statement),
                ExpressionTraversal::All,
                |expression| {
                    match &expression.kind {
                        ExpressionKind::Object(properties) => properties
                            .iter()
                            .map(|property| property.name_span)
                            .for_each(&mut record),
                        ExpressionKind::Member { name_span, .. } => record(*name_span),
                        ExpressionKind::ElementAccess { index, .. }
                            if matches!(
                                &index.peel_parentheses().kind,
                                ExpressionKind::Literal(
                                    Literal::String(_) | Literal::Number(_) | Literal::BigInt(_)
                                )
                            ) =>
                        {
                            record(index.peel_parentheses().span);
                        }
                        _ => {}
                    }
                    false
                },
            );
        });
    }
}

fn add_javascript_property_navigation_nonclaims(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    files: &[ProgramFile],
) {
    for file in files {
        let scopes = file
            .bindings
            .javascript_property_uses
            .iter()
            .map(|&owner| CapabilityScope::node(file.source.id, owner))
            .chain(
                file.bindings
                    .javascript_property_assignments
                    .iter()
                    .filter_map(|assignment| assignment.declaration)
                    .map(|declaration| {
                        let owner = file
                            .bindings
                            .declaration(declaration)
                            .expect("same-file JavaScript property declaration")
                            .owner;
                        CapabilityScope::node(declaration.file, owner)
                    }),
            );
        let mut scoped = ScopedNonclaims::new(nonclaims, CapabilityScope::File(file.source.id));
        for scope in scopes {
            scoped
                .at(scope)
                .semantic(&ALL_TARGETS[7..], SemanticGap::JavaScriptPropertyNavigation);
        }
    }
}

#[derive(Debug)]
pub(crate) enum CapabilityClaim<'a> {
    Claimed,
    Nonclaimed(CapabilityReasons<'a>),
}

impl CapabilityClaim<'_> {
    pub(crate) fn is_claimed(&self) -> bool {
        match self {
            Self::Claimed => true,
            Self::Nonclaimed(reasons) => {
                debug_assert!(reasons.clone().next().is_some());
                false
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CapabilityReasons<'a> {
    nonclaims: &'a [CapabilityNonclaim],
    scope: CapabilityScope,
    index: usize,
}

impl<'a> Iterator for CapabilityReasons<'a> {
    type Item = &'a CapabilityNonclaim;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(record) = self.nonclaims.get(self.index) {
            self.index += 1;
            if scope_applies(record.scope, self.scope) {
                return Some(record);
            }
        }
        None
    }
}

fn scope_applies(record: CapabilityScope, requested: CapabilityScope) -> bool {
    if let (
        CapabilityScope::Span { file, start, end },
        CapabilityScope::Position {
            file: requested,
            offset,
            ..
        },
    ) = (record, requested)
    {
        return file == requested && start <= offset && offset <= end;
    }
    if matches!(record, CapabilityScope::Span { .. }) {
        return record == requested;
    }
    if matches!(record, CapabilityScope::Program) || matches!(requested, CapabilityScope::Program) {
        return true;
    }
    let parts = |scope| match scope {
        CapabilityScope::File(file) => Some((file, None)),
        CapabilityScope::Node { file, owner } | CapabilityScope::Dependency { file, owner, .. } => {
            Some((file, Some(owner)))
        }
        CapabilityScope::Position { file, owner, .. } => Some((file, owner)),
        CapabilityScope::Program | CapabilityScope::Span { .. } => None,
    };
    match (parts(record), parts(requested)) {
        (Some((record_file, record_owner)), Some((requested_file, requested_owner))) => {
            record_file == requested_file
                && (record_owner.is_none()
                    || requested_owner.is_none()
                    || record_owner == requested_owner)
        }
        _ => false,
    }
}

fn close_declaration_groups(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    files: &[ProgramFile],
    function_like_owners: &[(FileId, NodeId)],
) {
    let groups = declaration_groups(files);
    loop {
        let before = nonclaims.len();
        for group in &groups {
            let scopes = group
                .iter()
                .filter_map(|&declaration| {
                    scope_for_declaration(files, declaration, function_like_owners)
                })
                .collect::<BTreeSet<_>>();
            for &target in ALL_TARGETS[..5].iter().chain(&ALL_TARGETS[7..]) {
                inherit_group_nonclaims(nonclaims, &scopes, target, &[target]);
            }
            inherit_group_nonclaims(
                nonclaims,
                &scopes,
                CapabilityTarget::DeclarationModel,
                &ALL_TARGETS[7..],
            );
        }
        nonclaims.sort_unstable();
        nonclaims.dedup();
        if nonclaims.len() == before {
            break;
        }
    }
}

fn inherit_group_nonclaims(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    scopes: &BTreeSet<CapabilityScope>,
    source: CapabilityTarget,
    targets: &[CapabilityTarget],
) {
    let inherited = nonclaims
        .iter()
        .filter(|nonclaim| {
            nonclaim.target == source
                && scopes
                    .iter()
                    .any(|scope| scope_applies(nonclaim.scope, *scope))
        })
        .map(|nonclaim| nonclaim.reason)
        .collect::<BTreeSet<_>>();
    for scope in scopes {
        let mut scoped = ScopedNonclaims::new(nonclaims, *scope);
        for reason in &inherited {
            scoped.add(targets, *reason);
        }
    }
}

fn declaration_groups(files: &[ProgramFile]) -> Vec<Vec<DeclId>> {
    let mut groups = BTreeSet::new();
    for file in files {
        for scope in &file.bindings.scopes {
            for declarations in scope.names.values() {
                for meaning in [Meaning::Value, Meaning::Type] {
                    let group = declarations
                        .iter()
                        .copied()
                        .filter(|declaration| {
                            file.bindings
                                .declaration(*declaration)
                                .is_some_and(|declaration| {
                                    declaration.meaning == meaning
                                        && declaration.kind != DeclarationKind::FunctionExpression
                                })
                        })
                        .collect::<Vec<_>>();
                    if group.len() > 1 {
                        groups.insert(group);
                    }
                }
            }
        }
    }
    let (global_values, global_types) = global_declarations(files);
    for mut group in global_values
        .into_values()
        .chain(global_types.into_values())
    {
        group.sort_unstable();
        group.dedup();
        if group.len() > 1 {
            groups.insert(group);
        }
    }
    groups.into_iter().collect()
}

enum ScopeQuery<'a> {
    Position(u32),
    Declaration(DeclId, &'a [(FileId, NodeId)]),
}

impl ProgramFile {
    pub(crate) fn capability_scope_at(&self, offset: u32) -> Option<CapabilityScope> {
        capability_scope(self, ScopeQuery::Position(offset))
    }
}

fn scope_for_declaration(
    files: &[ProgramFile],
    declaration: DeclId,
    function_like_owners: &[(FileId, NodeId)],
) -> Option<CapabilityScope> {
    let file = files.get(declaration.file.0 as usize)?;
    capability_scope(
        file,
        ScopeQuery::Declaration(declaration, function_like_owners),
    )
}

fn capability_scope(file: &ProgramFile, query: ScopeQuery<'_>) -> Option<CapabilityScope> {
    let owner = match query {
        ScopeQuery::Position(offset) => {
            let contains = |span: Span| span.start <= offset && offset <= span.end;
            if let Some(declaration) = file.bindings.declarations.iter().find(|declaration| {
                contains(declaration.name_span)
                    && file.bindings.declarations.iter().any(|candidate| {
                        matches!(
                            candidate.kind,
                            FunctionExpression | JavaScriptPropertyAssignment
                        ) && candidate.owner == declaration.owner
                    })
            }) {
                return Some(CapabilityScope::node(file.source.id, declaration.owner));
            }

            let mut initializer_owner = None;
            let mut owner = None;
            let mut best = None;
            let mut select = |span: Span, candidate_owner| {
                if contains(span) {
                    let candidate = (span.start != offset, span.len(), Reverse(span.start));
                    if best.is_none_or(|current| candidate < current) {
                        owner = Some(candidate_owner);
                        best = Some(candidate);
                    }
                }
            };
            for_each_statement_in(&file.syntax.statements, &mut |statement| {
                if initializer_owner.is_none()
                    && let StatementKind::Variable(variable) = &statement.kind
                    && let Some(declarator) = variable
                        .declarators
                        .iter()
                        .find(|declarator| contains(declarator.name_span))
                    && declarator.annotation.is_none()
                    && let Some(initializer) = declarator.initializer.as_ref()
                {
                    let initializer = initializer.peel_parentheses_and_assertions();
                    initializer_owner = matches!(
                        &initializer.kind,
                        ExpressionKind::FunctionLike(function)
                            if matches!(&function.syntax, FunctionLikeSyntax::Function { .. })
                    )
                    .then_some(initializer.id);
                }
                select(statement.span, statement.id);
            });
            contains_matching_expression(
                ExpressionRoot::Statements(&file.syntax.statements),
                ExpressionTraversal::All,
                |expression| {
                    let span = match &expression.kind {
                        ExpressionKind::FunctionLike(_) => expression.span,
                        ExpressionKind::Member { name_span, .. } => *name_span,
                        _ => return false,
                    };
                    select(span, expression.id);
                    false
                },
            );
            initializer_owner.or(owner)?
        }
        ScopeQuery::Declaration(declaration, function_like_owners) => {
            let declaration = file.bindings.declaration(declaration)?;
            if matches!(
                declaration.kind,
                FunctionExpression | JavaScriptPropertyAssignment
            ) || declaration.kind == DeclarationKind::Parameter
                && function_like_owners
                    .binary_search(&(file.source.id, declaration.owner))
                    .is_ok()
            {
                return Some(CapabilityScope::node(file.source.id, declaration.owner));
            }
            let mut exact_owner = None;
            for_each_statement_in(&file.syntax.statements, &mut |statement| {
                exact_owner =
                    exact_owner.or((statement.id == declaration.owner).then_some(statement.id));
            });
            exact_owner.or_else(|| {
                file.syntax
                    .statements
                    .iter()
                    .find(|statement| {
                        statement.span.start <= declaration.name_span.start
                            && declaration.name_span.end <= statement.span.end
                    })
                    .map(|statement| statement.id)
            })?
        }
    };
    Some(CapabilityScope::node(file.source.id, owner))
}

fn runtime_namespace_imports(file: &ProgramFile) -> BTreeSet<DeclId> {
    let binding_key = |owner: NodeId, span: Span| (owner, span.start, span.end);
    let mut namespace_bindings = BTreeSet::new();
    for statement in &file.syntax.statements {
        let StatementKind::Import(import) = &statement.kind else {
            continue;
        };
        namespace_bindings.extend(
            import
                .bindings
                .iter()
                .filter(|binding| binding.namespace && !binding.type_only)
                .map(|binding| binding_key(statement.id, binding.local_span)),
        );
    }
    file.bindings
        .declarations
        .iter()
        .filter(|declaration| {
            declaration.kind == DeclarationKind::Import
                && declaration.meaning == Meaning::Value
                && namespace_bindings
                    .contains(&binding_key(declaration.owner, declaration.name_span))
        })
        .map(|declaration| declaration.id)
        .collect()
}

fn derive_file_nonclaims(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    file: &ProgramFile,
    options: &CompilerOptions,
) -> BTreeSet<NodeId> {
    let id = file.source.id;
    let scope = CapabilityScope::File(id);
    let is_javascript = matches!(
        file.source.kind(),
        SourceKind::JavaScript | SourceKind::JavaScriptJsx
    );
    let has_syntax_fact = |fact| file.syntax.has_source_syntax_fact(fact);
    let javascript_jsdoc_casts = file
        .syntax
        .source_syntax_facts
        .iter()
        .filter_map(|fact| match fact {
            SourceSyntaxFact::JavaScriptJSDocCast(owner, _) => Some(*owner),
            _ => None,
        })
        .collect();
    let mut nodes = flow_containment::semantic_node_inventory(
        &file.syntax.statements,
        &file.syntax.parser_recovery_facts,
        &javascript_jsdoc_casts,
    );
    let mut scoped = ScopedNonclaims::new(nonclaims, scope);
    inferred_products::add_nonclaims(&mut scoped, file);
    if has_syntax_fact(SourceSyntaxFact::DecoratorRecovery) {
        scoped.syntactic_diagnostics(SyntaxGap::Class);
    }

    if file.syntax.statements.iter().any(|statement| {
        matches!(
            statement.kind,
            StatementKind::Import(_) | StatementKind::Export(_)
        ) && file.syntax.comments().iter().any(|comment| {
            statement.span.start < comment.span.start && comment.span.end < statement.span.end
        })
    }) {
        scoped.javascript(SyntaxGap::ModuleClauseComment);
    }

    if file.syntax.has_unmodeled_function_products() {
        scoped.emit(SyntaxGap::Function);
    }
    if has_syntax_fact(SourceSyntaxFact::TemplateExpression) {
        scoped.emit(SyntaxGap::Template);
    }
    if [
        SourceSyntaxFact::AsyncClassModifier,
        SourceSyntaxFact::InvalidClassModifierOrder,
    ]
    .into_iter()
    .any(has_syntax_fact)
        || nodes.boundaries.contains(&FileBoundary::ClassProduct)
    {
        scoped.emit(SyntaxGap::Class);
    }
    emit_targets::add_nonclaims(&mut scoped, file, options);
    if has_syntax_fact(SourceSyntaxFact::ExplicitCallTypeArguments) {
        scoped.declaration(SyntaxGap::Declaration);
    }
    if !file.syntax.unmodeled_declaration_hosts.is_empty()
        || is_javascript && nodes.boundaries.contains(&FileBoundary::Declaration)
    {
        scoped.emit(SyntaxGap::DeclarationHost);
        if is_javascript && nodes.boundaries.contains(&FileBoundary::Declaration) {
            scoped.syntax(
                &[CapabilityTarget::RequiredType],
                SyntaxGap::DeclarationHost,
            );
        }
        add_unmodeled_declaration_host_nodes(&mut scoped, file);
    }
    if has_syntax_fact(SourceSyntaxFact::DefaultExportOnUnsupportedHost) {
        scoped.emit(SyntaxGap::DefaultExportHost);
        scoped.semantic_diagnostics(SyntaxGap::DefaultExportHost);
        scoped.syntactic_diagnostics(SyntaxGap::DefaultExportHost);
    }
    if has_syntax_fact(SourceSyntaxFact::ExplicitNewTypeArguments) {
        scoped.declaration(SyntaxGap::Expression);
    }
    if has_syntax_fact(SourceSyntaxFact::AuthoredFunctionExpressionModifier) {
        scoped.emit(SyntaxGap::FunctionExpressionModifier);
    }
    let function_likes = std::mem::take(&mut nodes.function_likes);
    recovery::add_parser_nodes(&mut scoped, file, &nodes.function_like_signatures);
    let has_function_expression = !nodes.function_expressions.is_empty();
    if options.no_unused_locals || options.no_unused_parameters {
        for products in &nodes.function_expressions {
            let gap = SemanticGap::UnusedFunctionExpressionBindings;
            scoped
                .node(id, products.owner)
                .semantic(&[CapabilityTarget::SemanticDiagnostics], gap);
        }
    }
    if has_function_expression
        && emit_targets::target_requires_class_property_transform(&options.target)
        && nodes.boundaries.contains(&FileBoundary::ClassProperty)
    {
        let gap = SyntaxGap::FunctionExpressionClassPropertyTransform;
        scoped.javascript(gap);
    }
    if has_function_expression
        && is_effective_commonjs(&file.source.path, &options.module)
        && file.syntax.is_external_module()
    {
        let gap = SyntaxGap::FunctionExpressionCommonJsTransform;
        scoped.javascript(gap);
    }
    for owner in nodes.flow_regions {
        scoped
            .node(id, owner)
            .semantic(&ALL_TARGETS[1..4], SemanticGap::FlowTypeOfReference);
    }
    for (owner, gap) in nodes.function_like_gaps {
        if gap == SemanticGap::JavaScriptJSDocSignature && !is_javascript {
            continue;
        }
        let owner_scope = CapabilityScope::node(id, owner);
        let unchecked_accessor = options.no_check
            && matches!(
                gap,
                SemanticGap::ExplicitThisParameter | SemanticGap::FunctionLikeTypeParameters
            )
            && scoped.records.iter().any(|record| {
                record.target == CapabilityTarget::SemanticDiagnostics
                    && record.scope == owner_scope
                    && record.reason
                        == NonclaimReason::Semantic(SemanticGap::DeclarationAccessorSummary)
            });
        let mut owner_nonclaims = scoped.at(owner_scope);
        if gap == SemanticGap::DeclarationExpressionSummary {
            owner_nonclaims.semantic(&[CapabilityTarget::DeclarationValue], gap);
            continue;
        }
        owner_nonclaims.semantic(
            if unchecked_accessor {
                &ALL_TARGETS[1..5]
            } else {
                &ALL_TARGETS[1..6]
            },
            gap,
        );
        if gap == SemanticGap::JavaScriptJSDocSignature {
            owner_nonclaims.semantic(&ALL_TARGETS[7..9], gap);
        }
    }
    if is_javascript {
        for owner in nodes.javascript_jsdoc_values {
            scoped.node(id, owner).semantic(
                &[
                    CapabilityTarget::DeclarationValue,
                    CapabilityTarget::SemanticDiagnostics,
                    CapabilityTarget::RequiredType,
                    CapabilityTarget::Declaration,
                    CapabilityTarget::QuickInfo,
                    CapabilityTarget::TypeDefinition,
                ],
                SemanticGap::JavaScriptJSDocValue,
            );
        }
        for owner in nodes.javascript_jsdoc_checks {
            scoped.node(id, owner).semantic(
                &[CapabilityTarget::SemanticCheck],
                SemanticGap::JavaScriptJSDocValue,
            );
        }
    }
    for owner in nodes.function_like_binding_patterns {
        add_function_like_recovery_nonclaims(
            scoped.node(id, owner),
            SyntaxGap::FunctionLikeBindingPattern,
        );
    }
    let namespace_imports = is_effective_commonjs(&file.source.path, &options.module)
        .then(|| runtime_namespace_imports(file));
    for statement in &file.syntax.statements {
        if let StatementKind::Function(function) = &statement.kind
            && function.has_body
            && function.return_type.is_none()
            && !function.body.is_empty()
            && !declaration_function_summary_is_supported(function)
        {
            scoped.node(id, statement.id).semantic(
                &[CapabilityTarget::Declaration],
                SemanticGap::DeclarationFunctionSummary,
            );
        }
        if let (Some(namespace_imports), StatementKind::Export(export)) =
            (&namespace_imports, &statement.kind)
            && !export.type_only
            && !export.export_all
            && export.module_specifier.is_none()
            && export.specifiers.iter().any(|specifier| {
                !specifier.type_only
                    && file
                        .bindings
                        .resolve(ScopeId(0), &specifier.local, Meaning::Value)
                        .is_some_and(|declaration| namespace_imports.contains(&declaration))
            })
        {
            let gap = SyntaxGap::CommonJsNamespaceImportReexport;
            scoped.node(id, statement.id).javascript(gap);
        }
    }
    function_products::add_nonclaims(
        &mut scoped,
        file,
        nodes.function_expressions,
        nodes.object_method_owners,
        nodes.object_methods,
    );
    for (owner, gap) in nodes.recovered_function_likes {
        add_function_like_recovery_nonclaims(scoped.node(id, owner), gap);
    }
    for (family, boundary) in file
        .syntax
        .source_syntax_facts
        .iter()
        .filter_map(|fact| match fact {
            SourceSyntaxFact::LiteralBoundary(family, boundary) => Some((*family, *boundary)),
            _ => None,
        })
    {
        add_literal_boundary_nonclaims(scoped.at(scope), file, family, boundary);
    }
    if file.syntax.has_unicode_line_comment_terminator {
        scoped.semantic_diagnostics(SyntaxGap::UnicodeLineCommentTerminator);
    }
    if file.has_unmodeled_javascript_module_products() {
        scoped.emit(SyntaxGap::JavaScriptModuleFormat);
    }
    for owner in file.syntax.source_syntax_facts.iter().filter_map(|fact| {
        let SourceSyntaxFact::NumericRecoveryEmit(owner) = fact else {
            return None;
        };
        Some(*owner)
    }) {
        scoped.node(id, owner).emit(SyntaxGap::NumericRecovery);
    }
    if is_effective_commonjs(&file.source.path, &options.module)
        && has_syntax_fact(SourceSyntaxFact::ModuleExport)
        && nodes.boundaries.contains(&FileBoundary::CommonJsClass)
    {
        scoped.emit(SyntaxGap::CommonJsClass);
    }
    function_likes
}

fn declaration_function_summary_is_supported(
    function: &crate::syntax::FunctionDeclaration,
) -> bool {
    let [statement] = function.body.as_slice() else {
        return false;
    };
    !function.is_async
        && matches!(
            &statement.kind,
            StatementKind::Return(Some(expression))
                if matches!(
                    &expression.peel_parentheses_and_assertions().kind,
                    ExpressionKind::Literal(_)
                )
        )
}

fn add_literal_boundary_nonclaims(
    mut nonclaims: ScopedNonclaims<'_>,
    file: &ProgramFile,
    family: AuthoredLiteralKind,
    boundary: LiteralSyntaxBoundary,
) {
    let (gap, semantic_nodes) = match family {
        AuthoredLiteralKind::RegularExpression => (SyntaxGap::RegularExpression, false),
        AuthoredLiteralKind::NumericRecovery => (SyntaxGap::NumericRecovery, true),
        AuthoredLiteralKind::NumericSeparator => (SyntaxGap::NumericSeparator, true),
    };
    if boundary != LiteralSyntaxBoundary::SemanticValidation
        || matches!(
            family,
            AuthoredLiteralKind::NumericRecovery | AuthoredLiteralKind::NumericSeparator
        )
    {
        nonclaims.emit(gap);
    }
    if semantic_nodes {
        recovery::add_literal_nodes(&mut nonclaims, file, family, gap);
    } else if boundary == LiteralSyntaxBoundary::SemanticValidation {
        nonclaims.semantic_diagnostics(gap);
    }
}

fn span_is_single_line(source: &crate::source::SourceText, span: Span) -> bool {
    source
        .slice(span)
        .bytes()
        .all(|byte| !matches!(byte, b'\n' | b'\r'))
}

fn span_owns_comment(span: Span, comment: &CommentTrivia) -> bool {
    span.start <= comment.span.start && comment.span.end <= span.end
        || comment
            .preceding_token_end
            .is_some_and(|end| span.start <= end && end <= span.end)
}

fn add_function_like_recovery_nonclaims(mut nonclaims: ScopedNonclaims<'_>, gap: SyntaxGap) {
    let generator = gap == SyntaxGap::GeneratorFunctionLike;
    nonclaims.syntax_owned_by(&ALL_TARGETS[1..5], gap, generator);
    nonclaims.emit(gap);
    nonclaims.syntax_owned_by(&ALL_TARGETS[7..], gap, generator);
}

fn add_unmodeled_declaration_host_nodes(nonclaims: &mut ScopedNonclaims<'_>, file: &ProgramFile) {
    if file.syntax.unmodeled_declaration_hosts.is_empty() {
        add_declaration_host_nonclaims(nonclaims.at(CapabilityScope::File(file.source.id)), false);
    }
    for host in &file.syntax.unmodeled_declaration_hosts {
        let syntax_unowned = host.kind == UnmodeledDeclarationHostKind::Enum
            || host.kind == UnmodeledDeclarationHostKind::ExternalModule && !host.declared;
        if host.kind == UnmodeledDeclarationHostKind::Global && host.declared {
            add_declaration_host_nonclaims(nonclaims.at(CapabilityScope::Program), syntax_unowned);
            continue;
        }
        let mut owners = BTreeSet::new();
        for_each_statement_in(&file.syntax.statements, &mut |statement| {
            if statement.span.start == host.owner_start
                || host.recovery_extent.start <= statement.span.start
                    && statement.span.start < host.recovery_extent.end
            {
                owners.insert(statement.id);
            }
        });
        if owners.is_empty() {
            add_declaration_host_nonclaims(
                nonclaims.at(CapabilityScope::File(file.source.id)),
                syntax_unowned,
            );
            continue;
        }
        for owner in owners {
            add_declaration_host_nonclaims(nonclaims.node(file.source.id, owner), syntax_unowned);
        }
    }
}

fn add_declaration_host_nonclaims(mut nonclaims: ScopedNonclaims<'_>, syntactic: bool) {
    nonclaims.syntax(&ALL_TARGETS[..5], SyntaxGap::DeclarationHost);
    nonclaims.syntax(&ALL_TARGETS[7..], SyntaxGap::DeclarationHost);
    if syntactic {
        nonclaims.syntactic_diagnostics(SyntaxGap::DeclarationHost);
    }
}

fn derive_program_nonclaims(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    files: &[ProgramFile],
    options: &CompilerOptions,
) {
    let mut nonclaims = ScopedNonclaims::new(nonclaims, CapabilityScope::Program);
    for (&option, value) in &options.deferred_options {
        let false_is_default = matches!(value, super::DeferredCompilerOptionValue::Boolean(false))
            && (!options.strict
                || option.effect() != DeferredOptionEffect::SemanticTypes
                || option == DeferredCompilerOption::ExactOptionalPropertyTypes);
        if false_is_default || option.is_jsx() {
            continue;
        }
        nonclaims.add(
            deferred_compiler_option_targets(option),
            NonclaimReason::DeferredCompilerOption(option),
        );
        if option.affects_syntactic_diagnostics() {
            nonclaims.add(
                &[CapabilityTarget::SyntacticDiagnostics],
                NonclaimReason::DeferredCompilerOption(option),
            );
        }
    }
    if files
        .iter()
        .any(|file| file.syntax.has_unicode_line_comment_terminator)
    {
        nonclaims.emit(SyntaxGap::UnicodeLineCommentTerminator);
    }
    for option in [
        options.source_map.then_some(CompilerOptionKey::SourceMap),
        options
            .inline_source_map
            .then_some(CompilerOptionKey::InlineSourceMap),
    ]
    .into_iter()
    .flatten()
    {
        nonclaims.add(
            &[CapabilityTarget::JavaScript],
            NonclaimReason::UnsupportedCompilerOption(option),
        );
    }
    if options.declaration_map {
        nonclaims.add(
            &[CapabilityTarget::Declaration],
            NonclaimReason::UnsupportedCompilerOption(CompilerOptionKey::DeclarationMap),
        );
    }

    for id in declaration_groups::declaration_overload_files(files) {
        let scope = CapabilityScope::File(id);
        nonclaims
            .at(scope)
            .declaration(SyntaxGap::DeclarationOverloadSummary);
        let file = &files[id.0 as usize];
        if is_effective_commonjs(&file.source.path, &options.module) {
            nonclaims
                .at(scope)
                .javascript(SyntaxGap::DeclarationOverloadSummary);
        }
    }
}

fn deferred_compiler_option_targets(
    option: super::DeferredCompilerOption,
) -> &'static [CapabilityTarget] {
    match option.effect() {
        DeferredOptionEffect::SemanticTypes => &SEMANTIC_TYPE_TARGETS,
        DeferredOptionEffect::JavaScript => &[CapabilityTarget::JavaScript],
        DeferredOptionEffect::Declaration => &[CapabilityTarget::Declaration],
        DeferredOptionEffect::Emit => {
            &[CapabilityTarget::Declaration, CapabilityTarget::JavaScript]
        }
        DeferredOptionEffect::ImportHelpers | DeferredOptionEffect::StrictEmit => &[
            CapabilityTarget::SemanticDiagnostics,
            CapabilityTarget::JavaScript,
        ],
        DeferredOptionEffect::DecoratorMetadata => &[
            CapabilityTarget::RequiredType,
            CapabilityTarget::SemanticDiagnostics,
            CapabilityTarget::JavaScript,
        ],
        DeferredOptionEffect::Jsx => &ALL_TARGETS[1..9],
        DeferredOptionEffect::All => &ALL_TARGETS,
    }
}

pub(crate) fn is_effective_commonjs(path: &Path, module: &str) -> bool {
    let module = module.trim();
    module.eq_ignore_ascii_case("commonjs")
        || module.eq_ignore_ascii_case("cjs")
        || ((module.eq_ignore_ascii_case("node16") || module.eq_ignore_ascii_case("nodenext"))
            && path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("cts")))
}

#[cfg(test)]
mod tests;
