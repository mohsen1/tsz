use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use tsz_binder::SymbolId;
use tsz_common::perf_counters::{
    ComputeTypeOfSymbolInterfaceSimpleObjectNonPrimitiveAnnotationKind as AnnotationKind,
    ComputeTypeOfSymbolInterfaceSimpleObjectOutcome as Outcome,
    ComputeTypeOfSymbolInterfaceSimpleObjectTypeReferenceRejectOutcome as TypeReferenceOutcome,
};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::syntax_kind_ext::PROPERTY_SIGNATURE;
use tsz_scanner::SyntaxKind;
use tsz_solver::{PropertyInfo, TypeId, Visibility};

impl<'a> CheckerState<'a> {
    pub(super) fn try_lower_simple_local_interface_object(
        &mut self,
        sym_id: SymbolId,
        declarations: &[NodeIndex],
        has_out_of_arena_decl: bool,
        has_cross_file_same_index: bool,
        has_local_interface_decl: bool,
        has_local_interface_heritage_extends: bool,
        has_local_computed_property_name: bool,
    ) -> Option<TypeId> {
        if has_out_of_arena_decl {
            tsz_common::perf_counters::record_compute_type_of_symbol_interface_simple_object_outcome(
                Outcome::RejectOutOfArenaDecl,
            );
            return None;
        }
        if has_cross_file_same_index {
            tsz_common::perf_counters::record_compute_type_of_symbol_interface_simple_object_outcome(
                Outcome::RejectCrossFileSameIndex,
            );
            return None;
        }
        if !has_local_interface_decl {
            tsz_common::perf_counters::record_compute_type_of_symbol_interface_simple_object_outcome(
                Outcome::RejectMissingInterfaceDecl,
            );
            return None;
        }
        if has_local_interface_heritage_extends {
            tsz_common::perf_counters::record_compute_type_of_symbol_interface_simple_object_outcome(
                Outcome::RejectHeritageExtends,
            );
            return None;
        }
        if has_local_computed_property_name {
            tsz_common::perf_counters::record_compute_type_of_symbol_interface_simple_object_outcome(
                Outcome::RejectComputedName,
            );
            return None;
        }
        if declarations.len() != 1 {
            tsz_common::perf_counters::record_compute_type_of_symbol_interface_simple_object_outcome(
                Outcome::RejectDeclarationCount,
            );
            return None;
        }

        let decl_idx = declarations[0];
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            tsz_common::perf_counters::record_compute_type_of_symbol_interface_simple_object_outcome(
                Outcome::RejectMissingInterfaceDecl,
            );
            return None;
        };
        let Some(interface) = self.ctx.arena.get_interface(node) else {
            tsz_common::perf_counters::record_compute_type_of_symbol_interface_simple_object_outcome(
                Outcome::RejectMissingInterfaceDecl,
            );
            return None;
        };
        if interface
            .type_parameters
            .as_ref()
            .is_some_and(|params| !params.nodes.is_empty())
            || interface.members.nodes.is_empty()
        {
            tsz_common::perf_counters::record_compute_type_of_symbol_interface_simple_object_outcome(
                Outcome::RejectTypeParameters,
            );
            return None;
        }

