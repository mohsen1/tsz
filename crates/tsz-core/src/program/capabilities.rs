use crate::bind::DeclarationKind::{FunctionExpression, JavaScriptPropertyAssignment};
use crate::bind::{DeclarationKind, Meaning, ScopeId};
use crate::config::CompilerOptionKey;
use crate::source::{DeclId, FileId, NodeId, SourceKind, Span};
use crate::syntax::{
    AuthoredLiteralKind, CommentTrivia, ExpressionKind, ExpressionRoot, ExpressionTraversal,
    FunctionLikeSyntax, LiteralSyntaxBoundary, SourceCheckDirectiveKind, SourceSyntaxFact,
    StatementKind, UnmodeledDeclarationHostKind, contains_matching_expression,
    for_each_statement_in,
};
use std::cmp::Reverse;
use std::collections::BTreeSet;
use std::path::Path;

#[cfg(test)]
use crate::syntax::ParserRecoveryKind;
#[cfg(test)]
use recovery::{RecoveryStatementRole, recovery_nodes};
#[cfg(test)]
use std::collections::BTreeMap;

use super::{CompilerOptions, JavaScriptAssignments, ProgramFile};

mod declaration_groups;
mod emit_targets;
mod flow_containment;
mod function_products;
mod inferred_products;
mod recovery;
use flow_containment::FileBoundary;

/// A compiler operation or product whose answer is claimed or withheld by scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CapabilityTarget {
    SemanticCheck,
    DeclarationModel,
    /// Semantic value/type materialization for an already-bound declaration.
    /// Binding and declaration identity remain owned by `DeclarationModel`.
    DeclarationValue,
    RequiredType,
    SemanticDiagnostics,
    JavaScript,
    Declaration,
    QuickInfo,
    Definition,
    References,
    Highlights,
    Rename,
}

const ALL_TARGETS: [CapabilityTarget; 12] = [
    CapabilityTarget::DeclarationModel,
    CapabilityTarget::DeclarationValue,
    CapabilityTarget::SemanticCheck,
    CapabilityTarget::SemanticDiagnostics,
    CapabilityTarget::RequiredType,
    CapabilityTarget::Declaration,
    CapabilityTarget::JavaScript,
    CapabilityTarget::QuickInfo,
    CapabilityTarget::Definition,
    CapabilityTarget::References,
    CapabilityTarget::Highlights,
    CapabilityTarget::Rename,
];

/// Scope of one capability decision, keyed by stable statement identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CapabilityScope {
    Program,
    File(FileId),
    Node { file: FileId, owner: NodeId },
}

impl CapabilityScope {
    pub(crate) const fn node(file: FileId, owner: NodeId) -> Self {
        Self::Node { file, owner }
    }
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
    /// Remove when declaration emit consumes checked expression summaries.
    DeclarationExpressionSummary,
    FunctionLikeService,
    ExplicitThisParameter,
    /// Remove with TS7.0.2 TS18046/TS2365/TS6807, folding, and checked summaries.
    UnsignedRightShift,
}

/// Structural reason a target cannot yet publish a definitive answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum NonclaimReason {
    Syntax(SyntaxGap),
    Semantic(SemanticGap),
    MissingEssentialTypes,
    FatalCompilerOption,
    UnsupportedCompilerOption(CompilerOptionKey),
}

/// Typed exit criterion for temporary nonclaims. This is deliberately not a
/// prose tag: reviews can enumerate which owner removes each record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum DeletionCondition {
    SyntaxOwner(SyntaxGap),
    /// Remove once the deepest parser-authored semantic producer owns this
    /// recovered syntax rather than deferring its dependency-closed demand.
    DeepestSemanticOwner(SyntaxGap),
    SemanticOwner(SemanticGap),
    EssentialLibraryUniverse,
    CompilerOptionOwner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CapabilityNonclaim {
    pub(crate) target: CapabilityTarget,
    pub(crate) scope: CapabilityScope,
    pub(crate) reason: NonclaimReason,
    pub(crate) deletion: DeletionCondition,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CapabilityContext {
    pub(crate) has_fatal_option_error: bool,
    pub(crate) has_missing_essential_types: bool,
}

/// File-scoped semantic scheduling derived from source-authored check-control
/// pragmas. Deliberate suppression is complete, not an unsupported nonclaim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileSemanticMode {
    Checked,
    UncheckedJavaScript,
    UncheckedBySourceDirective,
}

