//! Helper types and functions for the ES5 class IR transformer.

use crate::transforms::emit_utils::identifier_text;
use rustc_hash::FxHashMap;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_parser::syntax::transform_utils::is_private_identifier;
use tsz_scanner::SyntaxKind;

/// Serialize a type annotation to a metadata runtime type string.
/// Mirrors the `Printer::serialize_type_for_metadata` logic for ES5 context.
pub(super) fn serialize_type_for_metadata(arena: &NodeArena, type_idx: NodeIndex) -> String {
    let Some(type_node) = arena.get(type_idx) else {
        return "Object".to_string();
    };
    let sk = |s: SyntaxKind| s as u16;
    match type_node.kind {
        k if k == sk(SyntaxKind::StringKeyword) => "String".to_string(),
        k if k == sk(SyntaxKind::NumberKeyword) => "Number".to_string(),
        k if k == sk(SyntaxKind::BooleanKeyword) => "Boolean".to_string(),
        k if k == sk(SyntaxKind::SymbolKeyword) => "Symbol".to_string(),
        k if k == sk(SyntaxKind::BigIntKeyword) => "BigInt".to_string(),
        k if k == sk(SyntaxKind::VoidKeyword)
            || k == sk(SyntaxKind::UndefinedKeyword)
            || k == sk(SyntaxKind::NullKeyword)
            || k == sk(SyntaxKind::NeverKeyword) =>
        {
            "void 0".to_string()
        }
        k if k == sk(SyntaxKind::AnyKeyword)
            || k == sk(SyntaxKind::UnknownKeyword)
            || k == sk(SyntaxKind::ObjectKeyword) =>
        {
            "Object".to_string()
        }
        k if k == syntax_kind_ext::TYPE_REFERENCE => {
            if let Some(type_ref) = arena.get_type_ref(type_node) {
                let name = get_identifier_text(arena, type_ref.type_name).unwrap_or_default();
                match name.as_str() {
                    "string" => "String".to_string(),
                    "number" => "Number".to_string(),
                    "boolean" => "Boolean".to_string(),
                    "symbol" => "Symbol".to_string(),
                    "bigint" => "BigInt".to_string(),
                    "void" | "undefined" | "null" | "never" => "void 0".to_string(),
                    "any" | "unknown" | "object" => "Object".to_string(),
                    _ => name,
                }
            } else {
                "Object".to_string()
            }
        }
        k if k == syntax_kind_ext::ARRAY_TYPE || k == syntax_kind_ext::TUPLE_TYPE => {
            "Array".to_string()
        }
        k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
            "Function".to_string()
        }
        k if k == syntax_kind_ext::UNION_TYPE => {
            if let Some(composite) = arena.get_composite_type(type_node) {
                let meaningful: Vec<NodeIndex> = composite
                    .types
                    .nodes
                    .iter()
                    .copied()
                    .filter(|&m_idx| {
                        let Some(m) = arena.get(m_idx) else {
                            return false;
                        };
                        if m.kind == sk(SyntaxKind::NullKeyword)
                            || m.kind == sk(SyntaxKind::UndefinedKeyword)
                            || m.kind == sk(SyntaxKind::VoidKeyword)
                            || m.kind == sk(SyntaxKind::NeverKeyword)
                        {
                            return false;
                        }
                        // Skip TypeReference to null/undefined/void/never
                        if m.kind == syntax_kind_ext::TYPE_REFERENCE
                            && let Some(type_ref) = arena.get_type_ref(m)
                        {
                            let ref_name =
                                get_identifier_text(arena, type_ref.type_name).unwrap_or_default();
                            if matches!(ref_name.as_str(), "null" | "undefined" | "void" | "never")
                            {
                                return false;
                            }
                        }
                        true
                    })
                    .collect();
                if meaningful.len() == 1 {
                    return serialize_type_for_metadata(arena, meaningful[0]);
                }
                if meaningful.len() > 1 {
                    let first = serialize_type_for_metadata(arena, meaningful[0]);
                    if first != "Object"
                        && meaningful[1..]
                            .iter()
                            .all(|&m| serialize_type_for_metadata(arena, m) == first)
                    {
                        return first;
                    }
                }
                if meaningful.is_empty() {
                    return "void 0".to_string();
                }
            }
            "Object".to_string()
        }
        k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
            if let Some(wrapped) = arena.get_wrapped_type(type_node) {
                return serialize_type_for_metadata(arena, wrapped.type_node);
            }
            "Object".to_string()
        }
        k if k == syntax_kind_ext::LITERAL_TYPE => {
            if let Some(lit) = arena.get_literal_type(type_node)
                && let Some(lit_node) = arena.get(lit.literal)
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
                    lk if lk == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => "Number".to_string(),
                    _ => "Object".to_string(),
                };
            }
            "Object".to_string()
        }
        k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => "String".to_string(),
        k if k == syntax_kind_ext::TYPE_OPERATOR => {
            if let Some(type_op) = arena.get_type_operator(type_node) {
                return serialize_type_for_metadata(arena, type_op.type_node);
            }
            "Object".to_string()
        }
        k if k == syntax_kind_ext::OPTIONAL_TYPE => {
            if let Some(wrapped) = arena.get_wrapped_type(type_node) {
                return serialize_type_for_metadata(arena, wrapped.type_node);
            }
            "Object".to_string()
        }
        k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
            if let Some(cond) = arena.get_conditional_type(type_node) {
                let true_type = serialize_type_for_metadata(arena, cond.true_type);
                let false_type = serialize_type_for_metadata(arena, cond.false_type);
                if true_type == false_type {
                    return true_type;
                }
            }
            "Object".to_string()
        }
        _ => "Object".to_string(),
    }
}

