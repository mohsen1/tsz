//! Private Fields ES5 Transform
//!
//! Transforms ES2022 private class fields (#field) to ES5-compatible `WeakMap` pattern.
//!
//! ## Transform Pattern
//! ```typescript
//! class C {
//!     #value = 42;
//!     getValue() { return this.#value; }
//!     setValue(v) { this.#value = v; }
//! }
//! ```
//! Becomes:
//! ```javascript
//! var _C_value;
//! class C {
//!     constructor() {
//!         _C_value.set(this, 42);
//!     }
//!     getValue() { return __classPrivateFieldGet(this, _C_value, "f"); }
//!     setValue(v) { __classPrivateFieldSet(this, _C_value, v, "f"); }
//! }
//! _C_value = new WeakMap();
//! ```
//!
//! # Architecture Note
//!
//! The `is_private_identifier` function has been moved to `syntax::transform_utils`
//! as a shared utility to avoid circular dependencies. This module re-exports it
//! for backward compatibility.

use rustc_hash::{FxHashMap, FxHashSet};
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

// Re-export from shared utilities to avoid duplication
pub use tsz_parser::syntax::transform_utils::is_private_identifier;

/// Information about a private field in a class
#[derive(Debug, Clone)]
pub struct PrivateFieldInfo {
    /// The private field name without # (e.g., "value" for "#value")
    pub name: String,
    /// The `WeakMap` variable name (e.g., "_`C_value`" for class C, field #value)
    pub weakmap_name: String,
    /// Whether this field has an initializer
    pub has_initializer: bool,
    /// The node index of the initializer expression (if any)
    pub initializer: NodeIndex,
    /// Whether this is a static private field
    pub is_static: bool,
}

/// Information about a private accessor (get/set) in a class
#[derive(Debug, Clone)]
pub struct PrivateAccessorInfo {
    /// The private accessor name without # (e.g., "value" for "#value")
    pub name: String,
    /// The `WeakMap` variable name for the getter (e.g., "_`C_value_get`")
    pub get_var_name: Option<String>,
    /// The `WeakMap` variable name for the setter (e.g., "_`C_value_set`")
    pub set_var_name: Option<String>,
    /// The node index of the getter body (if any)
    pub getter_body: Option<NodeIndex>,
    /// The node index of the setter body (if any)
    pub setter_body: Option<NodeIndex>,
    /// The node index of the setter parameter (if any)
    pub setter_param: Option<NodeIndex>,
    /// Whether this is a static private accessor
    pub is_static: bool,
}

/// State for tracking private fields during class transformation
#[derive(Debug, Default)]
pub struct PrivateFieldState {
    /// Counter for generating unique temp var names when needed
    pub temp_counter: u32,
    /// The current class name being processed
    pub current_class_name: Option<String>,
    /// Private fields collected for the current class
    pub private_fields: Vec<PrivateFieldInfo>,
}

impl PrivateFieldState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start processing a new class
    pub fn enter_class(&mut self, class_name: &str) {
        self.current_class_name = Some(class_name.to_string());
        self.private_fields.clear();
    }

    /// Finish processing the current class
    pub fn exit_class(&mut self) {
        self.current_class_name = None;
        self.private_fields.clear();
    }

    /// Register a private field found in the class
    pub fn register_private_field(
        &mut self,
        name: &str,
        has_initializer: bool,
        initializer: NodeIndex,
        is_static: bool,
    ) {
        let class_name = self.current_class_name.as_deref().unwrap_or("_");
        // Strip the leading # from the name
        let field_name = name.strip_prefix('#').unwrap_or(name);
        let weakmap_name = format!("_{class_name}_{field_name}");

        self.private_fields.push(PrivateFieldInfo {
            name: field_name.to_string(),
            weakmap_name,
            has_initializer,
            initializer,
            is_static,
        });
    }

    /// Get the `WeakMap` name for a private field
    pub fn get_weakmap_name(&self, field_name: &str) -> Option<String> {
        let name = field_name.strip_prefix('#').unwrap_or(field_name);
        self.private_fields
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.weakmap_name.clone())
    }

    /// Check if there are any private fields
    pub const fn has_private_fields(&self) -> bool {
        !self.private_fields.is_empty()
    }

    /// Get all `WeakMap` variable names (for var declaration)
    pub fn get_weakmap_names(&self) -> Vec<&str> {
        self.private_fields
            .iter()
            .map(|f| f.weakmap_name.as_str())
            .collect()
    }

    /// Get all non-static private fields (for constructor initialization)
    pub fn get_instance_fields(&self) -> impl Iterator<Item = &PrivateFieldInfo> {
        self.private_fields.iter().filter(|f| !f.is_static)
    }

    /// Get all static private fields
    pub fn get_static_fields(&self) -> impl Iterator<Item = &PrivateFieldInfo> {
        self.private_fields.iter().filter(|f| f.is_static)
    }

    /// Reset state for a new file
    pub fn reset(&mut self) {
        self.temp_counter = 0;
        self.current_class_name = None;
        self.private_fields.clear();
    }
}

