use crate::bind::{DeclarationKind, Meaning, ScopeId};
use crate::source::{DeclId, FileId, NodeId, SourceKind, Span};
use crate::syntax::{
    AuthoredLiteralKind, CommentTrivia, DescendantContainer, ExpressionKind, ParserRecoveryKind,
    SourceCheckDirectiveKind, SourceSyntaxFact, Statement, StatementKind,
    UnmodeledDeclarationHostKind, for_each_statement_in,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::{
    CompilerOptions, JavaScriptAssignments, ProgramFile,
    has_unmodeled_no_substitution_template_program_products,
    numeric_literal::has_unmodeled_numeric_recovery_program_products,
    regular_expression::has_unmodeled_regular_expression_program_products,
    string_literal::has_unmodeled_extended_unicode_string_program_products,
};

mod flow_containment;
mod function_products;
use flow_containment::FileBoundary;

/// A compiler operation or externally visible product whose answer must be
/// either claimed or withheld for a specific scope.
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
    VariableDeclaratorTail,
    CommonJsClass,
    CommonJsNamespaceImportReexport,
    DeclarationHost,
    DefaultExportHost,
    Expression,
    ObjectMember,
    ForStatement,
    ComputedPropertyName,
    ClassExpression,
    FunctionExpressionClassPropertyTransform,
    FunctionExpressionCommonJsTransform,
    FunctionExpressionModifier,
    FunctionLikeBindingPattern,
    FunctionExpressionOuterComments,
    FunctionExpressionRecovery,
    FunctionLikePrinter,
    AngleAssertion,
    RejectedGenericArrowPrefix,
    Template,
    ExtendedUnicodeString,
    RegularExpression,
    NumericRecovery,
    NumericSeparator,
    TypeRecovery,
    UnicodeLineCommentTerminator,
    JavaScriptModuleFormat,
    ModuleClauseComment,
    DeclarationOverloadSummary,
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
    FunctionLikeService,
    ExplicitThisParameter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ProgramLiteralFamily {
    NoSubstitutionTemplate,
    ExtendedUnicodeString,
    RegularExpression,
    NumericRecovery,
    NumericSeparator,
}

/// Structural reason a target cannot yet publish a definitive answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum NonclaimReason {
    Syntax(SyntaxGap),
    Semantic(SemanticGap),
    ProgramLiteralBoundary(ProgramLiteralFamily),
    MissingEssentialTypes,
    FatalCompilerOption,
    CompilerOptionWithAuthoredLiteral,
}

