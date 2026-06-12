//! Canonical registry of the `NodeArenaInner` typed data pools.
//!
//! Every typed data pool of `NodeArenaInner` (`crate::parser::node`) is listed
//! exactly once in [`for_each_node_pool!`] as `field_name => ElementType`. All
//! pool-shaped surfaces are generated from this single table via callback
//! macros:
//!
//! - the `NodeArenaInner` pool field declarations (`node.rs`),
//! - the `NodeArenaPoolLengths` snapshot struct (`node.rs`),
//! - `pool_checkpoint` / `restore_pool_checkpoint` (`node.rs`),
//! - the per-pool capacity sum inside `estimated_size_bytes` (`node.rs`),
//! - the per-pool clearing inside `clear` (`node_arena/mod.rs`).
//!
//! Because the struct fields themselves are macro-generated, a new pool can
//! only be added by editing this table — at which point every generated
//! surface picks it up automatically and none of them can drift.
//!
//! The table deliberately excludes the three non-pool fields of
//! `NodeArenaInner` (`nodes`, `interner`, `extended_info`): they are not
//! homogeneous `Vec<Data>` pools (the interner is not a `Vec`; node headers
//! and extended info are indexed by `NodeIndex`, not by a per-kind data
//! index), and the speculation checkpoint intentionally does not truncate
//! them through this registry.
//!
//! # Usage
//!
//! Define a callback `macro_rules!` whose matcher is
//! `($($pool:ident => $elem:ty),+ $(,)?)` and pass its name to
//! [`for_each_node_pool!`]:
//!
//! ```ignore
//! macro_rules! impl_my_surface {
//!     ($($pool:ident => $elem:ty),+ $(,)?) => { /* ... */ };
//! }
//! for_each_node_pool!(impl_my_surface);
//! ```

/// Invoke `$callback!` with the full `field => ElementType` pool table.
///
/// This is the single source of truth for which typed data pools exist on
/// `NodeArenaInner`, in struct-declaration (and thus serde) order. See the
/// module docs for the list of generated surfaces and for how to write a
/// callback macro.
macro_rules! for_each_node_pool {
    ($callback:ident) => {
        $callback! {
            // Names and identifiers
            identifiers => IdentifierData,
            qualified_names => QualifiedNameData,
            computed_properties => ComputedPropertyData,
            // Literals
            literals => LiteralData,
            // Expressions
            binary_exprs => BinaryExprData,
            unary_exprs => UnaryExprData,
            call_exprs => CallExprData,
            access_exprs => AccessExprData,
            conditional_exprs => ConditionalExprData,
            literal_exprs => LiteralExprData,
            parenthesized => ParenthesizedData,
            unary_exprs_ex => UnaryExprDataEx,
            type_assertions => TypeAssertionData,
            template_exprs => TemplateExprData,
            template_spans => TemplateSpanData,
            tagged_templates => TaggedTemplateData,
            // Functions and classes
            functions => FunctionData,
            classes => ClassData,
            interfaces => InterfaceData,
            type_aliases => TypeAliasData,
            enums => EnumData,
            enum_members => EnumMemberData,
            modules => ModuleData,
            module_blocks => ModuleBlockData,
            // Signatures and members
            signatures => SignatureData,
            index_signatures => IndexSignatureData,
            property_decls => PropertyDeclData,
            method_decls => MethodDeclData,
            constructors => ConstructorData,
            accessors => AccessorData,
            parameters => ParameterData,
            type_parameters => TypeParameterData,
            decorators => DecoratorData,
            heritage_clauses => HeritageData,
            expr_with_type_args => ExprWithTypeArgsData,
            // Statements
            if_statements => IfStatementData,
            loops => LoopData,
            blocks => BlockData,
            variables => VariableData,
            return_data => ReturnData,
            expr_statements => ExprStatementData,
            switch_data => SwitchData,
            case_clauses => CaseClauseData,
            try_data => TryData,
            catch_clauses => CatchClauseData,
            labeled_data => LabeledData,
            jump_data => JumpData,
            with_data => WithData,
            // Types
            type_refs => TypeRefData,
            composite_types => CompositeTypeData,
            function_types => FunctionTypeData,
            type_queries => TypeQueryData,
            type_literals => TypeLiteralData,
            array_types => ArrayTypeData,
            tuple_types => TupleTypeData,
            wrapped_types => WrappedTypeData,
            conditional_types => ConditionalTypeData,
            infer_types => InferTypeData,
            type_operators => TypeOperatorData,
            indexed_access_types => IndexedAccessTypeData,
            mapped_types => MappedTypeData,
            literal_types => LiteralTypeData,
            template_literal_types => TemplateLiteralTypeData,
            named_tuple_members => NamedTupleMemberData,
            type_predicates => TypePredicateData,
            // Import/export
            import_decls => ImportDeclData,
            import_clauses => ImportClauseData,
            named_imports => NamedImportsData,
            specifiers => SpecifierData,
            export_decls => ExportDeclData,
            export_assignments => ExportAssignmentData,
            import_attributes => ImportAttributesData,
            import_attribute => ImportAttributeData,
            // Binding patterns
            binding_patterns => BindingPatternData,
            binding_elements => BindingElementData,
            // Object literal members
            property_assignments => PropertyAssignmentData,
            shorthand_properties => ShorthandPropertyData,
            spread_data => SpreadData,
            // Variable declarations (individual)
            variable_declarations => VariableDeclarationData,
            // For-in/for-of
            for_in_of => ForInOfData,
            // JSX
            jsx_elements => JsxElementData,
            jsx_opening => JsxOpeningData,
            jsx_closing => JsxClosingData,
            jsx_fragments => JsxFragmentData,
            jsx_attributes => JsxAttributesData,
            jsx_attribute => JsxAttributeData,
            jsx_spread_attributes => JsxSpreadAttributeData,
            jsx_expressions => JsxExpressionData,
            jsx_text => JsxTextData,
            jsx_namespaced_names => JsxNamespacedNameData,
            // Source file
            source_files => SourceFileData,
        }
    };
}

pub(crate) use for_each_node_pool;
