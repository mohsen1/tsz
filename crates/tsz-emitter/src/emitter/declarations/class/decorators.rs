use super::super::super::{Printer, ScriptTarget};
use crate::context::transform::TransformDirective;
use tsz_parser::parser::node::{Node, NodeAccess};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_parser::syntax::transform_utils::collect_class_computed_name_this_references;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    // =========================================================================
    // Classes — Decorator Helpers
    // =========================================================================

    pub(in crate::emitter) fn collect_class_decorators(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> Vec<NodeIndex> {
        let Some(mods) = modifiers else {
            return Vec::new();
        };
        mods.nodes
            .iter()
            .copied()
            .filter(|&mod_idx| {
                self.arena
                    .get(mod_idx)
                    .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
            })
            .collect()
    }

    pub(in crate::emitter) fn emit_class_expression_with_captured_computed_names(
        &mut self,
        node: &Node,
        idx: NodeIndex,
    ) {
        let saved_transforms = self.transforms.clone();

        if let Some(alias) = self.scoped_static_this_alias.as_ref().cloned() {
            for this_ref in collect_class_computed_name_this_references(self.arena, idx) {
                self.transforms.insert(
                    this_ref,
                    TransformDirective::SubstituteThis {
                        capture_name: alias.clone(),
                    },
                );
            }
        }

        self.with_scoped_static_initializer_context_cleared(|this| {
            this.emit_class_declaration(node, idx);
        });
        self.transforms = saved_transforms;
    }

    /// Get the name of a class member for use in `__decorate` calls.
    /// Handles identifiers, string literals, numeric literals, and computed property
    /// names whose expression is a string literal (e.g. `["method"]`).
    fn get_decorator_member_name(&self, name_idx: NodeIndex) -> String {
        if name_idx.is_none() {
            return String::new();
        }
        // Try identifier first
        let text = self.get_identifier_text_idx(name_idx);
        if !text.is_empty() {
            return text;
        }
        // Check if it's a computed property name
        let Some(name_node) = self.arena.get(name_idx) else {
            return String::new();
        };
        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(cp) = self.arena.get_computed_property(name_node)
        {
            let expr_idx = cp.expression;
            if let Some(expr_node) = self.arena.get(expr_idx) {
                // String literal: ["method"] → "method"
                if expr_node.kind == SyntaxKind::StringLiteral as u16
                    && let Some(text) = self.arena.get_literal_text(expr_idx)
                {
                    return text.to_string();
                }
                // Numeric literal: [1] → "1"
                if expr_node.kind == SyntaxKind::NumericLiteral as u16
                    && let Some(text) = self.arena.get_literal_text(expr_idx)
                {
                    return text.to_string();
                }
            }
        }
        // String/numeric literal directly as property name
        if (name_node.kind == SyntaxKind::StringLiteral as u16
            || name_node.kind == SyntaxKind::NumericLiteral as u16)
            && let Some(text) = self.arena.get_literal_text(name_idx)
        {
            return text.to_string();
        }
        String::new()
    }

    /// Collect parameter decorators from a parameter list.
    /// Returns Vec of (`param_index`, `decorator_node_indices`) for parameters that have decorators.
    /// The index accounts for `this` parameter stripping — `this` is TypeScript-only
    /// and is not emitted, so decorator indices must be adjusted.
    fn collect_param_decorators(&self, parameters: &NodeList) -> Vec<(usize, Vec<NodeIndex>)> {
        let mut result = Vec::new();
        let mut runtime_index = 0usize;
        for &param_idx in &parameters.nodes {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };

            // Skip `this` parameter — it's erased in JS output and shouldn't
            // affect parameter decorator indices.
            let is_this_param = self.arena.get(param.name).is_some_and(|name_node| {
                name_node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16
                    || (name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                        && self
                            .arena
                            .get_identifier(name_node)
                            .is_some_and(|id| id.escaped_text == "this"))
            });
            if is_this_param {
                continue;
            }

            let decorators = self.collect_class_decorators(&param.modifiers);
            if !decorators.is_empty() {
                result.push((runtime_index, decorators));
            }
            runtime_index += 1;
        }
        result
    }

    /// Collect parameter decorators from the constructor of a class.
    /// Finds the constructor among class members, then collects decorators from its parameters.
    pub(in crate::emitter) fn collect_constructor_param_decorators(
        &self,
        members: &[NodeIndex],
    ) -> Vec<(usize, Vec<NodeIndex>)> {
        for &member_idx in members {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let Some(ctor) = self.arena.get_constructor(member_node) else {
                continue;
            };
            return self.collect_param_decorators(&ctor.parameters);
        }
        Vec::new()
    }

    // =========================================================================
    // Decorator Metadata
    // =========================================================================

    /// Serialize a type annotation node to its runtime metadata representation.
    /// Returns a string like "String", "Number", "Function", "Object", "void 0", etc.
    /// Uses `self.metadata_class_type_params` for in-scope type parameters; references
    /// to these are serialized as `"Object"` (matching tsc behavior).
    fn serialize_type_for_metadata(&self, type_idx: NodeIndex) -> String {
        let type_param_names = self.metadata_class_type_params.as_deref().unwrap_or(&[]);
        let Some(type_node) = self.arena.get(type_idx) else {
            return "Object".to_string();
        };

        let kind = type_node.kind;
        let sk = |s: SyntaxKind| s as u16;

        match kind {
            // Keyword types → wrapper constructors
            k if k == sk(SyntaxKind::StringKeyword) => "String".to_string(),
            k if k == sk(SyntaxKind::NumberKeyword) => "Number".to_string(),
            k if k == sk(SyntaxKind::BooleanKeyword) => "Boolean".to_string(),
            k if k == sk(SyntaxKind::SymbolKeyword) => "Symbol".to_string(),
            k if k == sk(SyntaxKind::BigIntKeyword) => "BigInt".to_string(),
            k if k == sk(SyntaxKind::VoidKeyword) => "void 0".to_string(),
            k if k == sk(SyntaxKind::UndefinedKeyword) => "void 0".to_string(),
            k if k == sk(SyntaxKind::NullKeyword) => "void 0".to_string(),
            k if k == sk(SyntaxKind::NeverKeyword) => "void 0".to_string(),
            k if k == sk(SyntaxKind::AnyKeyword) => "Object".to_string(),
            k if k == sk(SyntaxKind::UnknownKeyword) => "Object".to_string(),
            k if k == sk(SyntaxKind::ObjectKeyword) => "Object".to_string(),

            // Type reference → emit the type name (class/enum reference).
            // If the referenced name is a type parameter, emit "Object" instead.
            // If it's a built-in keyword type name (string, number, etc.) used as
            // a type reference, map to the wrapper constructor.
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.arena.get_type_ref(type_node) {
                    let name = self.get_identifier_text_idx(type_ref.type_name);
                    if !name.is_empty() {
                        if type_param_names.iter().any(|tp| tp == &name) {
                            return "Object".to_string();
                        }
                        // Map keyword type names to their wrapper constructors
                        match name.as_str() {
                            "string" => return "String".to_string(),
                            "number" => return "Number".to_string(),
                            "boolean" => return "Boolean".to_string(),
                            "symbol" => return "Symbol".to_string(),
                            "bigint" => return "BigInt".to_string(),
                            "void" | "undefined" | "null" | "never" => return "void 0".to_string(),
                            "any" | "unknown" | "object" => return "Object".to_string(),
                            _ => return name,
                        }
                    }
                }
                "Object".to_string()
            }

            // Array types → Array
            k if k == syntax_kind_ext::ARRAY_TYPE => "Array".to_string(),
            k if k == syntax_kind_ext::TUPLE_TYPE => "Array".to_string(),

            // Function/constructor types → Function
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                "Function".to_string()
            }

            // Union type → try to unwrap to single non-null/undefined type,
            // or check if all meaningful members serialize to the same type.
            k if k == syntax_kind_ext::UNION_TYPE => {
                if let Some(composite) = self.arena.get_composite_type(type_node) {
                    let strict_null_checks = self.ctx.options.strict_null_checks;
                    // Filter out null, undefined, void, never from union members.
                    // When strictNullChecks is true, null and undefined are meaningful
                    // types in unions and should NOT be stripped (only never is stripped).
                    let meaningful: Vec<NodeIndex> = composite
                        .types
                        .nodes
                        .iter()
                        .copied()
                        .filter(|&member_idx| {
                            let Some(member) = self.arena.get(member_idx) else {
                                return false;
                            };
                            let sk = |s: SyntaxKind| s as u16;
                            // Always skip never
                            if member.kind == sk(SyntaxKind::NeverKeyword) {
                                return false;
                            }
                            // Skip null/undefined/void only when strictNullChecks is false
                            if !strict_null_checks
                                && (member.kind == sk(SyntaxKind::NullKeyword)
                                    || member.kind == sk(SyntaxKind::UndefinedKeyword)
                                    || member.kind == sk(SyntaxKind::VoidKeyword))
                            {
                                return false;
                            }
                            // Skip literal type null (only when strictNullChecks is false)
                            if !strict_null_checks
                                && member.kind == syntax_kind_ext::LITERAL_TYPE
                                && let Some(lit) = self.arena.get_literal_type(member)
                                && let Some(lit_node) = self.arena.get(lit.literal)
                                && lit_node.kind == sk(SyntaxKind::NullKeyword)
                            {
                                return false;
                            }
                            // Skip TypeReference to null/undefined/void/never
                            if member.kind == syntax_kind_ext::TYPE_REFERENCE
                                && let Some(type_ref) = self.arena.get_type_ref(member)
                            {
                                let ref_name = self.get_identifier_text_idx(type_ref.type_name);
                                if ref_name == "never" {
                                    return false;
                                }
                                if !strict_null_checks
                                    && matches!(ref_name.as_str(), "null" | "undefined" | "void")
                                {
                                    return false;
                                }
                            }
                            true
                        })
                        .collect();
                    if meaningful.len() == 1 {
                        return self.serialize_type_for_metadata(meaningful[0]);
                    }
                    // If all meaningful members serialize to the same type, use that
                    if meaningful.len() > 1 {
                        let first = self.serialize_type_for_metadata(meaningful[0]);
                        if first != "Object"
                            && meaningful[1..]
                                .iter()
                                .all(|&m| self.serialize_type_for_metadata(m) == first)
                        {
                            return first;
                        }
                    }
                    // If only null/undefined/void/never, return void 0
                    if meaningful.is_empty() {
                        return "void 0".to_string();
                    }
                }
                "Object".to_string()
            }

            // Intersection → Object
            k if k == syntax_kind_ext::INTERSECTION_TYPE => "Object".to_string(),

            // Parenthesized type → unwrap
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
                if let Some(wrapped) = self.arena.get_wrapped_type(type_node) {
                    return self.serialize_type_for_metadata(wrapped.type_node);
                }
                "Object".to_string()
            }

            // Literal types → infer from the literal kind
            k if k == syntax_kind_ext::LITERAL_TYPE => {
                if let Some(lit) = self.arena.get_literal_type(type_node)
                    && let Some(lit_node) = self.arena.get(lit.literal)
                {
                    return match lit_node.kind {
                        lk if lk == sk(SyntaxKind::StringLiteral) => "String".to_string(),
                        lk if lk == sk(SyntaxKind::NumericLiteral) => "Number".to_string(),
                        lk if lk == sk(SyntaxKind::BigIntLiteral) => "BigInt".to_string(),
                        lk if lk == sk(SyntaxKind::TrueKeyword)
                            || lk == sk(SyntaxKind::FalseKeyword) =>
                        {
                            "Boolean".to_string()
                        }
                        lk if lk == sk(SyntaxKind::NullKeyword) => "void 0".to_string(),
                        // Negative numeric literal: `-1` → PrefixUnaryExpression → Number
                        lk if lk == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                            "Number".to_string()
                        }
                        _ => "Object".to_string(),
                    };
                }
                "Object".to_string()
            }

            // This type → Object
            k if k == syntax_kind_ext::THIS_TYPE => "Object".to_string(),

            // Template literal type → String
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => "String".to_string(),

            // Type operator (readonly, keyof, unique) → unwrap and serialize inner type
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                if let Some(type_op) = self.arena.get_type_operator(type_node) {
                    return self.serialize_type_for_metadata(type_op.type_node);
                }
                "Object".to_string()
            }

            // Optional type (T?) → unwrap inner type
            k if k == syntax_kind_ext::OPTIONAL_TYPE => {
                if let Some(wrapped) = self.arena.get_wrapped_type(type_node) {
                    return self.serialize_type_for_metadata(wrapped.type_node);
                }
                "Object".to_string()
            }

            // Rest type (...T) → Object (used in tuples)
            k if k == syntax_kind_ext::REST_TYPE => "Object".to_string(),

            // Conditional, mapped, indexed access, type query, infer, import → Object
            _ => "Object".to_string(),
        }
    }

    /// Emit `__metadata("design:type", ...)` for a property.
    /// Caller must have already emitted a trailing comma+newline after decorators.
    fn emit_metadata_for_property(&mut self, type_annotation: NodeIndex) {
        let serialized = if type_annotation.is_some() {
            self.serialize_type_for_metadata(type_annotation)
        } else {
            "Object".to_string()
        };
        self.write_helper("__metadata");
        self.write("(\"design:type\", ");
        self.write(&serialized);
        self.write(")");
    }

    /// Emit metadata calls for a method: design:type, design:paramtypes, design:returntype.
    /// Caller must have already emitted a trailing comma+newline after decorators.
    fn emit_metadata_for_method(&mut self, parameters: &NodeList, return_type: NodeIndex) {
        // design:type is always Function for methods
        self.write_helper("__metadata");
        self.write("(\"design:type\", Function),");
        self.write_line();

        // design:paramtypes
        self.write_helper("__metadata");
        self.write("(\"design:paramtypes\", [");
        self.emit_serialized_param_types(parameters);
        self.write("]),");
        self.write_line();

        // design:returntype
        if return_type.is_some() {
            let serialized = self.serialize_type_for_metadata(return_type);
            self.write_helper("__metadata");
            self.write("(\"design:returntype\", ");
            self.write(&serialized);
            self.write(")");
        } else {
            self.write_helper("__metadata");
            self.write("(\"design:returntype\", void 0)");
        }
    }

    /// Emit serialized parameter types as comma-separated values.
    fn emit_serialized_param_types(&mut self, parameters: &NodeList) {
        let mut first = true;
        for &param_idx in &parameters.nodes {
            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_parameter(param_node)
            {
                // Skip `this` parameter — it's TypeScript-only and erased in JS emit.
                if let Some(name_node) = self.arena.get(param.name) {
                    let sk = |s: SyntaxKind| s as u16;
                    if name_node.kind == sk(SyntaxKind::ThisKeyword) {
                        continue;
                    }
                    if name_node.kind == sk(SyntaxKind::Identifier)
                        && let Some(id) = self.arena.get_identifier(name_node)
                        && id.escaped_text == "this"
                    {
                        continue;
                    }
                }
                if !first {
                    self.write(", ");
                }
                first = false;
                if param.dot_dot_dot_token {
                    // Rest parameter: serialize the element type if it's an array type,
                    // otherwise emit Object (matching tsc behavior).
                    let serialized = self.serialize_rest_param_element_type(param.type_annotation);
                    self.write(&serialized);
                } else if param.type_annotation.is_some() {
                    let serialized = self.serialize_type_for_metadata(param.type_annotation);
                    self.write(&serialized);
                } else {
                    self.write("Object");
                }
            }
        }
    }

    /// For a rest parameter, serialize the element type of the array type annotation.
    /// e.g., `...args: string[]` → "String", `...args: number[]` → "Number".
    /// If the type is not an array type or has no annotation, returns "Object".
    fn serialize_rest_param_element_type(&self, type_annotation: NodeIndex) -> String {
        if let Some(type_node) = self.arena.get(type_annotation)
            && type_node.kind == syntax_kind_ext::ARRAY_TYPE
            && let Some(arr) = self.arena.get_array_type(type_node)
        {
            return self.serialize_type_for_metadata(arr.element_type);
        }
        "Object".to_string()
    }

    /// Emit metadata for constructor paramtypes (used with class-level decorators).
    /// Caller must have already emitted a trailing comma+newline after decorators.
    fn emit_metadata_for_constructor_params(&mut self, members: &[NodeIndex]) {
        for &member_idx in members {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let Some(ctor) = self.arena.get_constructor(member_node) else {
                continue;
            };

            self.write_helper("__metadata");
            self.write("(\"design:paramtypes\", [");
            self.emit_serialized_param_types(&ctor.parameters);
            self.write("])");
            return;
        }
    }

    pub(in crate::emitter) fn emit_legacy_class_decorator_assignment(
        &mut self,
        class_name: &str,
        decorators: &[NodeIndex],
        commonjs_exported: bool,
        commonjs_default: bool,
        emit_commonjs_pre_assignment: bool,
        class_members: &[NodeIndex],
    ) {
        if class_name.is_empty() {
            return;
        }

        // Check for constructor parameter decorators up front
        let ctor_param_decorators_early = self.collect_constructor_param_decorators(class_members);

        // Early return only if there's truly nothing to emit at the class level.
        // Class-level __decorate is needed when:
        // 1. There are class-level decorators, OR
        // 2. There are constructor parameter decorators
        if decorators.is_empty() && ctor_param_decorators_early.is_empty() {
            return;
        }

        let emit_metadata = self.ctx.options.emit_decorator_metadata;

        if commonjs_exported && !commonjs_default && emit_commonjs_pre_assignment {
            self.write("exports.");
            self.write(class_name);
            self.write(" = ");
            self.write(class_name);
            self.write(";");
            self.write_line();
        }

        if commonjs_exported {
            if commonjs_default {
                self.write("exports.default = ");
            } else {
                self.write("exports.");
                self.write(class_name);
                self.write(" = ");
            }
        }

        // Collect constructor parameter decorators
        let ctor_param_decorators = self.collect_constructor_param_decorators(class_members);

        self.write(class_name);
        self.write(" = ");
        self.write_helper("__decorate");
        self.write("([");
        self.write_line();
        self.increase_indent();
        let has_param_decs = !ctor_param_decorators.is_empty();
        // Only emit metadata if the class actually has a constructor.
        // `emit_metadata_for_constructor_params` only emits for constructors,
        // so has_metadata must match to avoid trailing comma + empty line.
        let has_ctor = class_members.iter().any(|&m_idx| {
            self.arena
                .get(m_idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::CONSTRUCTOR)
        });
        let has_metadata = emit_metadata && has_ctor;
        let has_more_after_decs = has_param_decs || has_metadata;
        for (i, &dec_idx) in decorators.iter().enumerate() {
            if let Some(dec_node) = self.arena.get(dec_idx)
                && let Some(dec) = self.arena.get_decorator(dec_node)
            {
                self.emit(dec.expression);
                if i + 1 != decorators.len() || has_more_after_decs {
                    self.write(",");
                }
                self.write_line();
            }
        }
        // Emit __param(index, decorator) for constructor parameter decorators
        for (pi, (param_idx, param_decs)) in ctor_param_decorators.iter().enumerate() {
            for (di, &dec_idx) in param_decs.iter().enumerate() {
                if let Some(dec_node) = self.arena.get(dec_idx)
                    && let Some(dec) = self.arena.get_decorator(dec_node)
                {
                    self.write_helper("__param");
                    self.write("(");
                    self.write(&param_idx.to_string());
                    self.write(", ");
                    self.emit(dec.expression);
                    self.write(")");
                    let is_last_dec = di + 1 >= param_decs.len();
                    let is_last_param = pi + 1 >= ctor_param_decorators.len();
                    if !(is_last_dec && is_last_param) || has_metadata {
                        self.write(",");
                    }
                    self.write_line();
                }
            }
        }
        if has_metadata {
            self.emit_metadata_for_constructor_params(class_members);
            self.write_line();
        }
        self.decrease_indent();
        self.write("], ");
        self.write(class_name);
        self.write(");");
    }

    /// Collect decorated class members and emit `__decorate` calls for them.
    ///
    /// For legacy (experimental) decorators, tsc emits `__decorate` calls after the
    /// class body for each decorated member:
    /// - Methods/accessors: `__decorate([...], ClassName.prototype, "name", null);`
    /// - Properties: `__decorate([...], ClassName.prototype, "name", void 0);`
    /// - Static members: `__decorate([...], ClassName, "name", ...);`
    pub(in crate::emitter) fn emit_legacy_member_decorator_calls(
        &mut self,
        class_name: &str,
        members: &[NodeIndex],
    ) {
        if class_name.is_empty() {
            return;
        }

        let emit_metadata = self.ctx.options.emit_decorator_metadata;

        // Track accessor names that have already been emitted so that
        // getter/setter pairs produce only one __decorate call (the first one).
        let mut emitted_accessor_names = std::collections::HashSet::<String>::new();

        // Metadata info extracted per member
        enum MemberMetadata {
            Property {
                type_annotation: NodeIndex,
            },
            Method {
                parameters: NodeList,
                return_type: NodeIndex,
            },
            Accessor,
        }

        for &member_idx in members {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            let (modifiers, name_idx, is_property, is_accessor, metadata) = match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    // Skip overload signatures (no body) — decorators on overloads
                    // are not emitted as __decorate targets
                    if !method.body.is_some() {
                        continue;
                    }
                    let meta = MemberMetadata::Method {
                        parameters: method.parameters.clone(),
                        return_type: method.type_annotation,
                    };
                    (&method.modifiers, method.name, false, false, meta)
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    let is_auto_accessor = self
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword);
                    let meta = MemberMetadata::Property {
                        type_annotation: prop.type_annotation,
                    };
                    (&prop.modifiers, prop.name, !is_auto_accessor, false, meta)
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(accessor) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    (
                        &accessor.modifiers,
                        accessor.name,
                        false,
                        true,
                        MemberMetadata::Accessor,
                    )
                }
                _ => continue,
            };

            // Collect decorator nodes from modifiers
            let decorators = self.collect_class_decorators(modifiers);

            // Collect parameter decorators for methods
            let param_decorators: Vec<(usize, Vec<NodeIndex>)> =
                if let MemberMetadata::Method { ref parameters, .. } = metadata {
                    self.collect_param_decorators(parameters)
                } else {
                    Vec::new()
                };

            // Skip members with no decorators at all (neither member nor parameter level)
            if decorators.is_empty() && param_decorators.is_empty() {
                continue;
            }

            let is_static = self
                .arena
                .has_modifier(modifiers, SyntaxKind::StaticKeyword);

            let member_name = self.get_decorator_member_name(name_idx);
            if member_name.is_empty() {
                continue;
            }

            // For getter/setter pairs, tsc emits only one __decorate call
            // for the first accessor that has decorators. Skip the second.
            if is_accessor && !emitted_accessor_names.insert(member_name.clone()) {
                continue;
            }

            self.write_helper("__decorate");
            self.write("([");
            self.write_line();
            self.increase_indent();

            // Determine if metadata or param decorators will follow
            let will_emit_metadata = emit_metadata && !matches!(metadata, MemberMetadata::Accessor);
            let has_more = will_emit_metadata || !param_decorators.is_empty();

            for (i, &dec_idx) in decorators.iter().enumerate() {
                if let Some(dec_node) = self.arena.get(dec_idx)
                    && let Some(dec) = self.arena.get_decorator(dec_node)
                {
                    self.emit(dec.expression);
                    if i + 1 != decorators.len() || has_more {
                        self.write(",");
                    }
                    self.write_line();
                }
            }

            // Emit __param(index, decorator) for each parameter decorator
            for (pi, (param_idx, param_decs)) in param_decorators.iter().enumerate() {
                for (di, &dec_idx) in param_decs.iter().enumerate() {
                    if let Some(dec_node) = self.arena.get(dec_idx)
                        && let Some(dec) = self.arena.get_decorator(dec_node)
                    {
                        self.write_helper("__param");
                        self.write("(");
                        self.write(&param_idx.to_string());
                        self.write(", ");
                        self.emit(dec.expression);
                        self.write(")");
                        let is_last_dec = di + 1 >= param_decs.len();
                        let is_last_param = pi + 1 >= param_decorators.len();
                        if !(is_last_dec && is_last_param) || will_emit_metadata {
                            self.write(",");
                        }
                        self.write_line();
                    }
                }
            }

            // Emit metadata calls after decorators
            if will_emit_metadata {
                match metadata {
                    MemberMetadata::Property { type_annotation } => {
                        self.emit_metadata_for_property(type_annotation);
                        self.write_line();
                    }
                    MemberMetadata::Method {
                        ref parameters,
                        return_type,
                    } => {
                        self.emit_metadata_for_method(parameters, return_type);
                        self.write_line();
                    }
                    MemberMetadata::Accessor => {}
                }
            }

            self.decrease_indent();
            self.write("], ");
            self.write(class_name);
            if !is_static {
                self.write(".prototype");
            }
            self.write(", ");
            self.emit_string_literal_text(&member_name);
            if is_property {
                self.write(", void 0);");
            } else {
                self.write(", null);");
            }
            self.write_line();
        }
    }
}
