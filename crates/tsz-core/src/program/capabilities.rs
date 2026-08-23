use crate::bind::Meaning;
use crate::source::{DeclId, FileId, NodeId, Span};
use crate::syntax::{
    AuthoredLiteralKind, ExpressionKind, ParserRecoveryKind, SourceCheckDirectiveKind,
    StatementKind, UnmodeledDeclarationHostKind,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::{
    CompilerOptions, ProgramFile, has_unmodeled_no_substitution_template_program_products,
    numeric_literal::has_unmodeled_numeric_recovery_program_products,
    regular_expression::has_unmodeled_regular_expression_program_products,
    string_literal::has_unmodeled_extended_unicode_string_program_products,
};

mod flow_containment;

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
    CommonJsClass,
    DeclarationHost,
    DefaultExportHost,
    Expression,
    Template,
    ExtendedUnicodeString,
    RegularExpression,
    NumericRecovery,
    NumericSeparator,
    TypeRecovery,
    UnicodeLineCommentTerminator,
    JavaScriptModuleFormat,
    DeclarationOverloadSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SemanticGap {
    FlowTypeOfReference,
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
    UncheckedBySourceDirective,
}

/// One immutable capability decision set for a parsed/bound program and its
/// normalized compiler-option snapshot.
#[derive(Debug, Clone, Default)]
pub(crate) struct CapabilityAnalysis {
    nonclaims: Box<[CapabilityNonclaim]>,
    file_semantic_modes: Box<[FileSemanticMode]>,
}

impl CapabilityAnalysis {
    pub(crate) fn derive(
        files: &[ProgramFile],
        options: &CompilerOptions,
        context: CapabilityContext,
    ) -> Self {
        let mut nonclaims = Vec::new();
        let mut file_semantic_modes = vec![FileSemanticMode::Checked; files.len()];
        for file in files {
            let mode = match file
                .syntax
                .source_check_directive()
                .map(|directive| directive.kind)
            {
                Some(SourceCheckDirectiveKind::NoCheck) => {
                    FileSemanticMode::UncheckedBySourceDirective
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
            for target in [
                CapabilityTarget::SemanticCheck,
                CapabilityTarget::DeclarationModel,
                CapabilityTarget::DeclarationValue,
            ] {
                push(
                    &mut nonclaims,
                    target,
                    CapabilityScope::Program,
                    NonclaimReason::MissingEssentialTypes,
                    DeletionCondition::EssentialLibraryUniverse,
                );
            }
        }
        if context.has_fatal_option_error {
            for target in [
                CapabilityTarget::SemanticCheck,
                CapabilityTarget::DeclarationModel,
                CapabilityTarget::DeclarationValue,
                CapabilityTarget::RequiredType,
                CapabilityTarget::SemanticDiagnostics,
                CapabilityTarget::JavaScript,
                CapabilityTarget::Declaration,
                CapabilityTarget::QuickInfo,
                CapabilityTarget::Definition,
                CapabilityTarget::References,
                CapabilityTarget::Highlights,
                CapabilityTarget::Rename,
            ] {
                push(
                    &mut nonclaims,
                    target,
                    CapabilityScope::Program,
                    NonclaimReason::FatalCompilerOption,
                    DeletionCondition::CompilerOptionOwner,
                );
            }
            nonclaims.sort_unstable();
            nonclaims.dedup();
            return Self {
                nonclaims: nonclaims.into_boxed_slice(),
                file_semantic_modes: file_semantic_modes.into_boxed_slice(),
            };
        }

        for file in files {
            derive_file_nonclaims(&mut nonclaims, file, options);
        }
        derive_program_literal_nonclaims(&mut nonclaims, files, options);

        if context.has_compiler_option_error
            && files.iter().any(|file| {
                file.syntax.has_authored_no_substitution_template()
                    || file.syntax.has_authored_extended_unicode_string()
                    || file.syntax.has_authored_regular_expression()
                    || file.syntax.has_authored_numeric_recovery()
                    || file.syntax.has_authored_numeric_separator()
            })
        {
            push(
                &mut nonclaims,
                CapabilityTarget::SemanticDiagnostics,
                CapabilityScope::Program,
                NonclaimReason::CompilerOptionWithAuthoredLiteral,
                DeletionCondition::CompilerOptionOwner,
            );
        }

        close_declaration_groups(&mut nonclaims, files);

        nonclaims.sort_unstable();
        nonclaims.dedup();
        Self {
            nonclaims: nonclaims.into_boxed_slice(),
            file_semantic_modes: file_semantic_modes.into_boxed_slice(),
        }
    }

    pub(crate) fn claim(
        &self,
        target: CapabilityTarget,
        scope: CapabilityScope,
    ) -> CapabilityClaim<'_> {
        let reasons = CapabilityReasons {
            analysis: self,
            target,
            scope,
            index: 0,
        };
        if reasons.clone().next().is_some() {
            CapabilityClaim::Nonclaimed(reasons)
        } else {
            CapabilityClaim::Claimed
        }
    }

    pub(crate) fn semantic_check_node_is_claimed(&self, file: FileId, owner: NodeId) -> bool {
        self.claim(
            CapabilityTarget::SemanticCheck,
            CapabilityScope::node(file, owner),
        )
        .is_claimed()
    }

    pub(crate) fn semantic_check_file_is_enabled(&self, file: FileId) -> bool {
        !matches!(
            self.file_semantic_modes.get(file.0 as usize),
            Some(FileSemanticMode::UncheckedBySourceDirective)
        )
    }

    /// Whether a nonclaimed syntax-recovery container may still enter nested
    /// statements that carry their own stable capability identities. A local
    /// flow region may accompany that recovery because every child rechecks
    /// its own typed claim. Representational recovery fragments may expose
    /// name-only descendants only when that same node is already contained by
    /// the typed flow region; broader program/file gaps never allow descent.
    pub(crate) fn semantic_check_node_allows_claimed_descendants(
        &self,
        file: FileId,
        owner: NodeId,
    ) -> bool {
        let CapabilityClaim::Nonclaimed(reasons) = self.claim(
            CapabilityTarget::SemanticCheck,
            CapabilityScope::node(file, owner),
        ) else {
            return false;
        };
        let requested_scope = CapabilityScope::node(file, owner);
        let mut has_semantic_recovery = false;
        let mut has_representational_recovery = false;
        let mut has_flow_region = false;
        for reason in reasons {
            match reason.deletion {
                DeletionCondition::DeepestSemanticOwner(_) if reason.scope == requested_scope => {
                    has_semantic_recovery = true;
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
                _ => return false,
            }
        }
        has_semantic_recovery && !has_representational_recovery
            || has_flow_region && (has_semantic_recovery || has_representational_recovery)
    }

    /// Whether a nonclaimed flow-region host may still run independently inventoried
    /// function-like expression semantics; broader recovery cannot publish a signature.
    pub(crate) fn semantic_check_node_allows_function_like_expression_semantics(
        &self,
        file: FileId,
        owner: NodeId,
    ) -> bool {
        let scope = CapabilityScope::node(file, owner);
        let CapabilityClaim::Nonclaimed(mut reasons) =
            self.claim(CapabilityTarget::SemanticCheck, scope)
        else {
            return false;
        };
        let flow = DeletionCondition::SemanticOwner(SemanticGap::FlowTypeOfReference);
        reasons.all(|reason| reason.scope == scope && reason.deletion == flow)
    }

    pub(crate) fn semantic_declaration_is_claimed(
        &self,
        files: &[ProgramFile],
        declaration: DeclId,
    ) -> bool {
        declaration_capability_scope(files, declaration).is_some_and(|scope| {
            self.claim(CapabilityTarget::DeclarationValue, scope)
                .is_claimed()
        })
    }

    pub(crate) fn semantic_diagnostics_file_is_claimed(&self, file: FileId) -> bool {
        !self.nonclaims.iter().any(|nonclaim| {
            nonclaim.target == CapabilityTarget::SemanticDiagnostics
                && scope_applies(nonclaim.scope, CapabilityScope::File(file))
                && (matches!(nonclaim.scope, CapabilityScope::Program)
                    || self.semantic_check_file_is_enabled(file))
        })
    }

    pub(crate) fn semantic_diagnostics_are_claimed(&self) -> bool {
        !self.nonclaims.iter().any(|nonclaim| {
            nonclaim.target == CapabilityTarget::SemanticDiagnostics
                && match nonclaim.scope {
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
        if options.no_emit {
            return true;
        }
        files
            .iter()
            .filter(|file| !is_declaration_source(&file.source.path))
            .all(|file| {
                self.claim(
                    CapabilityTarget::JavaScript,
                    CapabilityScope::File(file.source.id),
                )
                .is_claimed()
                    && (!options.declaration
                        || self
                            .claim(
                                CapabilityTarget::Declaration,
                                CapabilityScope::File(file.source.id),
                            )
                            .is_claimed())
            })
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
    analysis: &'a CapabilityAnalysis,
    target: CapabilityTarget,
    scope: CapabilityScope,
    index: usize,
}

impl<'a> Iterator for CapabilityReasons<'a> {
    type Item = &'a CapabilityNonclaim;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(nonclaim) = self.analysis.nonclaims.get(self.index) {
            self.index += 1;
            if nonclaim.target == self.target && scope_applies(nonclaim.scope, self.scope) {
                return Some(nonclaim);
            }
        }
        None
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

fn close_declaration_groups(nonclaims: &mut Vec<CapabilityNonclaim>, files: &[ProgramFile]) {
    let groups = declaration_groups(files);
    let targets = [
        CapabilityTarget::SemanticCheck,
        CapabilityTarget::DeclarationModel,
        CapabilityTarget::DeclarationValue,
        CapabilityTarget::RequiredType,
        CapabilityTarget::SemanticDiagnostics,
        CapabilityTarget::QuickInfo,
        CapabilityTarget::Definition,
        CapabilityTarget::References,
        CapabilityTarget::Highlights,
        CapabilityTarget::Rename,
    ];
    loop {
        let before = nonclaims.len();
        for group in &groups {
            let scopes = group
                .iter()
                .filter_map(|declaration| declaration_capability_scope(files, *declaration))
                .collect::<BTreeSet<_>>();
            for target in targets {
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
                        push(nonclaims, target, *scope, *reason, *deletion);
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
                for target in [
                    CapabilityTarget::QuickInfo,
                    CapabilityTarget::Definition,
                    CapabilityTarget::References,
                    CapabilityTarget::Highlights,
                    CapabilityTarget::Rename,
                ] {
                    for (reason, deletion) in &declaration_reasons {
                        push(nonclaims, target, *scope, *reason, *deletion);
                    }
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
                                .is_some_and(|declaration| declaration.meaning == meaning)
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

fn declaration_capability_scope(
    files: &[ProgramFile],
    declaration: DeclId,
) -> Option<CapabilityScope> {
    let file = files.get(declaration.file.0 as usize)?;
    let declaration = file.bindings.declaration(declaration)?;
    let mut exact_owner = None;
    for statement in &file.syntax.statements {
        statement.for_each_statement(&mut |statement| {
            if statement.id == declaration.owner {
                exact_owner = Some(statement.id);
            }
        });
    }
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

fn derive_file_nonclaims(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    file: &ProgramFile,
    options: &CompilerOptions,
) {
    let id = file.source.id;
    let scope = CapabilityScope::File(id);
    let declaration_statement_owners = declaration_statement_owners(file);

    if file.syntax.has_unmodeled_function_products() {
        add_both_emit(nonclaims, scope, SyntaxGap::Function);
    }
    if file.syntax.has_unmodeled_class_products() {
        add_both_emit(nonclaims, scope, SyntaxGap::Class);
    }
    if file.syntax.has_unmodeled_declaration_products() {
        add_syntax(
            nonclaims,
            CapabilityTarget::Declaration,
            scope,
            SyntaxGap::Declaration,
        );
    }
    if file.syntax.has_unmodeled_declaration_hosts() {
        add_both_emit(nonclaims, scope, SyntaxGap::DeclarationHost);
        add_syntax(
            nonclaims,
            CapabilityTarget::RequiredType,
            scope,
            SyntaxGap::DeclarationHost,
        );
        add_unmodeled_declaration_host_nodes(nonclaims, file);
    }
    if file.syntax.has_unmodeled_default_export_hosts() {
        add_both_emit(nonclaims, scope, SyntaxGap::DefaultExportHost);
        add_semantic_diagnostics(nonclaims, scope, SyntaxGap::DefaultExportHost);
    }
    if file.syntax.has_unmodeled_expression_products() {
        add_both_emit(nonclaims, scope, SyntaxGap::Expression);
        add_semantic_diagnostics(nonclaims, scope, SyntaxGap::Expression);
    }
    add_parser_recovery_semantic_nodes(nonclaims, file, &declaration_statement_owners);
    for owner in flow_containment::flow_region_nodes(
        &file.syntax.statements,
        file.syntax.parser_recovery_facts(),
    ) {
        add_flow_region_nonclaims(nonclaims, CapabilityScope::node(id, owner));
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
    if file.syntax.has_unmodeled_numeric_recovery_products() {
        add_both_emit(nonclaims, scope, SyntaxGap::NumericRecovery);
        if file.syntax.has_authored_numeric_recovery() {
            add_literal_semantic_nodes(
                nonclaims,
                file,
                &declaration_statement_owners,
                AuthoredLiteralKind::NumericRecovery,
                SyntaxGap::NumericRecovery,
            );
        } else {
            add_semantic_diagnostics(nonclaims, scope, SyntaxGap::NumericRecovery);
        }
    }
    if file.syntax.has_unmodeled_numeric_separator_products() {
        add_both_emit(nonclaims, scope, SyntaxGap::NumericSeparator);
        if file.syntax.has_authored_numeric_separator() {
            add_literal_semantic_nodes(
                nonclaims,
                file,
                &declaration_statement_owners,
                AuthoredLiteralKind::NumericSeparator,
                SyntaxGap::NumericSeparator,
            );
        } else {
            add_semantic_diagnostics(nonclaims, scope, SyntaxGap::NumericSeparator);
        }
    }
    if file.syntax.has_unicode_line_comment_terminator() {
        add_semantic_diagnostics(nonclaims, scope, SyntaxGap::UnicodeLineCommentTerminator);
    }
    if file.has_unmodeled_javascript_module_products() {
        add_both_emit(nonclaims, scope, SyntaxGap::JavaScriptModuleFormat);
    }
    if is_effective_commonjs(&file.source.path, &options.module)
        && file.syntax.has_unmodeled_commonjs_class_products()
    {
        add_both_emit(nonclaims, scope, SyntaxGap::CommonJsClass);
    }
}

fn add_parser_recovery_semantic_nodes(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    file: &ProgramFile,
    declaration_statement_owners: &BTreeSet<NodeId>,
) {
    let recovered_declarator_initializers = recovered_declarator_initializer_owners(file);
    for recovery in file.syntax.parser_recovery_facts() {
        let gap = match recovery.kind {
            ParserRecoveryKind::Declaration => SyntaxGap::Declaration,
            ParserRecoveryKind::Expression => SyntaxGap::Expression,
            ParserRecoveryKind::Type => SyntaxGap::TypeRecovery,
            ParserRecoveryKind::Template => SyntaxGap::Template,
        };
        for (owner, role) in recovery_statement_owners(
            file,
            recovery.owner,
            recovery.recovery_extent,
            RecoveryStatementSource::Parser {
                recovered_declarator_initializers: &recovered_declarator_initializers,
            },
        ) {
            let scope = CapabilityScope::node(file.source.id, owner);
            for target in [
                CapabilityTarget::SemanticCheck,
                CapabilityTarget::RequiredType,
                CapabilityTarget::SemanticDiagnostics,
            ] {
                match role {
                    RecoveryStatementRole::SemanticOwner
                    | RecoveryStatementRole::RecoveredDeclaratorInitializer => {
                        add_semantic_node_syntax(nonclaims, target, scope, gap);
                    }
                    RecoveryStatementRole::RepresentationalFragment => {
                        add_syntax(nonclaims, target, scope, gap);
                    }
                }
            }
            add_service_nonclaims(
                nonclaims,
                scope,
                gap,
                matches!(
                    role,
                    RecoveryStatementRole::SemanticOwner
                        | RecoveryStatementRole::RecoveredDeclaratorInitializer
                ),
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

fn add_flow_region_nonclaims(nonclaims: &mut Vec<CapabilityNonclaim>, scope: CapabilityScope) {
    let gap = SemanticGap::FlowTypeOfReference;
    for target in [
        CapabilityTarget::SemanticCheck,
        CapabilityTarget::DeclarationValue,
        CapabilityTarget::SemanticDiagnostics,
    ] {
        push(
            nonclaims,
            target,
            scope,
            NonclaimReason::Semantic(gap),
            DeletionCondition::SemanticOwner(gap),
        );
    }
}

fn add_unmodeled_declaration_host_nodes(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    file: &ProgramFile,
) {
    let mut found = false;
    let mut unmapped = file.syntax.unmodeled_declaration_hosts().is_empty();
    for host in file.syntax.unmodeled_declaration_hosts() {
        if host.kind == UnmodeledDeclarationHostKind::Global {
            for target in [
                CapabilityTarget::SemanticCheck,
                CapabilityTarget::DeclarationModel,
                CapabilityTarget::DeclarationValue,
                CapabilityTarget::RequiredType,
                CapabilityTarget::SemanticDiagnostics,
            ] {
                add_syntax(
                    nonclaims,
                    target,
                    CapabilityScope::Program,
                    SyntaxGap::DeclarationHost,
                );
            }
            add_service_nonclaims(
                nonclaims,
                CapabilityScope::Program,
                SyntaxGap::DeclarationHost,
                false,
            );
            continue;
        }
        let mut owners = BTreeSet::new();
        for root in &file.syntax.statements {
            root.for_each_statement(&mut |statement| {
                if statement.span.start == host.owner_start
                    || host.recovery_extent.start <= statement.span.start
                        && statement.span.start < host.recovery_extent.end
                {
                    owners.insert(statement.id);
                }
            });
        }
        if owners.is_empty() {
            unmapped = true;
            continue;
        }
        found = true;
        for owner in owners {
            let owner_scope = CapabilityScope::node(file.source.id, owner);
            for target in [
                CapabilityTarget::SemanticCheck,
                CapabilityTarget::DeclarationModel,
                CapabilityTarget::DeclarationValue,
                CapabilityTarget::RequiredType,
                CapabilityTarget::SemanticDiagnostics,
            ] {
                add_syntax(nonclaims, target, owner_scope, SyntaxGap::DeclarationHost);
            }
            add_service_nonclaims(nonclaims, owner_scope, SyntaxGap::DeclarationHost, false);
        }
    }
    if !found || unmapped {
        let scope = CapabilityScope::File(file.source.id);
        for target in [
            CapabilityTarget::SemanticCheck,
            CapabilityTarget::DeclarationModel,
            CapabilityTarget::DeclarationValue,
            CapabilityTarget::RequiredType,
            CapabilityTarget::SemanticDiagnostics,
        ] {
            add_syntax(nonclaims, target, scope, SyntaxGap::DeclarationHost);
        }
        add_service_nonclaims(nonclaims, scope, SyntaxGap::DeclarationHost, false);
    }
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
        add_program_syntax_emit(nonclaims, SyntaxGap::UnicodeLineCommentTerminator);
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
            add_program_literal(nonclaims, family);
        }
    }
    if files.iter().any(|file| {
        file.syntax.has_authored_numeric_separator()
            && file.syntax.has_unmodeled_numeric_separator_products()
    }) {
        add_program_emit(nonclaims, ProgramLiteralFamily::NumericSeparator);
    }

    let overload_files = declaration_overload_files(files);
    for id in overload_files {
        add_syntax(
            nonclaims,
            CapabilityTarget::Declaration,
            CapabilityScope::File(id),
            SyntaxGap::DeclarationOverloadSummary,
        );
        let file = &files[id.0 as usize];
        if is_effective_commonjs(&file.source.path, &options.module) {
            add_syntax(
                nonclaims,
                CapabilityTarget::JavaScript,
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

fn add_program_literal(nonclaims: &mut Vec<CapabilityNonclaim>, family: ProgramLiteralFamily) {
    add_program_emit(nonclaims, family);
}

fn add_program_syntax_emit(nonclaims: &mut Vec<CapabilityNonclaim>, gap: SyntaxGap) {
    for target in [CapabilityTarget::JavaScript, CapabilityTarget::Declaration] {
        add_syntax(nonclaims, target, CapabilityScope::Program, gap);
    }
}

fn add_program_emit(nonclaims: &mut Vec<CapabilityNonclaim>, family: ProgramLiteralFamily) {
    for target in [CapabilityTarget::JavaScript, CapabilityTarget::Declaration] {
        push(
            nonclaims,
            target,
            CapabilityScope::Program,
            NonclaimReason::ProgramLiteralBoundary(family),
            DeletionCondition::LiteralProgramOwner(family),
        );
    }
}

fn add_semantic_diagnostics(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    scope: CapabilityScope,
    gap: SyntaxGap,
) {
    add_syntax(nonclaims, CapabilityTarget::SemanticDiagnostics, scope, gap);
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
            fact.recovery_extent,
            RecoveryStatementSource::Literal,
        ) {
            let scope = CapabilityScope::node(file.source.id, owner);
            for target in [
                CapabilityTarget::SemanticCheck,
                CapabilityTarget::RequiredType,
                CapabilityTarget::SemanticDiagnostics,
            ] {
                match role {
                    RecoveryStatementRole::SemanticOwner
                    | RecoveryStatementRole::RecoveredDeclaratorInitializer => {
                        add_semantic_node_syntax(nonclaims, target, scope, gap);
                    }
                    RecoveryStatementRole::RepresentationalFragment => {
                        add_syntax(nonclaims, target, scope, gap);
                    }
                }
            }
            add_service_nonclaims(
                nonclaims,
                scope,
                gap,
                matches!(
                    role,
                    RecoveryStatementRole::SemanticOwner
                        | RecoveryStatementRole::RecoveredDeclaratorInitializer
                ),
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
    for statement in &file.syntax.statements {
        statement.for_each_statement(&mut |statement| {
            owner_is_return |= statement.id == semantic_owner
                && matches!(statement.kind, StatementKind::Return(_));
        });
    }
    let mut declaration_owner = None;
    for statement in &file.syntax.statements {
        statement.for_each_statement(&mut |statement| {
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
    }
    let Some((_, owner)) = declaration_owner else {
        return;
    };
    let scope = CapabilityScope::node(file.source.id, owner);
    for target in [
        CapabilityTarget::DeclarationModel,
        CapabilityTarget::DeclarationValue,
    ] {
        add_semantic_node_syntax(nonclaims, target, scope, gap);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoveryStatementRole {
    SemanticOwner,
    RecoveredDeclaratorInitializer,
    RepresentationalFragment,
}

#[derive(Debug, Clone, Copy)]
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
        for statement in &file.syntax.statements {
            statement.for_each_statement(&mut |candidate| {
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
        }
        if recovered_binding_spans.is_empty() {
            continue;
        }
        for statement in &file.syntax.statements {
            statement.for_each_statement(&mut |candidate| {
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
    }
    owners
}

fn recovery_statement_owners(
    file: &ProgramFile,
    owner: crate::syntax::ParserRecoveryOwner,
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
    for statement in &file.syntax.statements {
        statement.for_each_statement(&mut |candidate| {
            if candidate.id == owner.statement {
                candidate.for_each_statement(&mut |descendant| {
                    owner_subtree.insert(descendant.id);
                });
            }
        });
    }

    let mut owners = BTreeMap::from([(owner.statement, RecoveryStatementRole::SemanticOwner)]);
    for statement in &file.syntax.statements {
        statement.for_each_statement(&mut |statement| {
            if recovery_extent.start <= statement.span.start
                && statement.span.start < recovery_extent.end
            {
                let role = match source {
                    RecoveryStatementSource::Parser {
                        recovered_declarator_initializers,
                    } if recovered_declarator_initializers.contains(&statement.id) => {
                        RecoveryStatementRole::RecoveredDeclaratorInitializer
                    }
                    _ if owner_subtree.contains(&statement.id) => {
                        RecoveryStatementRole::SemanticOwner
                    }
                    _ => RecoveryStatementRole::RepresentationalFragment,
                };
                owners.insert(statement.id, role);
            }
        });
    }
    owners
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
    for target in [
        CapabilityTarget::DeclarationModel,
        CapabilityTarget::DeclarationValue,
    ] {
        add_syntax(nonclaims, target, CapabilityScope::node(file, owner), gap);
    }
}

fn declaration_statement_owners(file: &ProgramFile) -> BTreeSet<NodeId> {
    let mut owners = BTreeSet::new();
    for statement in &file.syntax.statements {
        statement.for_each_statement(&mut |statement| {
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
    }
    owners
}

fn add_service_nonclaims(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    scope: CapabilityScope,
    gap: SyntaxGap,
    semantic_node: bool,
) {
    for target in [
        CapabilityTarget::QuickInfo,
        CapabilityTarget::Definition,
        CapabilityTarget::References,
        CapabilityTarget::Highlights,
        CapabilityTarget::Rename,
    ] {
        if semantic_node {
            add_semantic_node_syntax(nonclaims, target, scope, gap);
        } else {
            add_syntax(nonclaims, target, scope, gap);
        }
    }
}

fn add_both_emit(nonclaims: &mut Vec<CapabilityNonclaim>, scope: CapabilityScope, gap: SyntaxGap) {
    add_syntax(nonclaims, CapabilityTarget::JavaScript, scope, gap);
    add_syntax(nonclaims, CapabilityTarget::Declaration, scope, gap);
}

fn add_syntax(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    target: CapabilityTarget,
    scope: CapabilityScope,
    gap: SyntaxGap,
) {
    push(
        nonclaims,
        target,
        scope,
        NonclaimReason::Syntax(gap),
        DeletionCondition::SyntaxOwner(gap),
    );
}

fn add_semantic_node_syntax(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    target: CapabilityTarget,
    scope: CapabilityScope,
    gap: SyntaxGap,
) {
    push(
        nonclaims,
        target,
        scope,
        NonclaimReason::Syntax(gap),
        DeletionCondition::DeepestSemanticOwner(gap),
    );
}

fn push(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    target: CapabilityTarget,
    scope: CapabilityScope,
    reason: NonclaimReason,
    deletion: DeletionCondition,
) {
    nonclaims.push(CapabilityNonclaim {
        target,
        scope,
        reason,
        deletion,
    });
}

pub(crate) fn is_declaration_source(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts")
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
