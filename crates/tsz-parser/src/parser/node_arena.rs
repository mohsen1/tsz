//! `NodeArena` creation methods (`add_*` methods).
//!
//! This module contains all node creation and initialization methods for `NodeArena`.

use super::base::{NodeIndex, NodeList};
use super::node::{
    AccessExprData, AccessorData, ArrayTypeData, BinaryExprData, BindingElementData,
    BindingPatternData, BlockData, CallExprData, CaseClauseData, CatchClauseData, ClassData,
    CompositeTypeData, ComputedPropertyData, ConditionalExprData, ConditionalTypeData,
    ConstructorData, DecoratorData, EnumData, EnumMemberData, ExportAssignmentData, ExportDeclData,
    ExprStatementData, ExprWithTypeArgsData, ExtendedNodeInfo, ForInOfData, FunctionData,
    FunctionTypeData, HeritageData, IdentifierData, IfStatementData, ImportAttributeData,
    ImportAttributesData, ImportClauseData, ImportDeclData, IndexSignatureData,
    IndexedAccessTypeData, InferTypeData, InterfaceData, JsxAttributeData, JsxAttributesData,
    JsxClosingData, JsxElementData, JsxExpressionData, JsxFragmentData, JsxNamespacedNameData,
    JsxOpeningData, JsxSpreadAttributeData, JsxTextData, JumpData, LabeledData, LiteralData,
    LiteralExprData, LiteralTypeData, LoopData, MappedTypeData, MethodDeclData, ModuleBlockData,
    ModuleData, NamedImportsData, NamedTupleMemberData, Node, NodeArena, ParameterData,
    ParenthesizedData, PropertyAssignmentData, PropertyDeclData, QualifiedNameData, ReturnData,
    ShorthandPropertyData, SignatureData, SourceFileData, SpecifierData, SpreadData, SwitchData,
    TaggedTemplateData, TemplateExprData, TemplateLiteralTypeData, TemplateSpanData, TryData,
    TupleTypeData, TypeAliasData, TypeAssertionData, TypeLiteralData, TypeOperatorData,
    TypeParameterData, TypePredicateData, TypeQueryData, TypeRefData, UnaryExprData,
    UnaryExprDataEx, VariableData, VariableDeclarationData, WithData, WrappedTypeData,
};

use tsz_common::interner::{Atom, Interner};

impl NodeArena {
    /// Maximum pre-allocation to avoid capacity overflow in huge files.
    const MAX_NODE_PREALLOC: usize = 5_000_000;
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the interner (called after parsing to transfer ownership from scanner)
    pub fn set_interner(&mut self, interner: Interner) {
        self.interner = interner;
    }

    /// Get a reference to the interner
    #[must_use]
    pub fn interner(&self) -> &Interner {
        &self.interner
    }

    /// Resolve an identifier text using `Atom` (fast) or `escaped_text` (fallback).
    #[inline]
    #[must_use]
    pub fn resolve_identifier_text<'a>(&'a self, data: &'a IdentifierData) -> &'a str {
        if data.atom == Atom::NONE {
            &data.escaped_text
        } else {
            self.interner.resolve(data.atom)
        }
    }