/// Get the private field name from a private identifier node
pub fn get_private_field_name(arena: &NodeArena, name_idx: NodeIndex) -> Option<String> {
    let node = arena.get(name_idx)?;
    if node.kind != SyntaxKind::PrivateIdentifier as u16 {
        return None;
    }
    let ident = arena.get_identifier(node)?;
    Some(ident.escaped_text.clone())
}

/// Collect top-level value bindings visible to generated private-name helpers.
pub fn collect_enclosing_source_binding_names(
    arena: &NodeArena,
    node_idx: NodeIndex,
) -> FxHashSet<String> {
    let mut root_idx = node_idx;
    while let Some(parent_idx) = arena.parent_of(root_idx) {
        if parent_idx.is_none() {
            break;
        }
        root_idx = parent_idx;
    }

    let mut names = FxHashSet::default();
    let source_file = arena
        .get(root_idx)
        .and_then(|root_node| arena.get_source_file(root_node))
        .or_else(|| find_containing_source_file(arena, node_idx));
    let Some(source_file) = source_file else {
        return names;
    };

    for &stmt_idx in &source_file.statements.nodes {
        collect_statement_binding_names(arena, stmt_idx, &mut names);
    }
    names
}

fn find_containing_source_file<'a>(
    arena: &'a NodeArena,
    node_idx: NodeIndex,
) -> Option<&'a tsz_parser::parser::node::SourceFileData> {
    let node = arena.get(node_idx)?;
    for candidate in &arena.nodes {
        if candidate.kind != syntax_kind_ext::SOURCE_FILE {
            continue;
        }
        if candidate.pos <= node.pos
            && candidate.end >= node.end
            && let Some(source_file) = arena.get_source_file(candidate)
        {
            return Some(source_file);
        }
    }
    None
}

/// Allocate a generated private helper name without colliding with existing names.
pub fn make_unique_private_name(base: &str, used_names: &mut FxHashSet<String>) -> String {
    if used_names.insert(base.to_string()) {
        return base.to_string();
    }

    let mut suffix = 1usize;
    loop {
        let candidate = format!("{base}_{suffix}");
        if used_names.insert(candidate.clone()) {
            return candidate;
        }
        suffix += 1;
    }
}

fn collect_statement_binding_names(
    arena: &NodeArena,
    stmt_idx: NodeIndex,
    names: &mut FxHashSet<String>,
) {
    let Some(stmt_node) = arena.get(stmt_idx) else {
        return;
    };

    match stmt_node.kind {
        k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
            if let Some(variable) = arena.get_variable(stmt_node) {
                for &decl_idx in &variable.declarations.nodes {
                    collect_variable_declaration_binding_names(arena, decl_idx, names);
                }
                if variable.declarations.nodes.is_empty() {
                    collect_variable_binding_names_from_subtree(arena, stmt_idx, names);
                }
            } else {
                collect_variable_binding_names_from_subtree(arena, stmt_idx, names);
            }
        }
        k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
            if let Some(function) = arena.get_function(stmt_node) {
                collect_identifier_binding_name(arena, function.name, names);
            }
        }
        k if k == syntax_kind_ext::CLASS_DECLARATION => {
            if let Some(class) = arena.get_class(stmt_node) {
                collect_identifier_binding_name(arena, class.name, names);
            }
        }
        k if k == syntax_kind_ext::ENUM_DECLARATION => {
            if let Some(enm) = arena.get_enum(stmt_node) {
                collect_identifier_binding_name(arena, enm.name, names);
            }
        }
        k if k == syntax_kind_ext::MODULE_DECLARATION => {
            if let Some(module) = arena.get_module(stmt_node) {
                collect_identifier_binding_name(arena, module.name, names);
            }
        }
        k if k == syntax_kind_ext::IMPORT_DECLARATION => {
            collect_import_binding_names(arena, stmt_node, names);
        }
        _ => {}
    }
}

