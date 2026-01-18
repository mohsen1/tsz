//! ES5 Class Transform
//!
//! Transforms ES6 classes to ES5 IIFE patterns:
//!
//! ```typescript
//! class Animal {
//!     constructor(name) { this.name = name; }
//!     speak() { console.log(this.name); }
//! }
//! ```
//!
//! Becomes:
//!
//! ```javascript
//! var Animal = /** @class */ (function () {
//!     function Animal(name) {
//!         this.name = name;
//!     }
//!     Animal.prototype.speak = function () {
//!         console.log(this.name);
//!     };
//!     return Animal;
//! }());
//! ```

use crate::parser::syntax_kind_ext;
use crate::parser::thin_node::{
    ClassData, FunctionData, TaggedTemplateData, TemplateExprData, ThinNode, ThinNodeArena,
};
use crate::parser::{NodeIndex, NodeList};
use crate::scanner::SyntaxKind;
use crate::source_map::Mapping;
use crate::source_writer::source_position_from_offset;
use crate::transforms::arrow_es5::contains_this_reference;
use crate::transforms::async_es5::AsyncES5Emitter;
use crate::transforms::emit_utils;
use crate::transforms::private_fields_es5::{
    PrivateAccessorInfo, PrivateFieldInfo, collect_private_accessors, collect_private_fields,
    is_private_identifier,
};
use memchr;

struct ParamTransform {
    name: String,
    pattern: Option<NodeIndex>,
    initializer: Option<NodeIndex>,
}

struct RestParamTransform {
    name: String,
    pattern: Option<NodeIndex>,
    index: usize,
}

struct ParamTransformPlan {
    params: Vec<ParamTransform>,
    rest: Option<RestParamTransform>,
}

impl ParamTransformPlan {
    fn is_empty(&self) -> bool {
        self.params.is_empty() && self.rest.is_none()
    }
}

struct TemplateParts {
    cooked: Vec<String>,
    raw: Vec<String>,
    expressions: Vec<NodeIndex>,
}

/// ES5 class emitter - emits ES5 IIFE pattern for classes
pub struct ClassES5Emitter<'a> {
    arena: &'a ThinNodeArena,
    output: String,
    indent_level: u32,
    source_text: Option<&'a str>,
    source_index: u32,
    mappings: Vec<Mapping>,
    line: u32,
    column: u32,
    /// Whether we're emitting inside a scope that uses _this capture
    use_this_capture: bool,
    /// Whether a `_this` capture is available in the current scope
    this_capture_available: bool,
    /// Whether to suppress arrow-function this capture (static fields).
    suppress_this_capture: bool,
    /// Counter for temporary variables (_a, _b, _c, etc.)
    temp_var_counter: u32,
    /// Private fields for the current class
    private_fields: Vec<PrivateFieldInfo>,
    /// Private accessors for the current class
    private_accessors: Vec<PrivateAccessorInfo>,
    /// Current class name (for private field WeakMap names)
    class_name: String,
}

impl<'a> ClassES5Emitter<'a> {
    pub fn new(arena: &'a ThinNodeArena) -> Self {
        ClassES5Emitter {
            arena,
            output: String::with_capacity(4096),
            indent_level: 0,
            source_text: None,
            source_index: 0,
            mappings: Vec::new(),
            line: 0,
            column: 0,
            use_this_capture: false,
            this_capture_available: false,
            suppress_this_capture: false,
            temp_var_counter: 0,
            private_fields: Vec::new(),
            private_accessors: Vec::new(),
            class_name: String::new(),
        }
    }

    /// Set the initial indentation level (to match the parent context)
    pub fn set_indent_level(&mut self, level: u32) {
        self.indent_level = level;
    }

    /// Set the source text (for single-line block detection)
    pub fn set_source_text(&mut self, source_text: &'a str) {
        self.source_text = Some(source_text);
    }

    pub fn set_source_map_context(&mut self, source_text: &'a str, source_index: u32) {
        self.source_text = Some(source_text);
        self.source_index = source_index;
    }

    pub fn take_mappings(&mut self) -> Vec<Mapping> {
        std::mem::take(&mut self.mappings)
    }

    fn reset_output(&mut self) {
        self.output.clear();
        self.mappings.clear();
        self.line = 0;
        self.column = 0;
    }

    fn record_mapping_for_node(&mut self, node: &ThinNode) {
        let Some(text) = self.source_text else {
            return;
        };

        let source_pos = source_position_from_offset(text, node.pos);
        self.mappings.push(Mapping {
            generated_line: self.line,
            generated_column: self.column,
            source_index: self.source_index,
            original_line: source_pos.line,
            original_column: source_pos.column,
            name_index: None,
        });
    }

    /// Emit trailing comments after a position in the source text
    fn emit_trailing_comments(&mut self, end_pos: u32) {
        use crate::thin_emitter::get_trailing_comment_ranges;

        let Some(text) = self.source_text else {
            return;
        };

        let comments = get_trailing_comment_ranges(text, end_pos as usize);
        for comment in comments {
            // Add space before trailing comment
            self.write(" ");
            // Emit the comment text
            let comment_text = &text[comment.pos as usize..comment.end as usize];
            self.write(comment_text);
        }
    }

    pub fn emit_class(&mut self, class_idx: NodeIndex) -> String {
        self.emit_class_internal(class_idx, None)
    }

    pub fn emit_class_with_name(&mut self, class_idx: NodeIndex, name: &str) -> String {
        self.emit_class_internal(class_idx, Some(name))
    }

    fn emit_class_internal(&mut self, class_idx: NodeIndex, override_name: Option<&str>) -> String {
        self.reset_output();

        let Some(class_node) = self.arena.get(class_idx) else {
            return String::new();
        };

        let Some(class_data) = self.arena.get_class(class_node) else {
            return String::new();
        };

        // Skip ambient/declare classes - they produce no output
        if self.has_declare_modifier(&class_data.modifiers) {
            return String::new();
        }

        // Get class name
        let class_name = if let Some(name) = override_name {
            name.to_string()
        } else {
            self.get_identifier_text(class_data.name)
        };
        let class_mapping_node = if override_name.is_none() {
            Some(class_node)
        } else {
            None
        };
        self.class_name = class_name.clone();

        // Collect private fields from the class
        self.private_fields = collect_private_fields(self.arena, class_idx, &class_name);

        // Collect private accessors from the class
        self.private_accessors = collect_private_accessors(self.arena, class_idx, &class_name);

        // Check for extends clause and get base class name
        let base_class_name = self.get_extends_class_name(&class_data.heritage_clauses);
        let has_extends = base_class_name.is_some();

        // Emit WeakMap variable declarations before the class (if we have private fields)
        // var _ClassName_field1, _ClassName_field2;
        let mut weakmap_names: Vec<String> = self
            .private_fields
            .iter()
            .map(|f| f.weakmap_name.clone())
            .collect();
        // Also add private accessor WeakMap variables
        for acc in &self.private_accessors {
            if let Some(get_var) = &acc.get_var_name {
                weakmap_names.push(get_var.clone());
            }
            if let Some(set_var) = &acc.set_var_name {
                weakmap_names.push(set_var.clone());
            }
        }
        if !weakmap_names.is_empty() {
            self.write("var ");
            self.write(&weakmap_names.join(", "));
            self.write(";");
            self.write_line();
        }

        // var ClassName = /** @class */ (function (_super) {
        if let Some(node) = class_mapping_node {
            self.record_mapping_for_node(node);
        }
        self.write("var ");
        self.write(&class_name);
        self.write(" = /** @class */ (function (");
        if has_extends {
            self.write("_super");
        }
        self.write(") {");
        self.write_line();
        self.increase_indent();

        // __extends(ClassName, _super);
        if has_extends {
            self.write_indent();
            self.write("__extends(");
            self.write(&class_name);
            self.write(", _super);");
            self.write_line();
        }

        // Constructor function
        self.emit_constructor(&class_name, class_data, has_extends);

        // Prototype methods
        self.emit_methods(&class_name, class_data);

        // Static members
        self.emit_static_members(&class_name, class_data);

        // return ClassName;
        self.write_indent();
        self.write("return ");
        self.write(&class_name);
        self.write(";");
        self.write_line();

        self.decrease_indent();
        self.write_indent();
        self.write("}(");

        // Pass base class if extends
        if let Some(ref base_name) = base_class_name {
            self.write(base_name);
        }

        self.write("));");

        // Emit WeakMap instantiations after the class (for instance private fields)
        // _ClassName_field1 = new WeakMap(), _ClassName_field2 = new WeakMap();
        let mut instantiations: Vec<String> = self
            .private_fields
            .iter()
            .filter(|f| !f.is_static)
            .map(|f| format!("{} = new WeakMap()", f.weakmap_name))
            .collect();
        // Also add private accessor WeakMap instantiations
        for acc in &self.private_accessors {
            if !acc.is_static {
                if let Some(get_var) = &acc.get_var_name {
                    instantiations.push(format!("{} = new WeakMap()", get_var));
                }
                if let Some(set_var) = &acc.set_var_name {
                    instantiations.push(format!("{} = new WeakMap()", set_var));
                }
            }
        }
        if !instantiations.is_empty() {
            self.write_line();
            self.write(&instantiations.join(", "));
            self.write(";");
        }

        std::mem::take(&mut self.output)
    }

    fn emit_constructor(&mut self, class_name: &str, class_data: &ClassData, has_extends: bool) {
        let prev_capture_available = self.this_capture_available;
        self.this_capture_available = false;

        // Collect instance property initializers (non-private only)
        let instance_props: Vec<NodeIndex> = class_data
            .members
            .nodes
            .iter()
            .filter_map(|&member_idx| {
                let member_node = self.arena.get(member_idx)?;
                if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                    return None;
                }
                let prop_data = self.arena.get_property_decl(member_node)?;
                // Skip static properties
                if self.is_static(&prop_data.modifiers) {
                    return None;
                }
                // Skip private fields (they use WeakMap pattern)
                if is_private_identifier(self.arena, prop_data.name) {
                    return None;
                }
                // Include if has initializer
                if !prop_data.initializer.is_none() {
                    Some(member_idx)
                } else {
                    None
                }
            })
            .collect();

