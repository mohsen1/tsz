#![allow(clippy::nonminimal_bool, clippy::type_complexity)]

use super::super::{Printer, ScriptTarget};
use crate::context::transform::TransformDirective;
use crate::emitter::core::PrivateMemberInfo;
use crate::transforms::private_fields_es5::{
    PrivateAccessorInfo, PrivateFieldInfo, PrivateMethodInfo, collect_private_accessors,
    collect_private_fields, collect_private_methods, get_private_field_name, is_private_identifier,
};
use crate::transforms::{ClassDecoratorInfo, ClassES5Emitter};
use tsz_parser::parser::node::{Node, NodeAccess};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_parser::syntax::transform_utils::{
    collect_class_computed_name_this_references, contains_super_reference, contains_this_reference,
};
use tsz_scanner::SyntaxKind;

use super::super::core::PropertyNameEmit;

/// Entry for a static field initializer that will be emitted after the class body.
/// Fields: (name, initializer node, member pos, leading comments with source pos, trailing comments)
type StaticFieldInit = (
    PropertyNameEmit,
    NodeIndex,
    u32,
    Vec<(String, u32)>,
    Vec<String>,
);
type AutoAccessorInfo = (NodeIndex, String, Option<NodeIndex>, bool);

/// Replace all occurrences of an identifier with a replacement, respecting word boundaries.
pub(crate) fn replace_identifier(text: &str, name: &str, replacement: &str) -> String {
    let bytes = text.as_bytes();
    let name_bytes = name.as_bytes();
    let name_len = name_bytes.len();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;
    while i + name_len <= bytes.len() {
        if let Some(pos) = text[i..].find(name) {
            let abs = i + pos;
            let before_ok = abs == 0
                || !(bytes[abs - 1].is_ascii_alphanumeric()
                    || bytes[abs - 1] == b'_'
                    || bytes[abs - 1] == b'$');
            let after_end = abs + name_len;
            let after_ok = after_end >= bytes.len()
                || !(bytes[after_end].is_ascii_alphanumeric()
                    || bytes[after_end] == b'_'
                    || bytes[after_end] == b'$');
            result.push_str(&text[i..abs]);
            if before_ok && after_ok {
                result.push_str(replacement);
            } else {
                result.push_str(name);
            }
            i = after_end;
        } else {
            result.push_str(&text[i..]);
            return result;
        }
    }
    result.push_str(&text[i..]);
    result
}

/// Check if a class body contains self-references to the class name.
/// This is needed for the `C_1` alias pattern in legacy decorator lowering.
/// When a decorated class references itself (e.g. `static x() { return C.y; }`),
/// tsc creates an alias `var C_1;` so the decorator can replace the class binding
/// without breaking internal references.
///
/// Uses source text scanning within member spans to detect references.
fn class_has_self_references(
    arena: &tsz_parser::parser::node::NodeArena,
    source_text: Option<&str>,
    class_name: &str,
    members: &[NodeIndex],
) -> bool {
    if class_name.is_empty() {
        return false;
    }
    let Some(src) = source_text else {
        return false;
    };

    for &member_idx in members {
        let Some(member_node) = arena.get(member_idx) else {
            continue;
        };

        // Get the span of the member body/initializer
        let body_span = match member_node.kind {
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let Some(method) = arena.get_method_decl(member_node) else {
                    continue;
                };
                arena
                    .get(method.body)
                    .map(|n| (n.pos as usize, n.end as usize))
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                let Some(prop) = arena.get_property_decl(member_node) else {
                    continue;
                };
                arena
                    .get(prop.initializer)
                    .map(|n| (n.pos as usize, n.end as usize))
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                let Some(acc) = arena.get_accessor(member_node) else {
                    continue;
                };
                arena
                    .get(acc.body)
                    .map(|n| (n.pos as usize, n.end as usize))
            }
            _ => continue,
        };

        if let Some((start, end)) = body_span
            && start < end
            && end <= src.len()
        {
            let body_text = &src[start..end];
            if text_contains_identifier(body_text, class_name) {
                return true;
            }
        }
    }
    false
}