fn collect_variable_binding_names_from_subtree(
    arena: &NodeArena,
    node_idx: NodeIndex,
    names: &mut FxHashSet<String>,
) {
    let Some(node) = arena.get(node_idx) else {
        return;
    };
    if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
        collect_variable_declaration_binding_names(arena, node_idx, names);
        return;
    }

    for child_idx in arena.get_children(node_idx) {
        collect_variable_binding_names_from_subtree(arena, child_idx, names);
    }
}

fn collect_import_binding_names(
    arena: &NodeArena,
    stmt_node: &tsz_parser::parser::node::Node,
    names: &mut FxHashSet<String>,
) {
    let Some(import_decl) = arena.get_import_decl(stmt_node) else {
        return;
    };
    let Some(import_clause_node) = arena.get(import_decl.import_clause) else {
        return;
    };
    let Some(import_clause) = arena.get_import_clause(import_clause_node) else {
        return;
    };

    collect_identifier_binding_name(arena, import_clause.name, names);

    let Some(named_bindings_node) = arena.get(import_clause.named_bindings) else {
        return;
    };
    let Some(named_bindings) = arena.get_named_imports(named_bindings_node) else {
        return;
    };

    collect_identifier_binding_name(arena, named_bindings.name, names);
    for &spec_idx in &named_bindings.elements.nodes {
        let Some(spec_node) = arena.get(spec_idx) else {
            continue;
        };
        if let Some(spec) = arena.get_specifier(spec_node) {
            collect_identifier_binding_name(arena, spec.name, names);
        }
    }
}

fn collect_variable_declaration_binding_names(
    arena: &NodeArena,
    decl_idx: NodeIndex,
    names: &mut FxHashSet<String>,
) {
    let Some(decl_node) = arena.get(decl_idx) else {
        return;
    };
    if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
        if let Some(decl_list) = arena.get_variable(decl_node) {
            for &inner_decl_idx in &decl_list.declarations.nodes {
                collect_variable_declaration_binding_names(arena, inner_decl_idx, names);
            }
        }
        return;
    }
    if let Some(decl) = arena.get_variable_declaration(decl_node) {
        collect_binding_pattern_names(arena, decl.name, names);
    }
}