/// One immutable capability decision set for a parsed/bound program and its
/// normalized compiler-option snapshot.
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
                Some(SourceCheckDirectiveKind::NoCheck) => {
                    FileSemanticMode::UncheckedBySourceDirective
                }
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
            // The missing global set is computed before checking and produces
            // the complete TS2318 diagnostic set. It blocks semantic models,
            // but does not make the aggregate diagnostic product speculative.
            add_nonclaims(
                &mut nonclaims,
                &ALL_TARGETS[..3],
                CapabilityScope::Program,
                NonclaimReason::MissingEssentialTypes,
                DeletionCondition::EssentialLibraryUniverse,
            );
        }
        if context.has_fatal_option_error {
            add_nonclaims(
                &mut nonclaims,
                &ALL_TARGETS,
                CapabilityScope::Program,
                NonclaimReason::FatalCompilerOption,
                DeletionCondition::CompilerOptionOwner,
            );
            nonclaims.sort_unstable();
            nonclaims.dedup();
            return Self {
                nonclaims: nonclaims.into_boxed_slice(),
                file_semantic_modes: file_semantic_modes.into_boxed_slice(),
                function_like_owners: Box::default(),
            };
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
            add_syntax(
                &mut nonclaims,
                &ALL_TARGETS[9..],
                CapabilityScope::Program,
                SyntaxGap::Template,
            );
        }
        derive_program_nonclaims(&mut nonclaims, files, options);

        close_declaration_groups(&mut nonclaims, files, &function_like_owners);
        if javascript_assignments.is_some() {
            add_javascript_property_navigation_nonclaims(&mut nonclaims, files);
        }

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
        let empty = 0..0;
        let ranges = match scope {
            CapabilityScope::Program => [self.target_range(target), empty.clone(), empty],
            CapabilityScope::File(file) => [
                self.scope_range(target, CapabilityScope::Program, CapabilityScope::Program),
                self.scope_range(target, scope, scope),
                self.scope_range(
                    target,
                    CapabilityScope::node(file, NodeId(0)),
                    CapabilityScope::node(file, NodeId(u32::MAX)),
                ),
            ],
            CapabilityScope::Node { file, .. } => [
                self.scope_range(target, CapabilityScope::Program, CapabilityScope::Program),
                self.scope_range(
                    target,
                    CapabilityScope::File(file),
                    CapabilityScope::File(file),
                ),
                self.scope_range(target, scope, scope),
            ],
        };
        let reasons = CapabilityReasons {
            nonclaims: &self.nonclaims,
            ranges,
            range: 0,
        };
        if reasons.clone().next().is_some() {
            CapabilityClaim::Nonclaimed(reasons)
        } else {
            CapabilityClaim::Claimed
        }
    }

    fn target_range(&self, target: CapabilityTarget) -> std::ops::Range<usize> {
        self.nonclaims
            .partition_point(|record| record.target < target)
            ..self
                .nonclaims
                .partition_point(|record| record.target <= target)
    }

    fn scope_range(
        &self,
        target: CapabilityTarget,
        start: CapabilityScope,
        end: CapabilityScope,
    ) -> std::ops::Range<usize> {
        self.nonclaims
            .partition_point(|record| (record.target, record.scope) < (target, start))
            ..self
                .nonclaims
                .partition_point(|record| (record.target, record.scope) <= (target, end))
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

    pub(crate) fn semantic_check_node_descendant_permissions(
        &self,
        file: FileId,
        owner: NodeId,
    ) -> (bool, bool) {
        let CapabilityClaim::Nonclaimed(reasons) = self.claim(
            CapabilityTarget::SemanticCheck,
            CapabilityScope::node(file, owner),
        ) else {
            return (false, false);
        };
        let requested_scope = CapabilityScope::node(file, owner);
        let mut has_semantic_recovery = false;
        let mut has_representational_recovery = false;
        let mut has_direct_identifier_recovery = false;
        let (mut has_literal_semantic_recovery, mut has_flow_region) = (false, false);
        let mut has_generator = false;
        for reason in reasons {
            match reason.deletion {
                DeletionCondition::DeepestSemanticOwner(_) if reason.scope == requested_scope => {
                    has_semantic_recovery = true;
                    match reason.reason {
                        NonclaimReason::Syntax(
                            SyntaxGap::RejectedGenericArrowPrefix | SyntaxGap::ForStatement,
                        ) => has_direct_identifier_recovery = true,
                        NonclaimReason::Syntax(
                            SyntaxGap::Template
                            | SyntaxGap::RegularExpression
                            | SyntaxGap::NumericRecovery
                            | SyntaxGap::NumericSeparator,
                        ) => has_literal_semantic_recovery = true,
                        NonclaimReason::Syntax(SyntaxGap::GeneratorFunctionLike) => {
                            has_generator = true
                        }
                        _ => {}
                    }
                }
                DeletionCondition::SyntaxOwner(
                    SyntaxGap::Declaration
                    | SyntaxGap::Expression
                    | SyntaxGap::Template
                    | SyntaxGap::NumericRecovery
                    | SyntaxGap::NumericSeparator
                    | SyntaxGap::TypeRecovery,
                ) if reason.scope == requested_scope => has_representational_recovery = true,
                DeletionCondition::SemanticOwner(SemanticGap::FlowTypeOfReference)
                    if reason.scope == requested_scope =>
                {
                    has_flow_region = true;
                }
                DeletionCondition::SemanticOwner(SemanticGap::JavaScriptJSDocValue)
                    if reason.scope == requested_scope =>
                {
                    has_semantic_recovery = true;
                    has_direct_identifier_recovery = true;
                }
                _ => return (false, false),
            }
        }
        let allows_descent =
            has_semantic_recovery || has_flow_region && has_representational_recovery;
        (
            allows_descent,
            allows_descent
                && !has_generator
                && (has_direct_identifier_recovery
                    || has_flow_region && has_literal_semantic_recovery),
        )
    }

    /// Whether an exact semantic `FunctionLike` owner may run independently inventoried
    /// nested function-like semantics; broader recovery cannot publish a signature.
    pub(crate) fn semantic_check_node_function_like_descendant_permissions(
        &self,
        file: FileId,
        owner: NodeId,
    ) -> (bool, bool) {
        let scope = CapabilityScope::node(file, owner);
        let CapabilityClaim::Nonclaimed(mut reasons) =
            self.claim(CapabilityTarget::SemanticCheck, scope)
        else {
            return (false, false);
        };
        let mut identifiers = true;
        let allowed = reasons.all(|reason| {
            if reason.scope != scope {
                return false;
            }
            match reason.deletion {
                DeletionCondition::SemanticOwner(
                    SemanticGap::FlowTypeOfReference
                    | SemanticGap::JavaScriptJSDocSignature
                    | SemanticGap::JavaScriptJSDocValue,
                ) => true,
                DeletionCondition::DeepestSemanticOwner(gap)
                    if gap == SyntaxGap::GeneratorFunctionLike
                        && reason.reason == NonclaimReason::Syntax(gap) =>
                {
                    identifiers = false;
                    true
                }
                DeletionCondition::SemanticOwner(
                    SemanticGap::ExplicitThisParameter | SemanticGap::FunctionExpressionBindingName,
                ) => {
                    identifiers = false;
                    true
                }
                _ => false,
            }
        });
        (allowed, allowed && identifiers)
    }

    /// Whether an exact nonclaim may ask nested function-like `RequiredType` gates.
    pub(crate) fn required_type_node_allows_function_like_reentry(
        &self,
        file: FileId,
        owner: NodeId,
    ) -> bool {
        let scope = CapabilityScope::node(file, owner);
        let CapabilityClaim::Nonclaimed(reasons) =
            self.claim(CapabilityTarget::RequiredType, scope)
        else {
            return false;
        };
        reasons.into_iter().all(|reason| {
            reason.scope == scope
                && matches!(reason.deletion, DeletionCondition::DeepestSemanticOwner(_))
        })
    }

    pub(crate) fn semantic_declaration_is_claimed(
        &self,
        files: &[ProgramFile],
        declaration: DeclId,
    ) -> bool {
        scope_for_declaration(files, declaration, &self.function_like_owners).is_some_and(|scope| {
            self.claim(CapabilityTarget::DeclarationValue, scope)
                .is_claimed()
        })
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
        let CapabilityClaim::Nonclaimed(mut reasons) = self.claim(
            CapabilityTarget::SemanticDiagnostics,
            CapabilityScope::Program,
        ) else {
            return true;
        };
        !reasons.any(|reason| {
            if options.no_check && matches!(reason.reason, NonclaimReason::Semantic(_)) {
                return false;
            }
            match reason.scope {
                CapabilityScope::Program => true,
                CapabilityScope::File(file) | CapabilityScope::Node { file, .. } => {
                    self.semantic_check_file_is_enabled(file)
                }
            }
        })
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
        options: &CompilerOptions,
    ) -> bool {
        let unchecked_accessor_signature = options.no_check
            && target == CapabilityTarget::Declaration
            && match self.claim(CapabilityTarget::SemanticDiagnostics, scope) {
                CapabilityClaim::Claimed => false,
                CapabilityClaim::Nonclaimed(mut reasons) => reasons.any(|reason| {
                    reason.reason
                        == NonclaimReason::Semantic(SemanticGap::DeclarationAccessorSummary)
                }),
            };
        let CapabilityClaim::Nonclaimed(mut reasons) = self.claim(target, scope) else {
            return true;
        };
        reasons.all(|reason| {
            unchecked_accessor_signature
                && matches!(
                    reason.reason,
                    NonclaimReason::Semantic(
                        SemanticGap::ExplicitThisParameter
                            | SemanticGap::FunctionLikeTypeParameters
                    )
                )
        })
    }
}