/// Check if `text` contains `name` as an identifier (word boundary match).
fn text_contains_identifier(text: &str, name: &str) -> bool {
    let name_bytes = name.as_bytes();
    let text_bytes = text.as_bytes();
    let name_len = name_bytes.len();
    if name_len == 0 || text_bytes.len() < name_len {
        return false;
    }
    let mut i = 0;
    while i + name_len <= text_bytes.len() {
        if &text_bytes[i..i + name_len] == name_bytes {
            let before_ok = i == 0 || !is_ident_char(text_bytes[i - 1]);
            let after_ok =
                i + name_len == text_bytes.len() || !is_ident_char(text_bytes[i + name_len]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

const fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

impl<'a> Printer<'a> {
    // =========================================================================
    // Classes
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
    fn collect_constructor_param_decorators(
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

    /// Emit a class declaration.
    pub(in crate::emitter) fn emit_class_declaration(&mut self, node: &Node, idx: NodeIndex) {
        let Some(class) = self.arena.get_class(node) else {
            return;
        };

        // Skip ambient declarations (declare class)
        if self
            .arena
            .has_modifier(&class.modifiers, SyntaxKind::DeclareKeyword)
        {
            self.skip_comments_for_erased_node(node);
            return;
        }

        if let Some(class_name) = self.get_identifier_text_opt(class.name)
            && let Some(output) =
                self.render_simple_tc39_decorated_class_es5(node, idx, &class_name, &class_name)
        {
            self.write(&output);
            while self.comment_emit_idx < self.all_comments.len()
                && self.all_comments[self.comment_emit_idx].end <= node.end
            {
                self.comment_emit_idx += 1;
            }
            return;
        }

        let legacy_class_decorators = if self.ctx.options.legacy_decorators
            && node.kind == syntax_kind_ext::CLASS_DECLARATION
        {
            self.collect_class_decorators(&class.modifiers)
        } else {
            Vec::new()
        };

        // Check if any members have legacy decorators (method, property, accessor decorators)
        // Also checks for parameter decorators on methods and constructors.
        let has_legacy_member_decorators = self.ctx.options.legacy_decorators
            && class.members.nodes.iter().any(|&m_idx| {
                let Some(m_node) = self.arena.get(m_idx) else {
                    return false;
                };
                // Check member-level decorators
                let mods = match m_node.kind {
                    k if k == syntax_kind_ext::METHOD_DECLARATION => self
                        .arena
                        .get_method_decl(m_node)
                        .and_then(|m| m.modifiers.as_ref()),
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                        .arena
                        .get_property_decl(m_node)
                        .and_then(|p| p.modifiers.as_ref()),
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        self.arena
                            .get_accessor(m_node)
                            .and_then(|a| a.modifiers.as_ref())
                    }
                    _ => None,
                };
                let has_member_decorator = mods.is_some_and(|m| {
                    m.nodes.iter().any(|&mod_idx| {
                        self.arena
                            .get(mod_idx)
                            .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                    })
                });
                if has_member_decorator {
                    return true;
                }
                // Check parameter decorators on methods and constructors
                let params: Option<&tsz_parser::parser::NodeList> = match m_node.kind {
                    k if k == syntax_kind_ext::METHOD_DECLARATION => {
                        self.arena.get_method_decl(m_node).map(|m| &m.parameters)
                    }
                    k if k == syntax_kind_ext::CONSTRUCTOR => {
                        self.arena.get_constructor(m_node).map(|c| &c.parameters)
                    }
                    _ => None,
                };
                params.is_some_and(|p| {
                    p.nodes.iter().any(|&param_idx| {
                        let Some(param_node) = self.arena.get(param_idx) else {
                            return false;
                        };
                        let Some(param) = self.arena.get_parameter(param_node) else {
                            return false;
                        };
                        !self.collect_class_decorators(&param.modifiers).is_empty()
                    })
                })
            });

        if !legacy_class_decorators.is_empty() || has_legacy_member_decorators {
            let class_name = if class.name.is_none() {
                // For anonymous default exports with decorators, ensure we have a name
                // so __decorate calls can reference it (e.g., `default_1.prototype`)
                self.anonymous_default_export_name
                    .clone()
                    .unwrap_or_else(|| "default_1".to_string())
            } else {
                self.get_identifier_text_idx(class.name)
            };

            if self.ctx.target_es5 {
                let mut es5_emitter = ClassES5Emitter::new(self.arena);
                es5_emitter.set_temp_var_counter(self.ctx.destructuring_state.temp_var_counter);
                es5_emitter.set_indent_level(self.writer.indent_level());
                es5_emitter.set_transforms(self.transforms.clone());
                es5_emitter.set_remove_comments(self.ctx.options.remove_comments);
                if let Some(text) = self.source_text_for_map() {
                    if self.writer.has_source_map() {
                        es5_emitter
                            .set_source_map_context(text, self.writer.current_source_index());
                    } else {
                        es5_emitter.set_source_text(text);
                    }
                }
                if self.ctx.options.import_helpers && self.ctx.is_effectively_commonjs() {
                    es5_emitter.set_tslib_prefix(true);
                }
                es5_emitter
                    .set_use_define_for_class_fields(self.ctx.options.use_define_for_class_fields);
                // Pass decorator info to the ES5 emitter so __decorate calls
                // are emitted INSIDE the IIFE (before `return ClassName;`)
                es5_emitter.set_decorator_info(ClassDecoratorInfo {
                    class_decorators: legacy_class_decorators,
                    has_member_decorators: has_legacy_member_decorators,
                    emit_decorator_metadata: self.ctx.options.emit_decorator_metadata,
                });
                let output = es5_emitter.emit_class_with_name(idx, &class_name);
                self.ctx.destructuring_state.temp_var_counter = es5_emitter.temp_var_counter();
                let mappings = es5_emitter.take_mappings();
                if !mappings.is_empty() && self.writer.has_source_map() {
                    self.writer.write("");
                    let base_line = self.writer.current_line();
                    let base_column = self.writer.current_column();
                    self.writer
                        .add_offset_mappings(base_line, base_column, &mappings);
                    self.writer.write(&output);
                } else {
                    self.write(&output);
                }
                while self.comment_emit_idx < self.all_comments.len()
                    && self.all_comments[self.comment_emit_idx].end <= node.end
                {
                    self.comment_emit_idx += 1;
                }
                return;
            }

            if class_name.is_empty() {
                self.emit_class_es6_with_options(node, idx, false, None);
                return;
            }

            // For anonymous classes that got a generated name (e.g., "default_1"),
            // ensure `anonymous_default_export_name` is set so `emit_class_es6_with_options`
            // can inject the name into the class expression.
            let prev_anon_name =
                if class.name.is_none() && self.anonymous_default_export_name.is_none() {
                    self.anonymous_default_export_name = Some(class_name.clone());
                    true
                } else {
                    false
                };

            // Check if the class needs a class-level __decorate call due to constructor
            // parameter decorators (even without class-level decorators).
            let has_ctor_param_decorators = !self
                .collect_constructor_param_decorators(&class.members.nodes)
                .is_empty();
            // A class-level __decorate is needed for class decorators OR ctor param decorators
            let needs_class_decorate =
                !legacy_class_decorators.is_empty() || has_ctor_param_decorators;

            // Detect if the class body has self-references that need aliasing.
            // When a decorated class references itself (e.g. `static x() { return C.y; }`),
            // tsc emits: `var C_1; let C = C_1 = class C { static x() { return C_1.y; } };`
            let needs_alias = !legacy_class_decorators.is_empty()
                && class_has_self_references(
                    self.arena,
                    self.source_text_for_map(),
                    &class_name,
                    &class.members.nodes,
                );

            let alias_name = if needs_alias {
                let alias = format!("{class_name}_1");
                // Emit `var C_1;\n` before the class declaration
                self.write("var ");
                self.write(&alias);
                self.write(";");
                self.write_line();
                Some(alias)
            } else {
                None
            };

            // When there are class-level decorators or ctor param decorators,
            // emit as `let Name = class { ... };`
            // When only member decorators, emit as normal `class Name { ... }`
            if needs_class_decorate {
                if let Some(ref alias) = alias_name {
                    // Emit: `let Name = Name_1 = class Name { ... };`
                    // First capture the class body, then replace self-refs
                    let before_len = self.writer.len();
                    self.emit_class_es6_with_options(
                        node,
                        idx,
                        true,
                        Some(("let", class_name.clone())),
                    );
                    let after_len = self.writer.len();

                    // Post-process: replace class name with alias in class body
                    let full_output = self.writer.get_output().to_string();
                    let emitted_str = &full_output[before_len..after_len];

                    // The emitted text starts with `let Name = class Name {`
                    // We need to insert `Name_1 = ` after `let Name = `
                    // and replace body references to Name with Name_1
                    let prefix = format!("let {class_name} = class {class_name}");
                    let alias_prefix = format!("let {class_name} = {alias} = class {class_name}");

                    let mut replaced = emitted_str.replacen(&prefix, &alias_prefix, 1);

                    // Replace self-references ONLY inside the class body (between { and };)
                    // Static fields after the class close brace should keep the original name.
                    if let Some(brace_pos) = replaced.find('{') {
                        // Find the matching close of the class expression: `};\n` or `};`
                        // The class body ends at `\n};` (the closing brace of the class expr)
                        let close_pattern = "\n};";
                        let body_end =
                            if let Some(close_pos) = replaced[brace_pos..].find(close_pattern) {
                                brace_pos + close_pos + close_pattern.len()
                            } else {
                                replaced.len()
                            };

                        let header = &replaced[..brace_pos];
                        let class_body = &replaced[brace_pos..body_end];
                        let after_class = &replaced[body_end..];

                        // Only replace identifiers within the class body
                        let mut new_body = String::with_capacity(class_body.len());
                        let name_bytes = class_name.as_bytes();
                        let body_bytes = class_body.as_bytes();
                        let mut i = 0;
                        while i < body_bytes.len() {
                            if i + name_bytes.len() <= body_bytes.len()
                                && &body_bytes[i..i + name_bytes.len()] == name_bytes
                            {
                                let before_ok = i == 0 || !is_ident_char(body_bytes[i - 1]);
                                let after_ok = i + name_bytes.len() == body_bytes.len()
                                    || !is_ident_char(body_bytes[i + name_bytes.len()]);
                                if before_ok && after_ok {
                                    new_body.push_str(alias);
                                    i += name_bytes.len();
                                    continue;
                                }
                            }
                            new_body.push(body_bytes[i] as char);
                            i += 1;
                        }
                        replaced = format!("{header}{new_body}{after_class}");
                    }

                    // Replace the emitted range with the modified text.
                    // Trim trailing newline to avoid double blank line before __decorate.
                    let replaced = replaced.trim_end_matches('\n');
                    self.writer.truncate(before_len);
                    self.write(replaced);
                    self.write_line();
                } else {
                    self.emit_class_es6_with_options(
                        node,
                        idx,
                        true,
                        Some(("let", class_name.clone())),
                    );
                }
            } else {
                self.emit_class_es6_with_options(node, idx, false, None);
            }

            // Restore anonymous_default_export_name if we temporarily set it
            if prev_anon_name {
                self.anonymous_default_export_name = None;
            }
            // Only write newline if not already at line start (class declarations
            // with lowered static fields already end with write_line()).
            if !self.writer.is_at_line_start() {
                self.write_line();
            }

            // Set type parameter names for metadata serialization so that
            // generic type params (T, U, etc.) serialize as "Object" not the param name.
            if self.ctx.options.emit_decorator_metadata
                && let Some(ref tp_list) = class.type_parameters
            {
                let tp_names: Vec<String> = tp_list
                    .nodes
                    .iter()
                    .filter_map(|&tp_idx| {
                        let tp_node = self.arena.get(tp_idx)?;
                        let tp = self.arena.get_type_parameter(tp_node)?;
                        let name = self.get_identifier_text_idx(tp.name);
                        if name.is_empty() { None } else { Some(name) }
                    })
                    .collect();
                if !tp_names.is_empty() {
                    self.metadata_class_type_params = Some(tp_names);
                }
            }

            // Emit __decorate calls for member decorators (methods, properties, accessors)
            if has_legacy_member_decorators {
                self.emit_legacy_member_decorator_calls(&class_name, &class.members.nodes);
            }

            let commonjs_exported = self.ctx.is_commonjs()
                && self
                    .arena
                    .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword)
                && !self.ctx.module_state.has_export_assignment;
            let commonjs_default = commonjs_exported
                && self
                    .arena
                    .has_modifier(&class.modifiers, SyntaxKind::DefaultKeyword);
            if let Some(ref alias) = alias_name {
                // Emit: `Name = Name_1 = __decorate([...], Name);`
                // We intercept the normal pattern and insert the alias assignment
                let before_len = self.writer.len();
                self.emit_legacy_class_decorator_assignment(
                    &class_name,
                    &legacy_class_decorators,
                    commonjs_exported,
                    commonjs_default,
                    false,
                    &class.members.nodes,
                );
                let after_len = self.writer.len();
                let full_output = self.writer.get_output().to_string();
                let emitted = &full_output[before_len..after_len];

                // Replace `Name = __decorate` with `Name = Name_1 = __decorate`
                let pattern = format!("{class_name} = __decorate");
                let replacement = format!("{class_name} = {alias} = __decorate");
                let modified = emitted.replacen(&pattern, &replacement, 1);
                self.writer.truncate(before_len);
                self.write(&modified);
            } else {
                self.emit_legacy_class_decorator_assignment(
                    &class_name,
                    &legacy_class_decorators,
                    commonjs_exported,
                    commonjs_default,
                    false,
                    &class.members.nodes,
                );
            }

            // Clear type parameter names after decorator emission
            self.metadata_class_type_params = None;

            return;
        }

        if self.ctx.target_es5 {
            let mut es5_emitter = ClassES5Emitter::new(self.arena);
            es5_emitter.set_temp_var_counter(self.ctx.destructuring_state.temp_var_counter);
            es5_emitter.set_indent_level(self.writer.indent_level());
            // Pass transform directives to the ClassES5Emitter
            es5_emitter.set_transforms(self.transforms.clone());
            es5_emitter.set_remove_comments(self.ctx.options.remove_comments);
            if let Some(text) = self.source_text_for_map() {
                if self.writer.has_source_map() {
                    es5_emitter.set_source_map_context(text, self.writer.current_source_index());
                } else {
                    es5_emitter.set_source_text(text);
                }
            }
            if self.ctx.options.import_helpers && self.ctx.is_effectively_commonjs() {
                es5_emitter.set_tslib_prefix(true);
            }
            es5_emitter
                .set_use_define_for_class_fields(self.ctx.options.use_define_for_class_fields);
            let output = es5_emitter.emit_class(idx);
            self.ctx.destructuring_state.temp_var_counter = es5_emitter.temp_var_counter();
            let mappings = es5_emitter.take_mappings();
            if !mappings.is_empty() && self.writer.has_source_map() {
                self.writer.write("");
                let base_line = self.writer.current_line();
                let base_column = self.writer.current_column();
                self.writer
                    .add_offset_mappings(base_line, base_column, &mappings);
                self.writer.write(&output);
            } else {
                self.write(&output);
            }
            // Emit any trailing comment from the class's closing `}` line
            // (e.g., `class Foo { ... } // comment` → `}()); // comment`).
            // This must happen BEFORE `skip_comments_for_erased_node` consumes
            // the trailing comment.
            let class_close_pos = self.find_token_end_before_trivia(node.pos, node.end);
            self.emit_trailing_comments(class_close_pos);
            // Skip comments within the class body range since the ES5 class emitter
            // handles them separately. Without this, they'd appear at end of file.
            // Skip comments that were part of this class declaration since the
            // ES5 class emitter handles class comments internally.
            self.skip_comments_for_erased_node(node);
            return;
        }

        self.emit_class_es6_with_options(node, idx, false, None);
    }

    pub(in crate::emitter) fn can_render_simple_tc39_decorated_class_es5(
        &self,
        node: &Node,
    ) -> bool {
        if self.ctx.options.legacy_decorators || !self.ctx.target_es5 {
            return false;
        }

        let Some(class) = self.arena.get_class(node) else {
            return false;
        };

        !self.collect_class_decorators(&class.modifiers).is_empty()
            && class.members.nodes.is_empty()
            && class.heritage_clauses.is_none()
    }

    pub(in crate::emitter) fn render_simple_tc39_decorated_class_es5(
        &mut self,
        node: &Node,
        idx: NodeIndex,
        binding_name: &str,
        display_name: &str,
    ) -> Option<String> {
        if !self.can_render_simple_tc39_decorated_class_es5(node) {
            return None;
        }

        let class = self.arena.get_class(node)?;
        let decorator_exprs = self
            .collect_class_decorators(&class.modifiers)
            .into_iter()
            .filter_map(|decorator_idx| {
                let decorator_node = self.arena.get(decorator_idx)?;
                let decorator = self.arena.get_decorator(decorator_node)?;
                let before_len = self.writer.len();
                self.emit_expression(decorator.expression);
                let after_len = self.writer.len();
                let full_output = self.writer.get_output().to_string();
                let emitted = full_output[before_len..after_len].trim().to_string();
                self.writer.truncate(before_len);
                Some(emitted)
            })
            .collect::<Vec<_>>();
        if decorator_exprs.is_empty() {
            return None;
        }

        let inner_name = if class.name.is_some() && !binding_name.ends_with("_1") {
            format!("{binding_name}_1")
        } else {
            binding_name.to_string()
        };

        let mut es5_emitter = ClassES5Emitter::new(self.arena);
        es5_emitter.set_temp_var_counter(self.ctx.destructuring_state.temp_var_counter);
        es5_emitter.set_indent_level(self.writer.indent_level() + 1);
        es5_emitter.set_transforms(self.transforms.clone());
        es5_emitter.set_remove_comments(self.ctx.options.remove_comments);
        if let Some(text) = self.source_text_for_map() {
            es5_emitter.set_source_text(text);
        }
        if self.ctx.options.import_helpers && self.ctx.is_effectively_commonjs() {
            es5_emitter.set_tslib_prefix(true);
        }
        es5_emitter.set_use_define_for_class_fields(self.ctx.options.use_define_for_class_fields);
        let mut inner_output = es5_emitter.emit_class_with_name(idx, &inner_name);
        self.ctx.destructuring_state.temp_var_counter = es5_emitter.temp_var_counter();
        inner_output = inner_output.trim_end_matches('\n').to_string();

        let base_indent = "    ".repeat(self.writer.indent_level() as usize);
        let body_indent = "    ".repeat((self.writer.indent_level() + 1) as usize);
        let decorator_indent = "    ".repeat((self.writer.indent_level() + 2) as usize);

        let inner_prefix = format!("var {inner_name} = ");
        let indented_inner_prefix = format!("{body_indent}{inner_prefix}");
        if inner_output.starts_with(&inner_prefix) {
            inner_output = format!(
                "{body_indent}var {binding_name} = _classThis = {}",
                &inner_output[inner_prefix.len()..]
            );
        } else if inner_output.starts_with(&indented_inner_prefix) {
            inner_output = format!(
                "{body_indent}var {binding_name} = _classThis = {}",
                &inner_output[indented_inner_prefix.len()..]
            );
        } else if !inner_output.starts_with(&body_indent) {
            inner_output = format!("{body_indent}{inner_output}");
        }

        Some(format!(
            "{base_indent}var {binding_name} = function () {{\n{body_indent}var _classDecorators = [{}];\n{body_indent}var _classDescriptor;\n{body_indent}var _classExtraInitializers = [];\n{body_indent}var _classThis;\n{inner_output}\n{body_indent}__setFunctionName(_classThis, \"{display_name}\");\n{body_indent}(function () {{\n{decorator_indent}var _metadata = typeof Symbol === \"function\" && Symbol.metadata ? Object.create(null) : void 0;\n{decorator_indent}__esDecorate(null, _classDescriptor = {{ value: _classThis }}, _classDecorators, {{ kind: \"class\", name: _classThis.name, metadata: _metadata }}, null, _classExtraInitializers);\n{decorator_indent}{binding_name} = _classThis = _classDescriptor.value;\n{decorator_indent}if (_metadata) Object.defineProperty(_classThis, Symbol.metadata, {{ enumerable: true, configurable: true, writable: true, value: _metadata }});\n{decorator_indent}__runInitializers(_classThis, _classExtraInitializers);\n{body_indent}}})();\n{body_indent}return {binding_name} = _classThis;\n{base_indent}}}();",
            decorator_exprs.join(", "),
        ))
    }

    pub(in crate::emitter) fn emit_tc39_decorated_class_expression(
        &mut self,
        class_node: NodeIndex,
        display_name: &str,
    ) -> bool {
        if self.ctx.options.legacy_decorators || self.ctx.options.target == ScriptTarget::ESNext {
            return false;
        }

        let Some(node) = self.arena.get(class_node) else {
            return false;
        };
        if node.kind != syntax_kind_ext::CLASS_DECLARATION {
            return false;
        }

        if self.ctx.target_es5 {
            return false;
        }

        use crate::transforms::es_decorators::TC39DecoratorEmitter;

        let mut emitter = TC39DecoratorEmitter::new(self.arena);
        emitter.set_indent_level(self.writer.indent_level() as usize);
        emitter.set_use_static_blocks(!self.ctx.needs_es2022_lowering);
        emitter.set_use_define_for_class_fields(self.ctx.options.use_define_for_class_fields);
        emitter.set_expression_mode(true);
        emitter.set_function_name(display_name.to_string());
        if self.ctx.options.import_helpers && self.ctx.is_effectively_commonjs() {
            emitter.set_tslib_prefix(true);
        }
        if let Some(text) = self.source_text_for_map() {
            emitter.set_source_text(text);
        }

        let output = emitter.emit_class(class_node);
        if output.is_empty() {
            return false;
        }

        let mut output = output.trim_end_matches('\n').to_string();
        if display_name == "default" {
            output = output.replace(
                "var class_1 = _classThis = class",
                "var default_1 = _classThis = class",
            );
            output = output.replace(
                "class_1 = _classThis = _classDescriptor.value",
                "default_1 = _classThis = _classDescriptor.value",
            );
            output = output.replace(
                "return class_1 = _classThis;",
                "return default_1 = _classThis;",
            );
            output = output.replace(
                "__setFunctionName(_classThis, \"class_1\")",
                "__setFunctionName(_classThis, \"default\")",
            );
        }

        self.write(&output);
        self.skip_comments_for_erased_node(node);
        true
    }

    /// Emit a class using ES6 native class syntax (no transforms).
    /// This is the pure emission logic that can be reused by both the old API
    /// and the new transform system.
    pub(in crate::emitter) fn emit_class_es6(&mut self, node: &Node, idx: NodeIndex) {
        self.emit_class_es6_with_options(node, idx, false, None);
    }

    pub(in crate::emitter) fn emit_class_es6_with_options(
        &mut self,
        node: &Node,
        _idx: NodeIndex,
        suppress_modifiers: bool,
        assignment_prefix: Option<(&str, String)>,
    ) {
        let Some(class) = self.arena.get_class(node) else {
            return;
        };
        let class_name = if class.name.is_none() {
            assignment_prefix
                .as_ref()
                .map(|(_, binding_name)| binding_name.clone())
                .or_else(|| self.anonymous_default_export_name.clone())
                .or_else(|| {
                    // For anonymous class expressions used as variable initializers
                    // (e.g. `const C = class { #field... }`), resolve the binding name
                    // from the parent VariableDeclaration node. This is needed for
                    // private field WeakMap naming (e.g., `_C_field`).
                    if node.kind == syntax_kind_ext::CLASS_EXPRESSION {
                        self.resolve_class_expr_binding_name(_idx)
                    } else {
                        None
                    }
                })
                .unwrap_or_default()
        } else {
            self.get_identifier_text_idx(class.name)
        };

        // Emit modifiers (including decorators) - skip TS-only modifiers for JS output
        if !suppress_modifiers && let Some(ref modifiers) = class.modifiers {
            for &mod_idx in &modifiers.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    // Skip export/default modifiers in CommonJS mode or namespace IIFE
                    if (self.ctx.is_commonjs() || self.in_namespace_iife)
                        && (mod_node.kind == SyntaxKind::ExportKeyword as u16
                            || mod_node.kind == SyntaxKind::DefaultKeyword as u16)
                    {
                        continue;
                    }
                    // Skip TypeScript-only modifiers (abstract, declare, etc.)
                    // Also skip `async` — it's an error on class declarations but
                    // TSC still emits the class without the modifier.
                    if mod_node.kind == SyntaxKind::AbstractKeyword as u16
                        || mod_node.kind == SyntaxKind::DeclareKeyword as u16
                        || mod_node.kind == SyntaxKind::AsyncKeyword as u16
                    {
                        continue;
                    }
                    self.emit(mod_idx);
                    // Add space or newline after decorator
                    if mod_node.kind == syntax_kind_ext::DECORATOR {
                        self.write_line();
                    } else {
                        self.write_space();
                    }
                }
            }
        }

        if let Some((keyword, binding_name)) = assignment_prefix.as_ref() {
            if !keyword.is_empty() {
                self.write(keyword);
                self.write(" ");
            }
            self.write(binding_name);
            self.write(" = ");
        }

        // Collect `accessor` fields to lower using one of two strategies:
        // - ES2022+ (except ESNext): emit native private storage + getter/setter.
        // - < ES2022: emit WeakMap-backed getter/setter pairs.
        let auto_accessor_target = self.ctx.options.target;
        let lower_auto_accessors_to_private_fields = auto_accessor_target != ScriptTarget::ESNext
            && (auto_accessor_target as u32) >= (ScriptTarget::ES2022 as u32);
        let lower_auto_accessors_to_weakmap = auto_accessor_target != ScriptTarget::ESNext
            && (auto_accessor_target as u32) < (ScriptTarget::ES2022 as u32);

        let mut auto_accessor_members: Vec<AutoAccessorInfo> = Vec::new();
        let mut auto_accessor_instance_inits: Vec<(String, Option<NodeIndex>)> = Vec::new();
        let mut auto_accessor_static_inits: Vec<(String, Option<NodeIndex>)> = Vec::new();
        let mut auto_accessor_class_alias: Option<String> = None;
        let mut next_auto_accessor_name_index = 0u32;
        let mut private_names_for_auto_accessors: Vec<String> = Vec::new();
        if lower_auto_accessors_to_private_fields {
            let mut nodes_to_visit: Vec<NodeIndex> = class.members.nodes.clone();
            while let Some(member_idx) = nodes_to_visit.pop() {
                let Some(member_node) = self.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || member_node.kind == syntax_kind_ext::CLASS_EXPRESSION
                {
                    continue;
                }
                if member_node.kind == SyntaxKind::PrivateIdentifier as u16
                    && let Some(name) = get_private_field_name(self.arena, member_idx)
                {
                    private_names_for_auto_accessors.push(name.trim_start_matches('#').to_string());
                }
                let mut children = self.arena.get_children(member_idx);
                nodes_to_visit.append(&mut children);
            }
        }

        let mut next_auto_accessor_name = || -> String {
            let name = if next_auto_accessor_name_index < 26 {
                let offset = next_auto_accessor_name_index as u8;
                format!("_{}", (b'a' + offset) as char)
            } else {
                format!("_{}", next_auto_accessor_name_index - 26)
            };
            next_auto_accessor_name_index += 1;
            name
        };

        let mut uniquify_private_accessor_name = |base: &str| -> String {
            if !lower_auto_accessors_to_private_fields {
                return base.to_string();
            }

            let mut candidate = base.to_string();
            let mut candidate_with_storage = format!("{candidate}_accessor_storage");
            let mut suffix = 1usize;
            while private_names_for_auto_accessors
                .iter()
                .any(|name| name == &candidate_with_storage)
            {
                candidate = format!("{base}_{suffix}");
                candidate_with_storage = format!("{candidate}_accessor_storage");
                suffix += 1;
            }
            private_names_for_auto_accessors.push(format!("{candidate}_accessor_storage"));
            candidate
        };

        if lower_auto_accessors_to_private_fields || lower_auto_accessors_to_weakmap {
            for &member_idx in &class.members.nodes {
                let Some(member_node) = self.arena.get(member_idx) else {
                    continue;
                };
                let Some(prop) = self.arena.get_property_decl(member_node).filter(|prop| {
                    self.arena
                        .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
                }) else {
                    continue;
                };
                if self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                {
                    continue;
                }
                if lower_auto_accessors_to_weakmap
                    && self
                        .arena
                        .get(prop.name)
                        .is_some_and(|name| name.kind == SyntaxKind::PrivateIdentifier as u16)
                {
                    continue;
                }
                if lower_auto_accessors_to_weakmap && class_name.is_empty() {
                    continue;
                }
                let is_static = self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::StaticKeyword);
                let Some(name_node) = self.arena.get(prop.name) else {
                    continue;
                };
                let mut accessor_name = match name_node.kind {
                    k if k == SyntaxKind::Identifier as u16 => {
                        self.get_identifier_text_idx(prop.name)
                    }
                    k if k == SyntaxKind::PrivateIdentifier as u16 => {
                        if lower_auto_accessors_to_private_fields {
                            get_private_field_name(self.arena, prop.name)
                                .unwrap_or_default()
                                .trim_start_matches('#')
                                .to_string()
                        } else {
                            String::new()
                        }
                    }
                    _ => String::new(),
                };
                if accessor_name.is_empty() {
                    accessor_name = next_auto_accessor_name();
                }
                if accessor_name.is_empty() {
                    continue;
                }
                let accessor_name = uniquify_private_accessor_name(&accessor_name);
                let storage_name = if lower_auto_accessors_to_weakmap {
                    format!("_{class_name}_{accessor_name}_accessor_storage")
                } else {
                    format!("{accessor_name}_accessor_storage")
                };
                let init = if prop.initializer.is_none() {
                    None
                } else {
                    Some(prop.initializer)
                };
                auto_accessor_members.push((member_idx, storage_name.clone(), init, is_static));
                if is_static {
                    if auto_accessor_class_alias.is_none() {
                        auto_accessor_class_alias = Some(self.make_unique_name());
                    }
                    auto_accessor_static_inits.push((storage_name, init));
                } else {
                    auto_accessor_instance_inits.push((storage_name, init));
                }
            }
        }

        if !auto_accessor_members.is_empty() && lower_auto_accessors_to_weakmap {
            // Hoist auto-accessor storage vars to the top of the scope,
            // matching tsc behavior (emits all class-related vars before the first class).
            if let Some(alias) = auto_accessor_class_alias.as_ref() {
                self.hoisted_assignment_temps.push(alias.clone());
            }
            for (_, storage_name, _, _) in &auto_accessor_members {
                self.hoisted_assignment_temps.push(storage_name.clone());
            }
            self.emit_comments_before_pos(node.pos);
        }

        // Private field lowering: when target < ES2022, transform #fields to WeakMap pattern
        let needs_private_field_lowering = !self.ctx.options.target.supports_es2022()
            && self.ctx.options.target != ScriptTarget::ESNext;
        let private_fields: Vec<PrivateFieldInfo> = if needs_private_field_lowering {
            collect_private_fields(self.arena, _idx, &class_name)
        } else {
            Vec::new()
        };
        let private_methods: Vec<PrivateMethodInfo> = if needs_private_field_lowering {
            collect_private_methods(self.arena, _idx, &class_name)
        } else {
            Vec::new()
        };
        let private_accessors: Vec<PrivateAccessorInfo> = if needs_private_field_lowering {
            collect_private_accessors(self.arena, _idx, &class_name)
        } else {
            Vec::new()
        };

        // Determine if we need a WeakSet for instance methods/accessors
        let has_instance_methods_or_accessors = private_methods.iter().any(|m| !m.is_static)
            || private_accessors.iter().any(|a| !a.is_static);
        let instances_weakset_name = if has_instance_methods_or_accessors {
            Some(format!("_{class_name}_instances"))
        } else {
            None
        };

        // Determine if we need a class alias for static private fields
        let has_static_privates = private_fields.iter().any(|f| f.is_static)
            || private_methods.iter().any(|m| m.is_static)
            || private_accessors.iter().any(|a| a.is_static);
        let private_class_alias = if has_static_privates {
            Some(self.make_unique_name())
        } else {
            None
        };

        // Save the previous private field map (for nested classes)
        let prev_private_field_weakmaps = std::mem::take(&mut self.private_field_weakmaps);
        let prev_pending_weakmap_inits = std::mem::take(&mut self.pending_weakmap_inits);
        let prev_pending_static_private_inits =
            std::mem::take(&mut self.pending_static_private_inits);
        let prev_pending_private_class_alias = self.pending_private_class_alias.take();
        let prev_private_member_info = std::mem::take(&mut self.private_member_info);
        let prev_pending_private_field_constructor_inits =
            std::mem::take(&mut self.pending_private_field_constructor_inits);
        let prev_pending_instances_weakset_add = self.pending_instances_weakset_add.take();
        let prev_pending_private_method_defs =
            std::mem::take(&mut self.pending_private_method_defs);
        let prev_pending_private_accessor_defs =
            std::mem::take(&mut self.pending_private_accessor_defs);
        let prev_private_members_to_skip = std::mem::take(&mut self.private_members_to_skip);

        let has_any_private_lowering = !private_fields.is_empty()
            || !private_methods.is_empty()
            || !private_accessors.is_empty();

        if has_any_private_lowering {
            // Collect all variable names needed for declaration
            let mut var_names: Vec<String> = Vec::new();

            // Class alias for static members
            if let Some(ref alias) = private_class_alias {
                var_names.push(alias.clone());
            }

            // WeakSet for instance methods/accessors
            if let Some(ref ws_name) = instances_weakset_name {
                var_names.push(ws_name.clone());
            }

            // Instance field WeakMaps
            for field in &private_fields {
                if !field.is_static {
                    var_names.push(field.weakmap_name.clone());
                }
            }

            // Static field value containers
            for field in &private_fields {
                if field.is_static {
                    var_names.push(field.weakmap_name.clone());
                }
            }

            // Private method function vars
            for method in &private_methods {
                var_names.push(method.fn_var_name.clone());
            }

            // Private accessor function vars
            for accessor in &private_accessors {
                if let Some(ref name) = accessor.get_var_name
                    && accessor.getter_body.is_some()
                {
                    var_names.push(name.clone());
                }
                if let Some(ref name) = accessor.set_var_name
                    && accessor.setter_body.is_some()
                {
                    var_names.push(name.clone());
                }
            }

            if !var_names.is_empty() {
                // Hoist private field vars to the top of the scope (after "use strict"
                // and CJS preamble), matching tsc behavior. tsc emits all private field
                // WeakMap/method vars before the first class in the scope.
                // NOTE: For class expressions in loop bodies, tsc uses block-scoped `let`
                // instead of `var` hoisting. This is a known limitation - we always use
                // `var` for now, which is semantically equivalent since the comma expression
                // reassigns new WeakMaps each iteration.
                self.hoisted_assignment_temps.extend(var_names);
            }

            // Set up the private field map for expression lowering
            for field in &private_fields {
                self.private_field_weakmaps
                    .insert(field.name.clone(), field.weakmap_name.clone());
                if field.is_static {
                    self.private_member_info.insert(
                        field.name.clone(),
                        PrivateMemberInfo {
                            kind: "f",
                            fn_ref: Some(field.weakmap_name.clone()),
                            setter_ref: None,
                            is_static: true,
                            state_var: private_class_alias.clone(),
                        },
                    );
                }
            }

            // Register methods
            for method in &private_methods {
                self.private_field_weakmaps
                    .insert(method.name.clone(), method.fn_var_name.clone());
                self.private_member_info.insert(
                    method.name.clone(),
                    PrivateMemberInfo {
                        kind: "m",
                        fn_ref: Some(method.fn_var_name.clone()),
                        setter_ref: None,
                        is_static: method.is_static,
                        state_var: if method.is_static {
                            private_class_alias.clone()
                        } else {
                            instances_weakset_name.clone()
                        },
                    },
                );
            }

            // Register accessors
            for accessor in &private_accessors {
                // Use the instances weakset name as the weakmap entry for lookup
                let weakmap_entry = if accessor.is_static {
                    private_class_alias.clone().unwrap_or_default()
                } else {
                    instances_weakset_name.clone().unwrap_or_default()
                };
                self.private_field_weakmaps
                    .insert(accessor.name.clone(), weakmap_entry);
                self.private_member_info.insert(
                    accessor.name.clone(),
                    PrivateMemberInfo {
                        kind: "a",
                        fn_ref: accessor
                            .get_var_name
                            .as_ref()
                            .filter(|_| accessor.getter_body.is_some())
                            .cloned(),
                        setter_ref: accessor
                            .set_var_name
                            .as_ref()
                            .filter(|_| accessor.setter_body.is_some())
                            .cloned(),
                        is_static: accessor.is_static,
                        state_var: if accessor.is_static {
                            private_class_alias.clone()
                        } else {
                            instances_weakset_name.clone()
                        },
                    },
                );
            }

            // Prepare WeakMap initializations for after the class body
            self.pending_weakmap_inits = private_fields
                .iter()
                .filter(|f| !f.is_static)
                .map(|f| format!("{} = new WeakMap()", f.weakmap_name))
                .collect();

            // Prepare static private field value initializations for after the class body.
            // tsc emits: `_A_field = { value: <init> };` for each static private field.
            self.pending_static_private_inits = private_fields
                .iter()
                .filter(|f| f.is_static)
                .map(|f| (f.weakmap_name.clone(), f.initializer))
                .collect();

            // Store class alias for static privates: emit `_a = ClassName;` after class body
            if let Some(ref alias) = private_class_alias
                && !class_name.is_empty()
            {
                self.pending_private_class_alias = Some((alias.clone(), class_name.clone()));
            }

            // Prepare private field constructor inits (WeakMap.set calls)
            self.pending_private_field_constructor_inits = private_fields
                .iter()
                .filter(|f| !f.is_static)
                .map(|f| (f.weakmap_name.clone(), f.has_initializer, f.initializer))
                .collect();

            // Prepare WeakSet instances.add(this) for constructor
            if let Some(ref ws_name) = instances_weakset_name {
                self.pending_instances_weakset_add = Some(ws_name.clone());
            }

            // Prepare private method function defs for after the class body
            // Both instance and static private methods are extracted.
            for method in &private_methods {
                if let Some(body_idx) = method.body {
                    self.pending_private_method_defs.push((
                        method.fn_var_name.clone(),
                        body_idx,
                        method.parameters.clone(),
                    ));
                }
            }

            // Prepare private accessor function defs for after the class body
            // Both instance and static private accessors are extracted.
            for accessor in &private_accessors {
                if let Some(body_idx) = accessor.getter_body
                    && let Some(ref var_name) = accessor.get_var_name
                {
                    self.pending_private_accessor_defs.push(
                        crate::emitter::core::PrivateAccessorDef {
                            var_name: var_name.clone(),
                            body: body_idx,
                            param: None,
                            is_async: false,
                        },
                    );
                }
                if let Some(body_idx) = accessor.setter_body
                    && let Some(ref var_name) = accessor.set_var_name
                {
                    self.pending_private_accessor_defs.push(
                        crate::emitter::core::PrivateAccessorDef {
                            var_name: var_name.clone(),
                            body: body_idx,
                            param: accessor.setter_param,
                            is_async: false,
                        },
                    );
                }
            }

            // Mark all private methods and accessors (instance + static) to skip from class body
            for method in &private_methods {
                self.private_members_to_skip.insert(method.name.clone());
            }
            for accessor in &private_accessors {
                self.private_members_to_skip.insert(accessor.name.clone());
            }
        }

        // For class expressions with private field lowering, we need to wrap the class
        // in a comma expression: `(_a = class C { ... }, _WeakMap = new WeakMap(), ..., _a)`
        // tsc uses this pattern so the WeakMap/WeakSet initialization happens inline.
        let is_class_expression = node.kind == syntax_kind_ext::CLASS_EXPRESSION;
        let needs_private_comma_expr = is_class_expression && has_any_private_lowering;

        // For class expressions with static field initializers, we need to wrap
        // in a comma expression: `(_a = class C {}, _a.a = 1, _a)`.
        // Allocate the class-expression temp before any computed-name temps so the
        // generated `_a`, `_b`, `_c` ordering matches tsc.
        let target_needs_field_lowering = (self.ctx.options.target as u32)
            < (tsz_common::ScriptTarget::ES2022 as u32)
            || !self.ctx.options.use_define_for_class_fields;
        let needs_static_comma_expr = is_class_expression
            && target_needs_field_lowering
            && class.members.nodes.iter().any(|&member_idx| {
                self.arena.get(member_idx).is_some_and(|m| {
                    m.kind == syntax_kind_ext::PROPERTY_DECLARATION
                        && self.arena.get_property_decl(m).is_some_and(|p| {
                            self.arena
                                .has_modifier(&p.modifiers, SyntaxKind::StaticKeyword)
                                && !self
                                    .arena
                                    .has_modifier(&p.modifiers, SyntaxKind::AbstractKeyword)
                                && !self
                                    .arena
                                    .has_modifier(&p.modifiers, SyntaxKind::DeclareKeyword)
                                && !(needs_private_field_lowering
                                    && is_private_identifier(self.arena, p.name))
                        })
                })
            });
        let needs_any_comma_expr = needs_static_comma_expr || needs_private_comma_expr;
        let class_expr_temp = if needs_any_comma_expr {
            let temp = if let Some(ref alias) = private_class_alias {
                alias.clone()
            } else {
                self.make_unique_name_hoisted()
            };
            self.write("(");
            self.write(&temp);
            self.write(" = ");
            Some(temp)
        } else {
            None
        };
        let class_expr_static_temp = if needs_static_comma_expr {
            class_expr_temp.clone()
        } else {
            None
        };

        // Computed property name hoisting for targets < ES2022.
        // tsc hoists non-constant computed property name expressions to temp variables
        // (e.g., `_a = n, _b = s + n`) so that the evaluation order is preserved and
        // the class body can reference the temp instead of the original expression.
        //
        // Only PROPERTY DECLARATIONS with computed names participate in hoisting.
        // Methods and accessors keep their computed names inline in ES6+.
        // After the class body, a comma expression joins all assignments and side effects.
        let needs_computed_prop_hoisting =
            (self.ctx.options.target as u32) < (ScriptTarget::ES2022 as u32);
        // Each entry: (Option<temp_name>, expr_idx) — None means side-effect only
        let mut computed_prop_entries: Vec<(Option<String>, NodeIndex)> = Vec::new();
        if needs_computed_prop_hoisting {
            for &member_idx in &class.members.nodes {
                let Some(member_node) = self.arena.get(member_idx) else {
                    continue;
                };
                // Only property declarations participate in computed property hoisting
                if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                    continue;
                }
                let Some(prop) = self.arena.get_property_decl(member_node) else {
                    continue;
                };
                let Some(name_node) = self.arena.get(prop.name) else {
                    continue;
                };
                if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    continue;
                }
                let Some(computed) = self.arena.get_computed_property(name_node) else {
                    continue;
                };
                let Some(expr_node) = self.arena.get(computed.expression) else {
                    continue;
                };
                // Check if expression is a constant that doesn't need hoisting
                let is_constant = expr_node.kind == SyntaxKind::StringLiteral as u16
                    || expr_node.kind == SyntaxKind::NumericLiteral as u16
                    || expr_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16;
                if is_constant {
                    continue;
                }
                // Check if this property is erased (type-only, abstract, etc.)
                let is_erased = if self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                {
                    true
                } else if self.ctx.options.use_define_for_class_fields {
                    false
                } else {
                    let is_private = self
                        .arena
                        .get(prop.name)
                        .is_some_and(|n| n.kind == SyntaxKind::PrivateIdentifier as u16);
                    let has_accessor = self
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword);
                    prop.initializer.is_none() && !is_private && !has_accessor
                };
                if is_erased {
                    // Side-effect only: expression is emitted for its effects but no temp.
                    let is_side_effect_free =
                        self.is_computed_name_expr_side_effect_free(computed.expression);
                    if !is_side_effect_free {
                        computed_prop_entries.push((None, computed.expression));
                    }
                } else {
                    // Allocate a temp variable for this computed property name
                    let temp = self.make_unique_name_hoisted();
                    self.computed_prop_temp_map
                        .insert(computed.expression, temp.clone());
                    computed_prop_entries.push((Some(temp), computed.expression));
                }
            }
        }

        let has_extends = class.heritage_clauses.as_ref().is_some_and(|clauses| {
            clauses.nodes.iter().any(|&idx| {
                self.arena
                    .get(idx)
                    .and_then(|n| self.arena.get_heritage(n))
                    .is_some_and(|h| h.token == SyntaxKind::ExtendsKeyword as u16)
            })
        });
        let extends_null = crate::transforms::emit_utils::extends_null_literal(
            self.arena,
            &class.heritage_clauses,
        );
        let needs_static_block_lowering =
            (self.ctx.options.target as u32) < (ScriptTarget::ES2022 as u32);
        let has_legacy_class_decorators = self.ctx.options.legacy_decorators
            && !self.collect_class_decorators(&class.modifiers).is_empty();
        let externalized_static_initializer_uses_undefined_receiver =
            !is_class_expression && needs_static_block_lowering && has_legacy_class_decorators;

        let static_initializer_nodes: Vec<NodeIndex> =
            if is_class_expression || !needs_static_block_lowering {
                Vec::new()
            } else {
                class
                    .members
                    .nodes
                    .iter()
                    .filter_map(|&member_idx| {
                        let member_node = self.arena.get(member_idx)?;
                        if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                            return None;
                        }
                        let prop = self.arena.get_property_decl(member_node)?;
                        if !self
                            .arena
                            .has_modifier(&prop.modifiers, SyntaxKind::StaticKeyword)
                            || self
                                .arena
                                .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                            || self
                                .arena
                                .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
                            || prop.initializer.is_none()
                        {
                            return None;
                        }
                        Some(prop.initializer)
                    })
                    .collect()
            };

        let static_initializer_needs_this_alias = !static_initializer_nodes.is_empty()
            && static_initializer_nodes
                .iter()
                .any(|init_idx| contains_this_reference(self.arena, *init_idx));
        let static_initializer_needs_super_alias = has_extends
            && !extends_null
            && !static_initializer_nodes.is_empty()
            && static_initializer_nodes
                .iter()
                .any(|init_idx| contains_super_reference(self.arena, *init_idx));
        let static_this_alias = if static_initializer_needs_this_alias
            && !externalized_static_initializer_uses_undefined_receiver
        {
            Some(self.make_unique_name_hoisted())
        } else {
            None
        };
        let static_super_base_alias = if static_initializer_needs_super_alias
            && !externalized_static_initializer_uses_undefined_receiver
        {
            Some(self.make_unique_name_hoisted())
        } else {
            None
        };
        let static_initializer_this_binding =
            if externalized_static_initializer_uses_undefined_receiver
                && static_initializer_needs_this_alias
            {
                Some("(void 0)")
            } else {
                static_this_alias.as_deref()
            };
        let static_initializer_super_base =
            if externalized_static_initializer_uses_undefined_receiver
                && static_initializer_needs_super_alias
            {
                Some("(void 0)")
            } else {
                static_super_base_alias.as_deref()
            };

        self.write("class");

        // Determine the class expression name.
        // When assignment_prefix is provided (e.g., `let C = class C {}`), a named class
        // keeps its name on the expression, but an anonymous class stays anonymous
        // (`let default_1 = class {}`), even if anonymous_default_export_name is set.
        if class.name.is_some() {
            self.write_space();
            self.emit_decl_name(class.name);
        } else if assignment_prefix.is_none() {
            // No assignment prefix — use anonymous_default_export_name if available
            // (e.g., `export default class {}` → `class default_1 {}`)
            let override_name = self.anonymous_default_export_name.clone();
            if let Some(name) = override_name
                && !name.is_empty()
            {
                self.write_space();
                self.write(&name);
            }
        }

        if let Some(ref heritage_clauses) = class.heritage_clauses {
            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = self.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.arena.get_heritage(clause_node) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }

                if !heritage.types.nodes.is_empty() {
                    self.write(" extends ");
                    for (i, &extends_type) in heritage.types.nodes.iter().enumerate() {
                        if i > 0 {
                            self.write(", ");
                        }
                        if let Some(base_alias) = static_super_base_alias.as_ref() {
                            self.write("(");
                            self.write(base_alias);
                            self.write(" = ");
                            self.emit_heritage_expression(extends_type);
                            self.write(")");
                        } else {
                            self.emit_heritage_expression(extends_type);
                        }
                    }
                } else {
                    // Error recovery: source has `extends` with no base type.
                    // Preserve the keyword to match tsc output.
                    self.write(" extends ");
                }
            }
        }

        self.write(" {");
        // Suppress trailing comments on class body opening brace.
        // tsc drops same-line comments on `{` for class bodies, just like function
        // bodies (e.g. `class C { // error` → `class C {`).
        if !self.ctx.options.remove_comments
            && let Some(text) = self.source_text
        {
            let bytes = text.as_bytes();
            let start = node.pos as usize;
            let end = (node.end as usize).min(bytes.len());
            if let Some(offset) = bytes[start..end].iter().position(|&b| b == b'{') {
                let brace_end = (start + offset + 1) as u32;
                // Only suppress if there's a newline between `{` and the first
                // member (or the closing `}` if empty).  Single-line class bodies
                // like `class C { x: T; } // error` have the comment after `}`,
                // so we must NOT suppress it.
                // For empty classes like `class C {} // comment`, scan_end must
                // be the closing `}` position, not node.end — otherwise a newline
                // after `}` (before the next statement) causes us to incorrectly
                // suppress the trailing comment that belongs to `}`.
                let scan_end = class
                    .members
                    .nodes
                    .first()
                    .and_then(|&idx| self.arena.get(idx))
                    .map_or_else(
                        || {
                            // Empty class: find the closing `}` to use as scan_end
                            let be = brace_end as usize;
                            if be <= end {
                                bytes[be..end]
                                    .iter()
                                    .position(|&b| b == b'}')
                                    .map_or(end, |p| be + p)
                            } else {
                                end
                            }
                        },
                        |m| m.pos as usize,
                    );
                let brace_end_usize = brace_end as usize;
                let scan_end_clamped = scan_end.min(end);
                let has_newline = if brace_end_usize <= scan_end_clamped {
                    bytes[brace_end_usize..scan_end_clamped]
                        .iter()
                        .any(|&b| b == b'\n' || b == b'\r')
                } else {
                    // Malformed source: first member pos precedes the opening
                    // brace we found — skip the suppression heuristic.
                    false
                };
                if has_newline {
                    self.skip_trailing_same_line_comments(brace_end, node.end);
                }
            }
        }
        self.write_line();
        self.increase_indent();
        // When inside a comma expression wrapper (class expression with private fields
        // or static fields), add one extra indent level for the class body to match tsc.
        if class_expr_temp.is_some() {
            self.increase_indent();
        }

        // Store auto-accessor inits for constructor emission.
        let prev_auto_accessor_inits = std::mem::take(&mut self.pending_auto_accessor_inits);
        if !auto_accessor_instance_inits.is_empty() && lower_auto_accessors_to_weakmap {
            self.pending_auto_accessor_inits = auto_accessor_instance_inits.clone();
        }

        // Private field WeakMap.set inits are handled via pending_private_field_constructor_inits
        // which is emitted in emit_constructor_prologue and the synthesized constructor path.

        // Check if we need to lower class fields to constructor.
        // This is needed when target < ES2022 OR when useDefineForClassFields is false
        // (legacy behavior where fields are assigned in the constructor).
        let needs_class_field_lowering = (self.ctx.options.target as u32)
            < (ScriptTarget::ES2022 as u32)
            || !self.ctx.options.use_define_for_class_fields;

        let mut deferred_static_blocks: Vec<(NodeIndex, usize)> = Vec::new();
        // Collect computed property name expressions from erased type-only members.
        // tsc emits these as standalone side-effect statements after the class body
        // (e.g., `[Symbol.iterator]: Type` → erased member, but `Symbol.iterator;` emitted).
        let mut computed_property_side_effects: Vec<NodeIndex> = Vec::new();

        // Collect property initializers that need lowering
        // (name, initializer_idx, init_end, leading_comments, trailing_comments)
        // Comments are collected eagerly here so they're available even
        // when the constructor appears before the property in source order.
        let mut field_inits: Vec<crate::emitter::core::FieldInit> = Vec::new();
        let mut static_field_inits: Vec<StaticFieldInit> = Vec::new();
        if needs_class_field_lowering {
            let members = &class.members.nodes;
            for (member_i, &member_idx) in members.iter().enumerate() {
                if let Some(member_node) = self.arena.get(member_idx)
                    && member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                    && let Some(prop) = self.arena.get_property_decl(member_node)
                {
                    if prop.initializer.is_none()
                        || self
                            .arena
                            .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
                        || self
                            .arena
                            .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                        || self
                            .arena
                            .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
                    {
                        continue;
                    }
                    // Skip private fields when they're being lowered to WeakMap pattern.
                    // They're handled separately via pending_private_field_constructor_inits.
                    if !private_fields.is_empty() && is_private_identifier(self.arena, prop.name) {
                        continue;
                    }
                    // If the property has a computed name with a hoisted temp, use the temp
                    // variable name. This takes priority over get_property_name_emit because
                    // the temp captures the expression value at class-evaluation time.
                    let name_emit = if let Some(name_node) = self.arena.get(prop.name)
                        && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                        && let Some(computed) = self.arena.get_computed_property(name_node)
                        && let Some(temp) = self.computed_prop_temp_map.get(&computed.expression)
                    {
                        Some(PropertyNameEmit::Bracket(temp.clone()))
                    } else {
                        self.get_property_name_emit(prop.name)
                    };
                    let Some(name_emit) = name_emit else {
                        continue;
                    };

                    // Pre-collect leading comments for this property declaration.
                    // Use the actual token end of the previous member (not its
                    // `end` field which can overshoot into the next member's trivia)
                    // so the range doesn't invert.
                    let leading_comments = if !self.ctx.options.remove_comments {
                        let prev_end = if member_i > 0 {
                            members
                                .get(member_i - 1)
                                .and_then(|&prev_idx| self.arena.get(prev_idx))
                                .map_or(member_node.pos, |prev| {
                                    self.find_token_end_before_trivia(prev.pos, prev.end)
                                })
                        } else {
                            member_node.pos.saturating_sub(64)
                        };
                        self.collect_leading_comments_in_range(prev_end, member_node.pos)
                    } else {
                        Vec::new()
                    };

                    // Pre-collect trailing comments for this property declaration.
                    let trailing_comments = if !self.ctx.options.remove_comments {
                        let skip_end = members
                            .get(member_i + 1)
                            .and_then(|&next_idx| self.arena.get(next_idx))
                            .map_or(member_node.end, |next| next.pos);
                        let actual_end =
                            self.find_token_end_before_trivia(member_node.pos, skip_end);
                        self.collect_trailing_comments_in_range(actual_end)
                    } else {
                        Vec::new()
                    };

                    if self
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::StaticKeyword)
                    {
                        // At ES2022+, static fields are emitted as `static { this.f = v; }`
                        // blocks inside the class body, not as external assignments.
                        if !needs_static_block_lowering {
                            // Don't collect for external emission; these will be
                            // emitted inline as static initialization blocks.
                        } else {
                            static_field_inits.push((
                                name_emit,
                                prop.initializer,
                                member_node.pos,
                                Vec::new(), // leading_comments filled during class body emission
                                Vec::new(), // trailing_comments filled during class body emission
                            ));
                        }
                    } else {
                        // Non-static field inits use String names for `this.name = val`,
                        // `this["name"] = val`, or `this[0] = val`. Bracket names use
                        // a `[` prefix to signal bracket notation at emit time.
                        let ident_name = match &name_emit {
                            PropertyNameEmit::Dot(s) => s.clone(),
                            PropertyNameEmit::Bracket(s) | PropertyNameEmit::BracketNumeric(s) => {
                                format!("[{s}]")
                            }
                        };
                        let init_end = self
                            .arena
                            .get(prop.initializer)
                            .map_or(member_node.end, |n| n.end);
                        field_inits.push((
                            ident_name,
                            prop.initializer,
                            init_end,
                            leading_comments,
                            trailing_comments,
                        ));
                    }
                }
            }
        }

        // Check if class has an explicit constructor with a body.
        // A constructor without a body (e.g., broken syntax `constructor` with no
        // parens/braces) should not prevent synthesis of a constructor for field
        // initialization (matches tsc error-recovery behavior).
        let has_constructor = class.members.nodes.iter().any(|&idx| {
            self.arena.get(idx).is_some_and(|n| {
                n.kind == syntax_kind_ext::CONSTRUCTOR
                    && self
                        .arena
                        .get_constructor(n)
                        .is_some_and(|ctor| ctor.body.is_some())
            })
        });

        // Store field inits for constructor emission
        let prev_field_inits = std::mem::take(&mut self.pending_class_field_inits);
        if !field_inits.is_empty() {
            self.pending_class_field_inits = field_inits.clone();
        }

        // If no constructor but we have field inits, synthesize one
        let has_private_field_inits = private_fields.iter().any(|f| !f.is_static);
        let has_instances_weakset = self.pending_instances_weakset_add.is_some();
        let synthesize_constructor = !has_constructor
            && (!field_inits.is_empty()
                || (lower_auto_accessors_to_weakmap && !auto_accessor_instance_inits.is_empty())
                || has_private_field_inits
                || has_instances_weakset);

        if synthesize_constructor {
            // Increment function_scope_depth so async arrow functions inside
            // the synthesized constructor use `this` instead of `void 0` as
            // the __awaiter first argument.
            self.function_scope_depth += 1;
            if has_extends && !extends_null {
                self.write("constructor() {");
                self.write_line();
                self.increase_indent();
                self.write("super(...arguments);");
                self.write_line();
            } else {
                self.write("constructor() {");
                self.write_line();
                self.increase_indent();
            }
            // Emit _X_instances.add(this) for private methods/accessors
            if let Some(ref ws_name) = self.pending_instances_weakset_add.clone() {
                self.write(ws_name);
                self.write(".add(this);");
                self.write_line();
            }
            // Private field WeakMap.set inits first (before non-private field inits)
            for field in &private_fields {
                if !field.is_static {
                    self.write(&field.weakmap_name);
                    self.write(".set(this, ");
                    if field.has_initializer {
                        self.emit_expression(field.initializer);
                    } else {
                        self.write("void 0");
                    }
                    self.write(");");
                    self.write_line();
                }
            }
            if lower_auto_accessors_to_weakmap {
                for (storage_name, init_idx) in &auto_accessor_instance_inits {
                    self.write(storage_name);
                    self.write(".set(this, ");
                    match init_idx {
                        Some(init) => {
                            self.with_scoped_static_initializer_context_cleared(|this| {
                                this.emit_expression(*init);
                            });
                        }
                        None => self.write("void 0"),
                    }
                    self.write(");");
                    self.write_line();
                }
            }
            // Non-private field inits after WeakMap.set calls
            for (name, init_idx, init_end, leading, trailing) in &field_inits {
                // Emit leading comments from the original property declaration
                for comment in leading {
                    self.write_comment(comment);
                    self.write_line();
                }
                if self.ctx.options.use_define_for_class_fields {
                    self.write("Object.defineProperty(this, ");
                    if name.starts_with('[') && name.ends_with(']') {
                        self.write(&name[1..name.len() - 1]);
                    } else {
                        self.emit_string_literal_text(name);
                    }
                    self.write(", {");
                    self.write_line();
                    self.increase_indent();
                    self.write("enumerable: true,");
                    self.write_line();
                    self.write("configurable: true,");
                    self.write_line();
                    self.write("writable: true,");
                    self.write_line();
                    self.write("value: ");
                    self.with_scoped_static_initializer_context_cleared(|this| {
                        this.emit_expression(*init_idx);
                    });
                    self.write_line();
                    self.decrease_indent();
                    self.write("});");
                } else {
                    if name.starts_with('[') {
                        self.write("this");
                        self.write(name);
                    } else {
                        self.write("this.");
                        self.write(name);
                    }
                    self.write(" = ");
                    self.with_scoped_static_initializer_context_cleared(|this| {
                        this.emit_expression(*init_idx);
                    });
                    self.write(";");
                    if !trailing.is_empty() {
                        for comment in trailing {
                            self.write_space();
                            self.write_comment(comment);
                        }
                    } else {
                        self.emit_trailing_comments(*init_end);
                    }
                }
                self.write_line();
            }
            self.decrease_indent();
            self.write("}");
            self.write_line();
            self.function_scope_depth -= 1;
        }

        // When useDefineForClassFields is true AND target >= ES2022 (native class fields),
        // emit parameter property field declarations (e.g. `foo;`) at the beginning of
        // the class body. TSC emits these before any other class members.
        // When target < ES2022, fields are lowered to the constructor body, so no
        // class-body declaration is needed.
        let mut emitted_any_member = false;
        let target_supports_native_fields =
            (self.ctx.options.target as u32) >= (ScriptTarget::ES2022 as u32);
        if self.ctx.options.use_define_for_class_fields && target_supports_native_fields {
            // Find the constructor and collect its parameter properties
            for &member_idx in &class.members.nodes {
                if let Some(member_node) = self.arena.get(member_idx)
                    && member_node.kind == syntax_kind_ext::CONSTRUCTOR
                    && let Some(ctor) = self.arena.get_constructor(member_node)
                    && ctor.body.is_some()
                {
                    let param_props = self.collect_parameter_properties(&ctor.parameters.nodes);
                    for name in &param_props {
                        self.write(name);
                        self.write(";");
                        self.write_line();
                        emitted_any_member = true;
                    }
                    break;
                }
            }
        }
        // Compute the class body's closing `}` position so the last member's
        // trailing comment scan doesn't overshoot into comments belonging to
        // the closing brace line (same pattern as namespace IIFE emitter).
        let class_body_close_pos = self
            .source_text
            .map(|text| {
                let end = std::cmp::min(node.end as usize, text.len());
                let bytes = text.as_bytes();
                let mut pos = end;
                while pos > 0 {
                    pos -= 1;
                    if bytes[pos] == b'}' {
                        return pos as u32;
                    }
                }
                node.end
            })
            .unwrap_or(node.end);

        let mut field_init_comment_idx = 0usize;
        for (member_i, &member_idx) in class.members.nodes.iter().enumerate() {
            // Skip private field declarations entirely when lowering to WeakMap pattern
            if !private_fields.is_empty()
                && let Some(member_node) = self.arena.get(member_idx)
                && member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                && let Some(prop) = self.arena.get_property_decl(member_node)
                && self
                    .arena
                    .get(prop.name)
                    .is_some_and(|n| n.kind == SyntaxKind::PrivateIdentifier as u16)
            {
                // Skip comments that belong to this erased member
                if let Some(mn) = self.arena.get(member_idx) {
                    let skip_end = class
                        .members
                        .nodes
                        .get(member_i + 1)
                        .and_then(|&next_idx| self.arena.get(next_idx))
                        .map_or(mn.end, |next| next.pos);
                    while self.comment_emit_idx < self.all_comments.len()
                        && self.all_comments[self.comment_emit_idx].end <= skip_end
                    {
                        self.comment_emit_idx += 1;
                    }
                }
                continue;
            }
            // Skip private methods and accessors that are extracted as standalone functions
            if !self.private_members_to_skip.is_empty()
                && let Some(member_node) = self.arena.get(member_idx)
            {
                let should_skip = match member_node.kind {
                    k if k == syntax_kind_ext::METHOD_DECLARATION => self
                        .arena
                        .get_method_decl(member_node)
                        .and_then(|m| get_private_field_name(self.arena, m.name))
                        .map(|n| n.strip_prefix('#').unwrap_or(&n).to_string())
                        .is_some_and(|n| self.private_members_to_skip.contains(&n)),
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        self.arena
                            .get_accessor(member_node)
                            .and_then(|a| get_private_field_name(self.arena, a.name))
                            .map(|n| n.strip_prefix('#').unwrap_or(&n).to_string())
                            .is_some_and(|n| self.private_members_to_skip.contains(&n))
                    }
                    _ => false,
                };
                if should_skip {
                    if let Some(mn) = self.arena.get(member_idx) {
                        let skip_end = class
                            .members
                            .nodes
                            .get(member_i + 1)
                            .and_then(|&next_idx| self.arena.get(next_idx))
                            .map_or(mn.end, |next| next.pos);
                        while self.comment_emit_idx < self.all_comments.len()
                            && self.all_comments[self.comment_emit_idx].end <= skip_end
                        {
                            self.comment_emit_idx += 1;
                        }
                    }
                    continue;
                }
            }
            // Skip property declarations that were lowered
            if needs_class_field_lowering
                && let Some(member_node) = self.arena.get(member_idx)
                && member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                    && let Some(prop) = self.arena.get_property_decl(member_node)
                    && !auto_accessor_members
                        .iter()
                        .any(|(accessor_idx, _, _, _)| *accessor_idx == member_idx)
                    && prop.initializer.is_some()
                    && !self
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                    // Auto-accessor properties (`accessor x = 1`) that are NOT being
                    // lowered (e.g. at esnext target) must be preserved verbatim — they
                    // are not regular field declarations.
                    && !self
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
                // Private fields (#name) are emitted verbatim at ES2022+ — they
                // use native private field syntax and are unaffected by
                // useDefineForClassFields.  Only skip them for lowering when the
                // target actually requires WeakMap-based lowering (< ES2022).
                && !(self.arena.get(prop.name).is_some_and(|n| {
                    n.kind == SyntaxKind::PrivateIdentifier as u16
                }) && (self.ctx.options.target as u32) >= (ScriptTarget::ES2022 as u32))
                // Static fields at ES2022+ are emitted inline as `static { this.f = v; }`
                // blocks, not deferred to external assignments.
                && (!self.arena.has_modifier(&prop.modifiers, SyntaxKind::StaticKeyword)
                    || needs_static_block_lowering)
            {
                // For static properties, save leading and trailing comments before
                // skipping so they can be emitted when the initialization is moved
                // after the class body.
                let is_static = self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::StaticKeyword);
                if is_static {
                    let leading = self.collect_leading_comments(member_node.pos);
                    if let Some(entry) = static_field_inits
                        .iter_mut()
                        .find(|e| e.2 == member_node.pos)
                    {
                        entry.3 = leading;
                    }
                }
                if let Some(member_node) = self.arena.get(member_idx) {
                    // Use a tighter bound for property declarations to avoid
                    // consuming comments that belong to the next class member.
                    // Property node.end can extend past newlines into the next
                    // member's territory, so we bound by the next member's pos.
                    let skip_end = class
                        .members
                        .nodes
                        .get(member_i + 1)
                        .and_then(|&next_idx| self.arena.get(next_idx))
                        .map_or(member_node.end, |next| next.pos);
                    // Find the actual end of the property's content
                    let actual_end = self.find_token_end_before_trivia(member_node.pos, skip_end);
                    // Find line end from actual_end
                    let line_end = if let Some(text) = self.source_text {
                        let bytes = text.as_bytes();
                        let mut pos = actual_end as usize;
                        while pos < bytes.len() && bytes[pos] != b'\n' && bytes[pos] != b'\r' {
                            pos += 1;
                        }
                        pos as u32
                    } else {
                        actual_end
                    };
                    // Collect trailing comments on the same line for both static and
                    // non-static fields. Static comments are stored on static_field_inits;
                    // non-static comments are stored on field_inits for replay in the
                    // constructor prologue.
                    if let Some(text) = self.source_text {
                        let mut trailing = Vec::new();
                        let mut idx = self.comment_emit_idx;
                        while idx < self.all_comments.len() {
                            let c = &self.all_comments[idx];
                            if c.pos >= actual_end && c.end <= line_end {
                                let comment_text =
                                    crate::safe_slice::slice(text, c.pos as usize, c.end as usize);
                                trailing.push(comment_text.to_string());
                            }
                            if c.end > line_end {
                                break;
                            }
                            idx += 1;
                        }
                        if is_static {
                            if let Some(entry) = static_field_inits
                                .iter_mut()
                                .find(|e| e.2 == member_node.pos)
                            {
                                entry.4 = trailing;
                            }
                        } else if !trailing.is_empty() {
                            if let Some(entry) = field_inits.get_mut(field_init_comment_idx) {
                                entry.4 = trailing.clone();
                            }
                            // Also update pending_class_field_inits so existing constructors
                            // that read from it during the member loop get the comments
                            if let Some(entry) = self
                                .pending_class_field_inits
                                .get_mut(field_init_comment_idx)
                            {
                                entry.4 = trailing;
                            }
                        }
                    }
                    if !is_static {
                        field_init_comment_idx += 1;
                    }
                    while self.comment_emit_idx < self.all_comments.len() {
                        let c = &self.all_comments[self.comment_emit_idx];
                        if c.end <= line_end {
                            self.comment_emit_idx += 1;
                        } else {
                            break;
                        }
                    }
                }
                continue;
            }

            // Skip static blocks that need lowering to IIFEs after the class
            if needs_static_block_lowering
                && let Some(member_node) = self.arena.get(member_idx)
                && member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
            {
                // Find the opening `{` of the static block to determine where
                // inner (body) comments start. We skip leading comments but save
                // the index of the first inner comment for replay during IIFE emission.
                let brace_pos = if let Some(text) = self.source_text {
                    let bytes = text.as_bytes();
                    let start = member_node.pos as usize;
                    let end = (member_node.end as usize).min(bytes.len());
                    bytes[start..end]
                        .iter()
                        .position(|&b| b == b'{')
                        .map(|off| (start + off + 1) as u32)
                        .unwrap_or(member_node.end)
                } else {
                    member_node.end
                };
                // Skip comments preceding the block opening `{`
                while self.comment_emit_idx < self.all_comments.len()
                    && self.all_comments[self.comment_emit_idx].end <= brace_pos
                {
                    self.comment_emit_idx += 1;
                }
                // Save index pointing at the first inner comment (if any)
                let inner_comment_idx = self.comment_emit_idx;
                // Skip remaining inner comments so they don't leak as leading
                // comments of subsequent class members
                self.skip_comments_for_erased_node(member_node);
                deferred_static_blocks.push((member_idx, inner_comment_idx));
                continue;
            }

            // Check if this member is erased (no runtime representation)
            if let Some(member_node) = self.arena.get(member_idx) {
                let is_erased = match member_node.kind {
                    // Bodyless methods are erased (abstract methods without body,
                    // overload signatures). Abstract methods WITH a body (an error
                    // in TS) are still emitted by tsc, so we must not erase them.
                    k if k == syntax_kind_ext::METHOD_DECLARATION => self
                        .arena
                        .get_method_decl(member_node)
                        .is_some_and(|m| m.body.is_none()),
                    // Abstract accessors without body are erased. Bodyless non-abstract
                    // accessors (error case) are kept — tsc emits them as `{}`.
                    // Abstract accessors WITH a body (error case) are also kept.
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        self.arena.get_accessor(member_node).is_some_and(|a| {
                            self.arena
                                .has_modifier(&a.modifiers, SyntaxKind::AbstractKeyword)
                                && a.body.is_none()
                        })
                    }
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                        if let Some(p) = self.arena.get_property_decl(member_node) {
                            // Abstract properties: erased
                            if self
                                .arena
                                .has_modifier(&p.modifiers, SyntaxKind::AbstractKeyword)
                            {
                                true
                            } else {
                                // Type-only properties (no initializer, not private, not accessor): erased
                                // But when useDefineForClassFields is true (ES2022+),
                                // uninitialised properties are real class field declarations.
                                if self.ctx.options.use_define_for_class_fields {
                                    false
                                } else {
                                    let is_private = self.arena.get(p.name).is_some_and(|n| {
                                        n.kind == SyntaxKind::PrivateIdentifier as u16
                                    });
                                    let has_accessor = self
                                        .arena
                                        .has_modifier(&p.modifiers, SyntaxKind::AccessorKeyword);
                                    p.initializer.is_none() && !is_private && !has_accessor
                                }
                            }
                        } else {
                            false
                        }
                    }
                    // Bodyless constructor overloads are erased
                    k if k == syntax_kind_ext::CONSTRUCTOR => self
                        .arena
                        .get_constructor(member_node)
                        .is_some_and(|c| c.body.is_none()),
                    // Index signatures are TypeScript-only
                    k if k == syntax_kind_ext::INDEX_SIGNATURE => true,
                    // Semicolon class elements are preserved in JS output (valid JS syntax)
                    k if k == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT => false,
                    _ => false,
                };
                if is_erased {
                    // When an erased property has a computed name whose expression
                    // could have runtime side effects, tsc emits the expression as
                    // a standalone statement after the class body.
                    // e.g., `[Symbol.iterator]: Type` → `Symbol.iterator;`
                    // Only expressions that might have observable effects are emitted:
                    // property accesses, element accesses, calls, assignments, etc.
                    // Simple identifiers and literals are NOT emitted (no side effects).
                    // Skip this when computed property hoisting is active — the comma
                    // expression already handles side effects.
                    if !needs_computed_prop_hoisting
                        && member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                        && let Some(p) = self.arena.get_property_decl(member_node)
                        && let Some(name_node) = self.arena.get(p.name)
                        && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                        && let Some(computed) = self.arena.get_computed_property(name_node)
                        && let Some(expr_node) = self.arena.get(computed.expression)
                    {
                        let k = expr_node.kind;
                        let is_side_effect_free = k == SyntaxKind::Identifier as u16
                            || k == SyntaxKind::StringLiteral as u16
                            || k == SyntaxKind::NumericLiteral as u16
                            || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                            || k == SyntaxKind::PrivateIdentifier as u16;
                        if !is_side_effect_free {
                            computed_property_side_effects.push(computed.expression);
                        }
                    }
                    self.skip_comments_for_erased_node(member_node);
                    continue;
                }
            }

            // Emit leading comments before this member
            if let Some(member_node) = self.arena.get(member_idx) {
                self.emit_comments_before_pos(member_node.pos);
            }

            let before_len = self.writer.len();
            let auto_accessor = auto_accessor_members
                .iter()
                .find(|(idx, _, _, _)| *idx == member_idx)
                .map(|(_, storage_name, _, is_static)| (storage_name.clone(), *is_static));
            if let Some(member_node) = self.arena.get(member_idx) {
                let property_end = if auto_accessor.is_some() {
                    let upper = class
                        .members
                        .nodes
                        .get(member_i + 1)
                        .and_then(|&next_idx| self.arena.get(next_idx))
                        .map(|n| n.pos)
                        .unwrap_or(member_node.end);
                    Some(self.find_token_end_before_trivia(member_node.pos, upper))
                } else {
                    None
                };

                if let Some((storage_name, is_static)) = auto_accessor {
                    self.emit_auto_accessor_methods(
                        member_node,
                        &storage_name,
                        is_static,
                        auto_accessor_class_alias.as_deref(),
                        lower_auto_accessors_to_private_fields,
                        &class_name,
                        property_end.unwrap_or(member_node.end),
                    );
                } else {
                    self.emit(member_idx);
                }
            }
            let mut emit_standalone_class_semicolon = false;
            if let Some(member_node) = self.arena.get(member_idx)
                && (member_node.kind == syntax_kind_ext::GET_ACCESSOR
                    || member_node.kind == syntax_kind_ext::SET_ACCESSOR
                    || member_node.kind == syntax_kind_ext::METHOD_DECLARATION)
            {
                let next_is_semicolon_member = class
                    .members
                    .nodes
                    .get(member_i + 1)
                    .and_then(|&idx| self.arena.get(idx))
                    .is_some_and(|n| n.kind == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT);

                // Check if the member has a body (method/accessor with `{}`).
                let member_has_body_for_semi = match member_node.kind {
                    k if k == syntax_kind_ext::METHOD_DECLARATION => self
                        .arena
                        .get_method_decl(member_node)
                        .is_some_and(|m| m.body.is_some()),
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        self.arena
                            .get_accessor(member_node)
                            .is_some_and(|a| a.body.is_some())
                    }
                    _ => false,
                };
                if !next_is_semicolon_member {
                    let has_source_semicolon = self.source_text.is_some_and(|text| {
                        let member_end = std::cmp::min(member_node.end as usize, text.len());
                        // For members WITHOUT bodies, check the gap after the member.
                        if !member_has_body_for_semi {
                            let gap_end = class
                                .members
                                .nodes
                                .get(member_i + 1)
                                .and_then(|&idx| self.arena.get(idx))
                                .map_or_else(
                                    || {
                                        let search_end =
                                            std::cmp::min(node.end as usize, text.len());
                                        text[member_end..search_end]
                                            .rfind('}')
                                            .map_or(search_end, |pos| member_end + pos)
                                    },
                                    |n| n.pos as usize,
                                );
                            let gap_end = std::cmp::min(gap_end, text.len());
                            if member_end < gap_end && text[member_end..gap_end].contains(';') {
                                return true;
                            }
                        }
                        // For members WITH bodies, the parser may absorb trailing `;`
                        // into the member span (e.g., `get x() { ... };`).
                        // Check if the member source ends with `} ;` pattern.
                        if member_has_body_for_semi && member_end >= 2 {
                            let tail = &text[member_node.pos as usize..member_end];
                            let trimmed = tail.trim_end();
                            if let Some(before_semi) = trimmed.strip_suffix(';')
                                && before_semi.trim_end().ends_with('}')
                            {
                                return true;
                            }
                        }
                        false
                    });
                    emit_standalone_class_semicolon = has_source_semicolon;
                }

                // Some parser recoveries include the semicolon in member.end without
                // creating a separate SEMICOLON_CLASS_ELEMENT; preserve it from source.
                // Only check this for methods/accessors that DON'T have a body (i.e.,
                // abstract methods or overload signatures like `foo(): void;`).
                if !member_has_body_for_semi
                    && self.source_text.is_some_and(|text| {
                        let start = std::cmp::min(member_node.pos as usize, text.len());
                        let end = std::cmp::min(member_node.end as usize, text.len());
                        if start >= end {
                            return false;
                        }
                        let member_text = text[start..end].trim_end();
                        member_text.ends_with(';')
                    })
                {
                    emit_standalone_class_semicolon = true;
                }
            }
            if self.writer.len() == before_len
                && let (Some(member_node), Some(text)) =
                    (self.arena.get(member_idx), self.source_text)
            {
                let start = std::cmp::min(member_node.pos as usize, text.len());
                let end = std::cmp::min(member_node.end as usize, text.len());
                if start < end {
                    let raw = &text[start..end];
                    let compact: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
                    if compact.starts_with("*(){") {
                        self.write("*() { }");
                    }
                }
            }
            // Only add newline if something was actually emitted
            if self.writer.len() > before_len && !self.writer.is_at_line_start() {
                emitted_any_member = true;
                // Emit trailing comments on the same line as the member.
                // For property declarations, member_node.end can include the leading trivia
                // of the next member (because the parser records token_end() = scanner.pos
                // which is after the lookahead token). Use the AST initializer/name end
                // to get the true end of the property's last token.
                if let Some(member_node) = self.arena.get(member_idx) {
                    // Use the next member's pos as upper bound to avoid scanning
                    // past the current member into the next member's trivia.
                    // For the last member, use the class body's closing `}` position
                    // so we don't steal comments that belong on the closing brace line.
                    let next_member_pos = class
                        .members
                        .nodes
                        .get(member_i + 1)
                        .and_then(|&next_idx| self.arena.get(next_idx))
                        .map(|n| n.pos);
                    let upper = next_member_pos.unwrap_or(member_node.end);
                    let token_end = self.find_token_end_before_trivia(member_node.pos, upper);
                    // For the last member, cap trailing comment scan at the class
                    // body's closing `}` to avoid stealing comments that belong
                    // on the closing brace line.
                    if next_member_pos.is_none() {
                        self.emit_trailing_comments_before(token_end, class_body_close_pos);
                    } else {
                        self.emit_trailing_comments(token_end);
                    }
                }
                self.write_line();
                if emit_standalone_class_semicolon {
                    self.write(";");
                    self.write_line();
                }
            }
        }

        if !emitted_any_member && let Some(text) = self.source_text {
            let start = std::cmp::min(node.pos as usize, text.len());
            let end = std::cmp::min(node.end as usize, text.len());
            if start < end {
                let raw = &text[start..end];
                let compact: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
                if compact.contains("*(){}") {
                    self.write("*() { }");
                    self.write_line();
                }
            }
        }

        // Skip orphaned comments inside the class body.
        // When class members are erased (type-only properties, abstract members, etc.),
        // comments on lines between erased members or between the last erased member
        // and the closing `}` are left unconsumed. Without this, they leak into the
        // output as spurious comments after the class.
        // Find the closing `}` position and skip any remaining comments before it.
        {
            let class_body_end = self.find_token_end_before_trivia(node.pos, node.end);
            while self.comment_emit_idx < self.all_comments.len() {
                let c = &self.all_comments[self.comment_emit_idx];
                if c.end <= class_body_end {
                    self.comment_emit_idx += 1;
                } else {
                    break;
                }
            }
        }

        // Restore field inits
        self.pending_class_field_inits = prev_field_inits;
        self.pending_auto_accessor_inits = prev_auto_accessor_inits;

        // Undo the extra indent level added for comma expression wrapper
        if class_expr_temp.is_some() {
            self.decrease_indent();
        }
        self.decrease_indent();
        if class_expr_temp.is_some() {
            // Indent the closing brace inside the comma-expression context
            self.increase_indent();
            self.write("}");
            self.decrease_indent();
        } else {
            self.write("}");
        }
        if assignment_prefix.is_some() && class_expr_temp.is_none() {
            self.write(";");
        }

        // Emit computed property name hoisting comma expression or standalone side effects.
        if !computed_prop_entries.is_empty() {
            if class_expr_temp.is_some() {
                for (temp_name, expr_idx) in computed_prop_entries.iter() {
                    self.write(",");
                    self.write_line();
                    self.increase_indent();
                    if let Some(temp) = temp_name {
                        self.write(temp);
                        self.write(" = ");
                    }
                    self.emit_expression(*expr_idx);
                    self.decrease_indent();
                }
            } else {
                // Emit as a single comma expression: `_a = expr1, sideEffect, _b = expr2;`
                self.write_line();
                for (i, (temp_name, expr_idx)) in computed_prop_entries.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    if let Some(temp) = temp_name {
                        self.write(temp);
                        self.write(" = ");
                    }
                    self.emit_expression(*expr_idx);
                }
                self.write(";");
            }
        } else {
            // Emit computed property name side-effect statements for erased members
            // (when hoisting is not active, e.g., ES2022+ targets).
            // e.g., `[Symbol.iterator]: Type` → `Symbol.iterator;`
            for expr_idx in &computed_property_side_effects {
                if class_expr_temp.is_some() {
                    self.write(",");
                    self.write_line();
                    self.increase_indent();
                    self.emit_expression(*expr_idx);
                    self.decrease_indent();
                } else {
                    self.write_line();
                    self.emit_expression(*expr_idx);
                    self.write(";");
                }
            }
        }

        if let Some(class_name) = self.pending_commonjs_class_export_name.take() {
            self.write_line();
            self.write("exports.");
            self.write(&class_name);
            self.write(" = ");
            self.write(&class_name);
            self.write(";");
        }

        if let Some(recovery_name) = self.class_var_function_recovery_name(node) {
            self.write_line();
            self.write("var ");
            self.write(&recovery_name);
            self.write(";");
            self.write_line();
            self.write("() => { };");
        }

        // Emit static field initializers after class body
        // For class expressions: use comma expression `(_a = class C {}, _a.field = value, _a)`
        // For class declarations: use separate statements `ClassName.field = value;`
        if !static_field_inits.is_empty()
            && !class_name.is_empty()
            && let Some(temp) = class_expr_static_temp.as_ref()
        {
            // Class expression comma-expression: `(_a = class C {}, _a.a = 1, _a)`
            // The `(_a = ` prefix was already emitted before the `class` keyword.
            for (name_emit, init_idx, _member_pos, _leading_comments, _trailing_comments) in
                &static_field_inits
            {
                self.write(",");
                self.write_line();
                self.increase_indent();
                if self.ctx.options.use_define_for_class_fields {
                    let define_name = match name_emit {
                        PropertyNameEmit::Dot(s) => format!("\"{s}\""),
                        PropertyNameEmit::Bracket(s) | PropertyNameEmit::BracketNumeric(s) => {
                            s.clone()
                        }
                    };
                    self.write("Object.defineProperty(");
                    self.write(temp);
                    self.write(", ");
                    self.write(&define_name);
                    self.write(", {");
                    self.write_line();
                    self.increase_indent();
                    self.write("enumerable: true,");
                    self.write_line();
                    self.write("configurable: true,");
                    self.write_line();
                    self.write("writable: true,");
                    self.write_line();
                    self.write("value: ");
                    // Emit the initializer, then substitute class name with temp var
                    let before = self.writer.len();
                    self.with_scoped_static_initializer_context_cleared(|this| {
                        this.emit_expression(*init_idx);
                    });
                    let after = self.writer.len();
                    if !class_name.is_empty() && class_name != *temp {
                        let full = self.writer.get_output().to_string();
                        let segment = &full[before..after];
                        let replaced = replace_identifier(segment, &class_name, temp);
                        if replaced != segment {
                            self.writer.truncate(before);
                            self.write(&replaced);
                        }
                    }
                    self.write_line();
                    self.decrease_indent();
                    self.write("})");
                } else {
                    self.write(temp);
                    match name_emit {
                        PropertyNameEmit::Dot(name) => {
                            self.write(".");
                            self.write(name);
                        }
                        PropertyNameEmit::Bracket(name)
                        | PropertyNameEmit::BracketNumeric(name) => {
                            self.write("[");
                            self.write(name);
                            self.write("]");
                        }
                    }
                    self.write(" = ");
                    // Emit the initializer, then substitute class name with temp var
                    let before = self.writer.len();
                    self.with_scoped_static_initializer_context_cleared(|this| {
                        this.emit_expression(*init_idx);
                    });
                    let after = self.writer.len();
                    if !class_name.is_empty() && class_name != *temp {
                        let full = self.writer.get_output().to_string();
                        let segment = &full[before..after];
                        let replaced = replace_identifier(segment, &class_name, temp);
                        if replaced != segment {
                            self.writer.truncate(before);
                            self.write(&replaced);
                        }
                    }
                }
                self.decrease_indent();
            }
            self.write(",");
            self.write_line();
            self.increase_indent();
            self.write(temp);
            self.write(")");
            self.decrease_indent();
        } else if !static_field_inits.is_empty() && !class_name.is_empty() {
            self.write_line();
            // If static field initializers reference `this`, emit `_a = ClassName;`
            // so that `this` can be replaced with the temp alias.
            if let Some(ref alias) = static_this_alias {
                self.write(alias);
                self.write(" = ");
                self.write(&class_name);
                self.write(";");
                self.write_line();
            }
            for (name_emit, init_idx, _member_pos, leading_comments, trailing_comments) in
                &static_field_inits
            {
                // Emit saved leading comments from the original static property declaration
                for (comment_text, source_pos) in leading_comments {
                    self.write_comment_with_reindent(comment_text, Some(*source_pos));
                    self.write_line();
                }
                if self.ctx.options.use_define_for_class_fields {
                    let define_name = match name_emit {
                        PropertyNameEmit::Dot(s) => format!("\"{s}\""),
                        PropertyNameEmit::Bracket(s) | PropertyNameEmit::BracketNumeric(s) => {
                            s.clone()
                        }
                    };
                    self.write("Object.defineProperty(");
                    self.write(&class_name);
                    self.write(", ");
                    self.write(&define_name);
                    self.write(", {");
                    self.write_line();
                    self.increase_indent();
                    self.write("enumerable: true,");
                    self.write_line();
                    self.write("configurable: true,");
                    self.write_line();
                    self.write("writable: true,");
                    self.write_line();
                    self.write("value: ");
                    self.emit_expression_with_scoped_static_initializer_mode(
                        *init_idx,
                        static_initializer_this_binding,
                        static_initializer_super_base,
                        externalized_static_initializer_uses_undefined_receiver,
                    );
                    self.write_line();
                    self.decrease_indent();
                    self.write("});");
                } else {
                    self.write(&class_name);
                    match name_emit {
                        PropertyNameEmit::Dot(name) => {
                            self.write(".");
                            self.write(name);
                        }
                        PropertyNameEmit::Bracket(name)
                        | PropertyNameEmit::BracketNumeric(name) => {
                            self.write("[");
                            self.write(name);
                            self.write("]");
                        }
                    }
                    self.write(" = ");
                    self.emit_expression_with_scoped_static_initializer_mode(
                        *init_idx,
                        static_initializer_this_binding,
                        static_initializer_super_base,
                        externalized_static_initializer_uses_undefined_receiver,
                    );
                    self.write(";");
                }
                // Emit saved trailing comments (e.g. `// ok` from
                // `static intance = new C3(); // ok`)
                for comment_text in trailing_comments {
                    self.write_space();
                    self.write_comment(comment_text);
                }
                self.write_line();
            }
        }

        // Emit auto-accessor WeakMap initializations after class body:
        // var _Class_prop_accessor_storage;
        // ...
        // _Class_prop_accessor_storage = new WeakMap();
        if lower_auto_accessors_to_weakmap
            && (!auto_accessor_instance_inits.is_empty()
                || !auto_accessor_static_inits.is_empty()
                || auto_accessor_class_alias.is_some())
        {
            self.write_line();
            let mut wrote_alias_line = false;

            if let Some(alias) = auto_accessor_class_alias.as_ref()
                && !alias.is_empty()
                && !class_name.is_empty()
            {
                self.write(alias);
                self.write(" = ");
                self.write(&class_name);
                wrote_alias_line = true;
            }

            if !auto_accessor_instance_inits.is_empty() {
                if wrote_alias_line {
                    self.write(", ");
                }
                let mut wrote_instance_line = false;
                for (i, (storage_name, _init_idx)) in
                    auto_accessor_instance_inits.iter().enumerate()
                {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.write(storage_name);
                    self.write(" = new WeakMap()");
                    wrote_instance_line = true;
                }
                if wrote_alias_line || wrote_instance_line {
                    self.write(";");
                    self.write_line();
                }
            }

            for (storage_name, init_idx) in &auto_accessor_static_inits {
                self.write(storage_name);
                self.write(" = { value: ");
                if let Some(init) = init_idx {
                    self.emit_expression(*init);
                } else {
                    self.write("void 0");
                }
                self.write(" };");
                self.write_line();
            }
        }

        // Emit private field WeakMap initializations after class body:
        // _C_field1 = new WeakMap();
        let has_weakmap_inits = !self.pending_weakmap_inits.is_empty();
        let static_private_inits = std::mem::take(&mut self.pending_static_private_inits);
        let private_class_alias_pair = self.pending_private_class_alias.take();

        // Emit combined initialization line after class body.
        // tsc joins class alias, WeakMap inits, WeakSet init, and private method/accessor defs
        // with commas on a single line, e.g.:
        // _a = A, _A_foo = new WeakMap(), _A_instances = new WeakSet(), _A_m = function _A_m() { };
        let instances_ws = self.pending_instances_weakset_add.clone();
        let method_defs = std::mem::take(&mut self.pending_private_method_defs);
        let accessor_defs = std::mem::take(&mut self.pending_private_accessor_defs);
        let has_post_class_inits = private_class_alias_pair.is_some()
            || has_weakmap_inits
            || instances_ws.is_some()
            || !method_defs.is_empty()
            || !accessor_defs.is_empty();

        // For class expressions with private field lowering, emit the WeakMap/WeakSet/method
        // initializations as comma-separated items inside the wrapping expression:
        //   (_a = class C { ... },
        //       _C_field = new WeakMap(),
        //       _C_instances = new WeakSet(),
        //       _C_method = function _C_method() { },
        //       _a)
        // For class declarations, emit as separate statements after the class body.
        if needs_private_comma_expr && has_post_class_inits {
            // Emit comma-separated inits inline in the expression.
            // The `(_a = ` prefix was already emitted before the `class` keyword.

            // WeakMap inits: _X_field = new WeakMap()
            let weakmap_inits = self.pending_weakmap_inits.clone();
            for init in &weakmap_inits {
                self.write(",");
                self.write_line();
                self.increase_indent();
                self.write(init);
                self.decrease_indent();
            }

            // WeakSet: _X_instances = new WeakSet()
            if let Some(ref ws_name) = instances_ws {
                self.write(",");
                self.write_line();
                self.increase_indent();
                self.write(ws_name);
                self.write(" = new WeakSet()");
                self.decrease_indent();
            }

            // Private method function definitions
            for (var_name, body_idx, params) in &method_defs {
                self.write(",");
                self.write_line();
                self.increase_indent();
                self.write(var_name);
                self.write(" = function ");
                self.write(var_name);
                self.write("(");
                for (i, &param_idx) in params.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    if let Some(param_node) = self.arena.get(param_idx)
                        && let Some(param_data) = self.arena.get_parameter(param_node)
                    {
                        self.emit(param_data.name);
                    }
                }
                self.write(") ");
                self.emit_single_line_block(*body_idx);
                self.decrease_indent();
            }

            // Private accessor function definitions
            for def in &accessor_defs {
                self.write(",");
                self.write_line();
                self.increase_indent();
                self.write(&def.var_name);
                self.write(" = function ");
                self.write(&def.var_name);
                self.write("(");
                if let Some(param_idx) = def.param
                    && let Some(param_node) = self.arena.get(param_idx)
                    && let Some(param_data) = self.arena.get_parameter(param_node)
                {
                    self.emit(param_data.name);
                }
                self.write(") ");
                self.emit_single_line_block(def.body);
                self.decrease_indent();
            }

            // Emit static private field value initializations as comma items
            // (e.g., `_D_field = { value: __classPrivateFieldGet(...) }`)
            for (var_name, init_idx) in &static_private_inits {
                self.write(",");
                self.write_line();
                self.increase_indent();
                self.write(var_name);
                self.write(" = { value: ");
                if init_idx.is_some() {
                    self.emit_expression(*init_idx);
                } else {
                    self.write("void 0");
                }
                self.write(" }");
                self.decrease_indent();
            }

            // Close the comma expression with the temp var, unless the static field
            // comma expr path will handle the closing.
            if !needs_static_comma_expr && let Some(ref temp) = class_expr_temp {
                self.write(",");
                self.write_line();
                self.increase_indent();
                self.write(temp);
                self.write(")");
                self.decrease_indent();
            }
        } else if has_post_class_inits {
            self.write_line();
            let mut first = true;

            // Class alias: _a = ClassName
            if let Some((ref alias, ref cls_name)) = private_class_alias_pair {
                self.write(alias);
                self.write(" = ");
                self.write(cls_name);
                first = false;
            }

            // WeakMap inits first (tsc order): _X_field = new WeakMap()
            let weakmap_inits = self.pending_weakmap_inits.clone();
            for init in &weakmap_inits {
                if !first {
                    self.write(", ");
                }
                self.write(init);
                first = false;
            }

            // WeakSet: _X_instances = new WeakSet()
            if let Some(ref ws_name) = instances_ws {
                if !first {
                    self.write(", ");
                }
                self.write(ws_name);
                self.write(" = new WeakSet()");
                first = false;
            }

            // Private method function definitions:
            // _C_method = function _C_method(params) { ... }
            for (var_name, body_idx, params) in &method_defs {
                if !first {
                    self.write(", ");
                }
                self.write(var_name);
                self.write(" = function ");
                self.write(var_name);
                self.write("(");
                for (i, &param_idx) in params.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    // Emit parameter name (identifier or pattern)
                    if let Some(param_node) = self.arena.get(param_idx)
                        && let Some(param_data) = self.arena.get_parameter(param_node)
                    {
                        self.emit(param_data.name);
                    }
                }
                self.write(") ");
                self.emit_single_line_block(*body_idx);
                first = false;
            }

            // Private accessor function definitions:
            // _C_prop_get = function _C_prop_get() { ... }
            // _C_prop_set = function _C_prop_set(param) { ... }
            for def in &accessor_defs {
                if !first {
                    self.write(", ");
                }
                self.write(&def.var_name);
                self.write(" = function ");
                self.write(&def.var_name);
                self.write("(");
                if let Some(param_idx) = def.param {
                    // Emit setter parameter name
                    if let Some(param_node) = self.arena.get(param_idx)
                        && let Some(param_data) = self.arena.get_parameter(param_node)
                    {
                        self.emit(param_data.name);
                    }
                }
                self.write(") ");
                self.emit_single_line_block(def.body);
                first = false;
            }

            self.write(";");
        }

        // Emit static private field value initializations after class body:
        // `_A_field = { value: 10 };`
        // For class expressions with private lowering, these are already emitted
        // as comma items above in the private comma expr block.
        if needs_private_comma_expr {
            // Already emitted above in the comma expression block.
        } else {
            for (var_name, init_idx) in &static_private_inits {
                self.write_line();
                self.write(var_name);
                self.write(" = { value: ");
                if init_idx.is_some() {
                    self.emit_expression(*init_idx);
                } else {
                    self.write("void 0");
                }
                self.write(" };");
            }
        }

        // Emit deferred static blocks as IIFEs after the class body.
        // When defer_class_static_blocks is true, store for caller to emit later.
        if self.defer_class_static_blocks {
            self.deferred_class_static_blocks
                .extend(deferred_static_blocks);
        } else {
            self.emit_static_block_iifes(deferred_static_blocks);
        }

        // Restore private field state (for nested classes)
        self.private_field_weakmaps = prev_private_field_weakmaps;
        self.pending_weakmap_inits = prev_pending_weakmap_inits;
        self.pending_static_private_inits = prev_pending_static_private_inits;
        self.pending_private_class_alias = prev_pending_private_class_alias;
        self.private_member_info = prev_private_member_info;
        self.pending_private_field_constructor_inits = prev_pending_private_field_constructor_inits;
        self.pending_instances_weakset_add = prev_pending_instances_weakset_add;
        self.pending_private_method_defs = prev_pending_private_method_defs;
        self.pending_private_accessor_defs = prev_pending_private_accessor_defs;
        self.private_members_to_skip = prev_private_members_to_skip;

        // Clear computed property temp map to avoid leaking to the next class.
        self.computed_prop_temp_map.clear();

        // Track class name to prevent duplicate var declarations for merged namespaces.
        // When a class and namespace have the same name (declaration merging), the class
        // provides the declaration, so the namespace shouldn't emit `var name;`.
        if class.name.is_some() {
            let class_name = self.get_identifier_text_idx(class.name);
            if !class_name.is_empty() {
                self.declared_namespace_names.insert(class_name);
            }
        }
    }

    /// Resolve the binding name for an anonymous class expression from its parent chain.
    /// For `const C = class { ... }`, this returns `Some("C")`.
    /// Walks up: `ClassExpression` -> `VariableDeclaration` -> name identifier.
    fn resolve_class_expr_binding_name(&self, class_idx: NodeIndex) -> Option<String> {
        let ext = self.arena.get_extended(class_idx)?;
        let parent_idx = ext.parent;
        if parent_idx.is_none() {
            return None;
        }
        let parent_node = self.arena.get(parent_idx)?;
        if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let decl = self.arena.get_variable_declaration(parent_node)?;
            let name_node = self.arena.get(decl.name)?;
            if name_node.kind == SyntaxKind::Identifier as u16 {
                let name = self.get_identifier_text_idx(decl.name);
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }
        None
    }

    /// Emit deferred static block IIFEs as `(() => { ... })();`.
    /// Check if a computed property name expression is side-effect-free.
    /// Looks through type assertions and parenthesized expressions to find
    /// the core expression, then checks if it's a literal/identifier/keyword.
    fn is_computed_name_expr_side_effect_free(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return true;
        };
        let k = expr_node.kind;
        // Simple side-effect-free expressions
        if k == SyntaxKind::Identifier as u16
            || k == SyntaxKind::PrivateIdentifier as u16
            || k == SyntaxKind::StringLiteral as u16
            || k == SyntaxKind::NumericLiteral as u16
            || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            || k == SyntaxKind::TrueKeyword as u16
            || k == SyntaxKind::FalseKeyword as u16
            || k == SyntaxKind::NullKeyword as u16
            || k == SyntaxKind::UndefinedKeyword as u16
        {
            return true;
        }
        // Type assertions: `<T>expr`, `expr as T`, `expr satisfies T` — look through
        if (k == syntax_kind_ext::TYPE_ASSERTION
            || k == syntax_kind_ext::AS_EXPRESSION
            || k == syntax_kind_ext::SATISFIES_EXPRESSION)
            && let Some(assertion) = self.arena.get_type_assertion(expr_node)
        {
            return self.is_computed_name_expr_side_effect_free(assertion.expression);
        }
        // Parenthesized expression: `(expr)` — look through
        if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.arena.get_parenthesized(expr_node)
        {
            return self.is_computed_name_expr_side_effect_free(paren.expression);
        }
        false
    }

    pub(in crate::emitter) fn emit_static_block_iifes(&mut self, blocks: Vec<(NodeIndex, usize)>) {
        for (static_block_idx, saved_comment_idx) in blocks {
            self.write_line();
            self.write("(() => ");
            self.comment_emit_idx = saved_comment_idx;
            if let Some(static_node) = self.arena.get(static_block_idx) {
                let prev = self.emitting_function_body_block;
                self.emitting_function_body_block = true;
                // Save and restore hoisted temps so outer-scope vars (e.g. private
                // field WeakMap names) don't get re-declared inside the IIFE body.
                let saved_temps = std::mem::take(&mut self.hoisted_assignment_temps);
                self.emit_block(static_node, static_block_idx);
                // Any temps generated inside the IIFE block have already been
                // emitted by emit_block; restore the outer-scope temps.
                self.hoisted_assignment_temps = saved_temps;
                self.emitting_function_body_block = prev;
            } else {
                self.write("{ }");
            }
            self.write(")();");
        }
    }

    pub(in crate::emitter) fn class_has_auto_accessor_members(
        &self,
        class: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let Some(prop_data) = self.arena.get_property_decl(member_node) else {
                continue;
            };
            if self
                .arena
                .has_modifier(&prop_data.modifiers, SyntaxKind::AccessorKeyword)
                && !self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword)
                && self
                    .arena
                    .get(prop_data.name)
                    .is_none_or(|n| n.kind != SyntaxKind::PrivateIdentifier as u16)
                && !self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::DeclareKeyword)
            {
                let Some(name_node) = self.arena.get(prop_data.name) else {
                    continue;
                };
                if name_node.kind == SyntaxKind::Identifier as u16 {
                    return true;
                }
            }
        }
        false
    }

    fn emit_auto_accessor_methods(
        &mut self,
        node: &Node,
        storage_name: &str,
        is_static: bool,
        static_accessor_alias: Option<&str>,
        lower_auto_accessor_to_private_fields: bool,
        class_name: &str,
        property_end: u32,
    ) {
        let Some(prop) = self.arena.get_property_decl(node) else {
            return;
        };

        if lower_auto_accessor_to_private_fields {
            if is_static {
                self.write("static ");
                self.write("#");
                self.write(storage_name);
                if prop.initializer.is_some() {
                    self.write(" = ");
                    self.emit_expression(prop.initializer);
                }
                self.write(";");
            } else {
                self.write("#");
                self.write(storage_name);
                if prop.initializer.is_some() {
                    self.write(" = ");
                    self.emit_expression(prop.initializer);
                }
                self.write(";");
            }
            self.write_line();

            if is_static {
                self.write("static ");
            }
            self.write("get ");
            self.emit(prop.name);
            self.write("() { return ");
            if is_static {
                self.write(if class_name.is_empty() {
                    "this"
                } else {
                    class_name
                });
                self.write(".#");
                self.write(storage_name.trim_start_matches('#'));
                self.write("; }");
                self.write_line();
                self.write("static ");
                self.write("set ");
                self.emit(prop.name);
                self.write("(value) { ");
                self.write(if class_name.is_empty() {
                    "this"
                } else {
                    class_name
                });
                self.write(".#");
                self.write(storage_name.trim_start_matches('#'));
                self.write(" = value; }");
            } else {
                self.write("this.");
                self.write("#");
                self.write(storage_name.trim_start_matches('#'));
                self.write("; }");
                self.write_line();
                self.write("set ");
                self.emit(prop.name);
                self.write("(value) { this.");
                self.write("#");
                self.write(storage_name.trim_start_matches('#'));
                self.write(" = value; }");
            }
            self.emit_trailing_comments(property_end);
            self.write_line();
        } else if is_static {
            let Some(alias) = static_accessor_alias else {
                return;
            };
            self.write("static ");
            self.write("get ");
            self.emit(prop.name);
            self.write("() { return ");
            self.write_helper("__classPrivateFieldGet");
            self.write("(");
            self.write(alias);
            self.write(", ");
            self.write(alias);
            self.write(", \"f\", ");
            self.write(storage_name);
            self.write("); }");
            self.emit_trailing_comments(property_end);
            self.write_line();
            self.write("static ");
            self.write("set ");
            self.emit(prop.name);
            self.write("(value) { ");
            self.write_helper("__classPrivateFieldSet");
            self.write("(");
            self.write(alias);
            self.write(", ");
            self.write(alias);
            self.write(", value, \"f\", ");
            self.write(storage_name);
            self.write("); }");
        } else {
            self.write("get ");
            self.emit(prop.name);
            self.write("() { return ");
            self.write_helper("__classPrivateFieldGet");
            self.write("(this, ");
            self.write(storage_name);
            self.write(", \"f\"); }");
            self.emit_trailing_comments(property_end);
            self.write_line();
            self.write("set ");
            self.emit(prop.name);
            self.write("(value) { ");
            self.write_helper("__classPrivateFieldSet");
            self.write("(this, ");
            self.write(storage_name);
            self.write(", value, \"f\"); }");
        }
    }

    /// Parser recovery parity for malformed class members like:
    /// `var constructor() { }`
    /// which TypeScript preserves as:
    /// `var constructor;`
    /// `() => { };`
    fn class_var_function_recovery_name(&self, class_node: &Node) -> Option<String> {
        let text = self.source_text?;
        let start = std::cmp::min(class_node.pos as usize, text.len());
        let end = std::cmp::min(class_node.end as usize, text.len());
        if start >= end {
            return None;
        }

        let slice = &text[start..end];
        let mut i = 0usize;
        let bytes = slice.as_bytes();

        while i < bytes.len() {
            if bytes[i].is_ascii_whitespace() {
                i += 1;
                continue;
            }
            if i + 3 > bytes.len() || &bytes[i..i + 3] != b"var" {
                i += 1;
                continue;
            }
            i += 3;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let ident_start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$')
            {
                i += 1;
            }
            if ident_start == i {
                continue;
            }
            let ident = String::from_utf8_lossy(&bytes[ident_start..i]).to_string();
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] != b'(' {
                continue;
            }
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] != b')' {
                continue;
            }
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] != b'{' {
                continue;
            }
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] != b'}' {
                continue;
            }

            return Some(ident);
        }

        None
    }
}

#[cfg(test)]
#[path = "../../../tests/declarations_class.rs"]
mod tests;