fn collect_binding_pattern_names(
    arena: &NodeArena,
    name_idx: NodeIndex,
    names: &mut FxHashSet<String>,
) {
    let Some(node) = arena.get(name_idx) else {
        return;
    };

    if node.kind == SyntaxKind::Identifier as u16 {
        collect_identifier_binding_name(arena, name_idx, names);
        return;
    }

    if (node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
        || node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
        && let Some(pattern) = arena.get_binding_pattern(node)
    {
        for &element_idx in &pattern.elements.nodes {
            let Some(element_node) = arena.get(element_idx) else {
                continue;
            };
            if let Some(element) = arena.get_binding_element(element_node) {
                collect_binding_pattern_names(arena, element.name, names);
            }
        }
    }
}

fn collect_identifier_binding_name(
    arena: &NodeArena,
    name_idx: NodeIndex,
    names: &mut FxHashSet<String>,
) {
    let Some(node) = arena.get(name_idx) else {
        return;
    };
    if node.kind == SyntaxKind::Identifier as u16
        && let Some(identifier) = arena.get_identifier(node)
    {
        names.insert(identifier.escaped_text.clone());
    }
}

/// Collect private fields from a class
pub fn collect_private_fields(
    arena: &NodeArena,
    class_idx: NodeIndex,
    class_name: &str,
) -> Vec<PrivateFieldInfo> {
    let mut used_names = collect_enclosing_source_binding_names(arena, class_idx);
    collect_private_fields_with_reserved(arena, class_idx, class_name, &mut used_names)
}

/// Collect private fields from a class, avoiding the provided reserved names.
pub fn collect_private_fields_with_reserved(
    arena: &NodeArena,
    class_idx: NodeIndex,
    class_name: &str,
    used_names: &mut FxHashSet<String>,
) -> Vec<PrivateFieldInfo> {
    let mut fields = Vec::new();

    let Some(class_node) = arena.get(class_idx) else {
        return fields;
    };
    let Some(class_data) = arena.get_class(class_node) else {
        return fields;
    };

    for &member_idx in &class_data.members.nodes {
        let Some(member_node) = arena.get(member_idx) else {
            continue;
        };

        if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
            let Some(prop_data) = arena.get_property_decl(member_node) else {
                continue;
            };

            // Check if this is a private field
            if is_private_identifier(arena, prop_data.name) {
                let field_name = get_private_field_name(arena, prop_data.name).unwrap_or_default();
                let clean_name = field_name.strip_prefix('#').unwrap_or(&field_name);
                let weakmap_name =
                    make_unique_private_name(&format!("_{class_name}_{clean_name}"), used_names);
                let is_static = arena.has_modifier(&prop_data.modifiers, SyntaxKind::StaticKeyword);

                fields.push(PrivateFieldInfo {
                    name: clean_name.to_string(),
                    weakmap_name,
                    has_initializer: prop_data.initializer.is_some(),
                    initializer: prop_data.initializer,
                    is_static,
                });
            }
        }
    }

    fields
}

/// Collect private accessors from a class
pub fn collect_private_accessors(
    arena: &NodeArena,
    class_idx: NodeIndex,
    class_name: &str,
) -> Vec<PrivateAccessorInfo> {
    let mut used_names = collect_enclosing_source_binding_names(arena, class_idx);
    collect_private_accessors_with_reserved(arena, class_idx, class_name, &mut used_names)
}

/// Collect private accessors from a class, avoiding the provided reserved names.
pub fn collect_private_accessors_with_reserved(
    arena: &NodeArena,
    class_idx: NodeIndex,
    class_name: &str,
    used_names: &mut FxHashSet<String>,
) -> Vec<PrivateAccessorInfo> {
    let mut accessors: FxHashMap<String, PrivateAccessorInfo> = FxHashMap::default();

    let Some(class_node) = arena.get(class_idx) else {
        return Vec::new();
    };
    let Some(class_data) = arena.get_class(class_node) else {
        return Vec::new();
    };

    for &member_idx in &class_data.members.nodes {
        let Some(member_node) = arena.get(member_idx) else {
            continue;
        };

        // Check for both GET_ACCESSOR and SET_ACCESSOR
        if member_node.is_accessor() {
            let Some(accessor_data) = arena.get_accessor(member_node) else {
                continue;
            };

            // Check if this is a private accessor (name is a private identifier)
            if !is_private_identifier(arena, accessor_data.name) {
                continue;
            }

            let field_name = get_private_field_name(arena, accessor_data.name).unwrap_or_default();
            let clean_name = field_name.strip_prefix('#').unwrap_or(&field_name);
            let is_static = arena.has_modifier(&accessor_data.modifiers, SyntaxKind::StaticKeyword);

            // Get or create the accessor info for this name
            let entry =
                accessors
                    .entry(clean_name.to_string())
                    .or_insert_with(|| PrivateAccessorInfo {
                        name: clean_name.to_string(),
                        get_var_name: Some(make_unique_private_name(
                            &format!("_{class_name}_{clean_name}_get"),
                            used_names,
                        )),
                        set_var_name: Some(make_unique_private_name(
                            &format!("_{class_name}_{clean_name}_set"),
                            used_names,
                        )),
                        getter_body: None,
                        setter_body: None,
                        setter_param: None,
                        is_static,
                    });

            // Update based on accessor type
            if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
                if accessor_data.body.is_some() {
                    entry.getter_body = Some(accessor_data.body);
                }
            } else if member_node.kind == syntax_kind_ext::SET_ACCESSOR {
                if accessor_data.body.is_some() {
                    entry.setter_body = Some(accessor_data.body);
                }
                // Get the first parameter from the setter
                let params = &accessor_data.parameters;
                if let Some(first_param) = params.nodes.first() {
                    entry.setter_param = Some(*first_param);
                }
            }
        }
    }

    // Convert to Vec, filtering out entries that have neither getter nor setter
    accessors
        .into_values()
        .filter(|a| a.getter_body.is_some() || a.setter_body.is_some())
        .collect()
}

