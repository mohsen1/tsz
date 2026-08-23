//! Import-alias handling for `typeof` type queries.

use crate::bind::ScopeId;
use crate::semantics::types::{DeferredType, TypeId, TypeKind};
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
        self.observe_semantic_declaration(file, declaration);
        let root = self
            .store
            .intern(TypeKind::Deferred(DeferredType::Value(declaration)));
        let mut property_order = self.property_order_for_declaration(declaration);
        segments
            .enumerate()
            .fold(root, |object, (index, property)| {
                let property_span = segment_spans.get(index + 1).copied().unwrap_or(name_span);
                let receiver_order = property_order.clone();
                property_order = property_order
                    .as_ref()
                    .and_then(|order| order.property(property))
                    .cloned();
                self.deferred_property_type_with_order(
                    object,
                    property,
                    property_span,
                    receiver_order,
                )
            })
    }
}
