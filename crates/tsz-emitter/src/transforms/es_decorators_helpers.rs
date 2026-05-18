//! Shared helper types and functions for TC39 decorator emission.

use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

/// Strip TypeScript type annotations from function/setter parameters in source text.
/// Handles `(value: number)` → `(value)`.
pub(super) fn strip_param_types(text: &str) -> String {
    let brace_pos = text.find('{').unwrap_or(text.len());
    let param_region = &text[..brace_pos];
    let Some(paren_open) = param_region.rfind('(') else {
        return text.to_string();
    };
    let rest = &text[paren_open + 1..];
    let Some(paren_close_rel) = rest.find(')') else {
        return text.to_string();
    };
    let params_str = &rest[..paren_close_rel];
    if !params_str.contains(':') {
        return text.to_string();
    }
    let mut cleaned = Vec::new();
    for param in params_str.split(',') {
        let param = param.trim();
        if param.is_empty() {
            continue;
        }
        if let Some(colon) = param.find(':') {
            cleaned.push(param[..colon].trim().to_string());
        } else {
            cleaned.push(param.to_string());
        }
    }
    let paren_close = paren_open + 1 + paren_close_rel;
    format!(
        "{}({}){}",
        &text[..paren_open],
        cleaned.join(", "),
        &text[paren_close + 1..]
    )
}

pub(super) fn normalize_member_indentation(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= 1 {
        return text.to_string();
    }

    let min_indent = lines
        .iter()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start_matches([' ', '\t']).len())
        .min()
        .unwrap_or(0);

    if min_indent == 0 {
        return text.to_string();
    }

    let mut out = String::new();
    for (idx, line) in lines.iter().enumerate() {
        if idx > 0 && line.len() >= min_indent {
            out.push_str(&line[min_indent..]);
        } else {
            out.push_str(line);
        }
        if idx + 1 < lines.len() {
            out.push('\n');
        }
    }
    out
}

pub(super) fn push_indented_lines(out: &mut String, indent: &str, text: &str) {
    for line in text.lines() {
        out.push_str(indent);
        out.push_str(line);
        out.push('\n');
    }
}

pub(super) fn normalize_decorator_expr_text(text: &str) -> String {
    text.replace("=> {}", "=> { }").replace("=>{}", "=> { }")
}

pub(super) fn is_comment_line(line: &str) -> bool {
    line.starts_with("//")
        || line.starts_with("/*")
        || line.starts_with('*')
        || line.ends_with("*/")
}

/// Information about a decorated member
#[derive(Debug, Clone)]
pub(super) struct DecoratedMember {
    /// The member node index
    pub(super) member_idx: NodeIndex,
    /// The member kind for the decorator context
    pub(super) kind: MemberKind,
    /// Name of the member
    pub(super) name: MemberName,
    /// Whether the member is static
    pub(super) is_static: bool,
    /// Whether the member is private (#name)
    pub(super) is_private: bool,
    /// Decorator expression texts (e.g. ["dec(1)"])
    pub(super) decorator_exprs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum MemberKind {
    Method,
    Getter,
    Setter,
    Field,
    Accessor,
}

#[derive(Debug, Clone)]
pub(super) enum MemberName {
    /// Simple identifier: `method1`
    Identifier(String),
    /// String literal in computed position: `["method2"]`
    StringLiteral(String),
    /// Computed expression: `[expr]` - needs `__propKey`
    Computed(NodeIndex),
    /// Private identifier: `#method1`
    Private(String),
}

/// Information about a decorated field for constructor rewrite
pub(super) struct DecoratedFieldInfo {
    /// The field access expression for assignment (e.g., "field1", "\"field2\"", "_a")
    pub(super) access_expr: String,
    /// Whether the access uses bracket notation (computed or string literal)
    pub(super) is_bracket_access: bool,
    /// The original initializer text (e.g., "1", "2"), or empty for no initializer
    pub(super) initializer_text: String,
    /// Index into `decorated_members` for this field
    pub(super) member_var_index: usize,
}

pub(super) struct ParameterPropertyInfo {
    pub(super) name: String,
}

pub(super) struct DecoratedAutoAccessorInfo {
    pub(super) name: String,
    pub(super) initializer_text: String,
    pub(super) storage_base: String,
    pub(super) member_var_index: usize,
}

pub(super) struct ClassDecoratorStaticPrivateMethodInfo {
    pub(super) member_idx: NodeIndex,
    pub(super) member_name: String,
    pub(super) needs_wrapper: bool,
    pub(super) temp_var: String,
    pub(super) function_name: String,
    pub(super) params: String,
    pub(super) body: String,
}

pub(super) fn indent_str(level: usize) -> String {
    "    ".repeat(level)
}

pub(super) fn next_temp_var(counter: &mut u32) -> String {
    let name = format!("_{}", (b'a' + (*counter % 26) as u8) as char);
    *counter += 1;
    name
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

pub(super) struct MemberVarInfo {
    pub(super) decorators_var: String,
    pub(super) has_initializers: bool,
    pub(super) initializers_var: Option<String>,
    pub(super) extra_initializers_var: Option<String>,
    pub(super) has_descriptor: bool,
    pub(super) descriptor_var: Option<String>,
}

pub(super) struct ConstructorInfo {
    pub(super) params: String,
    pub(super) body_lines: Vec<String>,
}