/// Information about a private method in a class
#[derive(Debug, Clone)]
pub struct PrivateMethodInfo {
    /// The private method name without # (e.g., "method" for "#method")
    pub name: String,
    /// The function variable name (e.g., "_`C_method`")
    pub fn_var_name: String,
    /// The node index of the method body
    pub body: Option<NodeIndex>,
    /// The node index of the parameter list
    pub parameters: Vec<NodeIndex>,
    /// Whether this is a static private method
    pub is_static: bool,
    /// Whether this is an async method
    pub is_async: bool,
    /// Whether this is a generator method
    pub is_generator: bool,
}

/// Collect private methods from a class
pub fn collect_private_methods(
    arena: &NodeArena,
    class_idx: NodeIndex,
    class_name: &str,
) -> Vec<PrivateMethodInfo> {
    let mut used_names = collect_enclosing_source_binding_names(arena, class_idx);
    collect_private_methods_with_reserved(arena, class_idx, class_name, &mut used_names)
}

/// Collect private methods from a class, avoiding the provided reserved names.
pub fn collect_private_methods_with_reserved(
    arena: &NodeArena,
    class_idx: NodeIndex,
    class_name: &str,
    used_names: &mut FxHashSet<String>,
) -> Vec<PrivateMethodInfo> {
    let mut methods = Vec::new();

    let Some(class_node) = arena.get(class_idx) else {
        return methods;
    };
    let Some(class_data) = arena.get_class(class_node) else {
        return methods;
    };

    for &member_idx in &class_data.members.nodes {
        let Some(member_node) = arena.get(member_idx) else {
            continue;
        };

        if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
            let Some(method_data) = arena.get_method_decl(member_node) else {
                continue;
            };

            // Check if this is a private method
            if !is_private_identifier(arena, method_data.name) {
                continue;
            }

            let field_name = get_private_field_name(arena, method_data.name).unwrap_or_default();
            let clean_name = field_name.strip_prefix('#').unwrap_or(&field_name);
            let fn_var_name =
                make_unique_private_name(&format!("_{class_name}_{clean_name}"), used_names);
            let is_static = arena.has_modifier(&method_data.modifiers, SyntaxKind::StaticKeyword);
            let is_async = arena.has_modifier(&method_data.modifiers, SyntaxKind::AsyncKeyword);

            methods.push(PrivateMethodInfo {
                name: clean_name.to_string(),
                fn_var_name,
                body: if method_data.body.is_some() {
                    Some(method_data.body)
                } else {
                    None
                },
                parameters: method_data.parameters.nodes.clone(),
                is_static,
                is_async,
                is_generator: method_data.asterisk_token,
            });
        }
    }

    methods
}

/// Generate the `WeakMap` variable declaration line
/// Returns: "var _`C_field1`, _`C_field2`;"
pub fn generate_weakmap_var_declaration(fields: &[PrivateFieldInfo]) -> String {
    if fields.is_empty() {
        return String::new();
    }
    let names: Vec<&str> = fields.iter().map(|f| f.weakmap_name.as_str()).collect();
    format!("var {};", names.join(", "))
}

/// Generate the `WeakMap` instantiation line (after class)
/// Returns: "_`C_field1` = new `WeakMap()`, _`C_field2` = new `WeakMap()`;"
pub fn generate_weakmap_instantiation(fields: &[PrivateFieldInfo]) -> String {
    if fields.is_empty() {
        return String::new();
    }
    let instantiations: Vec<String> = fields
        .iter()
        .filter(|f| !f.is_static)
        .map(|f| format!("{} = new WeakMap()", f.weakmap_name))
        .collect();
    if instantiations.is_empty() {
        return String::new();
    }
    format!("{};", instantiations.join(", "))
}

#[cfg(test)]
#[path = "../../tests/private_fields_es5.rs"]
mod tests;
