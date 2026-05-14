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
            self.record_simple_local_interface_declaration_provenance_residue(
                Outcome::RejectOutOfArenaDecl,
                sym_id,
                declarations,
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
            self.record_simple_local_interface_declaration_provenance_residue(
                Outcome::RejectMissingInterfaceDecl,
                sym_id,
                declarations,
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
            self.record_simple_local_interface_declaration_provenance_residue(
                Outcome::RejectMissingInterfaceDecl,
                sym_id,
                declarations,
            );
            return None;
        };
        let Some(interface) = self.ctx.arena.get_interface(node) else {
            tsz_common::perf_counters::record_compute_type_of_symbol_interface_simple_object_outcome(
                Outcome::RejectMissingInterfaceDecl,
            );
            self.record_simple_local_interface_declaration_provenance_residue(
                Outcome::RejectMissingInterfaceDecl,
                sym_id,
                declarations,
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
        let interface_name = if tsz_common::perf_counters::enabled_fast() {
            self.simple_local_interface_entity_name_text(interface.name)
        } else {
            None
        };
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
            let Some(property_name) = self.get_property_name_resolved(sig.name) else {
                tsz_common::perf_counters::record_compute_type_of_symbol_interface_simple_object_outcome(
                    Outcome::RejectUnresolvedPropertyName,
                );
                return None;
            };
            let name_atom = self.ctx.types.intern_string(&property_name);
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
                    tsz_common::perf_counters::record_compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residue(
                        annotation_kind,
                        interface_name.as_deref(),
                        Some(&property_name),
                    );
                    if annotation_kind == AnnotationKind::TypeReference
                        && tsz_common::perf_counters::enabled_fast()
                    {
                        let reject_outcome = self
                            .classify_simple_local_interface_type_reference_reject_outcome(
                                sig.type_annotation,
                            );
                        tsz_common::perf_counters::record_compute_type_of_symbol_interface_simple_object_type_reference_reject_outcome(
                            reject_outcome,
                        );
                        if let Some(name) =
                            self.simple_local_interface_type_reference_name(sig.type_annotation)
                        {
                            tsz_common::perf_counters::record_compute_type_of_symbol_interface_simple_object_type_reference_reject_residue(
                                reject_outcome,
                                &name,
                            );
                        }
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
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return false;
        };
        if matches!(
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
                || kind == syntax_kind_ext::LITERAL_TYPE
                || kind == syntax_kind_ext::TEMPLATE_LITERAL_TYPE
        ) {
            return true;
        }

        if node.kind == syntax_kind_ext::UNION_TYPE
            || node.kind == syntax_kind_ext::INTERSECTION_TYPE
        {
            return self
                .ctx
                .arena
                .get_composite_type(node)
                .is_some_and(|composite| {
                    composite
                        .types
                        .nodes
                        .iter()
                        .copied()
                        .all(|member| self.is_simple_local_interface_fastpath_type(member))
                });
        }

        if node.kind == syntax_kind_ext::ARRAY_TYPE {
            return self.ctx.arena.get_array_type(node).is_some_and(|array| {
                self.is_simple_local_interface_fastpath_type(array.element_type)
            });
        }

        if node.kind == syntax_kind_ext::TUPLE_TYPE {
            return self.ctx.arena.get_tuple_type(node).is_some_and(|tuple| {
                tuple
                    .elements
                    .nodes
                    .iter()
                    .copied()
                    .all(|element| self.is_simple_local_interface_fastpath_type(element))
            });
        }

        self.is_simple_local_interface_primitive_type_reference(node)
    }

    fn is_simple_local_interface_primitive_type_reference(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> bool {
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return false;
        };
        if type_ref
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty())
        {
            return false;
        }
        let Some(type_name_node) = self.ctx.arena.get(type_ref.type_name) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(type_name_node) else {
            return false;
        };
        matches!(
            ident.escaped_text.as_str(),
            "any"
                | "bigint"
                | "boolean"
                | "never"
                | "number"
                | "object"
                | "string"
                | "symbol"
                | "undefined"
                | "unknown"
                | "void"
        )
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

    fn simple_local_interface_type_reference_name(&self, type_idx: NodeIndex) -> Option<String> {
        let type_node = self.ctx.arena.get(type_idx)?;
        let type_ref = self.ctx.arena.get_type_ref(type_node)?;
        self.simple_local_interface_entity_name_text(type_ref.type_name)
    }

    fn simple_local_interface_entity_name_text(&self, name_idx: NodeIndex) -> Option<String> {
        let name_node = self.ctx.arena.get(name_idx)?;
        if name_node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(name_node)
                .map(|ident| ident.escaped_text.as_str().to_owned());
        }

        if name_node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qualified = self.ctx.arena.get_qualified_name(name_node)?;
            let left = self.simple_local_interface_entity_name_text(qualified.left)?;
            let right_node = self.ctx.arena.get(qualified.right)?;
            let right = self
                .ctx
                .arena
                .get_identifier(right_node)?
                .escaped_text
                .as_str();
            let mut out = String::with_capacity(left.len() + 1 + right.len());
            out.push_str(&left);
            out.push('.');
            out.push_str(right);
            return Some(out);
        }

        None
    }

    fn record_simple_local_interface_declaration_provenance_residue(
        &self,
        outcome: Outcome,
        sym_id: SymbolId,
        declarations: &[NodeIndex],
    ) {
        if !tsz_common::perf_counters::enabled_fast() {
            return;
        }
        let symbol_name = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .map(|symbol| symbol.escaped_name.as_str());
        tsz_common::perf_counters::record_compute_type_of_symbol_interface_simple_object_declaration_provenance_residue(
            outcome,
            symbol_name,
            declarations.len(),
        );
    }
}