fn add_javascript_property_navigation_nonclaims(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    files: &[ProgramFile],
) {
    for file in files {
        for &member in &file.bindings.javascript_property_uses {
            add_semantic(
                nonclaims,
                &ALL_TARGETS[7..],
                CapabilityScope::node(file.source.id, member),
                SemanticGap::JavaScriptPropertyNavigation,
            );
        }
        for declaration in file
            .bindings
            .javascript_property_assignments
            .iter()
            .filter_map(|assignment| assignment.declaration)
        {
            let owner = file
                .bindings
                .declaration(declaration)
                .expect("same-file JavaScript property declaration")
                .owner;
            add_semantic(
                nonclaims,
                &ALL_TARGETS[7..],
                CapabilityScope::node(declaration.file, owner),
                SemanticGap::JavaScriptPropertyNavigation,
            );
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
    ranges: [std::ops::Range<usize>; 3],
    range: usize,
}

impl<'a> Iterator for CapabilityReasons<'a> {
    type Item = &'a CapabilityNonclaim;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(index) = self.ranges.get_mut(self.range)?.next() {
                return self.nonclaims.get(index);
            }
            self.range += 1;
        }
    }
}

fn scope_applies(record: CapabilityScope, requested: CapabilityScope) -> bool {
    match (record, requested) {
        (CapabilityScope::Program, _) | (_, CapabilityScope::Program) => true,
        (CapabilityScope::File(record), CapabilityScope::File(requested)) => record == requested,
        (
            CapabilityScope::File(record),
            CapabilityScope::Node {
                file: requested, ..
            },
        )
        | (CapabilityScope::Node { file: record, .. }, CapabilityScope::File(requested)) => {
            record == requested
        }
        (
            CapabilityScope::Node {
                file: record_file,
                owner: record_owner,
            },
            CapabilityScope::Node {
                file: requested_file,
                owner: requested_owner,
            },
        ) => record_file == requested_file && record_owner == requested_owner,
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
                let inherited = nonclaims
                    .iter()
                    .filter(|nonclaim| {
                        nonclaim.target == target
                            && scopes
                                .iter()
                                .any(|scope| scope_applies(nonclaim.scope, *scope))
                    })
                    .map(|nonclaim| (nonclaim.reason, nonclaim.deletion))
                    .collect::<BTreeSet<_>>();
                for scope in &scopes {
                    for (reason, deletion) in &inherited {
                        add_nonclaims(nonclaims, &[target], *scope, *reason, *deletion);
                    }
                }
            }
            let declaration_reasons = nonclaims
                .iter()
                .filter(|nonclaim| {
                    nonclaim.target == CapabilityTarget::DeclarationModel
                        && scopes
                            .iter()
                            .any(|scope| scope_applies(nonclaim.scope, *scope))
                })
                .map(|nonclaim| (nonclaim.reason, nonclaim.deletion))
                .collect::<BTreeSet<_>>();
            for scope in &scopes {
                for (reason, deletion) in &declaration_reasons {
                    add_nonclaims(nonclaims, &ALL_TARGETS[7..], *scope, *reason, *deletion);
                }
            }
        }
        nonclaims.sort_unstable();
        nonclaims.dedup();
        if nonclaims.len() == before {
            break;
        }
    }
}