/// For a rest parameter, serialize the element type of the array type annotation.
/// e.g., `...args: string[]` → "String", `...args: number[]` → "Number".
/// If the type is not an array type or has no annotation, returns "Object".
fn serialize_rest_param_element_type(arena: &NodeArena, type_annotation: NodeIndex) -> String {
    if let Some(type_node) = arena.get(type_annotation)
        && type_node.kind == syntax_kind_ext::ARRAY_TYPE
        && let Some(arr) = arena.get_array_type(type_node)
    {
        return serialize_type_for_metadata(arena, arr.element_type);
    }
    "Object".to_string()
}

/// Serialize parameter types for `design:paramtypes` metadata.
pub(super) fn serialize_param_types(arena: &NodeArena, parameters: &NodeList) -> String {
    let mut parts = Vec::new();
    for &param_idx in &parameters.nodes {
        if let Some(param_node) = arena.get(param_idx)
            && let Some(param) = arena.get_parameter(param_node)
        {
            // Skip `this` parameter — it's TypeScript-only and erased in JS emit.
            if let Some(name_node) = arena.get(param.name) {
                if name_node.kind == SyntaxKind::ThisKeyword as u16 {
                    continue;
                }
                if name_node.kind == SyntaxKind::Identifier as u16
                    && arena
                        .get_identifier(name_node)
                        .is_some_and(|id| id.escaped_text == "this")
                {
                    continue;
                }
            }
            if param.dot_dot_dot_token {
                // Rest parameter: serialize the element type of the array type.
                let serialized = serialize_rest_param_element_type(arena, param.type_annotation);
                parts.push(serialized);
            } else if param.type_annotation.is_some() {
                parts.push(serialize_type_for_metadata(arena, param.type_annotation));
            } else {
                parts.push("Object".to_string());
            }
        }
    }
    parts.join(", ")
}

#[derive(Debug, Clone)]
pub(super) struct AutoAccessorFieldInfo {
    pub(super) member_idx: NodeIndex,
    pub(super) weakmap_name: String,
    pub(super) initializer: Option<NodeIndex>,
    pub(super) is_static: bool,
}

// =============================================================================
// Helper Types
// =============================================================================

/// Property name representation for IR building
pub(super) enum PropertyNameIR {
    Identifier(String),
    StringLiteral(String),
    NumericLiteral(String),
    Computed(NodeIndex),
}

// =============================================================================
// Helper Functions
// =============================================================================

pub(super) fn get_identifier_text(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    // Try simple identifier first
    if let Some(text) = identifier_text(arena, idx) {
        return Some(text);
    }
    let node = arena.get(idx)?;
    if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
        // For computed property names like ["goodbye"], extract the string literal text
        if let Some(computed) = arena.get_computed_property(node)
            && let Some(expr_node) = arena.get(computed.expression)
            && expr_node.kind == SyntaxKind::StringLiteral as u16
        {
            return arena.get_literal(expr_node).map(|lit| lit.text.clone());
        }
        None
    } else if node.kind == SyntaxKind::StringLiteral as u16 {
        arena.get_literal(node).map(|lit| lit.text.clone())
    } else {
        None
    }
}

/// Collect accessor pairs (getter/setter) from class members.
/// When `collect_static` is true, collects static accessors; otherwise collects instance accessors.
pub(super) fn has_effective_static_modifier(
    arena: &NodeArena,
    modifiers: &Option<NodeList>,
) -> bool {
    modifiers.as_ref().is_some_and(|mods| {
        mods.nodes
            .iter()
            .filter(|&&idx| {
                arena
                    .get(idx)
                    .is_some_and(|node| node.kind == SyntaxKind::StaticKeyword as u16)
            })
            .count()
            == 1
    })
}