/// Typed exit criterion for temporary nonclaims. This is deliberately not a
/// prose tag: reviews can enumerate which owner removes each record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum DeletionCondition {
    SyntaxOwner(SyntaxGap),
    /// Remove once the deepest parser-authored semantic producer owns this
    /// recovered syntax rather than deferring its dependency-closed demand.
    DeepestSemanticOwner(SyntaxGap),
    /// Remove once a recovered variable-list declarator has a first-class
    /// initializer owner instead of a represented assignment fragment.
    RecoveredDeclaratorInitializer(SyntaxGap),
    SemanticOwner(SemanticGap),
    LiteralProgramOwner(ProgramLiteralFamily),
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
    pub(crate) has_compiler_option_error: bool,
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
            let mode = match file.syntax.source_check_directive().map(|item| item.kind) {
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
        derive_program_literal_nonclaims(&mut nonclaims, files, options);

        if context.has_compiler_option_error
            && files.iter().any(|file| {
                file.syntax
                    .has_authored_literal(AuthoredLiteralKind::Template)
                    || file.syntax.has_authored_extended_unicode_string()
                    || file.syntax.has_authored_regular_expression()
                    || file
                        .syntax
                        .has_authored_literal(AuthoredLiteralKind::NumericRecovery)
                    || file
                        .syntax
                        .has_authored_literal(AuthoredLiteralKind::NumericSeparator)
            })
        {
            add_nonclaims(
                &mut nonclaims,
                &[CapabilityTarget::SemanticDiagnostics],
                CapabilityScope::Program,
                NonclaimReason::CompilerOptionWithAuthoredLiteral,
                DeletionCondition::CompilerOptionOwner,
            );
        }

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

    /// Whether a nonclaimed syntax-recovery container may still enter nested
    /// statements that carry their own stable capability identities. Direct
    /// names remain owned only by an identified recovered initializer or a
    /// typed flow region; broader program/file gaps never allow descent.
    pub(crate) fn semantic_check_node_allows_claimed_descendants(
        &self,
        file: FileId,
        owner: NodeId,
    ) -> bool {
        self.semantic_check_node_descendant_permissions(file, owner)
            .0
    }

    pub(crate) fn semantic_check_node_allows_recovery_identifiers(
        &self,
        file: FileId,
        owner: NodeId,
    ) -> bool {
        self.semantic_check_node_descendant_permissions(file, owner)
            .1
    }

    fn semantic_check_node_descendant_permissions(
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
        let (mut has_semantic_recovery, mut has_recovered_initializer) = (false, false);
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
                            | SyntaxGap::ExtendedUnicodeString
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
                DeletionCondition::RecoveredDeclaratorInitializer(_)
                    if reason.scope == requested_scope =>
                {
                    has_semantic_recovery = true;
                    has_recovered_initializer = true;
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
                && (has_recovered_initializer && !has_flow_region
                    || has_direct_identifier_recovery
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
                && matches!(
                    reason.deletion,
                    DeletionCondition::DeepestSemanticOwner(_)
                        | DeletionCondition::RecoveredDeclaratorInitializer(_)
                )
        })
    }

    pub(crate) fn semantic_declaration_is_claimed(
        &self,
        files: &[ProgramFile],
        declaration: DeclId,
    ) -> bool {
        declaration_scope(files, declaration, &self.function_like_owners).is_some_and(|scope| {
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

    pub(crate) fn semantic_diagnostics_are_claimed(&self) -> bool {
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
                        .all(|target| self.claim(target, scope).is_claimed())
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
                .filter_map(|declaration| {
                    declaration_scope(files, *declaration, function_like_owners)
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

fn declaration_scope(
    files: &[ProgramFile],
    declaration: DeclId,
    function_like_owners: &[(FileId, NodeId)],
) -> Option<CapabilityScope> {
    let file = files.get(declaration.file.0 as usize)?;
    let declaration = file.bindings.declaration(declaration)?;
    if matches!(
        declaration.kind,
        DeclarationKind::FunctionExpression | DeclarationKind::JavaScriptPropertyAssignment
    ) || declaration.kind == DeclarationKind::Parameter
        && function_like_owners
            .binary_search(&(file.source.id, declaration.owner))
            .is_ok()
    {
        return Some(CapabilityScope::node(file.source.id, declaration.owner));
    }
    let mut exact_owner = None;
    for_each_statement_in(&file.syntax.statements, &mut |statement| {
        if statement.id == declaration.owner {
            exact_owner = Some(statement.id);
        }
    });
    if let Some(owner) = exact_owner {
        return Some(CapabilityScope::node(file.source.id, owner));
    }
    file.syntax
        .statements
        .iter()
        .find(|statement| {
            statement.span.start <= declaration.name_span.start
                && declaration.name_span.end <= statement.span.end
        })
        .map(|statement| CapabilityScope::node(file.source.id, statement.id))
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
    let declaration_statement_owners = declaration_statement_owners(file);
    let javascript_jsdoc_casts = file
        .syntax
        .javascript_jsdoc_casts()
        .map(|(owner, _)| owner)
        .collect();
    let mut nodes = flow_containment::semantic_node_inventory(
        &file.syntax.statements,
        file.syntax.parser_recovery_facts(),
        &javascript_jsdoc_casts,
    );

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
    if file
        .syntax
        .has_source_syntax_fact(SourceSyntaxFact::ExplicitCallTypeArguments)
    {
        add_syntax(
            nonclaims,
            &[CapabilityTarget::Declaration],
            scope,
            SyntaxGap::Declaration,
        );
    }
    if !file.syntax.unmodeled_declaration_hosts().is_empty()
        || is_javascript && nodes.boundaries.contains(&FileBoundary::Declaration)
    {
        add_both_emit(nonclaims, scope, SyntaxGap::DeclarationHost);
        add_syntax(
            nonclaims,
            &[CapabilityTarget::RequiredType],
            scope,
            SyntaxGap::DeclarationHost,
        );
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
        add_both_emit(nonclaims, scope, SyntaxGap::Expression);
        add_semantic_diagnostics(nonclaims, scope, SyntaxGap::Expression);
    }
    if file
        .syntax
        .has_source_syntax_fact(SourceSyntaxFact::AuthoredFunctionExpressionModifier)
    {
        add_both_emit(nonclaims, scope, SyntaxGap::FunctionExpressionModifier);
    }
    let function_likes = std::mem::take(&mut nodes.function_likes);
    add_parser_recovery_semantic_nodes(
        nonclaims,
        file,
        &declaration_statement_owners,
        &nodes.function_like_signatures,
    );
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
        && target_requires_class_property_transform(&options.target)
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
    if file.syntax.has_unmodeled_template_products() {
        add_both_emit(nonclaims, scope, SyntaxGap::Template);
        add_literal_semantic_nodes(
            nonclaims,
            file,
            &declaration_statement_owners,
            AuthoredLiteralKind::Template,
            SyntaxGap::Template,
        );
    }
    if file.syntax.has_unmodeled_extended_unicode_string_products() {
        add_both_emit(nonclaims, scope, SyntaxGap::ExtendedUnicodeString);
        add_semantic_diagnostics(nonclaims, scope, SyntaxGap::ExtendedUnicodeString);
        add_service_nonclaims(nonclaims, scope, SyntaxGap::ExtendedUnicodeString, false);
    }
    if file.syntax.has_unmodeled_regular_expression_products() {
        add_both_emit(nonclaims, scope, SyntaxGap::RegularExpression);
    }
    let mut add_numeric_nonclaims = |present: bool, kind, gap| {
        if !present {
            return;
        }
        add_both_emit(nonclaims, scope, gap);
        if file.syntax.has_authored_literal(kind) {
            add_literal_semantic_nodes(nonclaims, file, &declaration_statement_owners, kind, gap);
        } else {
            add_semantic_diagnostics(nonclaims, scope, gap);
        }
    };
    add_numeric_nonclaims(
        file.syntax.has_unmodeled_numeric_recovery_products(),
        AuthoredLiteralKind::NumericRecovery,
        SyntaxGap::NumericRecovery,
    );
    add_numeric_nonclaims(
        file.syntax.has_unmodeled_numeric_separator_products(),
        AuthoredLiteralKind::NumericSeparator,
        SyntaxGap::NumericSeparator,
    );
    if file.syntax.has_unicode_line_comment_terminator() {
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

fn target_requires_class_property_transform(target: &str) -> bool {
    [
        "es3", "es5", "es6", "es2015", "es2016", "es2017", "es2018", "es2019", "es2020", "es2021",
    ]
    .iter()
    .any(|candidate| target.trim().eq_ignore_ascii_case(candidate))
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

fn add_parser_recovery_semantic_nodes(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    file: &ProgramFile,
    declaration_statement_owners: &BTreeSet<NodeId>,
    function_like_signatures: &[Span],
) {
    let recovered_declarator_initializers = recovered_declarator_initializer_owners(file);
    for recovery in file.syntax.parser_recovery_facts() {
        if recovery.kind != ParserRecoveryKind::GeneratorFunctionLike
            && function_like_signatures.iter().any(|signature| {
                signature.start <= recovery.authored_span.start
                    && recovery.authored_span.end <= signature.end
            })
        {
            continue;
        }
        let gap = match recovery.kind {
            ParserRecoveryKind::Declaration => SyntaxGap::Declaration,
            ParserRecoveryKind::GeneratorFunctionLike => SyntaxGap::GeneratorFunctionLike,
            ParserRecoveryKind::VariableDeclaratorTail => {
                let gap = SyntaxGap::VariableDeclaratorTail;
                add_both_emit(nonclaims, CapabilityScope::File(file.source.id), gap);
                continue;
            }
            ParserRecoveryKind::Expression => SyntaxGap::Expression,
            ParserRecoveryKind::ObjectMember => SyntaxGap::ObjectMember,
            ParserRecoveryKind::ForStatement => SyntaxGap::ForStatement,
            ParserRecoveryKind::ComputedPropertyName => SyntaxGap::ComputedPropertyName,
            ParserRecoveryKind::ClassExpression => SyntaxGap::ClassExpression,
            ParserRecoveryKind::AngleAssertion => SyntaxGap::AngleAssertion,
            ParserRecoveryKind::RejectedGenericArrowPrefix => SyntaxGap::RejectedGenericArrowPrefix,
            ParserRecoveryKind::Type => SyntaxGap::TypeRecovery,
            ParserRecoveryKind::Template => SyntaxGap::Template,
        };
        if matches!(
            recovery.kind,
            ParserRecoveryKind::ObjectMember
                | ParserRecoveryKind::ForStatement
                | ParserRecoveryKind::ComputedPropertyName
                | ParserRecoveryKind::ClassExpression
                | ParserRecoveryKind::AngleAssertion
        ) {
            add_both_emit(nonclaims, CapabilityScope::File(file.source.id), gap);
        }
        if recovery.kind == ParserRecoveryKind::RejectedGenericArrowPrefix {
            let scope = CapabilityScope::node(file.source.id, recovery.owner.statement);
            add_recovery_owner_nonclaims(
                nonclaims,
                scope,
                gap,
                RecoveryStatementRole::SemanticOwner,
                DeletionCondition::DeepestSemanticOwner(gap),
            );
            add_nonclaims(
                nonclaims,
                &ALL_TARGETS[1..2],
                scope,
                NonclaimReason::Syntax(gap),
                DeletionCondition::DeepestSemanticOwner(gap),
            );
            add_both_emit(nonclaims, scope, gap);
            continue;
        }
        for (owner, role) in recovery_statement_owners(
            file,
            recovery.owner,
            recovery.authored_span,
            recovery.recovery_extent,
            RecoveryStatementSource::Parser {
                recovered_declarator_initializers: &recovered_declarator_initializers,
            },
        ) {
            let scope = CapabilityScope::node(file.source.id, owner);
            add_recovery_owner_nonclaims(
                nonclaims,
                scope,
                gap,
                role,
                DeletionCondition::RecoveredDeclaratorInitializer(gap),
            );
            add_recovery_fragment_declaration_nonclaims(
                nonclaims,
                declaration_statement_owners,
                file.source.id,
                owner,
                gap,
                role,
            );
        }
        add_declaration_semantic_nonclaims(
            nonclaims,
            file,
            recovery.owner.statement,
            recovery.authored_span,
            gap,
        );
    }
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
    let mut unmapped = file.syntax.unmodeled_declaration_hosts().is_empty();
    for host in file.syntax.unmodeled_declaration_hosts() {
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

fn derive_program_literal_nonclaims(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    files: &[ProgramFile],
    options: &CompilerOptions,
) {
    if files
        .iter()
        .any(|file| file.syntax.has_unicode_line_comment_terminator())
    {
        add_both_emit(
            nonclaims,
            CapabilityScope::Program,
            SyntaxGap::UnicodeLineCommentTerminator,
        );
    }
    let families = [
        (
            ProgramLiteralFamily::NoSubstitutionTemplate,
            has_unmodeled_no_substitution_template_program_products(files, options),
        ),
        (
            ProgramLiteralFamily::RegularExpression,
            has_unmodeled_regular_expression_program_products(files, options),
        ),
        (
            ProgramLiteralFamily::ExtendedUnicodeString,
            has_unmodeled_extended_unicode_string_program_products(files, options),
        ),
        (
            ProgramLiteralFamily::NumericRecovery,
            has_unmodeled_numeric_recovery_program_products(files, options),
        ),
    ];
    for (family, unclaimed) in families {
        if unclaimed {
            add_program_emit(nonclaims, family);
        }
    }
    if files.iter().any(|file| {
        file.syntax
            .has_authored_literal(AuthoredLiteralKind::NumericSeparator)
            && file.syntax.has_unmodeled_numeric_separator_products()
    }) {
        add_program_emit(nonclaims, ProgramLiteralFamily::NumericSeparator);
    }

    let overload_files = declaration_overload_files(files);
    for id in overload_files {
        add_syntax(
            nonclaims,
            &[CapabilityTarget::Declaration],
            CapabilityScope::File(id),
            SyntaxGap::DeclarationOverloadSummary,
        );
        let file = &files[id.0 as usize];
        if is_effective_commonjs(&file.source.path, &options.module) {
            add_javascript(
                nonclaims,
                CapabilityScope::File(id),
                SyntaxGap::DeclarationOverloadSummary,
            );
        }
    }
}

fn declaration_overload_files(files: &[ProgramFile]) -> BTreeSet<FileId> {
    let mut incomplete = files
        .iter()
        .filter(|file| file.syntax.has_local_unmodeled_declaration_overloads())
        .map(|file| file.source.id)
        .collect::<BTreeSet<_>>();
    let mut global_functions =
        std::collections::BTreeMap::<String, (bool, bool, BTreeSet<FileId>)>::new();
    for file in files {
        if file.is_external_module() {
            continue;
        }
        for statement in &file.syntax.statements {
            let crate::syntax::StatementKind::Function(declaration) = &statement.kind else {
                continue;
            };
            let group = global_functions
                .entry(declaration.name.clone())
                .or_default();
            group.0 |= !declaration.has_body;
            group.1 |= declaration.has_body;
            group.2.insert(file.source.id);
        }
    }
    for (_, (has_signature, has_body, sources)) in global_functions {
        if has_signature && has_body {
            incomplete.extend(sources);
        }
    }
    incomplete
}

fn add_program_emit(nonclaims: &mut Vec<CapabilityNonclaim>, family: ProgramLiteralFamily) {
    add_nonclaims(
        nonclaims,
        &ALL_TARGETS[5..7],
        CapabilityScope::Program,
        NonclaimReason::ProgramLiteralBoundary(family),
        DeletionCondition::LiteralProgramOwner(family),
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

fn add_literal_semantic_nodes(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    file: &ProgramFile,
    declaration_statement_owners: &BTreeSet<NodeId>,
    kind: AuthoredLiteralKind,
    gap: SyntaxGap,
) {
    for fact in file
        .syntax
        .authored_literal_facts()
        .iter()
        .filter(|fact| fact.kind == kind)
    {
        for (owner, role) in recovery_statement_owners(
            file,
            fact.owner,
            fact.span,
            fact.recovery_extent,
            RecoveryStatementSource::Literal,
        ) {
            let scope = CapabilityScope::node(file.source.id, owner);
            add_recovery_owner_nonclaims(
                nonclaims,
                scope,
                gap,
                role,
                DeletionCondition::DeepestSemanticOwner(gap),
            );
            add_recovery_fragment_declaration_nonclaims(
                nonclaims,
                declaration_statement_owners,
                file.source.id,
                owner,
                gap,
                role,
            );
        }
        add_declaration_semantic_nonclaims(nonclaims, file, fact.owner.statement, fact.span, gap);
    }
}

fn add_declaration_semantic_nonclaims(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    file: &ProgramFile,
    semantic_owner: NodeId,
    authored_span: Span,
    gap: SyntaxGap,
) {
    let mut owner_is_return = false;
    for_each_statement_in(&file.syntax.statements, &mut |statement| {
        owner_is_return |=
            statement.id == semantic_owner && matches!(statement.kind, StatementKind::Return(_));
    });
    let mut declaration_owner = None;
    for_each_statement_in(&file.syntax.statements, &mut |statement| {
        if statement.span.start <= authored_span.start
            && authored_span.end <= statement.span.end
            && (statement.id == semantic_owner || owner_is_return)
            && matches!(
                statement.kind,
                StatementKind::Import(_)
                    | StatementKind::Variable(_)
                    | StatementKind::Function(_)
                    | StatementKind::Class(_)
                    | StatementKind::TypeAlias(_)
                    | StatementKind::Interface(_)
            )
            && declaration_owner.is_none_or(|(width, _)| statement.span.len() < width)
        {
            declaration_owner = Some((statement.span.len(), statement.id));
        }
    });
    let Some((_, owner)) = declaration_owner else {
        return;
    };
    let scope = CapabilityScope::node(file.source.id, owner);
    let targets = if gap == SyntaxGap::GeneratorFunctionLike {
        &ALL_TARGETS[1..2]
    } else {
        &ALL_TARGETS[..2]
    };
    add_nonclaims(
        nonclaims,
        targets,
        scope,
        NonclaimReason::Syntax(gap),
        DeletionCondition::DeepestSemanticOwner(gap),
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoveryStatementRole {
    SemanticOwner,
    RecoveredDeclaratorInitializer,
    RepresentationalFragment,
}

#[derive(Clone, Copy)]
enum RecoveryStatementSource<'a> {
    Parser {
        recovered_declarator_initializers: &'a BTreeSet<NodeId>,
    },
    Literal,
}

fn recovered_declarator_initializer_owners(file: &ProgramFile) -> BTreeSet<NodeId> {
    let mut owners = BTreeSet::new();
    for recovery in file
        .syntax
        .parser_recovery_facts()
        .iter()
        .filter(|recovery| recovery.kind == ParserRecoveryKind::Declaration)
    {
        let mut recovered_binding_spans = Vec::new();
        for_each_statement_in(&file.syntax.statements, &mut |candidate| {
            if candidate.id == recovery.owner.statement
                && let StatementKind::Variable(declaration) = &candidate.kind
            {
                recovered_binding_spans.extend(
                    declaration
                        .recovered_binding_names
                        .iter()
                        .map(|binding| binding.span),
                );
            }
        });
        if recovered_binding_spans.is_empty() {
            continue;
        }
        for_each_statement_in(&file.syntax.statements, &mut |candidate| {
            if recovery.recovery_extent.start <= candidate.span.start
                && candidate.span.start < recovery.recovery_extent.end
                && let StatementKind::Expression(expression) = &candidate.kind
                && let ExpressionKind::Assignment { left, .. } = &expression.kind
                && recovered_binding_spans.contains(&left.span)
            {
                owners.insert(candidate.id);
            }
        });
    }
    owners
}

fn recovery_statement_owners(
    file: &ProgramFile,
    owner: crate::syntax::ParserRecoveryOwner,
    authored_span: Span,
    recovery_extent: Span,
    source: RecoveryStatementSource<'_>,
) -> BTreeMap<NodeId, RecoveryStatementRole> {
    debug_assert!(
        file.syntax
            .statements
            .iter()
            .any(|statement| statement.id == owner.root_statement)
    );

    let mut owner_subtree = BTreeSet::new();
    let mut absorbed_ancestor = None;
    for_each_recovery_statement(&file.syntax.statements, authored_span, &mut |candidate| {
        if candidate.id == owner.statement {
            for_each_recovery_statement(
                std::slice::from_ref(candidate),
                authored_span,
                &mut |descendant| {
                    owner_subtree.insert(descendant.id);
                },
            );
        } else if candidate.span.start < recovery_extent.start
            && candidate.span.end <= recovery_extent.end
        {
            let mut contains_owner = false;
            for_each_recovery_statement(
                std::slice::from_ref(candidate),
                authored_span,
                &mut |descendant| {
                    contains_owner |= descendant.id == owner.statement;
                },
            );
            if contains_owner
                && absorbed_ancestor.is_none_or(|(width, _)| candidate.span.len() < width)
            {
                absorbed_ancestor = Some((candidate.span.len(), candidate.id));
            }
        }
    });

    let mut owners = BTreeMap::from([(owner.statement, RecoveryStatementRole::SemanticOwner)]);
    if let Some((_, ancestor)) = absorbed_ancestor {
        owners.insert(ancestor, RecoveryStatementRole::SemanticOwner);
    }
    for_each_recovery_statement(&file.syntax.statements, authored_span, &mut |statement| {
        if recovery_extent.start <= statement.span.start
            && statement.span.start < recovery_extent.end
        {
            let role = match source {
                RecoveryStatementSource::Parser {
                    recovered_declarator_initializers,
                    ..
                } if recovered_declarator_initializers.contains(&statement.id) => {
                    RecoveryStatementRole::RecoveredDeclaratorInitializer
                }
                _ if owner_subtree.contains(&statement.id) => RecoveryStatementRole::SemanticOwner,
                _ => RecoveryStatementRole::RepresentationalFragment,
            };
            owners.insert(statement.id, role);
        }
    });
    owners
}

fn for_each_recovery_statement<'ast>(
    statements: &'ast [Statement],
    authored_span: Span,
    visit: &mut impl FnMut(&'ast Statement),
) {
    let owns_authored =
        |span: Span| span.start <= authored_span.start && authored_span.end <= span.end;
    let is_independent_owner = |statement: &Statement| {
        matches!(
            &statement.kind,
            StatementKind::Function(_) | StatementKind::Class(_)
        ) && !owns_authored(statement.span)
    };
    for statement in statements {
        if is_independent_owner(statement) {
            continue;
        }
        statement.for_each_statement_where(
            &mut |container| match container {
                DescendantContainer::Statement(statement) => !is_independent_owner(statement),
                DescendantContainer::Function(statement, _)
                | DescendantContainer::Class(statement, _) => owns_authored(statement.span),
                DescendantContainer::ClassMember(member) => owns_authored(member.span),
                DescendantContainer::FunctionLike(expression, _) => owns_authored(expression.span),
            },
            visit,
        );
    }
}

fn add_recovery_fragment_declaration_nonclaims(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    declaration_statement_owners: &BTreeSet<NodeId>,
    file: FileId,
    owner: NodeId,
    gap: SyntaxGap,
    role: RecoveryStatementRole,
) {
    if role != RecoveryStatementRole::RepresentationalFragment
        || !declaration_statement_owners.contains(&owner)
    {
        return;
    }
    add_syntax(
        nonclaims,
        &ALL_TARGETS[..2],
        CapabilityScope::node(file, owner),
        gap,
    );
}

fn declaration_statement_owners(file: &ProgramFile) -> BTreeSet<NodeId> {
    let mut owners = BTreeSet::new();
    for_each_statement_in(&file.syntax.statements, &mut |statement| {
        if matches!(
            statement.kind,
            StatementKind::Import(_)
                | StatementKind::Variable(_)
                | StatementKind::Function(_)
                | StatementKind::Class(_)
                | StatementKind::TypeAlias(_)
                | StatementKind::Interface(_)
        ) {
            owners.insert(statement.id);
        }
    });
    owners
}

fn add_recovery_owner_nonclaims(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    scope: CapabilityScope,
    gap: SyntaxGap,
    role: RecoveryStatementRole,
    recovered_declarator_deletion: DeletionCondition,
) {
    let semantic_node = role != RecoveryStatementRole::RepresentationalFragment;
    let deletion = match role {
        RecoveryStatementRole::SemanticOwner => DeletionCondition::DeepestSemanticOwner(gap),
        RecoveryStatementRole::RecoveredDeclaratorInitializer => match gap {
            SyntaxGap::GeneratorFunctionLike => DeletionCondition::DeepestSemanticOwner(gap),
            _ => recovered_declarator_deletion,
        },
        RecoveryStatementRole::RepresentationalFragment => DeletionCondition::SyntaxOwner(gap),
    };
    let targets = if gap == SyntaxGap::TypeRecovery {
        &ALL_TARGETS[2..6]
    } else {
        &ALL_TARGETS[2..5]
    };
    add_nonclaims(
        nonclaims,
        targets,
        scope,
        NonclaimReason::Syntax(gap),
        deletion,
    );
    add_service_nonclaims(nonclaims, scope, gap, semantic_node);
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