fn declaration_groups(files: &[ProgramFile]) -> Vec<Vec<DeclId>> {
    let mut groups = BTreeSet::new();
    let mut global_values = std::collections::BTreeMap::<String, Vec<DeclId>>::new();
    let mut global_types = std::collections::BTreeMap::<String, Vec<DeclId>>::new();
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
        if file.is_external_module() {
            continue;
        }
        for declarations in file.bindings.scopes[0].names.values() {
            for declaration in declarations {
                let Some(bound) = file.bindings.declaration(*declaration) else {
                    continue;
                };
                match bound.meaning {
                    Meaning::Value => &mut global_values,
                    Meaning::Type => &mut global_types,
                }
                .entry(bound.name.clone())
                .or_default()
                .push(*declaration);
            }
        }
    }
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

/// Runtime namespace-import declaration identities are binder-owned.
/// Matching owner and name span connects syntax provenance without treating
/// authored spelling as semantic identity.
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
    inferred_products::add_nonclaims(nonclaims, file);

    if file.syntax.statements.iter().any(|statement| {
        matches!(
            statement.kind,
            StatementKind::Import(_) | StatementKind::Export(_)
        ) && file.syntax.comments().iter().any(|comment| {
            statement.span.start < comment.span.start && comment.span.end < statement.span.end
        })
    }) {
        add_javascript(nonclaims, scope, SyntaxGap::ModuleClauseComment);
    }

    if file.syntax.has_unmodeled_function_products() {
        add_both_emit(nonclaims, scope, SyntaxGap::Function);
    }
    if file
        .syntax
        .has_source_syntax_fact(SourceSyntaxFact::TemplateExpression)
    {
        add_both_emit(nonclaims, scope, SyntaxGap::Template);
    }
    if [
        SourceSyntaxFact::AsyncClassModifier,
        SourceSyntaxFact::InvalidClassModifierOrder,
    ]
    .into_iter()
    .any(|fact| file.syntax.has_source_syntax_fact(fact))
        || nodes.boundaries.contains(&FileBoundary::ClassProduct)
    {
        add_both_emit(nonclaims, scope, SyntaxGap::Class);
    }
    emit_targets::add_nonclaims(nonclaims, file, options);
    if file
        .syntax
        .has_source_syntax_fact(SourceSyntaxFact::ExplicitCallTypeArguments)
    {
        add_dts(nonclaims, scope, SyntaxGap::Declaration);
    }
    if !file.syntax.unmodeled_declaration_hosts.is_empty()
        || is_javascript && nodes.boundaries.contains(&FileBoundary::Declaration)
    {
        add_both_emit(nonclaims, scope, SyntaxGap::DeclarationHost);
        if is_javascript && nodes.boundaries.contains(&FileBoundary::Declaration)
            || file
                .syntax
                .unmodeled_declaration_hosts
                .iter()
                .any(|host| host.kind != UnmodeledDeclarationHostKind::Enum)
        {
            add_syntax(
                nonclaims,
                &[CapabilityTarget::RequiredType],
                scope,
                SyntaxGap::DeclarationHost,
            );
        }
        add_unmodeled_declaration_host_nodes(nonclaims, file);
    }
    if file
        .syntax
        .has_source_syntax_fact(SourceSyntaxFact::DefaultExportOnUnsupportedHost)
    {
        add_both_emit(nonclaims, scope, SyntaxGap::DefaultExportHost);
        add_semantic_diagnostics(nonclaims, scope, SyntaxGap::DefaultExportHost);
    }
    if file
        .syntax
        .has_source_syntax_fact(SourceSyntaxFact::ExplicitNewTypeArguments)
    {
        add_dts(nonclaims, scope, SyntaxGap::Expression);
    }
    if file
        .syntax
        .has_source_syntax_fact(SourceSyntaxFact::AuthoredFunctionExpressionModifier)
    {
        add_both_emit(nonclaims, scope, SyntaxGap::FunctionExpressionModifier);
    }
    let function_likes = std::mem::take(&mut nodes.function_likes);
    recovery::add_parser_nodes(nonclaims, file, &nodes.function_like_signatures);
    let has_function_expression = !nodes.function_expressions.is_empty();
    if options.no_unused_locals || options.no_unused_parameters {
        for products in &nodes.function_expressions {
            let gap = SemanticGap::UnusedFunctionExpressionBindings;
            add_semantic(
                nonclaims,
                &[CapabilityTarget::SemanticDiagnostics],
                CapabilityScope::node(id, products.owner),
                gap,
            );
        }
    }
    if has_function_expression
        && emit_targets::target_requires_class_property_transform(&options.target)
        && nodes.boundaries.contains(&FileBoundary::ClassProperty)
    {
        let gap = SyntaxGap::FunctionExpressionClassPropertyTransform;
        add_javascript(nonclaims, scope, gap);
    }
    if has_function_expression
        && is_effective_commonjs(&file.source.path, &options.module)
        && file.syntax.is_external_module()
    {
        let gap = SyntaxGap::FunctionExpressionCommonJsTransform;
        add_javascript(nonclaims, scope, gap);
    }
    for owner in nodes.flow_regions {
        let gap = SemanticGap::FlowTypeOfReference;
        add_semantic(
            nonclaims,
            &ALL_TARGETS[1..4],
            CapabilityScope::node(id, owner),
            gap,
        );
    }
    for (owner, gap) in nodes.function_like_gaps {
        if gap == SemanticGap::JavaScriptJSDocSignature && !is_javascript {
            continue;
        }
        add_semantic(
            nonclaims,
            &ALL_TARGETS[1..6],
            CapabilityScope::node(id, owner),
            gap,
        );
        if gap == SemanticGap::JavaScriptJSDocSignature {
            add_semantic(
                nonclaims,
                &[CapabilityTarget::QuickInfo],
                CapabilityScope::node(id, owner),
                gap,
            );
        }
    }
    if is_javascript {
        for owner in nodes.javascript_jsdoc_values {
            let gap = SemanticGap::JavaScriptJSDocValue;
            add_semantic(
                nonclaims,
                &[
                    CapabilityTarget::DeclarationValue,
                    CapabilityTarget::SemanticDiagnostics,
                    CapabilityTarget::RequiredType,
                    CapabilityTarget::Declaration,
                    CapabilityTarget::QuickInfo,
                ],
                CapabilityScope::node(id, owner),
                gap,
            );
        }
        for owner in nodes.javascript_jsdoc_checks {
            let gap = SemanticGap::JavaScriptJSDocValue;
            add_semantic(
                nonclaims,
                &[CapabilityTarget::SemanticCheck],
                CapabilityScope::node(id, owner),
                gap,
            );
        }
    }
    for owner in nodes.function_like_binding_patterns {
        add_function_like_recovery_nonclaims(
            nonclaims,
            CapabilityScope::node(id, owner),
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
        {
            add_semantic(
                nonclaims,
                &[CapabilityTarget::Declaration],
                CapabilityScope::node(id, statement.id),
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
            add_javascript(nonclaims, CapabilityScope::node(id, statement.id), gap);
        }
    }
    function_products::add_nonclaims(
        nonclaims,
        file,
        nodes.function_expressions,
        nodes.object_method_owners,
        nodes.object_methods,
    );
    for (owner, gap) in nodes.recovered_function_likes {
        add_function_like_recovery_nonclaims(nonclaims, CapabilityScope::node(id, owner), gap);
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
        add_literal_boundary_nonclaims(nonclaims, file, family, boundary);
    }
    if file.syntax.has_unicode_line_comment_terminator {
        add_semantic_diagnostics(nonclaims, scope, SyntaxGap::UnicodeLineCommentTerminator);
    }
    if file.has_unmodeled_javascript_module_products() {
        add_both_emit(nonclaims, scope, SyntaxGap::JavaScriptModuleFormat);
    }
    if is_effective_commonjs(&file.source.path, &options.module)
        && file
            .syntax
            .has_source_syntax_fact(SourceSyntaxFact::ModuleExport)
        && nodes.boundaries.contains(&FileBoundary::CommonJsClass)
    {
        add_both_emit(nonclaims, scope, SyntaxGap::CommonJsClass);
    }
    function_likes
}

fn add_literal_boundary_nonclaims(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    file: &ProgramFile,
    family: AuthoredLiteralKind,
    boundary: LiteralSyntaxBoundary,
) {
    let scope = CapabilityScope::File(file.source.id);
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
        add_both_emit(nonclaims, scope, gap);
    }
    if semantic_nodes {
        recovery::add_literal_nodes(nonclaims, file, family, gap);
    } else if boundary == LiteralSyntaxBoundary::SemanticValidation {
        add_semantic_diagnostics(nonclaims, scope, gap);
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

fn add_function_like_recovery_nonclaims(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    scope: CapabilityScope,
    gap: SyntaxGap,
) {
    let generator = gap == SyntaxGap::GeneratorFunctionLike;
    if generator {
        add_nonclaims(
            nonclaims,
            &ALL_TARGETS[1..5],
            scope,
            NonclaimReason::Syntax(gap),
            DeletionCondition::DeepestSemanticOwner(gap),
        );
    } else {
        add_syntax(nonclaims, &ALL_TARGETS[1..5], scope, gap);
    }
    add_both_emit(nonclaims, scope, gap);
    add_service_nonclaims(nonclaims, scope, gap, generator);
}

fn add_unmodeled_declaration_host_nodes(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    file: &ProgramFile,
) {
    let mut found = false;
    let mut unmapped = file.syntax.unmodeled_declaration_hosts.is_empty();
    for host in &file.syntax.unmodeled_declaration_hosts {
        if host.kind == UnmodeledDeclarationHostKind::Global {
            add_declaration_host_nonclaims(nonclaims, CapabilityScope::Program);
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
            unmapped = true;
            continue;
        }
        found = true;
        for owner in owners {
            let owner_scope = CapabilityScope::node(file.source.id, owner);
            add_declaration_host_nonclaims(nonclaims, owner_scope);
        }
    }
    if !found || unmapped {
        let scope = CapabilityScope::File(file.source.id);
        add_declaration_host_nonclaims(nonclaims, scope);
    }
}

fn add_declaration_host_nonclaims(nonclaims: &mut Vec<CapabilityNonclaim>, scope: CapabilityScope) {
    add_syntax(
        nonclaims,
        &ALL_TARGETS[..5],
        scope,
        SyntaxGap::DeclarationHost,
    );
    add_service_nonclaims(nonclaims, scope, SyntaxGap::DeclarationHost, false);
}

fn derive_program_nonclaims(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    files: &[ProgramFile],
    options: &CompilerOptions,
) {
    if files
        .iter()
        .any(|file| file.syntax.has_unicode_line_comment_terminator)
    {
        add_both_emit(
            nonclaims,
            CapabilityScope::Program,
            SyntaxGap::UnicodeLineCommentTerminator,
        );
    }
    if options.source_map {
        add_option_nonclaim(
            nonclaims,
            CapabilityTarget::JavaScript,
            CompilerOptionKey::SourceMap,
        );
    }
    if options.inline_source_map {
        add_option_nonclaim(
            nonclaims,
            CapabilityTarget::JavaScript,
            CompilerOptionKey::InlineSourceMap,
        );
    }
    if options.declaration_map {
        add_option_nonclaim(
            nonclaims,
            CapabilityTarget::Declaration,
            CompilerOptionKey::DeclarationMap,
        );
    }

    let overload_files = declaration_groups::declaration_overload_files(files);
    for id in overload_files {
        let scope = CapabilityScope::File(id);
        add_dts(nonclaims, scope, SyntaxGap::DeclarationOverloadSummary);
        let file = &files[id.0 as usize];
        if is_effective_commonjs(&file.source.path, &options.module) {
            add_javascript(nonclaims, scope, SyntaxGap::DeclarationOverloadSummary);
        }
    }
}

fn add_option_nonclaim(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    target: CapabilityTarget,
    option: CompilerOptionKey,
) {
    add_nonclaims(
        nonclaims,
        &[target],
        CapabilityScope::Program,
        NonclaimReason::UnsupportedCompilerOption(option),
        DeletionCondition::CompilerOptionOwner,
    );
}

fn add_semantic_diagnostics(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    scope: CapabilityScope,
    gap: SyntaxGap,
) {
    add_syntax(
        nonclaims,
        &[CapabilityTarget::SemanticDiagnostics],
        scope,
        gap,
    );
}

fn add_service_nonclaims(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    scope: CapabilityScope,
    gap: SyntaxGap,
    semantic_node: bool,
) {
    let deletion = if semantic_node {
        DeletionCondition::DeepestSemanticOwner(gap)
    } else {
        DeletionCondition::SyntaxOwner(gap)
    };
    add_nonclaims(
        nonclaims,
        &ALL_TARGETS[7..],
        scope,
        NonclaimReason::Syntax(gap),
        deletion,
    );
}

fn add_both_emit(nonclaims: &mut Vec<CapabilityNonclaim>, scope: CapabilityScope, gap: SyntaxGap) {
    add_syntax(nonclaims, &ALL_TARGETS[5..7], scope, gap);
}

fn add_javascript(nonclaims: &mut Vec<CapabilityNonclaim>, scope: CapabilityScope, gap: SyntaxGap) {
    add_syntax(nonclaims, &[CapabilityTarget::JavaScript], scope, gap);
}

fn add_dts(nonclaims: &mut Vec<CapabilityNonclaim>, scope: CapabilityScope, gap: SyntaxGap) {
    add_syntax(nonclaims, &[CapabilityTarget::Declaration], scope, gap);
}

fn add_syntax(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    targets: &[CapabilityTarget],
    scope: CapabilityScope,
    gap: SyntaxGap,
) {
    add_nonclaims(
        nonclaims,
        targets,
        scope,
        NonclaimReason::Syntax(gap),
        DeletionCondition::SyntaxOwner(gap),
    );
}

fn add_semantic(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    targets: &[CapabilityTarget],
    scope: CapabilityScope,
    gap: SemanticGap,
) {
    add_nonclaims(
        nonclaims,
        targets,
        scope,
        NonclaimReason::Semantic(gap),
        DeletionCondition::SemanticOwner(gap),
    );
}

fn add_nonclaims(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    targets: &[CapabilityTarget],
    scope: CapabilityScope,
    reason: NonclaimReason,
    deletion: DeletionCondition,
) {
    for &target in targets {
        nonclaims.push(CapabilityNonclaim {
            target,
            scope,
            reason,
            deletion,
        });
    }
}

pub(crate) fn is_declaration_source(path: &Path) -> bool {
    let path = path.to_string_lossy().to_ascii_lowercase();
    path.ends_with(".d.ts") || path.ends_with(".d.mts") || path.ends_with(".d.cts")
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
