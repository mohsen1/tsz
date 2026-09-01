//! Import-alias handling for `typeof` type queries.

use crate::bind::ScopeId;
use crate::semantics::types::{Completion, DeferredType, TypeId, TypeKind};
use crate::source::{FileId, Span};

use super::Checker;

impl Checker<'_> {
    pub(super) fn resolve_type_query_node(
        &mut self,
        file: FileId,
        scope: ScopeId,
        name: &str,
        name_span: Span,
        segment_spans: &[Span],
    ) -> TypeId {
        let mut segments = name.split('.');
        let root_name = segments.next().unwrap_or(name);
        let root_span = segment_spans.first().copied().unwrap_or(name_span);
        let Some(root) = self.program.resolve_type_query_root(file, scope, root_name) else {
            self.push_diagnostic(
                file,
                root_span,
                format!("Cannot find name '{root_name}'."),
                2304,
            );
            return self.store.builtins.error;
        };
        let declaration = root.semantic_declaration();
        let imported = root.navigation_declaration() != declaration;
        if !imported
            && let Some(parameter_type) = self.parameter_type_overrides.get(&declaration).copied()
        {
            return parameter_type;
        }
        self.observe_semantic_declaration(file, declaration);
        let deferred = if imported {
            DeferredType::ImportedTypeQuery(declaration)
        } else {
            DeferredType::Value(declaration)
        };
        let root = self.store.intern(TypeKind::Deferred(deferred));
        segments
            .enumerate()
            .fold(root, |object, (index, property)| {
                let property_span = segment_spans.get(index + 1).copied().unwrap_or(name_span);
                self.deferred_property_type(object, property, property_span)
            })
    }

    /// A direct object reached through the bounded import-alias bridge needs
    /// TS2739, which this rewrite does not own yet. Keep that exact producer
    /// incomplete; dependency-closed wrappers such as arrays remain claimed.
    pub(super) fn imported_type_query_value(
        &mut self,
        declaration: crate::source::DeclId,
    ) -> Completion<TypeId> {
        let value = completed!(self.declaration_value_type(declaration));
        if matches!(self.store.kind(value), TypeKind::Object(_)) {
            Completion::Deferred
        } else {
            Completion::Complete(value)
        }
    }
}