        // Find constructor implementation (the one with a body)
        // Skip declaration-only constructors (overload signatures)
        let mut found_constructor = false;

        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == syntax_kind_ext::CONSTRUCTOR {
                let Some(ctor_data) = self.arena.get_constructor(member_node) else {
                    continue;
                };

                // Only emit the constructor implementation (with a body), not overload signatures
                if ctor_data.body.is_none() {
                    continue;
                }

                found_constructor = true;

                self.write_indent();
                self.write("function ");
                self.write(class_name);
                self.write("(");
                let param_transforms = self.emit_parameters(&ctor_data.parameters);
                self.write(") {");
                self.write_line();
                self.increase_indent();

                // For derived classes with explicit constructor:
                // 1. Transform super(args) to var _this = _super.call(this, args) || this;
                // 2. Use _this instead of this for property assignments
                // 3. Add return _this; at the end
                if has_extends {
                    self.emit_derived_constructor_body(
                        ctor_data.body,
                        &ctor_data.parameters,
                        &instance_props,
                        &param_transforms,
                    );
                } else {
                    // Non-derived class: check if we need _this capture for arrow functions
                    // Check both field initializers AND constructor body for arrows with `this`
                    let needs_capture = self.needs_this_capture(&instance_props)
                        || self.body_contains_arrow_with_this(ctor_data.body);
                    if needs_capture {
                        self.write_indent();
                        self.write("var _this = this;");
                        self.write_line();
                        self.this_capture_available = true;
                        self.use_this_capture = true;
                    }

                    self.emit_param_destructuring_prologue(&param_transforms);

                    // Emit private field initializations FIRST
                    self.emit_private_field_initializations(false);
                    // Emit private accessor initializations
                    self.emit_private_accessor_initializations(false);

                    // Then emit instance props and parameter props
                    self.emit_instance_property_initializers(&instance_props);
                    self.emit_parameter_properties(&ctor_data.parameters);
                    self.emit_block_contents(ctor_data.body);

                    // Reset use_this_capture after constructor body
                    if needs_capture {
                        self.use_this_capture = false;
                    }
                }

                self.decrease_indent();
                self.write_indent();
                self.write("}");

                // Emit trailing comments from the original constructor body
                if let Some(body_node) = self.arena.get(ctor_data.body) {
                    // In TypeScript, node.end may include trailing trivia.
                    // We need to find the actual closing brace position.
                    // The block starts with '{' and ends with '}'.
                    // For an empty block {}, find the closing brace after the opening.
                    if let Some(text) = self.source_text {
                        let pos = body_node.pos as usize;
                        let end = std::cmp::min(body_node.end as usize, text.len());
                        // Search only within the block's span
                        if let Some(slice) = text.get(pos..end) {
                            // Skip the opening brace and find the matching closing brace
                            // For simple blocks, find the first }
                            if let Some(close_idx) = slice.find('}') {
                                let actual_end = pos + close_idx + 1;
                                self.emit_trailing_comments(actual_end as u32);
                            }
                        }
                    }
                }

                self.write_line();
                break;
            }
        }

        // Default constructor if none found
        if !found_constructor {
            self.write_indent();
            self.write("function ");
            self.write(class_name);
            self.write("(");

            // For derived classes without explicit constructor, accept variable args
            if has_extends {
                // No explicit params needed since we'll use arguments
            }

            self.write(") {");
            self.write_line();
            self.increase_indent();

            let has_private_fields = self.private_fields.iter().any(|f| !f.is_static);

            // For derived classes with no instance properties AND no private fields
            if has_extends && instance_props.is_empty() && !has_private_fields {
                self.write_indent();
                self.write("return _super !== null && _super.apply(this, arguments) || this;");
                self.write_line();
            } else if has_extends {
                // For derived classes with instance props or private fields, use _this variable
                self.write_indent();
                self.write("var _this = _super !== null && _super.apply(this, arguments) || this;");
                self.write_line();
                self.this_capture_available = true;

                // Emit private field initializations first
                self.emit_private_field_initializations(true);
                // Emit private accessor initializations
                self.emit_private_accessor_initializations(true);

                // Emit instance property initializers
                for &prop_idx in &instance_props {
                    let Some(prop_node) = self.arena.get(prop_idx) else {
                        continue;
                    };
                    let Some(prop_data) = self.arena.get_property_decl(prop_node) else {
                        continue;
                    };
                    self.write_indent();
                    self.emit_property_receiver_and_name("_this", prop_data.name);
                    self.write(" = ");
                    let needs_capture = contains_this_reference(self.arena, prop_data.initializer);
                    let prev = self.use_this_capture;
                    if needs_capture {
                        self.use_this_capture = true;
                    }
                    self.emit_expression(prop_data.initializer);
                    self.use_this_capture = prev;
                    self.write(";");
                    self.write_line();
                }

                // Return _this
                self.write_indent();
                self.write("return _this;");
                self.write_line();
            } else {
                // Non-derived class - emit private fields then instance property initializers
                let needs_capture = self.needs_this_capture(&instance_props);
                if needs_capture {
                    self.write_indent();
                    self.write("var _this = this;");
                    self.write_line();
                    self.this_capture_available = true;
                }
                self.emit_private_field_initializations(false);
                // Emit private accessor initializations
                self.emit_private_accessor_initializations(false);

                for &prop_idx in &instance_props {
                    let Some(prop_node) = self.arena.get(prop_idx) else {
                        continue;
                    };
                    let Some(prop_data) = self.arena.get_property_decl(prop_node) else {
                        continue;
                    };
                    self.write_indent();
                    self.emit_property_receiver_and_name("this", prop_data.name);
                    self.write(" = ");
                    self.emit_expression(prop_data.initializer);
                    self.write(";");
                    self.write_line();
                }
            }

            self.decrease_indent();
            self.write_indent();
            self.write("}");
            self.write_line();
        }

        self.this_capture_available = prev_capture_available;
    }

    /// Check if any property initializers contain arrow functions that reference `this`
    fn needs_this_capture(&self, props: &[NodeIndex]) -> bool {
        for &prop_idx in props {
            let Some(prop_node) = self.arena.get(prop_idx) else {
                continue;
            };
            let Some(prop_data) = self.arena.get_property_decl(prop_node) else {
                continue;
            };

            if !prop_data.initializer.is_none() {
                // Check if initializer is an arrow function with `this` in body
                let init_node = self.arena.get(prop_data.initializer);
                if let Some(node) = init_node {
                    if node.kind == syntax_kind_ext::ARROW_FUNCTION {
                        if contains_this_reference(self.arena, prop_data.initializer) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Check if a block body contains arrow functions that reference `this`
    fn body_contains_arrow_with_this(&self, body_idx: NodeIndex) -> bool {
        self.node_contains_arrow_with_this(body_idx)
    }

    /// Recursively check if a node contains an arrow function that references `this`
    fn node_contains_arrow_with_this(&self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };

        // If this is an arrow function, check if it references `this`
        if node.kind == syntax_kind_ext::ARROW_FUNCTION {
            return contains_this_reference(self.arena, node_idx);
        }

        // Don't recurse into regular functions (they have their own `this`)
        if node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || node.kind == syntax_kind_ext::FUNCTION_DECLARATION
        {
            return false;
        }

        // Check children based on node type
        match node.kind {
            k if k == syntax_kind_ext::BLOCK
                || k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION =>
            {
                if let Some(block) = self.arena.get_block(node) {
                    for &stmt_idx in &block.statements.nodes {
                        if self.node_contains_arrow_with_this(stmt_idx) {
                            return true;
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.arena.get_expression_statement(node) {
                    if self.node_contains_arrow_with_this(expr_stmt.expression) {
                        return true;
                    }
                }
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_data) = self.arena.get_variable(node) {
                    for &decl_idx in &var_data.declarations.nodes {
                        if let Some(decl_node) = self.arena.get(decl_idx) {
                            if let Some(decl) = self.arena.get_variable_declaration(decl_node) {
                                if self.node_contains_arrow_with_this(decl.initializer) {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(node) {
                    if self.node_contains_arrow_with_this(bin.left) {
                        return true;
                    }
                    if self.node_contains_arrow_with_this(bin.right) {
                        return true;
                    }
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION || k == syntax_kind_ext::NEW_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    if self.node_contains_arrow_with_this(call.expression) {
                        return true;
                    }
                    if let Some(ref args) = call.arguments {
                        for &arg_idx in &args.nodes {
                            if self.node_contains_arrow_with_this(arg_idx) {
                                return true;
                            }
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(access) = self.arena.get_access_expr(node) {
                    if self.node_contains_arrow_with_this(access.expression) {
                        return true;
                    }
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.arena.get_parenthesized(node) {
                    if self.node_contains_arrow_with_this(paren.expression) {
                        return true;
                    }
                }
            }
            _ => {}
        }
        false
    }

    /// Emit instance property initializers as this.prop = value; or this[key] = value;
    fn emit_instance_property_initializers(&mut self, props: &[NodeIndex]) {
        let receiver = if self.use_this_capture {
            "_this"
        } else {
            "this"
        };
        for &prop_idx in props {
            let Some(prop_node) = self.arena.get(prop_idx) else {
                continue;
            };
            let Some(prop_data) = self.arena.get_property_decl(prop_node) else {
                continue;
            };

            self.write_indent();
            self.record_mapping_for_node(prop_node);
            self.emit_property_receiver_and_name(receiver, prop_data.name);
            self.write(" = ");
            self.emit_expression(prop_data.initializer);
            self.write(";");
            self.write_line();
        }
    }

    /// Emit a property access with the given receiver: receiver.prop or receiver[key]
    fn emit_property_receiver_and_name(&mut self, receiver: &str, name_idx: NodeIndex) {
        self.write(receiver);
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(computed) = self.arena.get_computed_property(name_node) {
                self.write("[");
                self.emit_expression(computed.expression);
                self.write("]");
            }
        } else if name_node.kind == SyntaxKind::Identifier as u16 {
            self.write(".");
            self.write_identifier_text(name_idx);
        } else if name_node.kind == SyntaxKind::StringLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(name_node) {
                self.write("[\"");
                self.write(&lit.text);
                self.write("\"]");
            }
        } else if name_node.kind == SyntaxKind::NumericLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(name_node) {
                self.write("[");
                self.write(&lit.text);
                self.write("]");
            }
        }
    }

    /// Emit a method name for prototype assignment: .name or [expr]
    fn emit_method_name(&mut self, name_idx: NodeIndex) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(computed) = self.arena.get_computed_property(name_node) {
                self.write("[");
                self.emit_expression(computed.expression);
                self.write("]");
            }
        } else if name_node.kind == SyntaxKind::Identifier as u16 {
            self.write(".");
            self.write_identifier_text(name_idx);
        } else if name_node.kind == SyntaxKind::StringLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(name_node) {
                self.write("[\"");
                self.write(&lit.text);
                self.write("\"]");
            }
        } else if name_node.kind == SyntaxKind::NumericLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(name_node) {
                self.write("[");
                self.write(&lit.text);
                self.write("]");
            }
        }
    }

    /// Emit private field initializations using WeakMap.set() pattern
    /// For each private field:
    /// 1. _ClassName_field.set(this, void 0); - allocate slot
    /// 2. __classPrivateFieldSet(this, _ClassName_field, initialValue, "f"); - set value (if has initializer)
    fn emit_private_field_initializations(&mut self, use_this: bool) {
        let receiver = if use_this { "_this" } else { "this" };

        for field in &self.private_fields.clone() {
            // Skip static fields - they're handled differently
            if field.is_static {
                continue;
            }

            // Emit: _ClassName_field.set(this, void 0);
            self.write_indent();
            self.write(&field.weakmap_name);
            self.write(".set(");
            self.write(receiver);
            self.write(", void 0);");
            self.write_line();

            // If has initializer, emit: __classPrivateFieldSet(this, _ClassName_field, value, "f");
            if field.has_initializer && !field.initializer.is_none() {
                self.write_indent();
                self.write("__classPrivateFieldSet(");
                self.write(receiver);
                self.write(", ");
                self.write(&field.weakmap_name);
                self.write(", ");
                let needs_capture =
                    use_this && contains_this_reference(self.arena, field.initializer);
                let prev = self.use_this_capture;
                if needs_capture {
                    self.use_this_capture = true;
                }
                self.emit_expression(field.initializer);
                self.use_this_capture = prev;
                self.write(", \"f\");");
                self.write_line();
            }
        }
    }

    /// Emit private accessor initializations using WeakMap.set() pattern
    /// For each private accessor:
    /// - _ClassName_accessor_get.set(this, function() { ...getter body... });
    /// - _ClassName_accessor_set.set(this, function(param) { ...setter body... });
    fn emit_private_accessor_initializations(&mut self, use_this: bool) {
        let receiver = if use_this { "_this" } else { "this" };

        for acc in &self.private_accessors.clone() {
            // Skip static accessors - they're handled differently
            if acc.is_static {
                continue;
            }

            // Emit getter: _ClassName_accessor_get.set(this, function() { ... });
            if let Some(get_var) = &acc.get_var_name {
                if let Some(getter_body) = acc.getter_body {
                    self.write_indent();
                    self.write(get_var);
                    self.write(".set(");
                    self.write(receiver);
                    self.write(", function() {");
                    self.write_line();
                    self.increase_indent();
                    self.emit_block_contents(getter_body);
                    self.decrease_indent();
                    self.write_indent();
                    self.write("});");
                    self.write_line();
                }
            }

            // Emit setter: _ClassName_accessor_set.set(this, function(param) { ... });
            if let Some(set_var) = &acc.set_var_name {
                if let Some(setter_body) = acc.setter_body {
                    self.write_indent();
                    self.write(set_var);
                    self.write(".set(");
                    self.write(receiver);
                    self.write(", function(");
                    // Emit parameter name
                    if let Some(param) = acc.setter_param {
                        self.write_identifier_text(param);
                    } else {
                        self.write("value");
                    }
                    self.write(") {");
                    self.write_line();
                    self.increase_indent();
                    self.emit_block_contents(setter_body);
                    self.decrease_indent();
                    self.write_indent();
                    self.write("});");
                    self.write_line();
                }
            }
        }
    }

    /// Emit __classPrivateFieldGet(receiver, _ClassName_field, "f")
    /// Called when encountering `this.#field` in method bodies
    fn emit_private_field_get(&mut self, receiver_idx: NodeIndex, field_name_idx: NodeIndex) {
        let field_name = self.get_private_field_name(field_name_idx);
        let weakmap_name = self.get_weakmap_name_for_field(&field_name);

        self.write("__classPrivateFieldGet(");
        self.emit_expression(receiver_idx);
        self.write(", ");
        self.write(&weakmap_name);
        self.write(", \"f\")");
    }

    /// Emit __classPrivateFieldSet(receiver, _ClassName_field, value, "f")
    /// Called when encountering `this.#field = value` in method bodies
    fn emit_private_field_set(
        &mut self,
        receiver_idx: NodeIndex,
        field_name_idx: NodeIndex,
        value_idx: NodeIndex,
    ) {
        let field_name = self.get_private_field_name(field_name_idx);
        let weakmap_name = self.get_weakmap_name_for_field(&field_name);

        self.write("__classPrivateFieldSet(");
        self.emit_expression(receiver_idx);
        self.write(", ");
        self.write(&weakmap_name);
        self.write(", ");
        self.emit_expression(value_idx);
        self.write(", \"f\")");
    }

    /// Get the private field name from a PrivateIdentifier node (without #)
    fn get_private_field_name(&self, name_idx: NodeIndex) -> String {
        let Some(node) = self.arena.get(name_idx) else {
            return String::new();
        };
        let Some(ident) = self.arena.get_identifier(node) else {
            return String::new();
        };
        // Remove the # prefix
        ident
            .escaped_text
            .strip_prefix('#')
            .unwrap_or(&ident.escaped_text)
            .to_string()
    }

    /// Get the WeakMap name for a private field
    fn get_weakmap_name_for_field(&self, field_name: &str) -> String {
        // Look up in our collected private fields
        for field in &self.private_fields {
            if field.name == field_name {
                return field.weakmap_name.clone();
            }
        }
        // Fallback: construct the name
        format!("_{}_{}", self.class_name, field_name)
    }

    /// Check if an expression is a private field assignment (this.#field = ...)
    fn is_private_field_assignment(&self, left_idx: NodeIndex) -> bool {
        let Some(left_node) = self.arena.get(left_idx) else {
            return false;
        };
        if left_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.arena.get_access_expr(left_node) else {
            return false;
        };
        is_private_identifier(self.arena, access.name_or_argument)
    }

    /// Emit parameter properties as this.param = param;
    /// For constructor parameters with public, private, protected, or readonly modifiers
    fn emit_parameter_properties(&mut self, params: &NodeList) {
        for &param_idx in &params.nodes {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };

            // Check for modifiers that trigger property creation
            if self.has_parameter_property_modifier(&param.modifiers) {
                if !self.has_identifier_text(param.name) {
                    continue;
                }
                self.write_indent();
                self.write("this.");
                self.write_identifier_text(param.name);
                self.write(" = ");
                self.write_identifier_text(param.name);
                self.write(";");
                self.write_line();
            }
        }
    }

    /// Check if parameter has a modifier that makes it a property (public, private, protected, readonly)
    fn has_parameter_property_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        let Some(mods) = modifiers else {
            return false;
        };
        for &mod_idx in &mods.nodes {
            let Some(mod_node) = self.arena.get(mod_idx) else {
                continue;
            };
            match mod_node.kind {
                k if k == SyntaxKind::PublicKeyword as u16
                    || k == SyntaxKind::PrivateKeyword as u16
                    || k == SyntaxKind::ProtectedKeyword as u16
                    || k == SyntaxKind::ReadonlyKeyword as u16 =>
                {
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    /// Emit derived class constructor body with super() transformation
    /// - Transform super(args) to var _this = _super.call(this, args) || this;
    /// - Use _this for parameter properties
    /// - Add return _this; at the end
    fn emit_derived_constructor_body(
        &mut self,
        body_idx: NodeIndex,
        params: &NodeList,
        instance_props: &[NodeIndex],
        param_transforms: &ParamTransformPlan,
    ) {
        let Some(body_node) = self.arena.get(body_idx) else {
            return;
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return;
        };

        // First, find and emit the super() call as _super.call(this, ...)
        let mut found_super = false;
        let mut super_stmt_idx = None;
        for &stmt_idx in &block.statements.nodes {
            if self.is_super_call_statement(stmt_idx) {
                super_stmt_idx = Some(stmt_idx);
                found_super = true;
                break;
            }
        }

        self.emit_param_destructuring_prologue(param_transforms);

        if let Some(super_idx) = super_stmt_idx {
            // Emit statements before super() unchanged.
            for &stmt_idx in &block.statements.nodes {
                if stmt_idx == super_idx {
                    break;
                }
                self.write_indent();
                self.emit_statement(stmt_idx);
                self.write_line();
            }
            self.emit_super_call_as_this_assignment(super_idx);
        }

        // Emit parameter properties using _this
        for &param_idx in &params.nodes {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };

            if self.has_parameter_property_modifier(&param.modifiers) {
                if !self.has_identifier_text(param.name) {
                    continue;
                }
                self.write_indent();
                self.write("_this.");
                self.write_identifier_text(param.name);
                self.write(" = ");
                self.write_identifier_text(param.name);
                self.write(";");
                self.write_line();
            }
        }

        // Emit private field initializations using _this (after super)
        self.emit_private_field_initializations(true);
        // Emit private accessor initializations
        self.emit_private_accessor_initializations(true);

        // Emit instance property initializers using _this
        for &prop_idx in instance_props {
            let Some(prop_node) = self.arena.get(prop_idx) else {
                continue;
            };
            let Some(prop_data) = self.arena.get_property_decl(prop_node) else {
                continue;
            };

            // Skip properties without initializers
            if prop_data.initializer.is_none() {
                continue;
            }

            self.write_indent();
            self.record_mapping_for_node(prop_node);
            self.emit_property_receiver_and_name("_this", prop_data.name);
            self.write(" = ");

            // Check if this initializer contains `this` or `super` that needs capture.
            let needs_capture = contains_this_reference(self.arena, prop_data.initializer);

            // Emit the initializer, with _this capture if needed
            let prev = self.use_this_capture;
            if needs_capture {
                self.use_this_capture = true;
            }
            self.emit_expression(prop_data.initializer);
            self.use_this_capture = prev;

            self.write(";");
            self.write_line();
        }

        // Emit remaining statements (after super call), transforming this to _this
        let mut past_super = false;
        for &stmt_idx in &block.statements.nodes {
            if !past_super && self.is_super_call_statement(stmt_idx) {
                past_super = true;
                continue; // Skip the super call, already emitted
            }
            if past_super {
                self.write_indent();
                self.emit_statement_with_this_transform(stmt_idx);
                self.write_line();
            }
        }

        // Add return _this;
        if found_super {
            self.write_indent();
            self.write("return _this;");
            self.write_line();
        }
    }

    /// Check if a statement is a super() call expression
    fn is_super_call_statement(&self, stmt_idx: NodeIndex) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };

        if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
            return false;
        }

        let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) else {
            return false;
        };
        let Some(call_node) = self.arena.get(expr_stmt.expression) else {
            return false;
        };

        if call_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }

        let Some(call) = self.arena.get_call_expr(call_node) else {
            return false;
        };
        let Some(callee) = self.arena.get(call.expression) else {
            return false;
        };

        callee.kind == SyntaxKind::SuperKeyword as u16
    }

    /// Emit super(args) as var _this = _super.call(this, args) || this;
    fn emit_super_call_as_this_assignment(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) else {
            return;
        };
        let Some(call_node) = self.arena.get(expr_stmt.expression) else {
            return;
        };
        let Some(call) = self.arena.get_call_expr(call_node) else {
            return;
        };

        self.write_indent();
        self.write("var _this = _super.call(this");

        // Emit arguments
        if let Some(ref args) = call.arguments {
            for &arg_idx in &args.nodes {
                self.write(", ");
                self.emit_expression(arg_idx);
            }
        }

        self.write(") || this;");
        self.write_line();
        self.this_capture_available = true;
    }

    /// Emit a statement, but transform `this` references to `_this`
    fn emit_statement_with_this_transform(&mut self, stmt_idx: NodeIndex) {
        // Enable this capture for the duration of emitting this statement
        let prev = self.use_this_capture;
        self.use_this_capture = true;
        self.emit_statement(stmt_idx);
        self.use_this_capture = prev;
    }

    fn emit_methods(&mut self, class_name: &str, class_data: &ClassData) {
        // First, collect accessors by name for combining getter/setter pairs
        // We need to know which pairs exist so we emit them together
        let mut accessor_map: std::collections::HashMap<
            String,
            (Option<NodeIndex>, Option<NodeIndex>, bool),
        > = std::collections::HashMap::new();

        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
                if let Some(accessor_data) = self.arena.get_accessor(member_node) {
                    let is_static = self.is_static(&accessor_data.modifiers);
                    // Skip static accessors (handled in emit_static_members)
                    if is_static {
                        continue;
                    }
                    // Skip abstract accessors (they have no body and shouldn't be emitted)
                    if self.is_abstract(&accessor_data.modifiers) {
                        continue;
                    }
                    // Skip private accessors (they use WeakMap pattern)
                    if is_private_identifier(self.arena, accessor_data.name) {
                        continue;
                    }
                    let name = self.get_identifier_text(accessor_data.name);
                    let entry = accessor_map.entry(name).or_insert((None, None, is_static));
                    entry.0 = Some(member_idx);
                }
            } else if member_node.kind == syntax_kind_ext::SET_ACCESSOR {
                if let Some(accessor_data) = self.arena.get_accessor(member_node) {
                    let is_static = self.is_static(&accessor_data.modifiers);
                    // Skip static accessors (handled in emit_static_members)
                    if is_static {
                        continue;
                    }
                    // Skip abstract accessors (they have no body and shouldn't be emitted)
                    if self.is_abstract(&accessor_data.modifiers) {
                        continue;
                    }
                    // Skip private accessors (they use WeakMap pattern)
                    if is_private_identifier(self.arena, accessor_data.name) {
                        continue;
                    }
                    let name = self.get_identifier_text(accessor_data.name);
                    let entry = accessor_map.entry(name).or_insert((None, None, is_static));
                    entry.1 = Some(member_idx);
                }
            }
        }

        // Track which accessor names we've already emitted
        let mut emitted_accessors: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        // Emit in source order - methods inline, accessors when we first encounter them
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                let Some(method_data) = self.arena.get_method_decl(member_node) else {
                    continue;
                };

                // Skip static methods (handled separately)
                if self.is_static(&method_data.modifiers) {
                    continue;
                }

                // Skip if no body (declaration only)
                if method_data.body.is_none() {
                    continue;
                }

                // ClassName.prototype.methodName = function () { ... };
                self.write_indent();
                self.write(class_name);
                self.write(".prototype");
                self.emit_method_name(method_data.name);
                self.write(" = function (");
                let param_transforms = self.emit_parameters(&method_data.parameters);
                self.write(") ");
                let is_async = self.is_async(&method_data.modifiers) && !method_data.asterisk_token;

                // Check if body is empty - only empty bodies go on single line
                let body_node = self.arena.get(method_data.body);
                let is_empty_body = if let Some(block_node) = body_node {
                    if let Some(block) = self.arena.get_block(block_node) {
                        block.statements.nodes.is_empty()
                    } else {
                        false
                    }
                } else {
                    false
                };

                if is_async {
                    self.write("{");
                    self.write_line();
                    self.increase_indent();
                    self.emit_param_destructuring_prologue(&param_transforms);
                    self.emit_async_body(method_data.body);
                    self.decrease_indent();
                    self.write_indent();
                    self.write("}");
                } else if is_empty_body && param_transforms.is_empty() {
                    self.write("{ }");
                } else {
                    self.write("{");
                    self.write_line();
                    self.increase_indent();
                    self.emit_param_destructuring_prologue(&param_transforms);
                    self.emit_block_contents(method_data.body);
                    self.decrease_indent();
                    self.write_indent();
                    self.write("}");
                }

                self.write(";");
                self.write_line();
            } else if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                || member_node.kind == syntax_kind_ext::SET_ACCESSOR
            {
                // Get accessor name and check if we've already emitted this pair
                if let Some(accessor_data) = self.arena.get_accessor(member_node) {
                    // Skip static/abstract (already filtered above, but double-check)
                    if self.is_static(&accessor_data.modifiers)
                        || self.is_abstract(&accessor_data.modifiers)
                    {
                        continue;
                    }
                    let name = self.get_identifier_text(accessor_data.name);
                    if emitted_accessors.contains(&name) {
                        continue;
                    }
                    // Emit this accessor pair now
                    if let Some(&(getter_idx, setter_idx, is_static)) = accessor_map.get(&name) {
                        self.emit_combined_accessor(
                            class_name, &name, getter_idx, setter_idx, is_static,
                        );
                        emitted_accessors.insert(name);
                    }
                }
            }
        }
    }

    /// Emit a combined Object.defineProperty for getter/setter pairs
    fn emit_combined_accessor(
        &mut self,
        class_name: &str,
        name: &str,
        getter_idx: Option<NodeIndex>,
        setter_idx: Option<NodeIndex>,
        is_static: bool,
    ) {
        // Object.defineProperty(ClassName.prototype, "name", { get: ..., set: ..., ... })
        self.write_indent();
        self.write("Object.defineProperty(");
        self.write(class_name);
        if !is_static {
            self.write(".prototype");
        }
        self.write(", \"");
        self.write(name);
        self.write("\", {");
        self.write_line();
        self.increase_indent();

        // Emit getter if present
        if let Some(getter_idx) = getter_idx {
            self.emit_accessor_function(getter_idx, true);
        }

        // Emit setter if present
        if let Some(setter_idx) = setter_idx {
            self.emit_accessor_function(setter_idx, false);
        }

        self.write_indent();
        self.write("enumerable: false,");
        self.write_line();
        self.write_indent();
        self.write("configurable: true");
        self.write_line();

        self.decrease_indent();
        self.write_indent();
        self.write("});");
        self.write_line();
    }

    /// Emit just the function part of an accessor (get: function () {...}, or set: function (v) {...},)
    fn emit_accessor_function(&mut self, accessor_idx: NodeIndex, is_getter: bool) {
        let Some(accessor_node) = self.arena.get(accessor_idx) else {
            return;
        };
        let Some(accessor_data) = self.arena.get_accessor(accessor_node) else {
            return;
        };

        // Check if accessor body is empty
        let (body_is_empty, body_is_single_line) = if !accessor_data.body.is_none() {
            let body_node = self.arena.get(accessor_data.body);
            let is_empty = body_node.is_none_or(|n| {
                self.arena
                    .get_block(n)
                    .is_none_or(|b| b.statements.nodes.is_empty())
            });
            let is_single_line = body_node.is_some_and(|n| self.is_single_line_block(n));
            (is_empty, is_single_line)
        } else {
            (true, false)
        };

        self.write_indent();
        let mut param_transforms = ParamTransformPlan {
            params: Vec::new(),
            rest: None,
        };
        if is_getter {
            self.write("get: function () ");
        } else {
            self.write("set: function (");
            param_transforms = self.emit_parameters(&accessor_data.parameters);
            self.write(") ");
        }

        if body_is_empty && param_transforms.is_empty() {
            // Inline empty body: { },
            self.write("{ },");
        } else if body_is_single_line && param_transforms.is_empty() {
            // Single-line body: { return 1; },
            self.write("{ ");
            self.emit_block_contents_inline(accessor_data.body);
            self.write(" },");
        } else {
            // Multi-line body
            self.write("{");
            self.write_line();
            self.increase_indent();
            self.emit_param_destructuring_prologue(&param_transforms);
            self.emit_block_contents(accessor_data.body);
            self.decrease_indent();
            self.write_indent();
            self.write("},");
        }
        self.write_line();
    }

    /// Check if a block was on a single line in the source
    fn is_single_line_block(&self, block_node: &ThinNode) -> bool {
        if let Some(source_text) = self.source_text {
            let start = block_node.pos as usize;
            let end = block_node.end as usize;
            if start < end && end <= source_text.len() {
                // The block end position may be incorrect, so find the matching }
                // from the start position
                let block_text = &source_text[start..end];
                // Find the first } which closes the block
                if let Some(close_brace_pos) = block_text.find('}') {
                    let actual_block = &block_text[..=close_brace_pos];
                    // A single-line block has no newlines between { and }
                    !actual_block.contains('\n')
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Emit block contents inline (for single-line blocks)
    fn emit_block_contents_inline(&mut self, body_idx: NodeIndex) {
        let Some(body_node) = self.arena.get(body_idx) else {
            return;
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return;
        };

        for (i, &stmt_idx) in block.statements.nodes.iter().enumerate() {
            if i > 0 {
                self.write(" ");
            }
            self.emit_statement_inline(stmt_idx);
        }
    }

    /// Emit a statement inline (without newlines/indentation)
    fn emit_statement_inline(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };

        if stmt_node.kind == syntax_kind_ext::RETURN_STATEMENT {
            self.write("return");
            if let Some(ret_data) = self.arena.get_return_statement(stmt_node) {
                if !ret_data.expression.is_none() {
                    self.write(" ");
                    self.emit_expression(ret_data.expression);
                }
            }
            self.write(";");
        } else {
            // Fallback: emit statement normally but it might not look right
            self.emit_statement(stmt_idx);
        }
    }

    #[allow(dead_code)]
    fn emit_accessor(&mut self, class_name: &str, accessor_idx: NodeIndex, is_getter: bool) {
        let Some(accessor_node) = self.arena.get(accessor_idx) else {
            return;
        };
        let Some(accessor_data) = self.arena.get_accessor(accessor_node) else {
            return;
        };

        let is_static = self.is_static(&accessor_data.modifiers);
        let name = self.get_identifier_text(accessor_data.name);

        // Use combined accessor for single getter or setter
        if is_getter {
            self.emit_combined_accessor(class_name, &name, Some(accessor_idx), None, is_static);
        } else {
            self.emit_combined_accessor(class_name, &name, None, Some(accessor_idx), is_static);
        }
    }

    fn emit_static_members(&mut self, class_name: &str, class_data: &ClassData) {
        // First, collect static accessors by name for combining getter/setter pairs
        let mut static_accessor_map: std::collections::HashMap<
            String,
            (Option<NodeIndex>, Option<NodeIndex>),
        > = std::collections::HashMap::new();

        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
                if let Some(accessor_data) = self.arena.get_accessor(member_node) {
                    if self.is_static(&accessor_data.modifiers) {
                        // Skip private static accessors (they use WeakMap pattern)
                        if is_private_identifier(self.arena, accessor_data.name) {
                            continue;
                        }
                        let name = self.get_identifier_text(accessor_data.name);
                        let entry = static_accessor_map.entry(name).or_insert((None, None));
                        entry.0 = Some(member_idx);
                    }
                }
            } else if member_node.kind == syntax_kind_ext::SET_ACCESSOR {
                if let Some(accessor_data) = self.arena.get_accessor(member_node) {
                    if self.is_static(&accessor_data.modifiers) {
                        // Skip private static accessors (they use WeakMap pattern)
                        if is_private_identifier(self.arena, accessor_data.name) {
                            continue;
                        }
                        let name = self.get_identifier_text(accessor_data.name);
                        let entry = static_accessor_map.entry(name).or_insert((None, None));
                        entry.1 = Some(member_idx);
                    }
                }
            }
        }

        // Emit static methods and properties
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                let Some(method_data) = self.arena.get_method_decl(member_node) else {
                    continue;
                };

                if !self.is_static(&method_data.modifiers) {
                    continue;
                }

                if method_data.body.is_none() {
                    continue;
                }

                // ClassName.staticMethod = function () { ... };
                self.write_indent();
                self.write(class_name);
                self.write(".");
                self.write_identifier_text(method_data.name);
                self.write(" = function (");
                let param_transforms = self.emit_parameters(&method_data.parameters);
                self.write(") {");
                self.write_line();
                self.increase_indent();

                self.emit_param_destructuring_prologue(&param_transforms);
                self.emit_block_contents(method_data.body);

                self.decrease_indent();
                self.write_indent();
                self.write("};");
                self.write_line();
            } else if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                let Some(prop_data) = self.arena.get_property_decl(member_node) else {
                    continue;
                };

                if !self.is_static(&prop_data.modifiers) {
                    continue;
                }

                if prop_data.initializer.is_none() {
                    continue;
                }

                // ClassName.staticProp = value;
                self.write_indent();
                self.write(class_name);
                self.write(".");
                self.write_identifier_text(prop_data.name);
                self.write(" = ");
                let prev_suppress = self.suppress_this_capture;
                self.suppress_this_capture = true;
                self.emit_expression(prop_data.initializer);
                self.suppress_this_capture = prev_suppress;
                self.write(";");
                self.write_line();
            } else if member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                // Static block: emit contents directly
                let Some(block_data) = self.arena.get_block(member_node) else {
                    continue;
                };

                // Emit each statement in the static block
                for &stmt_idx in &block_data.statements.nodes {
                    self.write_indent();
                    self.emit_statement(stmt_idx);
                    self.write_line();
                }
            }
        }

        // Emit combined static accessors
        for (name, (getter_idx, setter_idx)) in static_accessor_map {
            self.emit_combined_accessor(class_name, &name, getter_idx, setter_idx, true);
        }
    }

    fn emit_parameters(&mut self, params: &NodeList) -> ParamTransformPlan {
        let mut plan = ParamTransformPlan {
            params: Vec::new(),
            rest: None,
        };
        let mut first = true;
        for (index, &param_idx) in params.nodes.iter().enumerate() {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param_data) = self.arena.get_parameter(param_node) else {
                continue;
            };

            if param_data.dot_dot_dot_token {
                let rest_target = param_data.name;
                let rest_is_pattern = self.is_binding_pattern(rest_target);
                let rest_name = if rest_is_pattern {
                    self.get_temp_var_name()
                } else {
                    self.get_identifier_text(rest_target)
                };

                if !rest_name.is_empty() {
                    plan.rest = Some(RestParamTransform {
                        name: rest_name,
                        pattern: if rest_is_pattern {
                            Some(rest_target)
                        } else {
                            None
                        },
                        index,
                    });
                }
                break;
            }

            if !first {
                self.write(", ");
            }
            first = false;

            if self.is_binding_pattern(param_data.name) {
                let temp_name = self.get_temp_var_name();
                self.write(&temp_name);
                plan.params.push(ParamTransform {
                    name: temp_name,
                    pattern: Some(param_data.name),
                    initializer: if param_data.initializer.is_none() {
                        None
                    } else {
                        Some(param_data.initializer)
                    },
                });
            } else {
                self.emit_binding_name(param_data.name);
                if !param_data.initializer.is_none() {
                    let name = self.get_identifier_text(param_data.name);
                    if !name.is_empty() {
                        plan.params.push(ParamTransform {
                            name,
                            pattern: None,
                            initializer: Some(param_data.initializer),
                        });
                    }
                }
            }
        }
        plan
    }

    fn emit_param_destructuring_prologue(&mut self, transforms: &ParamTransformPlan) {
        for param in &transforms.params {
            if let Some(initializer) = param.initializer {
                self.emit_param_default_assignment(&param.name, initializer);
            }
            if let Some(pattern) = param.pattern {
                let mut started = false;
                self.emit_param_binding_assignments(pattern, &param.name, &mut started);
                if started {
                    self.write(";");
                    self.write_line();
                }
            }
        }

        if let Some(rest) = &transforms.rest {
            if !rest.name.is_empty() {
                self.write_indent();
                self.write("var ");
                self.write(&rest.name);
                self.write(" = [];");
                self.write_line();

                let iter_name = self.get_temp_var_name();
                self.write_indent();
                self.write("for (var ");
                self.write(&iter_name);
                self.write(" = ");
                self.write_usize(rest.index);
                self.write("; ");
                self.write(&iter_name);
                self.write(" < arguments.length; ");
                self.write(&iter_name);
                self.write("++) ");
                self.write(&rest.name);
                self.write("[");
                self.write(&iter_name);
                self.write(" - ");
                self.write_usize(rest.index);
                self.write("] = arguments[");
                self.write(&iter_name);
                self.write("];");
                self.write_line();
            }

            if let Some(pattern) = rest.pattern {
                let mut started = false;
                self.emit_param_binding_assignments(pattern, &rest.name, &mut started);
                if started {
                    self.write(";");
                    self.write_line();
                }
            }
        }
    }

    fn emit_async_body(&mut self, body: NodeIndex) {
        let mut async_emitter = AsyncES5Emitter::new(self.arena);
        async_emitter.set_indent_level(self.indent_level + 1);
        async_emitter.set_lexical_this(self.use_this_capture);
        async_emitter.set_class_name(&self.class_name);

        let generator_body = if async_emitter.body_contains_await(body) {
            async_emitter.emit_generator_body_with_await(body)
        } else {
            async_emitter.emit_simple_generator_body(body)
        };

        self.write_indent();
        self.write("return __awaiter(");
        if self.use_this_capture {
            self.write("_this");
        } else {
            self.write("this");
        }
        self.write(", void 0, void 0, function () {");
        self.write_line();
        self.increase_indent();
        self.write(&generator_body);
        self.decrease_indent();
        self.write_line();
        self.write_indent();
        self.write("});");
        self.write_line();
    }

    fn emit_async_arrow_function(&mut self, func: &FunctionData, this_expr: &str) {
        self.write("function (");
        let param_transforms = self.emit_parameters(&func.parameters);
        self.write(") {");
        self.write_line();
        self.increase_indent();

        self.emit_param_destructuring_prologue(&param_transforms);

        let mut async_emitter = AsyncES5Emitter::new(self.arena);
        async_emitter.set_indent_level(self.indent_level + 1);
        async_emitter.set_lexical_this(this_expr != "this");
        async_emitter.set_class_name(&self.class_name);

        let generator_body = if async_emitter.body_contains_await(func.body) {
            async_emitter.emit_generator_body_with_await(func.body)
        } else {
            async_emitter.emit_simple_generator_body(func.body)
        };

        self.write_indent();
        self.write("return __awaiter(");
        self.write(this_expr);
        self.write(", void 0, void 0, function () {");
        self.write_line();
        self.increase_indent();
        self.write(&generator_body);
        self.decrease_indent();
        self.write_line();
        self.write_indent();
        self.write("});");
        self.write_line();

        self.decrease_indent();
        self.write_indent();
        self.write("}");
    }

    fn emit_param_default_assignment(&mut self, name: &str, initializer: NodeIndex) {
        if name.is_empty() {
            return;
        }
        self.write_indent();
        self.write("if (");
        self.write(name);
        self.write(" === void 0) { ");
        self.write(name);
        self.write(" = ");
        self.emit_expression(initializer);
        self.write("; }");
        self.write_line();
    }

    fn get_binding_element_property_key(
        &self,
        elem: &crate::parser::thin_node::BindingElementData,
    ) -> Option<NodeIndex> {
        let key_idx = if !elem.property_name.is_none() {
            elem.property_name
        } else {
            elem.name
        };
        let Some(key_node) = self.arena.get(key_idx) else {
            return None;
        };
        match key_node.kind {
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                || k == SyntaxKind::Identifier as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16 =>
            {
                Some(key_idx)
            }
            _ => None,
        }
    }

    fn emit_binding_element_access(&mut self, key_idx: NodeIndex, temp_name: &str) {
        self.write(temp_name);

        let Some(name_node) = self.arena.get(key_idx) else {
            return;
        };
        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(computed) = self.arena.get_computed_property(name_node) {
                self.write("[");
                self.emit_expression(computed.expression);
                self.write("]");
            }
        } else if name_node.kind == SyntaxKind::Identifier as u16 {
            self.write(".");
            self.write_identifier_text(key_idx);
        } else if name_node.kind == SyntaxKind::StringLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(name_node) {
                self.write("[\"");
                self.write(&lit.text);
                self.write("\"]");
            }
        } else if name_node.kind == SyntaxKind::NumericLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(name_node) {
                self.write("[");
                self.write(&lit.text);
                self.write("]");
            }
        }
    }

    fn emit_param_binding_assignments(
        &mut self,
        pattern_idx: NodeIndex,
        temp_name: &str,
        started: &mut bool,
    ) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };

        match pattern_node.kind {
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                if let Some(pattern) = self.arena.get_binding_pattern(pattern_node) {
                    let rest_props = self.collect_object_rest_props(pattern);
                    for &elem_idx in &pattern.elements.nodes {
                        if elem_idx.is_none() {
                            continue;
                        }
                        let Some(elem_node) = self.arena.get(elem_idx) else {
                            continue;
                        };
                        let Some(elem) = self.arena.get_binding_element(elem_node) else {
                            continue;
                        };
                        if elem.dot_dot_dot_token {
                            self.emit_param_object_rest_element(
                                elem,
                                &rest_props,
                                temp_name,
                                started,
                            );
                        } else {
                            self.emit_param_object_binding_element(elem_idx, temp_name, started);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                if let Some(pattern) = self.arena.get_binding_pattern(pattern_node) {
                    for (i, &elem_idx) in pattern.elements.nodes.iter().enumerate() {
                        self.emit_param_array_binding_element(elem_idx, temp_name, i, started);
                    }
                }
            }
            _ => {}
        }
    }

    fn emit_param_object_binding_element(
        &mut self,
        elem_idx: NodeIndex,
        temp_name: &str,
        started: &mut bool,
    ) {
        let Some(elem_node) = self.arena.get(elem_idx) else {
            return;
        };
        let Some(elem) = self.arena.get_binding_element(elem_node) else {
            return;
        };

        if elem.dot_dot_dot_token {
            return;
        }

        let Some(key_idx) = self.get_binding_element_property_key(elem) else {
            return;
        };

        if self.is_binding_pattern(elem.name) {
            let value_name = self.get_temp_var_name();
            self.emit_param_assignment_prefix(started);
            self.write(&value_name);
            self.write(" = ");
            self.emit_binding_element_access(key_idx, temp_name);

            if !elem.initializer.is_none() {
                self.write(", ");
                self.write(&value_name);
                self.write(" = ");
                self.write(&value_name);
                self.write(" === void 0 ? ");
                self.emit_expression(elem.initializer);
                self.write(" : ");
                self.write(&value_name);
            }

            self.emit_param_binding_assignments(elem.name, &value_name, started);
            return;
        }

        if !self.has_identifier_text(elem.name) {
            return;
        }

        self.emit_param_assignment_prefix(started);
        if !elem.initializer.is_none() {
            let value_name = self.get_temp_var_name();
            self.write(&value_name);
            self.write(" = ");
            self.emit_binding_element_access(key_idx, temp_name);
            self.write(", ");
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.write(&value_name);
            self.write(" === void 0 ? ");
            self.emit_expression(elem.initializer);
            self.write(" : ");
            self.write(&value_name);
        } else {
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.emit_binding_element_access(key_idx, temp_name);
        }
    }

    fn emit_param_array_binding_element(
        &mut self,
        elem_idx: NodeIndex,
        temp_name: &str,
        index: usize,
        started: &mut bool,
    ) {
        if elem_idx.is_none() {
            return;
        }
        let Some(elem_node) = self.arena.get(elem_idx) else {
            return;
        };
        let Some(elem) = self.arena.get_binding_element(elem_node) else {
            return;
        };

        if elem.dot_dot_dot_token {
            self.emit_param_array_rest_element(elem.name, temp_name, index, started);
            return;
        }

        if self.is_binding_pattern(elem.name) {
            let value_name = self.get_temp_var_name();
            self.emit_param_assignment_prefix(started);
            self.write(&value_name);
            self.write(" = ");
            self.write(temp_name);
            self.write("[");
            self.write_usize(index);
            self.write("]");

            if !elem.initializer.is_none() {
                self.write(", ");
                self.write(&value_name);
                self.write(" = ");
                self.write(&value_name);
                self.write(" === void 0 ? ");
                self.emit_expression(elem.initializer);
                self.write(" : ");
                self.write(&value_name);
            }

            self.emit_param_binding_assignments(elem.name, &value_name, started);
            return;
        }

        if !self.has_identifier_text(elem.name) {
            return;
        }

        self.emit_param_assignment_prefix(started);
        if !elem.initializer.is_none() {
            let value_name = self.get_temp_var_name();
            self.write(&value_name);
            self.write(" = ");
            self.write(temp_name);
            self.write("[");
            self.write_usize(index);
            self.write("]");
            self.write(", ");
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.write(&value_name);
            self.write(" === void 0 ? ");
            self.emit_expression(elem.initializer);
            self.write(" : ");
            self.write(&value_name);
        } else {
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.write(temp_name);
            self.write("[");
            self.write_usize(index);
            self.write("]");
        }
    }

    fn emit_param_object_rest_element(
        &mut self,
        elem: &crate::parser::thin_node::BindingElementData,
        rest_props: &[NodeIndex],
        temp_name: &str,
        started: &mut bool,
    ) {
        let rest_target = elem.name;
        let is_pattern = self.is_binding_pattern(rest_target);
        let rest_temp = if is_pattern {
            Some(self.get_temp_var_name())
        } else {
            None
        };

        self.emit_param_assignment_prefix(started);
        if let Some(ref name) = rest_temp {
            self.write(name);
        } else {
            self.emit_binding_name(rest_target);
        }
        self.write(" = __rest(");
        self.write(temp_name);
        self.write(", ");
        self.emit_rest_exclude_list(rest_props);
        self.write(")");

        if let Some(ref name) = rest_temp {
            self.emit_param_binding_assignments(rest_target, name, started);
        }
    }

    fn emit_param_array_rest_element(
        &mut self,
        rest_target: NodeIndex,
        temp_name: &str,
        index: usize,
        started: &mut bool,
    ) {
        let is_pattern = self.is_binding_pattern(rest_target);
        let rest_temp = if is_pattern {
            Some(self.get_temp_var_name())
        } else {
            None
        };

        self.emit_param_assignment_prefix(started);
        if let Some(ref name) = rest_temp {
            self.write(name);
        } else {
            self.emit_binding_name(rest_target);
        }
        self.write(" = ");
        self.write(temp_name);
        self.write(".slice(");
        self.write_usize(index);
        self.write(")");

        if let Some(ref name) = rest_temp {
            self.emit_param_binding_assignments(rest_target, name, started);
        }
    }

    fn emit_param_assignment_prefix(&mut self, started: &mut bool) {
        if !*started {
            self.write_indent();
            self.write("var ");
            *started = true;
        } else {
            self.write(", ");
        }
    }

    fn emit_binding_name(&mut self, name_idx: NodeIndex) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        if let Some(ident) = self.arena.get_identifier(name_node) {
            self.write(&ident.escaped_text);
            return;
        }

        match name_node.kind {
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                self.emit_object_binding_pattern(name_node);
            }
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                self.emit_array_binding_pattern(name_node);
            }
            _ => {}
        }
    }

    fn emit_object_binding_pattern(&mut self, pattern_node: &ThinNode) {
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        self.write("{ ");
        let mut first = true;
        for &elem_idx in &pattern.elements.nodes {
            if !first {
                self.write(", ");
            }
            first = false;
            if elem_idx.is_none() {
                continue;
            }
            self.emit_binding_element(elem_idx);
        }
        self.write(" }");
    }

    fn emit_array_binding_pattern(&mut self, pattern_node: &ThinNode) {
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        self.write("[");
        let mut first = true;
        for &elem_idx in &pattern.elements.nodes {
            if !first {
                self.write(", ");
            }
            first = false;
            if elem_idx.is_none() {
                continue;
            }
            self.emit_binding_element(elem_idx);
        }
        self.write("]");
    }

    fn emit_binding_element(&mut self, elem_idx: NodeIndex) {
        let Some(elem_node) = self.arena.get(elem_idx) else {
            return;
        };
        let Some(elem) = self.arena.get_binding_element(elem_node) else {
            return;
        };

        if elem.dot_dot_dot_token {
            self.write("...");
        }

        if !elem.property_name.is_none() {
            self.emit_binding_property_name(elem.property_name);
            self.write(": ");
        }

        self.emit_binding_name(elem.name);

        if !elem.initializer.is_none() {
            self.write(" = ");
            self.emit_expression(elem.initializer);
        }
    }

    fn emit_binding_property_name(&mut self, name_idx: NodeIndex) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(computed) = self.arena.get_computed_property(name_node) {
                self.write("[");
                self.emit_expression(computed.expression);
                self.write("]");
            }
            return;
        }

        self.emit_expression(name_idx);
    }

    fn emit_block_contents(&mut self, block_idx: NodeIndex) {
        let Some(block_node) = self.arena.get(block_idx) else {
            return;
        };

        if let Some(block_data) = self.arena.get_block(block_node) {
            for &stmt_idx in &block_data.statements.nodes {
                self.write_indent();
                self.emit_statement(stmt_idx);
                self.write_line();
            }
        }
    }

    fn emit_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) {
                    self.emit_expression(expr_stmt.expression);
                    self.write(";");
                }
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.arena.get_return_statement(stmt_node) {
                    self.write("return");
                    if !ret.expression.is_none() {
                        self.write(" ");
                        self.emit_expression(ret.expression);
                    }
                    self.write(";");
                }
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.emit_variable_statement(stmt_idx);
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                self.emit_if_statement(stmt_idx);
            }
            k if k == syntax_kind_ext::BLOCK => {
                self.write("{");
                self.write_line();
                self.increase_indent();
                self.emit_block_contents(stmt_idx);
                self.decrease_indent();
                self.write_indent();
                self.write("}");
            }
            k if k == syntax_kind_ext::FOR_STATEMENT => {
                self.emit_for_statement(stmt_idx);
            }
            k if k == syntax_kind_ext::FOR_IN_STATEMENT => {
                self.emit_for_in_statement(stmt_idx);
            }
            k if k == syntax_kind_ext::FOR_OF_STATEMENT => {
                self.emit_for_of_statement(stmt_idx);
            }
            k if k == syntax_kind_ext::WHILE_STATEMENT => {
                self.emit_while_statement(stmt_idx);
            }
            k if k == syntax_kind_ext::DO_STATEMENT => {
                self.emit_do_statement(stmt_idx);
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                self.emit_switch_statement(stmt_idx);
            }
            k if k == syntax_kind_ext::CASE_CLAUSE => {
                self.emit_case_clause(stmt_idx);
            }
            k if k == syntax_kind_ext::DEFAULT_CLAUSE => {
                self.emit_default_clause(stmt_idx);
            }
            k if k == syntax_kind_ext::BREAK_STATEMENT => {
                self.emit_break_statement(stmt_idx);
            }
            k if k == syntax_kind_ext::CONTINUE_STATEMENT => {
                self.emit_continue_statement(stmt_idx);
            }
            k if k == syntax_kind_ext::THROW_STATEMENT => {
                self.emit_throw_statement(stmt_idx);
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                self.emit_try_statement(stmt_idx);
            }
            _ => {
                // Fallback: emit expression if possible
                self.emit_expression(stmt_idx);
                self.write(";");
            }
        }
    }

    fn emit_variable_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
            return;
        };

        self.write("var ");

        let mut first = true;
        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };

            if decl_list_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                if let Some(decl_list) = self.arena.get_variable(decl_list_node) {
                    for &decl_idx in &decl_list.declarations.nodes {
                        self.emit_variable_declaration_with_first(decl_idx, &mut first);
                    }
                }
            } else {
                // Single declaration
                self.emit_variable_declaration_with_first(decl_list_idx, &mut first);
            }
        }
        self.write(";");
    }

    fn emit_variable_declaration_with_first(&mut self, decl_idx: NodeIndex, first: &mut bool) {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return;
        };

        // Check if this is a destructuring pattern
        if self.is_binding_pattern(decl.name) && !decl.initializer.is_none() {
            // ES5 destructuring transform
            self.emit_es5_destructuring(decl_idx, first);
        } else {
            // Normal variable declaration
            if !*first {
                self.write(", ");
            }
            *first = false;
            self.emit_binding_name(decl.name);

            if !decl.initializer.is_none() {
                self.write(" = ");
                self.emit_expression(decl.initializer);
            }
        }
    }

    fn is_binding_pattern(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
            || node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
    }

    /// Emit ES5 destructuring: { x, y } = obj → _a = obj, x = _a.x, y = _a.y
    fn emit_es5_destructuring(&mut self, decl_idx: NodeIndex, first: &mut bool) {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return;
        };
        let Some(pattern_node) = self.arena.get(decl.name) else {
            return;
        };

        // Get temp variable name
        let temp_name = self.get_temp_var_name();

        // Emit temp variable assignment: _a = initializer
        if !*first {
            self.write(", ");
        }
        *first = false;
        self.write(&temp_name);
        self.write(" = ");
        self.emit_expression(decl.initializer);

        self.emit_es5_destructuring_pattern(pattern_node, &temp_name);
    }

    /// Emit a single binding element for ES5 object destructuring
    fn emit_es5_binding_element(&mut self, elem_idx: NodeIndex, temp_name: &str) {
        let Some(elem_node) = self.arena.get(elem_idx) else {
            return;
        };
        let Some(elem) = self.arena.get_binding_element(elem_node) else {
            return;
        };
        if elem.dot_dot_dot_token {
            return;
        }

        let Some(key_idx) = self.get_binding_element_property_key(elem) else {
            return;
        };

        if self.is_binding_pattern(elem.name) {
            let value_name = self.get_temp_var_name();
            self.write(", ");
            self.write(&value_name);
            self.write(" = ");
            self.emit_binding_element_access(key_idx, temp_name);

            if !elem.initializer.is_none() {
                self.write(", ");
                self.write(&value_name);
                self.write(" = ");
                self.write(&value_name);
                self.write(" === void 0 ? ");
                self.emit_expression(elem.initializer);
                self.write(" : ");
                self.write(&value_name);
            }

            self.emit_es5_destructuring_pattern_idx(elem.name, &value_name);
            return;
        }

        if !self.has_identifier_text(elem.name) {
            return;
        }

        if elem.initializer.is_none() {
            // Emit: , bindingName = temp.propName
            self.write(", ");
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.emit_binding_element_access(key_idx, temp_name);
        } else {
            let value_name = self.get_temp_var_name();
            self.write(", ");
            self.write(&value_name);
            self.write(" = ");
            self.emit_binding_element_access(key_idx, temp_name);
            self.write(", ");
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.write(&value_name);
            self.write(" === void 0 ? ");
            self.emit_expression(elem.initializer);
            self.write(" : ");
            self.write(&value_name);
        }
    }

    /// Emit a single binding element for ES5 array destructuring
    fn emit_es5_array_binding_element(
        &mut self,
        elem_idx: NodeIndex,
        temp_name: &str,
        index: usize,
    ) {
        let Some(elem_node) = self.arena.get(elem_idx) else {
            return;
        };
        let Some(elem) = self.arena.get_binding_element(elem_node) else {
            return;
        };

        if elem.dot_dot_dot_token {
            self.emit_es5_array_rest_element(elem.name, temp_name, index);
            return;
        }

        if self.is_binding_pattern(elem.name) {
            let value_name = self.get_temp_var_name();
            self.write(", ");
            self.write(&value_name);
            self.write(" = ");
            self.write(temp_name);
            self.write("[");
            self.write_usize(index);
            self.write("]");

            if !elem.initializer.is_none() {
                self.write(", ");
                self.write(&value_name);
                self.write(" = ");
                self.write(&value_name);
                self.write(" === void 0 ? ");
                self.emit_expression(elem.initializer);
                self.write(" : ");
                self.write(&value_name);
            }

            self.emit_es5_destructuring_pattern_idx(elem.name, &value_name);
            return;
        }

        if !self.has_identifier_text(elem.name) {
            return;
        }

        if elem.initializer.is_none() {
            // Emit: , bindingName = temp[index]
            self.write(", ");
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.write(temp_name);
            self.write("[");
            self.write_usize(index);
            self.write("]");
        } else {
            let value_name = self.get_temp_var_name();
            self.write(", ");
            self.write(&value_name);
            self.write(" = ");
            self.write(temp_name);
            self.write("[");
            self.write_usize(index);
            self.write("]");
            self.write(", ");
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.write(&value_name);
            self.write(" === void 0 ? ");
            self.emit_expression(elem.initializer);
            self.write(" : ");
            self.write(&value_name);
        }
    }

    fn emit_es5_destructuring_pattern(&mut self, pattern_node: &ThinNode, temp_name: &str) {
        if pattern_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
            let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
                return;
            };
            let rest_props = self.collect_object_rest_props(pattern);
            for &elem_idx in &pattern.elements.nodes {
                if elem_idx.is_none() {
                    continue;
                }
                let Some(elem_node) = self.arena.get(elem_idx) else {
                    continue;
                };
                let Some(elem) = self.arena.get_binding_element(elem_node) else {
                    continue;
                };
                if elem.dot_dot_dot_token {
                    self.emit_es5_object_rest_element(elem, &rest_props, temp_name);
                } else {
                    self.emit_es5_binding_element(elem_idx, temp_name);
                }
            }
        } else if pattern_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
            if let Some(pattern) = self.arena.get_binding_pattern(pattern_node) {
                for (i, &elem_idx) in pattern.elements.nodes.iter().enumerate() {
                    self.emit_es5_array_binding_element(elem_idx, temp_name, i);
                }
            }
        }
    }

    fn emit_es5_object_rest_element(
        &mut self,
        elem: &crate::parser::thin_node::BindingElementData,
        rest_props: &[NodeIndex],
        temp_name: &str,
    ) {
        let rest_target = elem.name;
        let is_pattern = self.is_binding_pattern(rest_target);
        let rest_temp = if is_pattern {
            Some(self.get_temp_var_name())
        } else {
            None
        };

        self.write(", ");
        if let Some(ref name) = rest_temp {
            self.write(name);
        } else {
            self.emit_binding_name(rest_target);
        }
        self.write(" = __rest(");
        self.write(temp_name);
        self.write(", ");
        self.emit_rest_exclude_list(rest_props);
        self.write(")");

        if let Some(ref name) = rest_temp {
            self.emit_es5_destructuring_pattern_idx(rest_target, name);
        }
    }

    fn emit_es5_array_rest_element(
        &mut self,
        rest_target: NodeIndex,
        temp_name: &str,
        index: usize,
    ) {
        let is_pattern = self.is_binding_pattern(rest_target);
        let rest_temp = if is_pattern {
            Some(self.get_temp_var_name())
        } else {
            None
        };

        self.write(", ");
        if let Some(ref name) = rest_temp {
            self.write(name);
        } else {
            if !self.has_identifier_text(rest_target) {
                return;
            }
            self.write_identifier_text(rest_target);
        }
        self.write(" = ");
        self.write(temp_name);
        self.write(".slice(");
        self.write_usize(index);
        self.write(")");

        if let Some(ref name) = rest_temp {
            self.emit_es5_destructuring_pattern_idx(rest_target, name);
        }
    }

    fn emit_es5_destructuring_pattern_idx(&mut self, pattern_idx: NodeIndex, temp_name: &str) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };
        self.emit_es5_destructuring_pattern(pattern_node, temp_name);
    }

    fn collect_object_rest_props(
        &self,
        pattern: &crate::parser::thin_node::BindingPatternData,
    ) -> Vec<NodeIndex> {
        let mut props = Vec::new();
        for &elem_idx in &pattern.elements.nodes {
            let Some(elem_node) = self.arena.get(elem_idx) else {
                continue;
            };
            let Some(elem) = self.arena.get_binding_element(elem_node) else {
                continue;
            };
            if elem.dot_dot_dot_token {
                continue;
            }
            let key_idx = if !elem.property_name.is_none() {
                elem.property_name
            } else {
                elem.name
            };
            if let Some(key_node) = self.arena.get(key_idx) {
                if key_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                    || key_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                {
                    continue;
                }
            }
            props.push(key_idx);
        }
        props
    }

    fn emit_rest_exclude_list(&mut self, props: &[NodeIndex]) {
        self.write("[");
        let mut first = true;
        for &prop_idx in props {
            if !first {
                self.write(", ");
            }
            first = false;
            self.emit_rest_property_key(prop_idx);
        }
        self.write("]");
    }

    fn emit_rest_property_key(&mut self, key_idx: NodeIndex) {
        let Some(key_node) = self.arena.get(key_idx) else {
            return;
        };

        if key_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(computed) = self.arena.get_computed_property(key_node) {
                self.emit_expression(computed.expression);
            }
            return;
        }

        if let Some(ident) = self.arena.get_identifier(key_node) {
            self.write("\"");
            self.write(&ident.escaped_text);
            self.write("\"");
            return;
        }

        if let Some(lit) = self.arena.get_literal(key_node) {
            self.write("\"");
            self.write(&lit.text);
            self.write("\"");
            return;
        }

        self.emit_expression(key_idx);
    }

    /// Get the next temporary variable name (_a, _b, _c, etc.)
    fn get_temp_var_name(&mut self) -> String {
        let name = format!("_{}", (b'a' + (self.temp_var_counter % 26) as u8) as char);
        self.temp_var_counter += 1;
        name
    }

    /// Emit a single variable declaration (for for-loop initializers, etc.)
    #[allow(dead_code)]
    fn emit_variable_declaration(&mut self, decl_idx: NodeIndex) {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return;
        };

        self.emit_binding_name(decl.name);

        if !decl.initializer.is_none() {
            self.write(" = ");
            self.emit_expression(decl.initializer);
        }
    }

    fn emit_if_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(if_stmt) = self.arena.get_if_statement(stmt_node) else {
            return;
        };

        self.write("if (");
        self.emit_expression(if_stmt.expression);
        self.write(") ");
        self.emit_statement(if_stmt.then_statement);

        if !if_stmt.else_statement.is_none() {
            self.write_line();
            self.write_indent();
            self.write("else ");
            self.emit_statement(if_stmt.else_statement);
        }
    }

    fn emit_for_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(for_stmt) = self.arena.get_loop(stmt_node) else {
            return;
        };

        self.write("for (");
        if !for_stmt.initializer.is_none() {
            // Check if it's a variable declaration list
            if let Some(init_node) = self.arena.get(for_stmt.initializer) {
                if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                    self.write("var ");
                    if let Some(decl_list) = self.arena.get_variable(init_node) {
                        let mut first = true;
                        for &decl_idx in &decl_list.declarations.nodes {
                            self.emit_variable_declaration_with_first(decl_idx, &mut first);
                        }
                    }
                } else {
                    self.emit_expression(for_stmt.initializer);
                }
            }
        }
        self.write("; ");
        if !for_stmt.condition.is_none() {
            self.emit_expression(for_stmt.condition);
        }
        self.write("; ");
        if !for_stmt.incrementor.is_none() {
            self.emit_expression(for_stmt.incrementor);
        }
        self.write(") ");
        self.emit_statement(for_stmt.statement);
    }

    fn emit_for_in_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(for_in_of) = self.arena.get_for_in_of(stmt_node) else {
            return;
        };

        self.write("for (");
        self.emit_for_in_of_initializer(for_in_of.initializer);
        self.write(" in ");
        self.emit_expression(for_in_of.expression);
        self.write(") ");
        self.emit_statement(for_in_of.statement);
    }

    fn emit_for_of_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(for_in_of) = self.arena.get_for_in_of(stmt_node) else {
            return;
        };

        if for_in_of.await_modifier {
            self.write("for await (");
            self.emit_for_in_of_initializer(for_in_of.initializer);
            self.write(" of ");
            self.emit_expression(for_in_of.expression);
            self.write(") ");
            self.emit_statement(for_in_of.statement);
            return;
        }

        let error_name = self.get_temp_var_name();
        let return_name = self.get_temp_var_name();
        let iterator_name = self.get_temp_var_name();
        let result_name = self.get_temp_var_name();

        self.write("var ");
        self.write(&error_name);
        self.write(", ");
        self.write(&return_name);
        self.write(";");
        self.write_line();

        self.write("try {");
        self.write_line();
        self.increase_indent();

        self.write_indent();
        self.write("for (var ");
        self.write(&iterator_name);
        self.write(" = __values(");
        self.emit_expression(for_in_of.expression);
        self.write("), ");
        self.write(&result_name);
        self.write(" = ");
        self.write(&iterator_name);
        self.write(".next(); !");
        self.write(&result_name);
        self.write(".done; ");
        self.write(&result_name);
        self.write(" = ");
        self.write(&iterator_name);
        self.write(".next()) ");

        self.write("{");
        self.write_line();
        self.increase_indent();

        self.write_indent();
        self.emit_for_of_value_binding(for_in_of.initializer, &result_name);
        self.write_line();

        self.write_indent();
        self.emit_statement(for_in_of.statement);
        self.write_line();

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();

        self.write_indent();
        self.write("catch (");
        self.write(&error_name);
        self.write("_1) { ");
        self.write(&error_name);
        self.write(" = { error: ");
        self.write(&error_name);
        self.write("_1 }; }");
        self.write_line();

        self.write_indent();
        self.write("finally {");
        self.write_line();
        self.increase_indent();

        self.write_indent();
        self.write("try {");
        self.write_line();
        self.increase_indent();

        self.write_indent();
        self.write("if (");
        self.write(&result_name);
        self.write(" && !");
        self.write(&result_name);
        self.write(".done && (");
        self.write(&return_name);
        self.write(" = ");
        self.write(&iterator_name);
        self.write(".return)) ");
        self.write(&return_name);
        self.write(".call(");
        self.write(&iterator_name);
        self.write(");");
        self.write_line();

        self.decrease_indent();
        self.write_indent();
        self.write("} finally {");
        self.write_line();
        self.increase_indent();

        self.write_indent();
        self.write("if (");
        self.write(&error_name);
        self.write(") throw ");
        self.write(&error_name);
        self.write(".error;");
        self.write_line();

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();

        self.decrease_indent();
        self.write_indent();
        self.write("}");
    }

    fn emit_for_in_of_initializer(&mut self, initializer: NodeIndex) {
        if initializer.is_none() {
            return;
        }

        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };
        if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            self.write("var ");
            if let Some(decl_list) = self.arena.get_variable(init_node) {
                let mut first = true;
                for &decl_idx in &decl_list.declarations.nodes {
                    self.emit_variable_declaration_with_first(decl_idx, &mut first);
                }
            }
        } else {
            self.emit_expression(initializer);
        }
    }

    fn emit_for_of_value_binding(&mut self, initializer: NodeIndex, result_name: &str) {
        if initializer.is_none() {
            return;
        }

        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };
        if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            self.write("var ");
            if let Some(decl_list) = self.arena.get_variable(init_node) {
                let mut first = true;
                for &decl_idx in &decl_list.declarations.nodes {
                    self.emit_for_of_declaration_value(decl_idx, result_name, &mut first);
                }
            }
            self.write(";");
        } else if self.is_binding_pattern(initializer) {
            self.write("var ");
            let mut first = true;
            self.emit_es5_destructuring_from_value(initializer, result_name, &mut first);
            self.write(";");
        } else {
            self.emit_expression(initializer);
            self.write(" = ");
            self.write(result_name);
            self.write(".value;");
        }
    }

    fn emit_for_of_declaration_value(
        &mut self,
        decl_idx: NodeIndex,
        result_name: &str,
        first: &mut bool,
    ) {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return;
        };

        if self.is_binding_pattern(decl.name) {
            self.emit_es5_destructuring_from_value(decl.name, result_name, first);
            return;
        }

        if !*first {
            self.write(", ");
        }
        *first = false;
        self.emit_binding_name(decl.name);
        self.write(" = ");
        self.write(result_name);
        self.write(".value");
    }

    fn emit_es5_destructuring_from_value(
        &mut self,
        pattern_idx: NodeIndex,
        result_name: &str,
        first: &mut bool,
    ) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };

        let temp_name = self.get_temp_var_name();
        if !*first {
            self.write(", ");
        }
        *first = false;
        self.write(&temp_name);
        self.write(" = ");
        self.write(result_name);
        self.write(".value");

        self.emit_es5_destructuring_pattern(pattern_node, &temp_name);
    }

    fn emit_while_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(while_stmt) = self.arena.get_loop(stmt_node) else {
            return;
        };

        self.write("while (");
        self.emit_expression(while_stmt.condition);
        self.write(") ");
        self.emit_statement(while_stmt.statement);
    }

    fn emit_do_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(loop_stmt) = self.arena.get_loop(stmt_node) else {
            return;
        };

        self.write("do ");
        self.emit_statement(loop_stmt.statement);
        self.write(" while (");
        self.emit_expression(loop_stmt.condition);
        self.write(");");
    }

    fn emit_switch_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(switch_stmt) = self.arena.get_switch(stmt_node) else {
            return;
        };

        self.write("switch (");
        self.emit_expression(switch_stmt.expression);
        self.write(") {");
        self.write_line();
        self.increase_indent();

        if let Some(case_block_node) = self.arena.get(switch_stmt.case_block) {
            if let Some(case_block) = self.arena.blocks.get(case_block_node.data_index as usize) {
                for &clause_idx in &case_block.statements.nodes {
                    self.write_indent();
                    if let Some(clause_node) = self.arena.get(clause_idx) {
                        if clause_node.kind == syntax_kind_ext::CASE_CLAUSE {
                            self.emit_case_clause(clause_idx);
                        } else if clause_node.kind == syntax_kind_ext::DEFAULT_CLAUSE {
                            self.emit_default_clause(clause_idx);
                        }
                    }
                }
            }
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
    }

    fn emit_case_clause(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(case_clause) = self.arena.get_case_clause(stmt_node) else {
            return;
        };

        self.write("case ");
        self.emit_expression(case_clause.expression);
        self.write(":");
        self.write_line();
        self.increase_indent();

        for &case_stmt_idx in &case_clause.statements.nodes {
            self.write_indent();
            self.emit_statement(case_stmt_idx);
            self.write_line();
        }

        self.decrease_indent();
    }

    fn emit_default_clause(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(case_clause) = self.arena.get_case_clause(stmt_node) else {
            return;
        };

        self.write("default:");
        self.write_line();
        self.increase_indent();

        for &case_stmt_idx in &case_clause.statements.nodes {
            self.write_indent();
            self.emit_statement(case_stmt_idx);
            self.write_line();
        }

        self.decrease_indent();
    }

    fn emit_break_statement(&mut self, stmt_idx: NodeIndex) {
        self.emit_jump_statement(stmt_idx, "break");
    }

    fn emit_continue_statement(&mut self, stmt_idx: NodeIndex) {
        self.emit_jump_statement(stmt_idx, "continue");
    }

    fn emit_jump_statement(&mut self, stmt_idx: NodeIndex, keyword: &str) {
        self.write(keyword);
        if let Some(label) = self.get_jump_label(stmt_idx) {
            self.write(" ");
            self.write(&label);
        }
        self.write(";");
    }

    fn get_jump_label(&self, stmt_idx: NodeIndex) -> Option<String> {
        let Some(node) = self.arena.get(stmt_idx) else {
            return None;
        };
        if !node.has_data() {
            return None;
        }
        let jump = self.arena.jump_data.get(node.data_index as usize)?;
        if jump.label.is_none() {
            None
        } else {
            Some(self.get_identifier_text(jump.label))
        }
    }

    fn emit_try_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(try_stmt) = self.arena.get_try(stmt_node) else {
            return;
        };

        self.write("try ");
        self.emit_statement(try_stmt.try_block);

        if !try_stmt.catch_clause.is_none() {
            self.write_line();
            self.write_indent();
            if let Some(catch_node) = self.arena.get(try_stmt.catch_clause) {
                if let Some(catch_data) = self.arena.get_catch_clause(catch_node) {
                    self.write("catch (");
                    // variable_declaration is a VARIABLE_DECLARATION node, need to get its name
                    if !catch_data.variable_declaration.is_none() {
                        if let Some(var_decl_node) = self.arena.get(catch_data.variable_declaration)
                        {
                            if let Some(var_decl) =
                                self.arena.get_variable_declaration(var_decl_node)
                            {
                                self.emit_binding_name(var_decl.name);
                            }
                        }
                    }
                    self.write(") ");
                    self.emit_statement(catch_data.block);
                }
            }
        }

        if !try_stmt.finally_block.is_none() {
            self.write_line();
            self.write_indent();
            self.write("finally ");
            self.emit_statement(try_stmt.finally_block);
        }
    }

    fn emit_throw_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(throw_data) = self.arena.get_return_statement(stmt_node) else {
            self.write("throw;");
            return;
        };

        self.write("throw");
        if !throw_data.expression.is_none() {
            self.write(" ");
            self.emit_expression(throw_data.expression);
        }
        self.write(";");
    }

    fn emit_expression(&mut self, expr_idx: NodeIndex) {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return;
        };

        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(ident) = self.arena.get_identifier(expr_node) {
                    self.write(&ident.escaped_text);
                }
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(expr_node) {
                    self.write(&lit.text);
                }
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(expr_node) {
                    self.write("\"");
                    self.write(&lit.text);
                    self.write("\"");
                }
            }
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(expr_node) {
                    self.emit_string_literal_text(&lit.text);
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                if let Some(tpl) = self.arena.get_template_expr(expr_node) {
                    self.emit_template_expression_es5(tpl);
                }
            }
            k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                if let Some(tagged) = self.arena.get_tagged_template(expr_node) {
                    self.emit_tagged_template_expression_es5(expr_idx, tagged);
                }
            }
            k if k == SyntaxKind::TrueKeyword as u16 => self.write("true"),
            k if k == SyntaxKind::FalseKeyword as u16 => self.write("false"),
            k if k == SyntaxKind::NullKeyword as u16 => self.write("null"),
            k if k == SyntaxKind::ThisKeyword as u16 => {
                // Use _this when inside an arrow function that needs capture
                if self.use_this_capture {
                    self.write("_this")
                } else {
                    self.write("this")
                }
            }
            k if k == SyntaxKind::SuperKeyword as u16 => {
                // In static context, super refers to the base class directly
                self.write("_super");
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(expr_node) {
                    // Check if this is a private field access (this.#field)
                    if is_private_identifier(self.arena, access.name_or_argument) {
                        // Transform to __classPrivateFieldGet(this, _ClassName_field, "f")
                        self.emit_private_field_get(access.expression, access.name_or_argument);
                    } else {
                        self.emit_expression(access.expression);
                        self.write(".");
                        self.emit_expression(access.name_or_argument);
                    }
                }
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(expr_node) {
                    self.emit_expression(access.expression);
                    self.write("[");
                    self.emit_expression(access.name_or_argument);
                    self.write("]");
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(expr_node) {
                    // Check if this is super.method(args) - transform to _super.prototype.method.call(this, args)
                    if self.is_super_method_call(call.expression) {
                        self.emit_super_method_call(call.expression, &call.arguments);
                    } else if self.is_super_element_call(call.expression) {
                        self.emit_super_element_call(call.expression, &call.arguments);
                    } else {
                        self.emit_expression(call.expression);
                        self.write("(");
                        if let Some(ref args) = call.arguments {
                            let mut first = true;
                            for &arg_idx in &args.nodes {
                                if !first {
                                    self.write(", ");
                                }
                                first = false;
                                self.emit_expression(arg_idx);
                            }
                        }
                        self.write(")");
                    }
                }
            }
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(expr_node) {
                    self.write("new ");
                    self.emit_expression(call.expression);
                    self.write("(");
                    if let Some(ref args) = call.arguments {
                        let mut first = true;
                        for &arg_idx in &args.nodes {
                            if !first {
                                self.write(", ");
                            }
                            first = false;
                            self.emit_expression(arg_idx);
                        }
                    }
                    self.write(")");
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(expr_node) {
                    // Check if this is a private field assignment (this.#field = value)
                    let is_assignment = bin.operator_token == SyntaxKind::EqualsToken as u16;

                    if is_assignment && self.is_private_field_assignment(bin.left) {
                        // Transform to __classPrivateFieldSet(this, _field, value, "f")
                        let left_node = self.arena.get(bin.left);
                        if let Some(left) = left_node {
                            if let Some(access) = self.arena.get_access_expr(left) {
                                self.emit_private_field_set(
                                    access.expression,
                                    access.name_or_argument,
                                    bin.right,
                                );
                            }
                        }
                    } else {
                        self.emit_expression(bin.left);
                        self.write(" ");
                        self.emit_binary_operator(bin.operator_token);
                        self.write(" ");
                        self.emit_expression(bin.right);
                    }
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr(expr_node) {
                    self.emit_prefix_operator(unary.operator);
                    self.emit_expression(unary.operand);
                }
            }
            k if k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr(expr_node) {
                    self.emit_expression(unary.operand);
                    self.emit_postfix_operator(unary.operator);
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.arena.get_parenthesized(expr_node) {
                    self.write("(");
                    self.emit_expression(paren.expression);
                    self.write(")");
                }
            }
            // TypeScript-only type assertions - strip the type and emit just the expression
            k if k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                if let Some(assertion) = self.arena.get_type_assertion(expr_node) {
                    self.emit_expression(assertion.expression);
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.arena.get_conditional_expr(expr_node) {
                    self.emit_expression(cond.condition);
                    self.write(" ? ");
                    self.emit_expression(cond.when_true);
                    self.write(" : ");
                    self.emit_expression(cond.when_false);
                }
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                if let Some(arr) = self.arena.get_literal_expr(expr_node) {
                    // Check if array has spread elements
                    let has_spread = arr.elements.nodes.iter().any(|&elem_idx| {
                        self.arena
                            .get(elem_idx)
                            .is_some_and(|n| n.kind == syntax_kind_ext::SPREAD_ELEMENT)
                    });

                    if has_spread {
                        // ES5: [].concat(part1, part2, ...)
                        self.emit_array_with_spread_es5(&arr.elements.nodes);
                    } else {
                        // No spread, emit normally
                        self.write("[");
                        let mut first = true;
                        for &elem_idx in &arr.elements.nodes {
                            if !first {
                                self.write(", ");
                            }
                            first = false;
                            self.emit_expression(elem_idx);
                        }
                        self.write("]");
                    }
                }
            }
            k if k == syntax_kind_ext::SPREAD_ELEMENT => {
                // This case is for spread in function arguments, not arrays
                // For arrays, we handle it in emit_array_with_spread_es5
                if let Some(spread) = self.arena.unary_exprs_ex.get(expr_node.data_index as usize) {
                    // In ES5 context for call arguments, use apply pattern
                    self.write("...");
                    self.emit_expression(spread.expression);
                }
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                if let Some(obj) = self.arena.get_literal_expr(expr_node) {
                    if self.has_computed_property_in_object(&obj.elements.nodes) {
                        self.emit_object_literal_es5(&obj.elements.nodes);
                    } else {
                        self.write("{ ");
                        let mut first = true;
                        for &prop_idx in &obj.elements.nodes {
                            if !first {
                                self.write(", ");
                            }
                            first = false;
                            self.emit_object_property(prop_idx);
                        }
                        self.write(" }");
                    }
                }
            }
            k if k == syntax_kind_ext::ARROW_FUNCTION => {
                // Transform arrow to function expression
                if let Some(func) = self.arena.get_function(expr_node) {
                    let captures_this = !self.suppress_this_capture
                        && contains_this_reference(self.arena, expr_idx);
                    let has_outer_capture = self.use_this_capture || self.this_capture_available;
                    let use_iife = captures_this && !has_outer_capture;
                    let prev_capture = self.use_this_capture;
                    if captures_this {
                        self.use_this_capture = true;
                    }

                    if use_iife {
                        self.write("(function (_this) { return ");
                    }

                    if func.is_async {
                        let parent_this = if prev_capture { "_this" } else { "this" };
                        let this_expr = if captures_this { "_this" } else { parent_this };
                        self.emit_async_arrow_function(func, this_expr);
                    } else {
                        self.write("function (");
                        let param_transforms = self.emit_parameters(&func.parameters);
                        self.write(") ");

                        // Check if body is an expression or block
                        if let Some(body_node) = self.arena.get(func.body) {
                            if body_node.kind == syntax_kind_ext::BLOCK {
                                if param_transforms.is_empty() {
                                    self.emit_statement(func.body);
                                } else {
                                    self.write("{");
                                    self.write_line();
                                    self.increase_indent();
                                    self.emit_param_destructuring_prologue(&param_transforms);
                                    self.emit_block_contents(func.body);
                                    self.decrease_indent();
                                    self.write_indent();
                                    self.write("}");
                                }
                            } else {
                                // Expression body - wrap in return
                                self.write("{");
                                self.write_line();
                                self.increase_indent();
                                self.emit_param_destructuring_prologue(&param_transforms);
                                self.write_indent();
                                self.write("return ");
                                self.emit_expression(func.body);
                                self.write(";");
                                self.write_line();
                                self.decrease_indent();
                                self.write_indent();
                                self.write("}");
                            }
                        }
                    }

                    if use_iife {
                        self.write("; })(");
                        self.write("this");
                        self.write("))");
                    }

                    // Restore previous capture state
                    self.use_this_capture = prev_capture;
                }
            }
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                if let Some(func) = self.arena.get_function(expr_node) {
                    self.write("function");
                    if !func.name.is_none() {
                        self.write(" ");
                        self.emit_expression(func.name);
                    }
                    // Space before ( for TypeScript compatibility
                    self.write(" (");
                    let param_transforms = self.emit_parameters(&func.parameters);
                    self.write(") ");

                    // Check if body is a single return statement - emit on one line
                    let body_node = self.arena.get(func.body);
                    let is_simple_body =
                        if let Some(block) = body_node.and_then(|n| self.arena.get_block(n)) {
                            block.statements.nodes.len() == 1 && {
                                let stmt_node = self.arena.get(block.statements.nodes[0]);
                                stmt_node
                                    .map(|s| s.kind == syntax_kind_ext::RETURN_STATEMENT)
                                    .unwrap_or(false)
                            }
                        } else {
                            false
                        };

                    if is_simple_body && param_transforms.is_empty() {
                        // Single-line: { return expr; }
                        if let Some(block_node) = body_node {
                            if let Some(block) = self.arena.get_block(block_node) {
                                self.write("{ ");
                                for &stmt_idx in &block.statements.nodes {
                                    self.emit_statement(stmt_idx);
                                }
                                self.write(" }");
                            }
                        }
                    } else {
                        self.write("{");
                        self.write_line();
                        self.increase_indent();
                        self.emit_param_destructuring_prologue(&param_transforms);
                        self.emit_block_contents(func.body);
                        self.decrease_indent();
                        self.write_indent();
                        self.write("}");
                    }
                }
            }
            _ => {
                // Unknown expression - try to get text from source
            }
        }
    }

    fn emit_string_literal_text(&mut self, text: &str) {
        self.write("\"");
        self.write(text);
        self.write("\"");
    }

    fn emit_template_expression_es5(&mut self, tpl: &TemplateExprData) {
        let head_text = self
            .arena
            .get(tpl.head)
            .and_then(|node| self.arena.get_literal(node))
            .map(|lit| lit.text.as_str())
            .unwrap_or("");

        self.write("(");
        self.emit_string_literal_text(head_text);

        for &span_idx in &tpl.template_spans.nodes {
            let Some(span_node) = self.arena.get(span_idx) else {
                continue;
            };
            let Some(span) = self.arena.get_template_span(span_node) else {
                continue;
            };

            self.write(" + ");
            self.write("(");
            self.emit_expression(span.expression);
            self.write(")");

            let literal_text = self
                .arena
                .get(span.literal)
                .and_then(|node| self.arena.get_literal(node))
                .map(|lit| lit.text.as_str())
                .unwrap_or("");
            self.write(" + ");
            self.emit_string_literal_text(literal_text);
        }

        self.write(")");
    }

    fn emit_tagged_template_expression_es5(
        &mut self,
        expr_idx: NodeIndex,
        tagged: &TaggedTemplateData,
    ) {
        let Some(parts) = self.collect_template_parts(tagged.template) else {
            self.emit_expression(tagged.tag);
            self.write("(");
            self.emit_expression(tagged.template);
            self.write(")");
            return;
        };

        let temp_var = self.tagged_template_var_name(expr_idx);

        self.emit_expression(tagged.tag);
        self.write("(");
        self.write(&temp_var);
        self.write(" || (");
        self.write(&temp_var);
        self.write(" = __makeTemplateObject(");
        self.emit_string_array_literal(&parts.cooked);
        self.write(", ");
        self.emit_string_array_literal(&parts.raw);
        self.write("))");
        for expr in parts.expressions {
            self.write(", ");
            self.emit_expression(expr);
        }
        self.write(")");
    }

    fn emit_string_array_literal(&mut self, parts: &[String]) {
        self.write("[");
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.emit_string_literal_text(part);
        }
        self.write("]");
    }

    /// Emit array with spread elements as ES5: [].concat(part1, part2, ...)
    fn emit_array_with_spread_es5(&mut self, elements: &[NodeIndex]) {
        // Group consecutive non-spread elements into arrays
        // [...a, 1, 2, ...b, 3] => [].concat(a, [1, 2], b, [3])
        self.write("[].concat(");

        let mut first_part = true;
        let mut current_group: Vec<NodeIndex> = Vec::new();

        for &elem_idx in elements {
            let is_spread = self
                .arena
                .get(elem_idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::SPREAD_ELEMENT);

            if is_spread {
                // Flush current group first
                if !current_group.is_empty() {
                    if !first_part {
                        self.write(", ");
                    }
                    first_part = false;
                    self.write("[");
                    for (i, &idx) in current_group.iter().enumerate() {
                        if i > 0 {
                            self.write(", ");
                        }
                        self.emit_expression(idx);
                    }
                    self.write("]");
                    current_group.clear();
                }

                // Emit spread expression (without the ...)
                if !first_part {
                    self.write(", ");
                }
                first_part = false;
                if let Some(spread_node) = self.arena.get(elem_idx) {
                    if let Some(spread) = self
                        .arena
                        .unary_exprs_ex
                        .get(spread_node.data_index as usize)
                    {
                        self.emit_expression(spread.expression);
                    }
                }
            } else {
                // Add to current group
                current_group.push(elem_idx);
            }
        }

        // Flush remaining group
        if !current_group.is_empty() {
            if !first_part {
                self.write(", ");
            }
            self.write("[");
            for (i, &idx) in current_group.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.emit_expression(idx);
            }
            self.write("]");
        }

        self.write(")");
    }

    fn collect_template_parts(&self, template_idx: NodeIndex) -> Option<TemplateParts> {
        let node = self.arena.get(template_idx)?;
        match node.kind {
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => {
                let cooked = self
                    .arena
                    .get_literal(node)
                    .map(|lit| lit.text.clone())
                    .unwrap_or_default();
                let raw = self.template_raw_text(node, &cooked);
                Some(TemplateParts {
                    cooked: vec![cooked],
                    raw: vec![raw],
                    expressions: Vec::new(),
                })
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                let tpl = self.arena.get_template_expr(node)?;
                let mut cooked = Vec::with_capacity(tpl.template_spans.nodes.len() + 1);
                let mut raw = Vec::with_capacity(tpl.template_spans.nodes.len() + 1);
                let mut expressions = Vec::with_capacity(tpl.template_spans.nodes.len());

                let head_node = self.arena.get(tpl.head)?;
                let head_text = self
                    .arena
                    .get_literal(head_node)
                    .map(|lit| lit.text.clone())
                    .unwrap_or_default();
                let head_raw = self.template_raw_text(head_node, &head_text);
                cooked.push(head_text);
                raw.push(head_raw);

                for &span_idx in &tpl.template_spans.nodes {
                    let span_node = self.arena.get(span_idx)?;
                    let span = self.arena.get_template_span(span_node)?;
                    expressions.push(span.expression);

                    let literal_node = self.arena.get(span.literal)?;
                    let literal_text = self
                        .arena
                        .get_literal(literal_node)
                        .map(|lit| lit.text.clone())
                        .unwrap_or_default();
                    let literal_raw = self.template_raw_text(literal_node, &literal_text);
                    cooked.push(literal_text);
                    raw.push(literal_raw);
                }

                Some(TemplateParts {
                    cooked,
                    raw,
                    expressions,
                })
            }
            _ => None,
        }
    }

    fn template_raw_text(&self, node: &ThinNode, cooked_fallback: &str) -> String {
        let Some(text) = self.source_text else {
            return cooked_fallback.to_string();
        };

        let (skip_leading, allow_dollar_brace, allow_backtick) = match node.kind {
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => (1_usize, false, true),
            k if k == SyntaxKind::TemplateHead as u16 => (1_usize, true, true),
            k if k == SyntaxKind::TemplateMiddle as u16 => (1_usize, true, true),
            k if k == SyntaxKind::TemplateTail as u16 => (1_usize, false, true),
            _ => return cooked_fallback.to_string(),
        };

        let start = node.pos as usize;
        if start >= text.len() {
            return cooked_fallback.to_string();
        }

        let bytes = text.as_bytes();
        let mut i = start + skip_leading;
        while i < bytes.len() {
            let ch = bytes[i];
            if ch == b'\\' {
                i += 1;
                if i < bytes.len() {
                    i += 1;
                }
                continue;
            }

            if allow_backtick && ch == b'`' {
                return text[start + skip_leading..i].to_string();
            }

            if allow_dollar_brace && ch == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                return text[start + skip_leading..i].to_string();
            }

            i += 1;
        }

        cooked_fallback.to_string()
    }

    fn tagged_template_var_name(&self, idx: NodeIndex) -> String {
        format!("__templateObject_{}", idx.0)
    }

    fn emit_object_property(&mut self, prop_idx: NodeIndex) {
        let Some(prop_node) = self.arena.get(prop_idx) else {
            return;
        };

        if prop_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
            if let Some(prop_data) = self.arena.get_property_assignment(prop_node) {
                self.emit_expression(prop_data.name);
                self.write(": ");
                self.emit_expression(prop_data.initializer);
            }
        } else if prop_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
            if let Some(shorthand) = self.arena.get_shorthand_property(prop_node) {
                self.emit_expression(shorthand.name);
            } else if let Some(ident) = self.arena.get_identifier(prop_node) {
                self.write(&ident.escaped_text);
            }
        }
    }

    fn has_computed_property_in_object(&self, elements: &[NodeIndex]) -> bool {
        for &idx in elements {
            if self.is_computed_property_member(idx) {
                return true;
            }
            if let Some(node) = self.arena.get(idx) {
                if node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT
                    || node.kind == syntax_kind_ext::SPREAD_ELEMENT
                {
                    return true;
                }
            }
        }
        false
    }

    fn is_computed_property_member(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        let name_idx = match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                self.arena.get_property_assignment(node).map(|p| p.name)
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                self.arena.get_method_decl(node).map(|m| m.name)
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                self.arena.get_accessor(node).map(|a| a.name)
            }
            _ => None,
        };

        if let Some(name_idx) = name_idx {
            if let Some(name_node) = self.arena.get(name_idx) {
                return name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME;
            }
        }

        false
    }

    fn emit_object_literal_es5(&mut self, elements: &[NodeIndex]) {
        let temp_var = self.get_temp_var_name();

        let first_computed_idx = elements
            .iter()
            .position(|&idx| {
                self.is_computed_property_member(idx) || {
                    self.arena
                        .get(idx)
                        .map(|n| {
                            n.kind == syntax_kind_ext::SPREAD_ASSIGNMENT
                                || n.kind == syntax_kind_ext::SPREAD_ELEMENT
                        })
                        .unwrap_or(false)
                }
            })
            .unwrap_or(elements.len());

        self.write("(");
        self.write(&temp_var);
        self.write(" = ");

        if first_computed_idx > 0 {
            self.write("{ ");
            let mut first = true;
            for i in 0..first_computed_idx {
                if !first {
                    self.write(", ");
                }
                first = false;
                self.emit_object_property(elements[i]);
            }
            self.write(" }");
        } else {
            self.write("{}");
        }

        for i in first_computed_idx..elements.len() {
            self.write(", ");
            self.emit_property_assignment_es5(elements[i], &temp_var);
        }

        self.write(", ");
        self.write(&temp_var);
        self.write(")");
    }

    fn emit_property_assignment_es5(&mut self, prop_idx: NodeIndex, temp_var: &str) {
        let Some(node) = self.arena.get(prop_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                if let Some(prop) = self.arena.get_property_assignment(node) {
                    self.emit_binding_element_access(prop.name, temp_var);
                    self.write(" = ");
                    self.emit_expression(prop.initializer);
                }
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                if let Some(shorthand) = self.arena.get_shorthand_property(node) {
                    self.write(temp_var);
                    self.write(".");
                    self.write_identifier_text(shorthand.name);
                    self.write(" = ");
                    self.write_identifier_text(shorthand.name);
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.arena.get_method_decl(node) {
                    self.emit_binding_element_access(method.name, temp_var);
                    self.write(" = function");
                    if method.asterisk_token {
                        self.write("*");
                    }
                    self.write(" (");
                    let param_transforms = self.emit_parameters(&method.parameters);
                    self.write(") ");

                    if let Some(body_node) = self.arena.get(method.body) {
                        if body_node.kind == syntax_kind_ext::BLOCK {
                            if param_transforms.is_empty() {
                                self.emit_statement(method.body);
                            } else {
                                self.write("{");
                                self.write_line();
                                self.increase_indent();
                                self.emit_param_destructuring_prologue(&param_transforms);
                                self.emit_block_contents(method.body);
                                self.decrease_indent();
                                self.write_indent();
                                self.write("}");
                            }
                        } else {
                            self.emit_expression(method.body);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    self.write("Object.defineProperty(");
                    self.write(temp_var);
                    self.write(", ");
                    self.emit_property_key_string(accessor.name);
                    self.write(", { get: function () ");
                    self.emit_statement(accessor.body);
                    self.write(", enumerable: true, configurable: true })");
                }
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    self.write("Object.defineProperty(");
                    self.write(temp_var);
                    self.write(", ");
                    self.emit_property_key_string(accessor.name);
                    self.write(", { set: function (");
                    let param_transforms = self.emit_parameters(&accessor.parameters);
                    self.write(") ");

                    if let Some(body_node) = self.arena.get(accessor.body) {
                        if body_node.kind == syntax_kind_ext::BLOCK {
                            if param_transforms.is_empty() {
                                self.emit_statement(accessor.body);
                            } else {
                                self.write("{");
                                self.write_line();
                                self.increase_indent();
                                self.emit_param_destructuring_prologue(&param_transforms);
                                self.emit_block_contents(accessor.body);
                                self.decrease_indent();
                                self.write_indent();
                                self.write("}");
                            }
                        } else {
                            self.emit_expression(accessor.body);
                        }
                    }
                    self.write(", enumerable: true, configurable: true })");
                }
            }
            k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                if let Some(spread) = self.arena.get_spread(node) {
                    self.write("Object.assign(");
                    self.write(temp_var);
                    self.write(", ");
                    self.emit_expression(spread.expression);
                    self.write(")");
                }
            }
            k if k == syntax_kind_ext::SPREAD_ELEMENT => {
                if let Some(spread) = self.arena.unary_exprs_ex.get(node.data_index as usize) {
                    self.write("Object.assign(");
                    self.write(temp_var);
                    self.write(", ");
                    self.emit_expression(spread.expression);
                    self.write(")");
                }
            }
            _ => {}
        }
    }

    fn emit_property_key_string(&mut self, name_idx: NodeIndex) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(computed) = self.arena.get_computed_property(name_node) {
                self.emit_expression(computed.expression);
            }
            return;
        }

        if name_node.kind == SyntaxKind::Identifier as u16 {
            self.write("\"");
            self.write_identifier_text(name_idx);
            self.write("\"");
        } else if name_node.kind == SyntaxKind::StringLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(name_node) {
                self.write("\"");
                self.write(&lit.text);
                self.write("\"");
            }
        } else if name_node.kind == SyntaxKind::NumericLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(name_node) {
                self.write(&lit.text);
            }
        }
    }

    fn emit_binary_operator(&mut self, op: u16) {
        let op_str = match op {
            x if x == SyntaxKind::PlusToken as u16 => "+",
            x if x == SyntaxKind::MinusToken as u16 => "-",
            x if x == SyntaxKind::AsteriskToken as u16 => "*",
            x if x == SyntaxKind::SlashToken as u16 => "/",
            x if x == SyntaxKind::PercentToken as u16 => "%",
            x if x == SyntaxKind::EqualsToken as u16 => "=",
            x if x == SyntaxKind::EqualsEqualsToken as u16 => "==",
            x if x == SyntaxKind::EqualsEqualsEqualsToken as u16 => "===",
            x if x == SyntaxKind::ExclamationEqualsToken as u16 => "!=",
            x if x == SyntaxKind::ExclamationEqualsEqualsToken as u16 => "!==",
            x if x == SyntaxKind::LessThanToken as u16 => "<",
            x if x == SyntaxKind::LessThanEqualsToken as u16 => "<=",
            x if x == SyntaxKind::GreaterThanToken as u16 => ">",
            x if x == SyntaxKind::GreaterThanEqualsToken as u16 => ">=",
            x if x == SyntaxKind::AmpersandAmpersandToken as u16 => "&&",
            x if x == SyntaxKind::BarBarToken as u16 => "||",
            x if x == SyntaxKind::PlusEqualsToken as u16 => "+=",
            x if x == SyntaxKind::MinusEqualsToken as u16 => "-=",
            x if x == SyntaxKind::AsteriskEqualsToken as u16 => "*=",
            x if x == SyntaxKind::SlashEqualsToken as u16 => "/=",
            x if x == SyntaxKind::AmpersandToken as u16 => "&",
            x if x == SyntaxKind::BarToken as u16 => "|",
            x if x == SyntaxKind::CaretToken as u16 => "^",
            x if x == SyntaxKind::LessThanLessThanToken as u16 => "<<",
            x if x == SyntaxKind::GreaterThanGreaterThanToken as u16 => ">>",
            x if x == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16 => ">>>",
            x if x == SyntaxKind::InKeyword as u16 => "in",
            x if x == SyntaxKind::InstanceOfKeyword as u16 => "instanceof",
            _ => "?",
        };
        self.write(op_str);
    }

    fn emit_prefix_operator(&mut self, op: u16) {
        let op_str = match op {
            x if x == SyntaxKind::PlusPlusToken as u16 => "++",
            x if x == SyntaxKind::MinusMinusToken as u16 => "--",
            x if x == SyntaxKind::ExclamationToken as u16 => "!",
            x if x == SyntaxKind::TildeToken as u16 => "~",
            x if x == SyntaxKind::PlusToken as u16 => "+",
            x if x == SyntaxKind::MinusToken as u16 => "-",
            x if x == SyntaxKind::TypeOfKeyword as u16 => "typeof ",
            x if x == SyntaxKind::VoidKeyword as u16 => "void ",
            x if x == SyntaxKind::DeleteKeyword as u16 => "delete ",
            _ => "",
        };
        self.write(op_str);
    }

    fn emit_postfix_operator(&mut self, op: u16) {
        let op_str = match op {
            x if x == SyntaxKind::PlusPlusToken as u16 => "++",
            x if x == SyntaxKind::MinusMinusToken as u16 => "--",
            _ => "",
        };
        self.write(op_str);
    }

    fn has_identifier_text(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        self.arena.get_identifier(node).is_some() || self.arena.get_literal(node).is_some()
    }

    fn write_identifier_text(&mut self, idx: NodeIndex) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };
        if let Some(ident) = self.arena.get_identifier(node) {
            self.write(&ident.escaped_text);
            return;
        }
        if let Some(lit) = self.arena.get_literal(node) {
            self.write(&lit.text);
        }
    }

    /// Check if modifiers include the `declare` keyword
    fn has_declare_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    if mod_node.kind == SyntaxKind::DeclareKeyword as u16 {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn get_identifier_text(&self, idx: NodeIndex) -> String {
        if let Some(node) = self.arena.get(idx) {
            if let Some(ident) = self.arena.get_identifier(node) {
                return ident.escaped_text.clone();
            }
            // Handle numeric literals as property names
            if let Some(lit) = self.arena.get_literal(node) {
                return lit.text.clone();
            }
        }
        String::new()
    }

    /// Check if a name is a valid identifier (can use dot notation) or needs bracket notation
    #[allow(dead_code)]
    fn is_valid_identifier_name(&self, idx: NodeIndex) -> bool {
        if let Some(node) = self.arena.get(idx) {
            // Identifiers use dot notation
            if self.arena.get_identifier(node).is_some() {
                return true;
            }
            // Numeric and string literals need bracket notation
            if node.kind == SyntaxKind::NumericLiteral as u16 {
                return false;
            }
            if node.kind == SyntaxKind::StringLiteral as u16 {
                return false;
            }
        }
        true // Default to dot notation
    }

    /// Get the property name for bracket notation (with quotes for strings)
    #[allow(dead_code)]
    fn get_computed_property_name(&self, idx: NodeIndex) -> String {
        if let Some(node) = self.arena.get(idx) {
            // String literals need quotes in bracket notation
            if node.kind == SyntaxKind::StringLiteral as u16 {
                if let Some(lit) = self.arena.get_literal(node) {
                    return format!("\"{}\"", lit.text);
                }
            }
            // Other literals (numbers) are used as-is
            if let Some(lit) = self.arena.get_literal(node) {
                return lit.text.clone();
            }
            // Identifiers
            if let Some(ident) = self.arena.get_identifier(node) {
                return ident.escaped_text.clone();
            }
        }
        String::new()
    }

    fn is_static(&self, modifiers: &Option<NodeList>) -> bool {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    if mod_node.kind == SyntaxKind::StaticKeyword as u16 {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn is_async(&self, modifiers: &Option<NodeList>) -> bool {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    if mod_node.kind == SyntaxKind::AsyncKeyword as u16 {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn is_abstract(&self, modifiers: &Option<NodeList>) -> bool {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    if mod_node.kind == SyntaxKind::AbstractKeyword as u16 {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if heritage clauses contain an `extends` clause (not just `implements`)
    #[allow(dead_code)]
    fn has_extends_clause(&self, heritage_clauses: &Option<NodeList>) -> bool {
        self.get_extends_class_name(heritage_clauses).is_some()
    }

    /// Get the base class name from the extends clause
    fn get_extends_class_name(&self, heritage_clauses: &Option<NodeList>) -> Option<String> {
        let clauses = heritage_clauses.as_ref()?;

        for &clause_idx in &clauses.nodes {
            let clause_node = self.arena.get(clause_idx)?;
            let heritage_data = self.arena.get_heritage(clause_node)?;

            // Check if this is an extends clause (not implements)
            if heritage_data.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            // Get the first type in the extends clause (the base class)
            let first_type_idx = heritage_data.types.nodes.first()?;
            let type_node = self.arena.get(*first_type_idx)?;

            // The type could be:
            // 1. A simple identifier (B in `extends B`)
            // 2. An ExpressionWithTypeArguments (B<T> in `extends B<T>`)
            // 3. A PropertyAccessExpression (A.B in `extends A.B`)

            // Try as simple identifier first
            if let Some(ident) = self.arena.get_identifier(type_node) {
                return Some(ident.escaped_text.clone());
            }

            // Try as ExpressionWithTypeArguments (for generics)
            if let Some(expr_data) = self.arena.get_expr_type_args(type_node) {
                return Some(self.get_identifier_text(expr_data.expression));
            }

            // For property access, just get the text (simplified - not handling A.B yet)
        }
        None
    }

    /// Check if expression is super.method (property access on super)
    fn is_super_method_call(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };

        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.arena.get_access_expr(expr_node) else {
            return false;
        };
        let Some(base_node) = self.arena.get(access.expression) else {
            return false;
        };

        base_node.kind == SyntaxKind::SuperKeyword as u16
    }

    /// Check if expression is super[expr] (element access on super)
    fn is_super_element_call(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };

        if expr_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.arena.get_access_expr(expr_node) else {
            return false;
        };
        let Some(base_node) = self.arena.get(access.expression) else {
            return false;
        };

        base_node.kind == SyntaxKind::SuperKeyword as u16
    }

    /// Emit super[expr](args) as _super.prototype[expr].call(this, args)
    fn emit_super_element_call(&mut self, callee_idx: NodeIndex, args: &Option<NodeList>) {
        let Some(callee_node) = self.arena.get(callee_idx) else {
            return;
        };
        let Some(access) = self.arena.get_access_expr(callee_node) else {
            return;
        };

        self.write("_super.prototype[");
        self.emit_expression(access.name_or_argument);
        self.write("].call(");
        if self.use_this_capture {
            self.write("_this");
        } else {
            self.write("this");
        }

        if let Some(arg_list) = args {
            for &arg_idx in &arg_list.nodes {
                self.write(", ");
                self.emit_expression(arg_idx);
            }
        }

        self.write(")");
    }

    /// Emit super.method(args) as _super.prototype.method.call(this, args)
    fn emit_super_method_call(&mut self, callee_idx: NodeIndex, args: &Option<NodeList>) {
        let Some(callee_node) = self.arena.get(callee_idx) else {
            return;
        };
        let Some(access) = self.arena.get_access_expr(callee_node) else {
            return;
        };

        // Get method name
        // Emit _super.prototype.method.call(this, args)
        self.write("_super.prototype.");
        self.write_identifier_text(access.name_or_argument);
        self.write(".call(");
        if self.use_this_capture {
            self.write("_this");
        } else {
            self.write("this");
        }

        if let Some(arg_list) = args {
            for &arg_idx in &arg_list.nodes {
                self.write(", ");
                self.emit_expression(arg_idx);
            }
        }

        self.write(")");
    }

    // Helper methods
    fn write(&mut self, s: &str) {
        self.output.push_str(s);
        self.advance_position(s);
    }

    fn write_usize(&mut self, value: usize) {
        emit_utils::push_usize(&mut self.output, value);
        let mut remaining = value;
        let mut digits = 1;
        while remaining >= 10 {
            remaining /= 10;
            digits += 1;
        }
        self.column += digits as u32;
    }

    fn write_line(&mut self) {
        self.output.push('\n');
        self.line += 1;
        self.column = 0;
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent_level {
            self.output.push_str("    ");
        }
        self.column += self.indent_level * 4;
    }

    fn increase_indent(&mut self) {
        self.indent_level += 1;
    }

    fn decrease_indent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    fn advance_position(&mut self, text: &str) {
        let bytes = text.as_bytes();
        let mut i = 0;

        while i < bytes.len() {
            match memchr::memchr(b'\n', &bytes[i..]) {
                Some(offset) => {
                    let segment_end = i + offset;
                    let segment = &text[i..segment_end];

                    if segment.is_ascii() {
                        self.column += segment.len() as u32;
                    } else {
                        self.column += segment.chars().map(|c| c.len_utf16() as u32).sum::<u32>();
                    }

                    self.line += 1;
                    self.column = 0;
                    i = segment_end + 1;
                }
                None => {
                    let segment = &text[i..];
                    if segment.is_ascii() {
                        self.column += segment.len() as u32;
                    } else {
                        self.column += segment.chars().map(|c| c.len_utf16() as u32).sum::<u32>();
                    }
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thin_parser::ThinParserState;

    #[test]
    fn test_simple_class_to_iife() {
        let source = r#"class Animal {
            constructor(name) {
                this.name = name;
            }
        }"#;

        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        // Find the class declaration
        if let Some(root_node) = parser.arena.get(root) {
            if let Some(source_file) = parser.arena.get_source_file(root_node) {
                if let Some(&class_idx) = source_file.statements.nodes.first() {
                    let mut emitter = ClassES5Emitter::new(&parser.arena);
                    let output = emitter.emit_class(class_idx);

                    assert!(
                        output.contains("var Animal = /** @class */"),
                        "Expected IIFE pattern: {}",
                        output
                    );
                    assert!(
                        output.contains("function Animal(name)"),
                        "Expected constructor function: {}",
                        output
                    );
                    assert!(
                        output.contains("return Animal;"),
                        "Expected return statement: {}",
                        output
                    );
                }
            }
        }
    }

    #[test]
    fn test_class_with_method() {
        let source = r#"class Animal {
            speak() {
                console.log("Hello");
            }
        }"#;

        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        if let Some(root_node) = parser.arena.get(root) {
            if let Some(source_file) = parser.arena.get_source_file(root_node) {
                if let Some(&class_idx) = source_file.statements.nodes.first() {
                    let mut emitter = ClassES5Emitter::new(&parser.arena);
                    let output = emitter.emit_class(class_idx);

                    assert!(
                        output.contains("Animal.prototype.speak = function"),
                        "Expected prototype method: {}",
                        output
                    );
                }
            }
        }
    }

    #[test]
    fn test_class_with_static_method() {
        let source = r#"class Counter {
            static count = 0;
            static increment() {
                Counter.count++;
            }
        }"#;

        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        if let Some(root_node) = parser.arena.get(root) {
            if let Some(source_file) = parser.arena.get_source_file(root_node) {
                if let Some(&class_idx) = source_file.statements.nodes.first() {
                    let mut emitter = ClassES5Emitter::new(&parser.arena);
                    let output = emitter.emit_class(class_idx);

                    assert!(
                        output.contains("Counter.count = 0"),
                        "Expected static property: {}",
                        output
                    );
                    assert!(
                        output.contains("Counter.increment = function"),
                        "Expected static method: {}",
                        output
                    );
                }
            }
        }
    }

    #[test]
    fn test_class_with_private_fields() {
        let source = r#"class Counter {
            #count = 0;
            increment() {
                this.#count++;
            }
            getCount() {
                return this.#count;
            }
        }"#;

        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        if let Some(root_node) = parser.arena.get(root) {
            if let Some(source_file) = parser.arena.get_source_file(root_node) {
                if let Some(&class_idx) = source_file.statements.nodes.first() {
                    let mut emitter = ClassES5Emitter::new(&parser.arena);
                    let output = emitter.emit_class(class_idx);

                    // Check for WeakMap variable declaration
                    assert!(
                        output.contains("var _Counter_count;"),
                        "Expected WeakMap var declaration: {}",
                        output
                    );

                    // Check for WeakMap instantiation
                    assert!(
                        output.contains("_Counter_count = new WeakMap();"),
                        "Expected WeakMap instantiation: {}",
                        output
                    );

                    // Check for .set() in constructor
                    assert!(
                        output.contains("_Counter_count.set(this, void 0);"),
                        "Expected WeakMap.set() in constructor: {}",
                        output
                    );

                    // Check for __classPrivateFieldSet in constructor (for initializer)
                    assert!(
                        output.contains("__classPrivateFieldSet(this, _Counter_count, 0, \"f\")"),
                        "Expected __classPrivateFieldSet for initializer: {}",
                        output
                    );
                }
            }
        }
    }

    #[test]
    fn test_class_method_object_literal_computed_es5() {
        let source = r#"class Foo {
            method() {
                return { [k]: 1 };
            }
        }"#;

        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        if let Some(root_node) = parser.arena.get(root) {
            if let Some(source_file) = parser.arena.get_source_file(root_node) {
                if let Some(&class_idx) = source_file.statements.nodes.first() {
                    let mut emitter = ClassES5Emitter::new(&parser.arena);
                    let output = emitter.emit_class(class_idx);

                    assert!(
                        output.contains("[k] = 1"),
                        "Expected computed property assignment in ES5 output: {}",
                        output
                    );
                }
            }
        }
    }

    #[test]
    fn test_class_method_object_literal_spread_es5() {
        let source = r#"class Foo {
            method() {
                return { ...a, b: 1 };
            }
        }"#;

        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        if let Some(root_node) = parser.arena.get(root) {
            if let Some(source_file) = parser.arena.get_source_file(root_node) {
                if let Some(&class_idx) = source_file.statements.nodes.first() {
                    let mut emitter = ClassES5Emitter::new(&parser.arena);
                    let output = emitter.emit_class(class_idx);

                    assert!(
                        output.contains("Object.assign("),
                        "Expected Object.assign in ES5 output for object spread: {}",
                        output
                    );
                    assert!(
                        !output.contains("...a"),
                        "ES5 output should not contain object spread syntax: {}",
                        output
                    );
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "class_es5_tests.rs"]
mod class_es5_tests;