    /// Create an arena with pre-allocated capacity.
    /// Uses heuristic ratios based on typical TypeScript AST composition.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        let safe_capacity = capacity.min(Self::MAX_NODE_PREALLOC);
        // Use Default for all the new pools, just set capacity for main ones
        Self {
            nodes: Vec::with_capacity(safe_capacity),
            extended_info: Vec::with_capacity(safe_capacity),
            identifiers: Vec::with_capacity(safe_capacity / 4), // ~25% identifiers
            literals: Vec::with_capacity(safe_capacity / 8),    // ~12% literals
            binary_exprs: Vec::with_capacity(safe_capacity / 8), // ~12% binary
            call_exprs: Vec::with_capacity(safe_capacity / 8),  // ~12% calls
            access_exprs: Vec::with_capacity(safe_capacity / 8), // ~12% property access
            blocks: Vec::with_capacity(safe_capacity / 8),      // ~12% blocks
            variables: Vec::with_capacity(safe_capacity / 16),  // ~6% variables
            functions: Vec::with_capacity(safe_capacity / 16),  // ~6% functions
            type_refs: Vec::with_capacity(safe_capacity / 8),   // ~12% type refs
            source_files: Vec::with_capacity(1),                // Usually 1
            ..Default::default()
        }
    }

    pub fn clear(&mut self) {
        macro_rules! clear_vecs {
            ($($field:ident),+ $(,)?) => {
                $(self.$field.clear();)+
            };
        }

        clear_vecs!(
            nodes,
            identifiers,
            qualified_names,
            computed_properties,
            literals,
            binary_exprs,
            unary_exprs,
            call_exprs,
            access_exprs,
            conditional_exprs,
            literal_exprs,
            parenthesized,
            unary_exprs_ex,
            type_assertions,
            template_exprs,
            template_spans,
            tagged_templates,
            functions,
            classes,
            interfaces,
            type_aliases,
            enums,
            enum_members,
            modules,
            module_blocks,
            signatures,
            index_signatures,
            property_decls,
            method_decls,
            constructors,
            accessors,
            parameters,
            type_parameters,
            decorators,
            heritage_clauses,
            expr_with_type_args,
            if_statements,
            loops,
            blocks,
            variables,
            return_data,
            expr_statements,
            switch_data,
            case_clauses,
            try_data,
            catch_clauses,
            labeled_data,
            jump_data,
            with_data,
            type_refs,
            composite_types,
            function_types,
            type_queries,
            type_literals,
            array_types,
            tuple_types,
            wrapped_types,
            conditional_types,
            infer_types,
            type_operators,
            indexed_access_types,
            mapped_types,
            literal_types,
            template_literal_types,
            named_tuple_members,
            type_predicates,
            import_decls,
            import_clauses,
            named_imports,
            specifiers,
            export_decls,
            export_assignments,
            import_attributes,
            import_attribute,
            binding_patterns,
            binding_elements,
            property_assignments,
            shorthand_properties,
            spread_data,
            variable_declarations,
            for_in_of,
            jsx_elements,
            jsx_opening,
            jsx_closing,
            jsx_fragments,
            jsx_attributes,
            jsx_attribute,
            jsx_spread_attributes,
            jsx_expressions,
            jsx_text,
            jsx_namespaced_names,
            source_files,
            extended_info,
        );
    }

    #[inline]
    #[must_use]
    fn len_u32(&self, len: usize) -> u32 {
        let _ = self;
        u32::try_from(len).expect("node arena length exceeds u32::MAX")
    }

    // ============================================================================
    // Parent Mapping Helpers
    // ============================================================================

    /// Set the parent for a single child node.
    /// This is called during node creation to maintain parent pointers.
    #[inline]
    fn set_parent(&mut self, child: NodeIndex, parent: NodeIndex) {
        if !child.is_none() {
            // Safety: child index is guaranteed to be valid and < current index
            // because we build bottom-up (children are created before parents).
            if let Some(info) = self.extended_info.get_mut(child.0 as usize) {
                info.parent = parent;
            }
        }
    }

    /// Set the parent for a list of children.
    #[inline]
    fn set_parent_list(&mut self, list: &NodeList, parent: NodeIndex) {
        for &child in &list.nodes {
            self.set_parent(child, parent);
        }
    }

    /// Set the parent for an optional list of children.
    #[inline]
    fn set_parent_opt_list(&mut self, list: Option<&NodeList>, parent: NodeIndex) {
        if let Some(l) = list {
            self.set_parent_list(l, parent);
        }
    }

    // ============================================================================
    // Node Creation Methods
    // ============================================================================

    /// Add a token node (no additional data)
    pub fn add_token(&mut self, kind: u16, pos: u32, end: u32) -> NodeIndex {
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::new(kind, pos, end));
        self.extended_info.push(ExtendedNodeInfo::default());
        NodeIndex(index)
    }

    /// Create a modifier token (static, public, private, etc.)
    pub fn create_modifier(&mut self, kind: tsz_scanner::SyntaxKind, pos: u32) -> NodeIndex {
        // Modifiers are simple tokens, their kind IS the modifier type
        // End position is pos + keyword length
        let end = pos
            + match kind {
                tsz_scanner::SyntaxKind::AsyncKeyword | tsz_scanner::SyntaxKind::ConstKeyword => 5,
                tsz_scanner::SyntaxKind::StaticKeyword
                | tsz_scanner::SyntaxKind::PublicKeyword
                | tsz_scanner::SyntaxKind::ExportKeyword => 6,
                tsz_scanner::SyntaxKind::PrivateKeyword
                | tsz_scanner::SyntaxKind::DefaultKeyword
                | tsz_scanner::SyntaxKind::DeclareKeyword => 7,
                tsz_scanner::SyntaxKind::ReadonlyKeyword
                | tsz_scanner::SyntaxKind::AbstractKeyword
                | tsz_scanner::SyntaxKind::OverrideKeyword => 8,
                tsz_scanner::SyntaxKind::ProtectedKeyword => 9,
                _ => 0,
            };
        self.add_token(kind as u16, pos, end)
    }

    /// Add an identifier node
    pub fn add_identifier(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: IdentifierData,
    ) -> NodeIndex {
        let data_index = self.len_u32(self.identifiers.len());
        self.identifiers.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        NodeIndex(index)
    }

    /// Add a literal node
    pub fn add_literal(&mut self, kind: u16, pos: u32, end: u32, data: LiteralData) -> NodeIndex {
        let data_index = self.len_u32(self.literals.len());
        self.literals.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        NodeIndex(index)
    }

    /// Add a binary expression
    pub fn add_binary_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: BinaryExprData,
    ) -> NodeIndex {
        let left = data.left;
        let right = data.right;

        let data_index = self.len_u32(self.binary_exprs.len());
        self.binary_exprs.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(left, parent);
        self.set_parent(right, parent);

        parent
    }

    /// Add a call expression
    pub fn add_call_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: CallExprData,
    ) -> NodeIndex {
        let expression = data.expression;
        let type_arguments = data.type_arguments.clone();
        let arguments = data.arguments.clone();

        let data_index = self.len_u32(self.call_exprs.len());
        self.call_exprs.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent_opt_list(type_arguments.as_ref(), parent);
        self.set_parent_opt_list(arguments.as_ref(), parent);

        parent
    }

    /// Add a function node
    pub fn add_function(&mut self, kind: u16, pos: u32, end: u32, data: FunctionData) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_parameters = data.type_parameters.clone();
        let parameters = data.parameters.clone();
        let type_annotation = data.type_annotation;
        let body = data.body;

        let data_index = self.len_u32(self.functions.len());
        self.functions.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(type_parameters.as_ref(), parent);
        self.set_parent_list(&parameters, parent);
        self.set_parent(type_annotation, parent);
        self.set_parent(body, parent);

        parent
    }

    /// Add a class node
    pub fn add_class(&mut self, kind: u16, pos: u32, end: u32, data: ClassData) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_parameters = data.type_parameters.clone();
        let heritage_clauses = data.heritage_clauses.clone();
        let members = data.members.clone();

        let data_index = self.len_u32(self.classes.len());
        self.classes.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(type_parameters.as_ref(), parent);
        self.set_parent_opt_list(heritage_clauses.as_ref(), parent);
        self.set_parent_list(&members, parent);

        parent
    }

    /// Add a block node
    pub fn add_block(&mut self, kind: u16, pos: u32, end: u32, data: BlockData) -> NodeIndex {
        let statements = data.statements.clone();

        let data_index = self.len_u32(self.blocks.len());
        self.blocks.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_list(&statements, parent);

        parent
    }

    /// Add a source file node
    pub fn add_source_file(&mut self, pos: u32, end: u32, data: SourceFileData) -> NodeIndex {
        use super::syntax_kind_ext::SOURCE_FILE;
        let statements = data.statements.clone();
        let end_of_file_token = data.end_of_file_token;

        let data_index = self.len_u32(self.source_files.len());
        self.source_files.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes
            .push(Node::with_data(SOURCE_FILE, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_list(&statements, parent);
        self.set_parent(end_of_file_token, parent);

        parent
    }

    // ==========================================================================
    // Additional add_* methods for all data pools
    // ==========================================================================

    /// Add a qualified name node
    pub fn add_qualified_name(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: QualifiedNameData,
    ) -> NodeIndex {
        let left = data.left;
        let right = data.right;

        let data_index = self.len_u32(self.qualified_names.len());
        self.qualified_names.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(left, parent);
        self.set_parent(right, parent);

        parent
    }

    /// Add a computed property name node
    pub fn add_computed_property(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ComputedPropertyData,
    ) -> NodeIndex {
        let expression = data.expression;

        let data_index = self.len_u32(self.computed_properties.len());
        self.computed_properties.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        parent
    }

    /// Add a unary expression node
    pub fn add_unary_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: UnaryExprData,
    ) -> NodeIndex {
        let operand = data.operand;

        let data_index = self.len_u32(self.unary_exprs.len());
        self.unary_exprs.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(operand, parent);

        parent
    }

    /// Add a property/element access expression node
    pub fn add_access_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: AccessExprData,
    ) -> NodeIndex {
        let expression = data.expression;
        let name_or_argument = data.name_or_argument;

        let data_index = self.len_u32(self.access_exprs.len());
        self.access_exprs.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent(name_or_argument, parent);

        parent
    }

    /// Add a conditional expression node (a ? b : c)
    pub fn add_conditional_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ConditionalExprData,
    ) -> NodeIndex {
        let condition = data.condition;
        let when_true = data.when_true;
        let when_false = data.when_false;

        let data_index = self.len_u32(self.conditional_exprs.len());
        self.conditional_exprs.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(condition, parent);
        self.set_parent(when_true, parent);
        self.set_parent(when_false, parent);
        parent
    }

    /// Add an object/array literal expression node
    pub fn add_literal_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: LiteralExprData,
    ) -> NodeIndex {
        let elements = data.elements.clone();

        let data_index = self.len_u32(self.literal_exprs.len());
        self.literal_exprs.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_list(&elements, parent);
        parent
    }

    /// Add a parenthesized expression node
    pub fn add_parenthesized(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ParenthesizedData,
    ) -> NodeIndex {
        let expression = data.expression;
        let data_index = self.len_u32(self.parenthesized.len());
        self.parenthesized.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        parent
    }

    /// Add a spread/await/yield expression node
    pub fn add_unary_expr_ex(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: UnaryExprDataEx,
    ) -> NodeIndex {
        let expression = data.expression;
        let data_index = self.len_u32(self.unary_exprs_ex.len());
        self.unary_exprs_ex.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        parent
    }

    /// Add a type assertion expression node
    pub fn add_type_assertion(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeAssertionData,
    ) -> NodeIndex {
        let expression = data.expression;
        let type_node = data.type_node;
        let data_index = self.len_u32(self.type_assertions.len());
        self.type_assertions.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent(type_node, parent);
        parent
    }

    /// Add a template expression node
    pub fn add_template_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TemplateExprData,
    ) -> NodeIndex {
        let head = data.head;
        let template_spans = data.template_spans.clone();

        let data_index = self.len_u32(self.template_exprs.len());
        self.template_exprs.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(head, parent);
        self.set_parent_list(&template_spans, parent);

        parent
    }

    /// Add a template span node
    pub fn add_template_span(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TemplateSpanData,
    ) -> NodeIndex {
        let expression = data.expression;
        let literal = data.literal;

        let data_index = self.len_u32(self.template_spans.len());
        self.template_spans.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent(literal, parent);

        parent
    }

    /// Add a tagged template expression node
    pub fn add_tagged_template(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TaggedTemplateData,
    ) -> NodeIndex {
        let tag = data.tag;
        let type_arguments = data.type_arguments.clone();
        let template = data.template;

        let data_index = self.len_u32(self.tagged_templates.len());
        self.tagged_templates.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(tag, parent);
        self.set_parent_opt_list(type_arguments.as_ref(), parent);
        self.set_parent(template, parent);

        parent
    }

    /// Add an interface declaration node
    pub fn add_interface(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: InterfaceData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_parameters = data.type_parameters.clone();
        let heritage_clauses = data.heritage_clauses.clone();
        let members = data.members.clone();

        let data_index = self.len_u32(self.interfaces.len());
        self.interfaces.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(type_parameters.as_ref(), parent);
        self.set_parent_opt_list(heritage_clauses.as_ref(), parent);
        self.set_parent_list(&members, parent);

        parent
    }

    /// Add a type alias declaration node
    pub fn add_type_alias(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeAliasData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_parameters = data.type_parameters.clone();
        let type_node = data.type_node;

        let data_index = self.len_u32(self.type_aliases.len());
        self.type_aliases.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(type_parameters.as_ref(), parent);
        self.set_parent(type_node, parent);

        parent
    }

    /// Add an enum declaration node
    pub fn add_enum(&mut self, kind: u16, pos: u32, end: u32, data: EnumData) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let members = data.members.clone();

        let data_index = self.len_u32(self.enums.len());
        self.enums.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent_list(&members, parent);

        parent
    }

    /// Add an enum member node
    pub fn add_enum_member(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: EnumMemberData,
    ) -> NodeIndex {
        let name = data.name;
        let initializer = data.initializer;

        let data_index = self.len_u32(self.enum_members.len());
        self.enum_members.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(name, parent);
        self.set_parent(initializer, parent);

        parent
    }

    /// Add a module declaration node
    pub fn add_module(&mut self, kind: u16, pos: u32, end: u32, data: ModuleData) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let body = data.body;

        let data_index = self.len_u32(self.modules.len());
        self.modules.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent(body, parent);

        parent
    }

    /// Add a module block node: { statements }
    pub fn add_module_block(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ModuleBlockData,
    ) -> NodeIndex {
        let statements = data.statements.clone();

        let data_index = self.len_u32(self.module_blocks.len());
        self.module_blocks.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(statements.as_ref(), parent);

        parent
    }

    /// Add a signature node (property/method signature)
    pub fn add_signature(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: SignatureData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_parameters = data.type_parameters.clone();
        let parameters = data.parameters.clone();
        let type_annotation = data.type_annotation;

        let data_index = self.len_u32(self.signatures.len());
        self.signatures.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(type_parameters.as_ref(), parent);
        self.set_parent_opt_list(parameters.as_ref(), parent);
        self.set_parent(type_annotation, parent);

        parent
    }

    /// Add an index signature node
    pub fn add_index_signature(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: IndexSignatureData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let parameters = data.parameters.clone();
        let type_annotation = data.type_annotation;

        let data_index = self.len_u32(self.index_signatures.len());
        self.index_signatures.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent_list(&parameters, parent);
        self.set_parent(type_annotation, parent);

        parent
    }

    /// Add a property declaration node
    pub fn add_property_decl(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: PropertyDeclData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_annotation = data.type_annotation;
        let initializer = data.initializer;

        let data_index = self.len_u32(self.property_decls.len());
        self.property_decls.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent(type_annotation, parent);
        self.set_parent(initializer, parent);
        parent
    }

    /// Add a method declaration node
    pub fn add_method_decl(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: MethodDeclData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_parameters = data.type_parameters.clone();
        let parameters = data.parameters.clone();
        let type_annotation = data.type_annotation;
        let body = data.body;

        let data_index = self.len_u32(self.method_decls.len());
        self.method_decls.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(type_parameters.as_ref(), parent);
        self.set_parent_list(&parameters, parent);
        self.set_parent(type_annotation, parent);
        self.set_parent(body, parent);
        parent
    }

    /// Add a constructor declaration node
    pub fn add_constructor(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ConstructorData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let type_parameters = data.type_parameters.clone();
        let parameters = data.parameters.clone();
        let body = data.body;

        let data_index = self.len_u32(self.constructors.len());
        self.constructors.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent_opt_list(type_parameters.as_ref(), parent);
        self.set_parent_list(&parameters, parent);
        self.set_parent(body, parent);
        parent
    }

    /// Add an accessor declaration node (get/set)
    pub fn add_accessor(&mut self, kind: u16, pos: u32, end: u32, data: AccessorData) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_parameters = data.type_parameters.clone();
        let parameters = data.parameters.clone();
        let type_annotation = data.type_annotation;
        let body = data.body;

        let data_index = self.len_u32(self.accessors.len());
        self.accessors.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(type_parameters.as_ref(), parent);
        self.set_parent_list(&parameters, parent);
        self.set_parent(type_annotation, parent);
        self.set_parent(body, parent);

        parent
    }

    /// Add a parameter declaration node
    pub fn add_parameter(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ParameterData,
    ) -> NodeIndex {
        let name = data.name;
        let type_annotation = data.type_annotation;
        let initializer = data.initializer;
        let modifiers = data.modifiers.clone();
        let data_index = self.len_u32(self.parameters.len());
        self.parameters.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        // Set parent pointers for children
        self.set_parent(name, parent);
        self.set_parent(type_annotation, parent);
        self.set_parent(initializer, parent);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        parent
    }

    /// Add a type parameter declaration node
    pub fn add_type_parameter(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeParameterData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let constraint = data.constraint;
        let default = data.default;

        let data_index = self.len_u32(self.type_parameters.len());
        self.type_parameters.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent(constraint, parent);
        self.set_parent(default, parent);

        parent
    }

    /// Add a decorator node
    pub fn add_decorator(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: DecoratorData,
    ) -> NodeIndex {
        let expression = data.expression;
        let data_index = self.len_u32(self.decorators.len());
        self.decorators.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        parent
    }

    /// Add a heritage clause node
    pub fn add_heritage(&mut self, kind: u16, pos: u32, end: u32, data: HeritageData) -> NodeIndex {
        let types = data.types.clone();
        let data_index = self.len_u32(self.heritage_clauses.len());
        self.heritage_clauses.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_list(&types, parent);
        parent
    }

    /// Add an expression with type arguments node
    pub fn add_expr_with_type_args(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ExprWithTypeArgsData,
    ) -> NodeIndex {
        let expression = data.expression;
        let type_arguments = data.type_arguments.clone();
        let data_index = self.len_u32(self.expr_with_type_args.len());
        self.expr_with_type_args.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent_opt_list(type_arguments.as_ref(), parent);
        parent
    }

    /// Add an if statement node
    pub fn add_if_statement(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: IfStatementData,
    ) -> NodeIndex {
        let expression = data.expression;
        let then_statement = data.then_statement;
        let else_statement = data.else_statement;

        let data_index = self.len_u32(self.if_statements.len());
        self.if_statements.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent(then_statement, parent);
        self.set_parent(else_statement, parent);

        parent
    }

    /// Add a loop node (for/while/do)
    pub fn add_loop(&mut self, kind: u16, pos: u32, end: u32, data: LoopData) -> NodeIndex {
        let initializer = data.initializer;
        let condition = data.condition;
        let incrementor = data.incrementor;
        let statement = data.statement;
        let data_index = self.len_u32(self.loops.len());
        self.loops.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(initializer, parent);
        self.set_parent(condition, parent);
        self.set_parent(incrementor, parent);
        self.set_parent(statement, parent);
        parent
    }

    /// Add a variable statement/declaration list node
    pub fn add_variable(&mut self, kind: u16, pos: u32, end: u32, data: VariableData) -> NodeIndex {
        self.add_variable_with_flags(kind, pos, end, data, 0)
    }

    /// Add a variable statement/declaration list node with flags
    pub fn add_variable_with_flags(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: VariableData,
        flags: u16,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let declarations = data.declarations.clone();

        let data_index = self.len_u32(self.variables.len());
        self.variables.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes
            .push(Node::with_data_and_flags(kind, pos, end, data_index, flags));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent_list(&declarations, parent);

        parent
    }

    /// Add a return/throw statement node
    pub fn add_return(&mut self, kind: u16, pos: u32, end: u32, data: ReturnData) -> NodeIndex {
        let expression = data.expression;

        let data_index = self.len_u32(self.return_data.len());
        self.return_data.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);

        parent
    }

    /// Add an expression statement node
    pub fn add_expr_statement(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ExprStatementData,
    ) -> NodeIndex {
        let expression = data.expression;
        let data_index = self.len_u32(self.expr_statements.len());
        self.expr_statements.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        parent
    }

    /// Add a switch statement node
    pub fn add_switch(&mut self, kind: u16, pos: u32, end: u32, data: SwitchData) -> NodeIndex {
        let expression = data.expression;
        let case_block = data.case_block;
        let data_index = self.len_u32(self.switch_data.len());
        self.switch_data.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent(case_block, parent);
        parent
    }

    /// Add a case/default clause node
    pub fn add_case_clause(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: CaseClauseData,
    ) -> NodeIndex {
        let expression = data.expression;
        let statements = data.statements.clone();
        let data_index = self.len_u32(self.case_clauses.len());
        self.case_clauses.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent_list(&statements, parent);
        parent
    }

    /// Add a try statement node
    pub fn add_try(&mut self, kind: u16, pos: u32, end: u32, data: TryData) -> NodeIndex {
        let try_block = data.try_block;
        let catch_clause = data.catch_clause;
        let finally_block = data.finally_block;
        let data_index = self.len_u32(self.try_data.len());
        self.try_data.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(try_block, parent);
        self.set_parent(catch_clause, parent);
        self.set_parent(finally_block, parent);
        parent
    }

    /// Add a catch clause node
    pub fn add_catch_clause(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: CatchClauseData,
    ) -> NodeIndex {
        let variable_declaration = data.variable_declaration;
        let block = data.block;
        let data_index = self.len_u32(self.catch_clauses.len());
        self.catch_clauses.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(variable_declaration, parent);
        self.set_parent(block, parent);

        parent
    }

    /// Add a labeled statement node
    pub fn add_labeled(&mut self, kind: u16, pos: u32, end: u32, data: LabeledData) -> NodeIndex {
        let label = data.label;
        let statement = data.statement;
        let data_index = self.len_u32(self.labeled_data.len());
        self.labeled_data.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(label, parent);
        self.set_parent(statement, parent);
        parent
    }

    /// Add a break/continue statement node
    pub fn add_jump(&mut self, kind: u16, pos: u32, end: u32, data: JumpData) -> NodeIndex {
        let label = data.label;
        let data_index = self.len_u32(self.jump_data.len());
        self.jump_data.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(label, parent);
        parent
    }

    /// Add a with statement node
    pub fn add_with(&mut self, kind: u16, pos: u32, end: u32, data: WithData) -> NodeIndex {
        let expression = data.expression;
        let statement = data.statement;
        let data_index = self.len_u32(self.with_data.len());
        self.with_data.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent(statement, parent);
        parent
    }

    /// Add a type reference node
    pub fn add_type_ref(&mut self, kind: u16, pos: u32, end: u32, data: TypeRefData) -> NodeIndex {
        let type_name = data.type_name;
        let type_arguments = data.type_arguments.clone();
        let data_index = self.len_u32(self.type_refs.len());
        self.type_refs.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(type_name, parent);
        self.set_parent_opt_list(type_arguments.as_ref(), parent);
        parent
    }

    /// Add a union/intersection type node
    pub fn add_composite_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: CompositeTypeData,
    ) -> NodeIndex {
        let types = data.types.clone();

        let data_index = self.len_u32(self.composite_types.len());
        self.composite_types.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_list(&types, parent);

        parent
    }

    /// Add a function/constructor type node
    pub fn add_function_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: FunctionTypeData,
    ) -> NodeIndex {
        let type_parameters = data.type_parameters.clone();
        let parameters = data.parameters.clone();
        let type_annotation = data.type_annotation;

        let data_index = self.len_u32(self.function_types.len());
        self.function_types.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(type_parameters.as_ref(), parent);
        self.set_parent_list(&parameters, parent);
        self.set_parent(type_annotation, parent);

        parent
    }

    /// Add a type query node (typeof)
    pub fn add_type_query(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeQueryData,
    ) -> NodeIndex {
        let expr_name = data.expr_name;
        let type_arguments = data.type_arguments.clone();
        let data_index = self.len_u32(self.type_queries.len());
        self.type_queries.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expr_name, parent);
        self.set_parent_opt_list(type_arguments.as_ref(), parent);
        parent
    }

    /// Add a type literal node
    pub fn add_type_literal(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeLiteralData,
    ) -> NodeIndex {
        let members = data.members.clone();
        let data_index = self.len_u32(self.type_literals.len());
        self.type_literals.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_list(&members, parent);
        parent
    }

    /// Add an array type node
    pub fn add_array_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ArrayTypeData,
    ) -> NodeIndex {
        let element_type = data.element_type;
        let data_index = self.len_u32(self.array_types.len());
        self.array_types.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(element_type, parent);
        parent
    }

    /// Add a tuple type node
    pub fn add_tuple_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TupleTypeData,
    ) -> NodeIndex {
        let elements = data.elements.clone();
        let data_index = self.len_u32(self.tuple_types.len());
        self.tuple_types.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_list(&elements, parent);
        parent
    }

    /// Add an optional/rest type node
    pub fn add_wrapped_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: WrappedTypeData,
    ) -> NodeIndex {
        let type_node = data.type_node;
        let data_index = self.len_u32(self.wrapped_types.len());
        self.wrapped_types.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(type_node, parent);
        parent
    }

    /// Add a conditional type node
    pub fn add_conditional_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ConditionalTypeData,
    ) -> NodeIndex {
        let check_type = data.check_type;
        let extends_type = data.extends_type;
        let true_type = data.true_type;
        let false_type = data.false_type;
        let data_index = self.len_u32(self.conditional_types.len());
        self.conditional_types.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(check_type, parent);
        self.set_parent(extends_type, parent);
        self.set_parent(true_type, parent);
        self.set_parent(false_type, parent);
        parent
    }

    /// Add an infer type node
    pub fn add_infer_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: InferTypeData,
    ) -> NodeIndex {
        let type_parameter = data.type_parameter;
        let data_index = self.len_u32(self.infer_types.len());
        self.infer_types.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(type_parameter, parent);
        parent
    }

    /// Add a type operator node (keyof, unique, readonly)
    pub fn add_type_operator(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeOperatorData,
    ) -> NodeIndex {
        let type_node = data.type_node;
        let data_index = self.len_u32(self.type_operators.len());
        self.type_operators.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(type_node, parent);
        parent
    }

    /// Add an indexed access type node
    pub fn add_indexed_access_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: IndexedAccessTypeData,
    ) -> NodeIndex {
        let object_type = data.object_type;
        let index_type = data.index_type;
        let data_index = self.len_u32(self.indexed_access_types.len());
        self.indexed_access_types.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(object_type, parent);
        self.set_parent(index_type, parent);
        parent
    }

    /// Add a mapped type node
    pub fn add_mapped_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: MappedTypeData,
    ) -> NodeIndex {
        let readonly_token = data.readonly_token;
        let type_parameter = data.type_parameter;
        let name_type = data.name_type;
        let question_token = data.question_token;
        let type_node = data.type_node;
        let members = data.members.clone();
        let data_index = self.len_u32(self.mapped_types.len());
        self.mapped_types.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(readonly_token, parent);
        self.set_parent(type_parameter, parent);
        self.set_parent(name_type, parent);
        self.set_parent(question_token, parent);
        self.set_parent(type_node, parent);
        self.set_parent_opt_list(members.as_ref(), parent);
        parent
    }

    /// Add a literal type node
    pub fn add_literal_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: LiteralTypeData,
    ) -> NodeIndex {
        let literal = data.literal;
        let data_index = self.len_u32(self.literal_types.len());
        self.literal_types.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(literal, parent);
        parent
    }

    /// Add a template literal type node
    pub fn add_template_literal_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TemplateLiteralTypeData,
    ) -> NodeIndex {
        let head = data.head;
        let template_spans = data.template_spans.clone();
        let data_index = self.len_u32(self.template_literal_types.len());
        self.template_literal_types.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(head, parent);
        self.set_parent_list(&template_spans, parent);
        parent
    }

    /// Add a named tuple member node
    pub fn add_named_tuple_member(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: NamedTupleMemberData,
    ) -> NodeIndex {
        let name = data.name;
        let type_node = data.type_node;

        let data_index = self.len_u32(self.named_tuple_members.len());
        self.named_tuple_members.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(name, parent);
        self.set_parent(type_node, parent);
        parent
    }

    /// Add a type predicate node
    pub fn add_type_predicate(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypePredicateData,
    ) -> NodeIndex {
        let parameter_name = data.parameter_name;
        let type_node = data.type_node;

        let data_index = self.len_u32(self.type_predicates.len());
        self.type_predicates.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(parameter_name, parent);
        self.set_parent(type_node, parent);
        parent
    }

    /// Add an import declaration node
    pub fn add_import_decl(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ImportDeclData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let import_clause = data.import_clause;
        let module_specifier = data.module_specifier;
        let attributes = data.attributes;

        let data_index = self.len_u32(self.import_decls.len());
        self.import_decls.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(import_clause, parent);
        self.set_parent(module_specifier, parent);
        self.set_parent(attributes, parent);
        parent
    }

    /// Add an import clause node
    pub fn add_import_clause(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ImportClauseData,
    ) -> NodeIndex {
        let name = data.name;
        let named_bindings = data.named_bindings;

        let data_index = self.len_u32(self.import_clauses.len());
        self.import_clauses.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(name, parent);
        self.set_parent(named_bindings, parent);
        parent
    }

    /// Add a namespace/named imports node
    pub fn add_named_imports(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: NamedImportsData,
    ) -> NodeIndex {
        let name = data.name;
        let elements = data.elements.clone();

        let data_index = self.len_u32(self.named_imports.len());
        self.named_imports.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(name, parent);
        self.set_parent_list(&elements, parent);
        parent
    }

    /// Add an import/export specifier node
    pub fn add_specifier(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: SpecifierData,
    ) -> NodeIndex {
        let property_name = data.property_name;
        let name = data.name;

        let data_index = self.len_u32(self.specifiers.len());
        self.specifiers.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(property_name, parent);
        self.set_parent(name, parent);
        parent
    }

    /// Add an export declaration node
    pub fn add_export_decl(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ExportDeclData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let export_clause = data.export_clause;
        let module_specifier = data.module_specifier;
        let attributes = data.attributes;

        let data_index = self.len_u32(self.export_decls.len());
        self.export_decls.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(export_clause, parent);
        self.set_parent(module_specifier, parent);
        self.set_parent(attributes, parent);
        parent
    }
    /// Add an export assignment node
    pub fn add_export_assignment(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ExportAssignmentData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let expression = data.expression;

        let data_index = self.len_u32(self.export_assignments.len());
        self.export_assignments.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(expression, parent);
        parent
    }

    /// Add an import attributes node
    pub fn add_import_attributes(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ImportAttributesData,
    ) -> NodeIndex {
        let elements = data.elements.clone();

        let data_index = self.len_u32(self.import_attributes.len());
        self.import_attributes.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_list(&elements, parent);
        parent
    }

    /// Add an import attribute node
    pub fn add_import_attribute(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ImportAttributeData,
    ) -> NodeIndex {
        let name = data.name;
        let value = data.value;

        let data_index = self.len_u32(self.import_attribute.len());
        self.import_attribute.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(name, parent);
        self.set_parent(value, parent);
        parent
    }

    /// Add a binding pattern node
    pub fn add_binding_pattern(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: BindingPatternData,
    ) -> NodeIndex {
        let elements = data.elements.clone();

        let data_index = self.len_u32(self.binding_patterns.len());
        self.binding_patterns.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_list(&elements, parent);
        parent
    }

    /// Add a binding element node
    pub fn add_binding_element(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: BindingElementData,
    ) -> NodeIndex {
        let property_name = data.property_name;
        let name = data.name;
        let initializer = data.initializer;

        let data_index = self.len_u32(self.binding_elements.len());
        self.binding_elements.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(property_name, parent);
        self.set_parent(name, parent);
        self.set_parent(initializer, parent);
        parent
    }

    /// Add a property assignment node
    pub fn add_property_assignment(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: PropertyAssignmentData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let initializer = data.initializer;

        let data_index = self.len_u32(self.property_assignments.len());
        self.property_assignments.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent(initializer, parent);
        parent
    }

    /// Add a shorthand property assignment node
    pub fn add_shorthand_property(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ShorthandPropertyData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let object_assignment_initializer = data.object_assignment_initializer;

        let data_index = self.len_u32(self.shorthand_properties.len());
        self.shorthand_properties.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent(object_assignment_initializer, parent);
        parent
    }

    /// Add a spread assignment node
    pub fn add_spread(&mut self, kind: u16, pos: u32, end: u32, data: SpreadData) -> NodeIndex {
        let expression = data.expression;

        let data_index = self.len_u32(self.spread_data.len());
        self.spread_data.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        parent
    }

    /// Add a JSX element node
    pub fn add_jsx_element(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxElementData,
    ) -> NodeIndex {
        let opening_element = data.opening_element;
        let children = data.children.clone();
        let closing_element = data.closing_element;

        let data_index = self.len_u32(self.jsx_elements.len());
        self.jsx_elements.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(opening_element, parent);
        self.set_parent_list(&children, parent);
        self.set_parent(closing_element, parent);
        parent
    }

    /// Add a JSX opening/self-closing element node
    pub fn add_jsx_opening(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxOpeningData,
    ) -> NodeIndex {
        let tag_name = data.tag_name;
        let type_arguments = data.type_arguments.clone();
        let attributes = data.attributes;

        let data_index = self.len_u32(self.jsx_opening.len());
        self.jsx_opening.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(tag_name, parent);
        self.set_parent_opt_list(type_arguments.as_ref(), parent);
        self.set_parent(attributes, parent);
        parent
    }

    /// Add a JSX closing element node
    pub fn add_jsx_closing(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxClosingData,
    ) -> NodeIndex {
        let tag_name = data.tag_name;

        let data_index = self.len_u32(self.jsx_closing.len());
        self.jsx_closing.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(tag_name, parent);
        parent
    }

    /// Add a JSX fragment node
    pub fn add_jsx_fragment(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxFragmentData,
    ) -> NodeIndex {
        let opening_fragment = data.opening_fragment;
        let children = data.children.clone();
        let closing_fragment = data.closing_fragment;

        let data_index = self.len_u32(self.jsx_fragments.len());
        self.jsx_fragments.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(opening_fragment, parent);
        self.set_parent_list(&children, parent);
        self.set_parent(closing_fragment, parent);
        parent
    }

    /// Add a JSX attributes node
    pub fn add_jsx_attributes(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxAttributesData,
    ) -> NodeIndex {
        let properties = data.properties.clone();

        let data_index = self.len_u32(self.jsx_attributes.len());
        self.jsx_attributes.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_list(&properties, parent);
        parent
    }

    /// Add a JSX attribute node
    pub fn add_jsx_attribute(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxAttributeData,
    ) -> NodeIndex {
        let name = data.name;
        let initializer = data.initializer;

        let data_index = self.len_u32(self.jsx_attribute.len());
        self.jsx_attribute.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(name, parent);
        self.set_parent(initializer, parent);
        parent
    }

    /// Add a JSX spread attribute node
    pub fn add_jsx_spread_attribute(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxSpreadAttributeData,
    ) -> NodeIndex {
        let expression = data.expression;

        let data_index = self.len_u32(self.jsx_spread_attributes.len());
        self.jsx_spread_attributes.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        parent
    }

    /// Add a JSX expression node
    pub fn add_jsx_expression(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxExpressionData,
    ) -> NodeIndex {
        let expression = data.expression;

        let data_index = self.len_u32(self.jsx_expressions.len());
        self.jsx_expressions.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        parent
    }

    /// Add a JSX text node
    pub fn add_jsx_text(&mut self, kind: u16, pos: u32, end: u32, data: JsxTextData) -> NodeIndex {
        let data_index = self.len_u32(self.jsx_text.len());
        self.jsx_text.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        NodeIndex(index)
    }

    /// Add a JSX namespaced name node
    pub fn add_jsx_namespaced_name(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxNamespacedNameData,
    ) -> NodeIndex {
        let namespace = data.namespace;
        let name = data.name;

        let data_index = self.len_u32(self.jsx_namespaced_names.len());
        self.jsx_namespaced_names.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(namespace, parent);
        self.set_parent(name, parent);
        parent
    }

    /// Add a variable declaration node (individual)
    pub fn add_variable_declaration(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: VariableDeclarationData,
    ) -> NodeIndex {
        let name = data.name;
        let type_annotation = data.type_annotation;
        let initializer = data.initializer;

        let data_index = self.len_u32(self.variable_declarations.len());
        self.variable_declarations.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(name, parent);
        self.set_parent(type_annotation, parent);
        self.set_parent(initializer, parent);

        parent
    }

    /// Add a for-in/for-of statement node
    pub fn add_for_in_of(&mut self, kind: u16, pos: u32, end: u32, data: ForInOfData) -> NodeIndex {
        let initializer = data.initializer;
        let expression = data.expression;
        let statement = data.statement;
        let data_index = self.len_u32(self.for_in_of.len());
        self.for_in_of.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(initializer, parent);
        self.set_parent(expression, parent);
        self.set_parent(statement, parent);
        parent
    }
}