        let mut properties = Vec::with_capacity(interface.members.nodes.len());
        for (member_order, &member_idx) in interface.members.nodes.iter().enumerate() {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                tsz_common::perf_counters::record_compute_type_of_symbol_interface_simple_object_outcome(
                    Outcome::RejectNonPropertyMember,
                );
                return None;
            };
            if member_node.kind != PROPERTY_SIGNATURE {
                tsz_common::perf_counters::record_compute_type_of_symbol_interface_simple_object_outcome(
                    Outcome::RejectNonPropertyMember,
                );
                return None;
            }
            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                tsz_common::perf_counters::record_compute_type_of_symbol_interface_simple_object_outcome(
                    Outcome::RejectNonPropertyMember,
                );
                return None;
            };
            let name_atom = self
                .get_property_name_resolved(sig.name)
                .map(|name| self.ctx.types.intern_string(&name));
            let Some(name_atom) = name_atom else {
                tsz_common::perf_counters::record_compute_type_of_symbol_interface_simple_object_outcome(
                    Outcome::RejectUnresolvedPropertyName,
                );
                return None;
            };
            let type_id = if sig.type_annotation.is_some() {
                if !self.is_simple_local_interface_fastpath_type(sig.type_annotation) {
                    tsz_common::perf_counters::record_compute_type_of_symbol_interface_simple_object_outcome(
                        Outcome::RejectNonPrimitiveAnnotation,
                    );
                    let annotation_kind = self
                        .classify_simple_local_interface_non_primitive_annotation_kind(
                            sig.type_annotation,
                        );
                    tsz_common::perf_counters::record_compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kind(
                        annotation_kind,
                    );
                    if annotation_kind == AnnotationKind::TypeReference {
                        tsz_common::perf_counters::record_compute_type_of_symbol_interface_simple_object_type_reference_reject_outcome(
                            self.classify_simple_local_interface_type_reference_reject_outcome(
                                sig.type_annotation,
                            ),
                        );
                    }
                    return None;
                }
                self.get_type_from_type_node_in_type_literal(sig.type_annotation)
            } else {
                TypeId::ANY
            };
            let is_symbol_named = self.is_symbol_property_name(sig.name);
            properties.push(PropertyInfo {
                name: name_atom,
                type_id,
                write_type: type_id,
                optional: sig.question_token,
                readonly: self.has_readonly_modifier(&sig.modifiers),
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: member_order as u32 + 1,
                is_string_named: false,
                is_symbol_named,
                single_quoted_name: false,
            });
        }

        let factory = self.ctx.types.factory();
        tsz_common::perf_counters::record_compute_type_of_symbol_interface_simple_object_outcome(
            Outcome::Success,
        );
        if properties.is_empty() {
            Some(TypeId::ANY)
        } else {
            Some(factory.object_with_symbol(properties, Some(sym_id)))
        }
    }

    fn is_simple_local_interface_fastpath_type(&self, type_idx: NodeIndex) -> bool {
        self.ctx.arena.get(type_idx).is_some_and(|node| {
            matches!(
                node.kind,
                kind if kind == SyntaxKind::AnyKeyword as u16
                    || kind == SyntaxKind::BigIntKeyword as u16
                    || kind == SyntaxKind::BooleanKeyword as u16
                    || kind == SyntaxKind::NeverKeyword as u16
                    || kind == SyntaxKind::NumberKeyword as u16
                    || kind == SyntaxKind::ObjectKeyword as u16
                    || kind == SyntaxKind::StringKeyword as u16
                    || kind == SyntaxKind::SymbolKeyword as u16
                    || kind == SyntaxKind::UndefinedKeyword as u16
                    || kind == SyntaxKind::UnknownKeyword as u16
                    || kind == SyntaxKind::VoidKeyword as u16
            )
        })
    }

    fn classify_simple_local_interface_non_primitive_annotation_kind(
        &self,
        type_idx: NodeIndex,
    ) -> AnnotationKind {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return AnnotationKind::Other;
        };

        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => AnnotationKind::TypeReference,
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                AnnotationKind::UnionOrIntersection
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => AnnotationKind::TypeLiteral,
            k if k == syntax_kind_ext::ARRAY_TYPE || k == syntax_kind_ext::TUPLE_TYPE => {
                AnnotationKind::ArrayOrTuple
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                AnnotationKind::FunctionOrConstructor
            }
            k if k == syntax_kind_ext::CONDITIONAL_TYPE || k == syntax_kind_ext::INFER_TYPE => {
                AnnotationKind::ConditionalOrInfer
            }
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE || k == syntax_kind_ext::MAPPED_TYPE => {
                AnnotationKind::IndexedOrMapped
            }
            k if k == syntax_kind_ext::IMPORT_TYPE || k == syntax_kind_ext::TYPE_QUERY => {
                AnnotationKind::ImportOrTypeQuery
            }
            k if k == syntax_kind_ext::LITERAL_TYPE
                || k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE =>
            {
                AnnotationKind::LiteralOrTemplateLiteral
            }
            k if k == syntax_kind_ext::TYPE_OPERATOR
                || k == syntax_kind_ext::PARENTHESIZED_TYPE =>
            {
                AnnotationKind::OperatorOrParenthesized
            }
            k if k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE
                || k == syntax_kind_ext::THIS_TYPE =>
            {
                AnnotationKind::OptionalRestOrThis
            }
            _ => AnnotationKind::Other,
        }
    }

    fn classify_simple_local_interface_type_reference_reject_outcome(
        &self,
        type_idx: NodeIndex,
    ) -> TypeReferenceOutcome {
        let Some(type_node) = self.ctx.arena.get(type_idx) else {
            return TypeReferenceOutcome::MalformedTypeReference;
        };
        let Some(type_ref) = self.ctx.arena.get_type_ref(type_node) else {
            return TypeReferenceOutcome::MalformedTypeReference;
        };

        let type_name_idx = type_ref.type_name;
        let Some(type_name_node) = self.ctx.arena.get(type_name_idx) else {
            return TypeReferenceOutcome::OtherTypeNameSyntax;
        };

        if type_name_node.kind == syntax_kind_ext::QUALIFIED_NAME {
            return match self.resolve_qualified_symbol_in_type_position(type_name_idx) {
                TypeSymbolResolution::Type(_) => {
                    TypeReferenceOutcome::QualifiedNameResolvableSymbol
                }
                TypeSymbolResolution::ValueOnly(_) => {
                    TypeReferenceOutcome::QualifiedNameValueOnlySymbol
                }
                TypeSymbolResolution::NotFound => TypeReferenceOutcome::QualifiedNameNotFoundSymbol,
            };
        }

        if type_name_node.kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.ctx.arena.get_identifier(type_name_node)
                && crate::query_boundaries::common::is_compiler_managed_type(
                    ident.escaped_text.as_str(),
                )
            {
                return TypeReferenceOutcome::IdentifierCompilerManagedType;
            }
            return match self
                .resolve_identifier_symbol_in_type_position_without_tracking(type_name_idx)
            {
                TypeSymbolResolution::Type(_) => TypeReferenceOutcome::IdentifierResolvableSymbol,
                TypeSymbolResolution::ValueOnly(_) => {
                    TypeReferenceOutcome::IdentifierValueOnlySymbol
                }
                TypeSymbolResolution::NotFound => TypeReferenceOutcome::IdentifierNotFoundSymbol,
            };
        }

        TypeReferenceOutcome::OtherTypeNameSyntax
    }
}