pub(super) fn collect_accessor_pairs(
    arena: &NodeArena,
    members: &NodeList,
    collect_static: bool,
) -> FxHashMap<String, (Option<NodeIndex>, Option<NodeIndex>)> {
    let mut accessor_map: FxHashMap<String, (Option<NodeIndex>, Option<NodeIndex>)> =
        FxHashMap::default();

    for &member_idx in &members.nodes {
        let Some(member_node) = arena.get(member_idx) else {
            continue;
        };

        if (member_node.kind == syntax_kind_ext::GET_ACCESSOR
            || member_node.kind == syntax_kind_ext::SET_ACCESSOR)
            && let Some(accessor_data) = arena.get_accessor(member_node)
        {
            // Check static modifier matches what we're collecting
            let is_static = has_effective_static_modifier(arena, &accessor_data.modifiers);
            if is_static != collect_static {
                continue;
            }
            // Skip abstract declarations, but keep invalid abstract accessors that
            // still have bodies; tsc emits those bodies in recovery mode.
            if arena.has_modifier(&accessor_data.modifiers, SyntaxKind::AbstractKeyword)
                && accessor_data.body.is_none()
            {
                continue;
            }
            // Skip private
            if is_private_identifier(arena, accessor_data.name) {
                continue;
            }

            let name = match get_identifier_text(arena, accessor_data.name) {
                Some(name) => name,
                // Non-literal computed property name (e.g., [1 << 6]) — use a unique
                // key per accessor so they are NOT merged into a single ODP call.
                // tsc emits separate Object.defineProperty for each.
                None => format!("__computed_{}", member_idx.0),
            };
            let entry = accessor_map.entry(name).or_insert((None, None));

            if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
                entry.0.get_or_insert(member_idx);
            } else {
                entry.1.get_or_insert(member_idx);
            }
        }
    }

    accessor_map
}

pub(super) fn collect_auto_accessor_fields(
    arena: &NodeArena,
    class_idx: NodeIndex,
    class_name: &str,
) -> Vec<AutoAccessorFieldInfo> {
    let mut accessors = Vec::new();

    let Some(class_node) = arena.get(class_idx) else {
        return accessors;
    };
    let Some(class_data) = arena.get_class(class_node) else {
        return accessors;
    };

    let has_static_auto_accessor = class_data.members.nodes.iter().any(|&member_idx| {
        let Some(member_node) = arena.get(member_idx) else {
            return false;
        };
        if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
            return false;
        }
        let Some(prop_data) = arena.get_property_decl(member_node) else {
            return false;
        };
        arena.has_modifier(&prop_data.modifiers, SyntaxKind::AccessorKeyword)
            && arena.is_static(&prop_data.modifiers)
            && !arena.has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword)
            && !arena.has_modifier(&prop_data.modifiers, SyntaxKind::DeclareKeyword)
            && !is_private_identifier(arena, prop_data.name)
    });
    let mut generated_name_index = if has_static_auto_accessor { 1 } else { 0 };

    for &member_idx in &class_data.members.nodes {
        let Some(member_node) = arena.get(member_idx) else {
            continue;
        };
        if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
            continue;
        }
        let Some(prop_data) = arena.get_property_decl(member_node) else {
            continue;
        };
        if arena.has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword) {
            continue;
        }
        if arena.has_modifier(&prop_data.modifiers, SyntaxKind::DeclareKeyword) {
            continue;
        }
        if is_private_identifier(arena, prop_data.name) {
            continue;
        }
        let has_accessor = arena.has_modifier(&prop_data.modifiers, SyntaxKind::AccessorKeyword);
        if !has_accessor {
            continue;
        }
        let Some(name_node) = arena.get(prop_data.name) else {
            continue;
        };
        let name = if name_node.kind == SyntaxKind::Identifier as u16 {
            let Some(name) = arena
                .get_identifier(name_node)
                .map(|id| id.escaped_text.clone())
            else {
                continue;
            };
            name
        } else {
            let name = generated_auto_accessor_name(generated_name_index);
            generated_name_index += 1;
            name
        };

        accessors.push(AutoAccessorFieldInfo {
            member_idx,
            weakmap_name: format!("_{class_name}_{name}_accessor_storage"),
            initializer: prop_data
                .initializer
                .is_some()
                .then_some(prop_data.initializer),
            is_static: arena.is_static(&prop_data.modifiers),
        });
    }

    accessors
}

pub(super) fn generated_auto_accessor_name(index: u32) -> String {
    if index < 26 {
        format!("_{}", (b'a' + index as u8) as char)
    } else {
        format!("_{}", index - 26)
    }
}

pub(super) fn has_parameter_property_modifier(
    arena: &NodeArena,
    modifiers: &Option<NodeList>,
) -> bool {
    arena.has_modifier(modifiers, SyntaxKind::PublicKeyword)
        || arena.has_modifier(modifiers, SyntaxKind::PrivateKeyword)
        || arena.has_modifier(modifiers, SyntaxKind::ProtectedKeyword)
        || arena.has_modifier(modifiers, SyntaxKind::ReadonlyKeyword)
        || arena.has_modifier(modifiers, SyntaxKind::OverrideKeyword)
}
