//! Private Fields ES5 Transform
//!
//! Transforms ES2022 private class fields (#field) to ES5-compatible WeakMap pattern.
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

use crate::parser::node::NodeArena;
use crate::parser::{NodeIndex, NodeList, syntax_kind_ext};
use crate::scanner::SyntaxKind;
use rustc_hash::FxHashMap;

// Re-export from shared utilities to avoid duplication
pub use crate::syntax::transform_utils::is_private_identifier;

/// Information about a private field in a class
#[derive(Debug, Clone)]
pub struct PrivateFieldInfo {
    /// The private field name without # (e.g., "value" for "#value")
    pub name: String,
    /// The WeakMap variable name (e.g., "_C_value" for class C, field #value)
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
    /// The WeakMap variable name for the getter (e.g., "_C_value_get")
    pub get_var_name: Option<String>,
    /// The WeakMap variable name for the setter (e.g., "_C_value_set")
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
        let weakmap_name = format!("_{}_{}", class_name, field_name);

        self.private_fields.push(PrivateFieldInfo {
            name: field_name.to_string(),
            weakmap_name,
            has_initializer,
            initializer,
            is_static,
        });
    }

    /// Get the WeakMap name for a private field
    pub fn get_weakmap_name(&self, field_name: &str) -> Option<String> {
        let name = field_name.strip_prefix('#').unwrap_or(field_name);
        self.private_fields
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.weakmap_name.clone())
    }

    /// Check if there are any private fields
    pub fn has_private_fields(&self) -> bool {
        !self.private_fields.is_empty()
    }

    /// Get all WeakMap variable names (for var declaration)
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

/// Collect private fields from a class
pub fn collect_private_fields(
    arena: &NodeArena,
    class_idx: NodeIndex,
    class_name: &str,
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
                let weakmap_name = format!("_{}_{}", class_name, clean_name);
                let is_static = has_static_modifier(arena, &prop_data.modifiers);

                fields.push(PrivateFieldInfo {
                    name: clean_name.to_string(),
                    weakmap_name,
                    has_initializer: !prop_data.initializer.is_none(),
                    initializer: prop_data.initializer,
                    is_static,
                });
            }
        }
    }

    fields
}

/// Check if modifiers contain the static keyword
fn has_static_modifier(arena: &NodeArena, modifiers: &Option<NodeList>) -> bool {
    let Some(mods) = modifiers else {
        return false;
    };
    for &mod_idx in &mods.nodes {
        let Some(mod_node) = arena.get(mod_idx) else {
            continue;
        };
        if mod_node.kind == SyntaxKind::StaticKeyword as u16 {
            return true;
        }
    }
    false
}

/// Collect private accessors from a class
pub fn collect_private_accessors(
    arena: &NodeArena,
    class_idx: NodeIndex,
    class_name: &str,
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
        if member_node.kind == syntax_kind_ext::GET_ACCESSOR
            || member_node.kind == syntax_kind_ext::SET_ACCESSOR
        {
            let Some(accessor_data) = arena.get_accessor(member_node) else {
                continue;
            };

            // Check if this is a private accessor (name is a private identifier)
            if !is_private_identifier(arena, accessor_data.name) {
                continue;
            }

            let field_name = get_private_field_name(arena, accessor_data.name).unwrap_or_default();
            let clean_name = field_name.strip_prefix('#').unwrap_or(&field_name);
            let is_static = has_static_modifier(arena, &accessor_data.modifiers);

            // Get or create the accessor info for this name
            let entry =
                accessors
                    .entry(clean_name.to_string())
                    .or_insert_with(|| PrivateAccessorInfo {
                        name: clean_name.to_string(),
                        get_var_name: Some(format!("_{}_{}_get", class_name, clean_name)),
                        set_var_name: Some(format!("_{}_{}_set", class_name, clean_name)),
                        getter_body: None,
                        setter_body: None,
                        setter_param: None,
                        is_static,
                    });

            // Update based on accessor type
            if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
                if !accessor_data.body.is_none() {
                    entry.getter_body = Some(accessor_data.body);
                }
            } else if member_node.kind == syntax_kind_ext::SET_ACCESSOR {
                if !accessor_data.body.is_none() {
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

/// Generate the WeakMap variable declaration line
/// Returns: "var _C_field1, _C_field2;"
pub fn generate_weakmap_var_declaration(fields: &[PrivateFieldInfo]) -> String {
    if fields.is_empty() {
        return String::new();
    }
    let names: Vec<&str> = fields.iter().map(|f| f.weakmap_name.as_str()).collect();
    format!("var {};", names.join(", "))
}

/// Generate the WeakMap instantiation line (after class)
/// Returns: "_C_field1 = new WeakMap(), _C_field2 = new WeakMap();"
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
mod tests {
    use super::*;

    #[test]
    fn test_private_field_state() {
        let mut state = PrivateFieldState::new();

        state.enter_class("MyClass");
        state.register_private_field("#value", true, NodeIndex::NONE, false);
        state.register_private_field("#count", false, NodeIndex::NONE, false);

        assert!(state.has_private_fields());
        assert_eq!(
            state.get_weakmap_name("#value"),
            Some("_MyClass_value".to_string())
        );
        assert_eq!(
            state.get_weakmap_name("value"),
            Some("_MyClass_value".to_string())
        );

        let names = state.get_weakmap_names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"_MyClass_value"));
        assert!(names.contains(&"_MyClass_count"));

        state.exit_class();
        assert!(!state.has_private_fields());
    }

    #[test]
    fn test_generate_weakmap_var_declaration() {
        let fields = vec![
            PrivateFieldInfo {
                name: "value".to_string(),
                weakmap_name: "_C_value".to_string(),
                has_initializer: true,
                initializer: NodeIndex::NONE,
                is_static: false,
            },
            PrivateFieldInfo {
                name: "count".to_string(),
                weakmap_name: "_C_count".to_string(),
                has_initializer: false,
                initializer: NodeIndex::NONE,
                is_static: false,
            },
        ];

        let decl = generate_weakmap_var_declaration(&fields);
        assert_eq!(decl, "var _C_value, _C_count;");
    }

    #[test]
    fn test_generate_weakmap_instantiation() {
        let fields = vec![
            PrivateFieldInfo {
                name: "value".to_string(),
                weakmap_name: "_C_value".to_string(),
                has_initializer: true,
                initializer: NodeIndex::NONE,
                is_static: false,
            },
            PrivateFieldInfo {
                name: "count".to_string(),
                weakmap_name: "_C_count".to_string(),
                has_initializer: false,
                initializer: NodeIndex::NONE,
                is_static: false,
            },
        ];

        let inst = generate_weakmap_instantiation(&fields);
        assert_eq!(inst, "_C_value = new WeakMap(), _C_count = new WeakMap();");
    }
}
