//! TC39 (non-legacy) Decorator Transform
//!
//! Transforms decorated classes using the TC39 decorator protocol.
//! For ES2015 targets, outputs an IIFE with comma-separated decorator application.
//! For ES2022+ targets, uses static initializer blocks.

use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

/// Strip TypeScript type annotations from function/setter parameters in source text.
/// Handles `(value: number)` → `(value)`.
fn strip_param_types(text: &str) -> String {
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

/// Information about a decorated member
#[derive(Debug, Clone)]
struct DecoratedMember {
    /// The member node index
    member_idx: NodeIndex,
    /// The member kind for the decorator context
    kind: MemberKind,
    /// Name of the member
    name: MemberName,
    /// Whether the member is static
    is_static: bool,
    /// Whether the member is private (#name)
    is_private: bool,
    /// Decorator expression texts (e.g. ["dec(1)"])
    decorator_exprs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum MemberKind {
    Method,
    Getter,
    Setter,
    Field,
    Accessor,
}

#[derive(Debug, Clone)]
enum MemberName {
    /// Simple identifier: `method1`
    Identifier(String),
    /// String literal in computed position: `["method2"]`
    StringLiteral(String),
    /// Computed expression: `[expr]` - needs `__propKey`
    Computed(NodeIndex),
    /// Private identifier: `#method1`
    Private(String),
}

/// TC39 Decorator Emitter
pub struct TC39DecoratorEmitter<'a> {
    arena: &'a NodeArena,
    source_text: Option<&'a str>,
    indent: usize,
    /// When true, uses `static { }` blocks (ES2022+) instead of IIFE pattern (ES2015).
    use_static_blocks: bool,
    /// When true, prefix helper calls with `tslib_1.` (importHelpers + commonjs).
    tslib_prefix: bool,
    /// When true, emit as an expression (no `let C = ` wrapper) for class expressions.
    expression_mode: bool,
    /// Function name for class expression named evaluation (__setFunctionName).
    function_name: Option<String>,
    /// When true, decorated fields stay as class field declarations (ES2022+).
    /// When false, decorated fields move to constructor assignments.
    use_define_for_class_fields: bool,
}

/// Information about a decorated field for constructor rewrite
struct DecoratedFieldInfo {
    /// The field access expression for assignment (e.g., "field1", "\"field2\"", "_a")
    access_expr: String,
    /// Whether the access uses bracket notation (computed or string literal)
    is_bracket_access: bool,
    /// The original initializer text (e.g., "1", "2"), or empty for no initializer
    initializer_text: String,
    /// Index into `decorated_members` for this field
    member_var_index: usize,
}

impl<'a> TC39DecoratorEmitter<'a> {
    pub const fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            source_text: None,
            indent: 0,
            use_static_blocks: false,
            tslib_prefix: false,
            expression_mode: false,
            function_name: None,
            use_define_for_class_fields: false,
        }
    }

    pub const fn set_source_text(&mut self, text: &'a str) {
        self.source_text = Some(text);
    }

    pub const fn set_indent_level(&mut self, level: usize) {
        self.indent = level;
    }

    pub const fn set_use_static_blocks(&mut self, use_static: bool) {
        self.use_static_blocks = use_static;
    }

    pub const fn set_tslib_prefix(&mut self, prefix: bool) {
        self.tslib_prefix = prefix;
    }

    pub const fn set_expression_mode(&mut self, expr: bool) {
        self.expression_mode = expr;
    }

    /// Set the function name for class expression named evaluation.
    /// Used for `__setFunctionName(_classThis, name)` in ES2022 mode.
    pub fn set_function_name(&mut self, name: String) {
        self.function_name = Some(name);
    }

    pub const fn set_use_define_for_class_fields(&mut self, use_define: bool) {
        self.use_define_for_class_fields = use_define;
    }

    /// Returns the helper function name with optional tslib prefix.
    fn helper(&self, name: &str) -> String {
        if self.tslib_prefix {
            format!("tslib_1.{name}")
        } else {
            name.to_string()
        }
    }

    /// Emit the TC39 decorator transform for a class declaration.
    pub fn emit_class(&self, class_idx: NodeIndex) -> String {
        let Some(class_node) = self.arena.get(class_idx) else {
            return String::new();
        };
        let Some(class_data) = self.arena.get_class(class_node) else {
            return String::new();
        };

        let class_name = self
            .get_identifier_text(class_data.name)
            .unwrap_or_default();
        let class_decorators = self.collect_class_decorator_exprs(&class_data.modifiers);
        // For anonymous class expressions WITH class decorators, generate a temp name
        // (needed for the var assignment pattern). Without class decorators, keep anonymous.
        let class_name = if class_name.is_empty() && !class_decorators.is_empty() {
            "class_1".to_string()
        } else {
            class_name
        };
        let decorated_members = self.collect_decorated_members(&class_data.members);
        let has_extends = self.has_extends_clause(&class_data.heritage_clauses);

        let has_class_decorators = !class_decorators.is_empty();

        // If there are no class decorators and no decorated members (e.g., all members
        // are abstract), return empty to signal that no transform is needed.
        if !has_class_decorators && decorated_members.is_empty() {
            return String::new();
        }

        let has_any_instance = decorated_members.iter().any(|m| !m.is_static);
        let has_any_static = decorated_members.iter().any(|m| m.is_static);

        // Compute temp var allocation.
        // For IIFE mode (ES2015), always need a class alias (_a).
        // For static block mode (ES2022+) with class decorators, we use `var C = class {}`
        // and _classThis instead of a temp var.
        let mut temp_counter: u32 = 0;
        let class_alias = if self.use_static_blocks {
            String::new()
        } else {
            next_temp_var(&mut temp_counter) // _a
        };

        // Compute propKey temp vars for computed members
        let mut computed_key_vars: Vec<(usize, String)> = Vec::new();
        for (i, member) in decorated_members.iter().enumerate() {
            if let MemberName::Computed(_) = &member.name {
                let var = next_temp_var(&mut temp_counter);
                computed_key_vars.push((i, var));
            }
        }

        // Compute member variable names
        let member_vars = self.compute_all_member_vars(&decorated_members);

        let mut out = String::new();
        let i1 = indent_str(self.indent + 1);
        let i2 = indent_str(self.indent + 2);
        let i3 = indent_str(self.indent + 3);
        let i4 = indent_str(self.indent + 4);

        // --- IIFE header ---
        if self.expression_mode {
            out.push_str("(() => {\n");
        } else {
            out.push_str(&format!("let {class_name} = (() => {{\n"));
        }

        // Var declarations (class alias only when IIFE without class decorators)
        if !self.use_static_blocks && !has_class_decorators {
            out.push_str(&format!("{i1}var {class_alias};\n"));
        }
        if !computed_key_vars.is_empty() {
            let key_names: Vec<&str> = computed_key_vars.iter().map(|(_, v)| v.as_str()).collect();
            out.push_str(&format!("{i1}var {};\n", key_names.join(", ")));
        }

        // Class decorator variables
        if !class_decorators.is_empty() {
            out.push_str(&format!(
                "{i1}let _classDecorators = [{}];\n",
                class_decorators.join(", ")
            ));
            out.push_str(&format!("{i1}let _classDescriptor;\n"));
            out.push_str(&format!("{i1}let _classExtraInitializers = [];\n"));
            out.push_str(&format!("{i1}let _classThis;\n"));
            // When a decorated class extends a base class, tsc captures the super class
            // in a _classSuper variable so it can be used for metadata and super access.
            if has_extends && let Some(extends_text) = self.get_extends_text(class_data) {
                out.push_str(&format!("{i1}let _classSuper = {extends_text};\n"));
            }
        }

        // Instance/static extra initializer arrays
        // Only emit when there are method/getter/setter members that need them
        // (field/accessor members use per-field extra initializers instead)
        let has_instance_method = decorated_members
            .iter()
            .any(|m| !m.is_static && !matches!(m.kind, MemberKind::Field | MemberKind::Accessor));
        let has_static_method = decorated_members
            .iter()
            .any(|m| m.is_static && !matches!(m.kind, MemberKind::Field | MemberKind::Accessor));
        if has_instance_method {
            out.push_str(&format!("{i1}let _instanceExtraInitializers = [];\n"));
        }
        if has_static_method {
            out.push_str(&format!("{i1}let _staticExtraInitializers = [];\n"));
        }

        // Per-member decorator and initializer variables
        for var_info in &member_vars {
            out.push_str(&format!("{i1}let {};\n", var_info.decorators_var));
            if var_info.has_initializers {
                out.push_str(&format!(
                    "{i1}let {} = [];\n",
                    var_info
                        .initializers_var
                        .as_ref()
                        .expect("has_initializers guard ensures initializers_var is Some")
                ));
                out.push_str(&format!(
                    "{i1}let {} = [];\n",
                    var_info
                        .extra_initializers_var
                        .as_ref()
                        .expect("has_initializers guard ensures extra_initializers_var is Some")
                ));
            }
            if var_info.has_descriptor {
                out.push_str(&format!(
                    "{i1}let {};\n",
                    var_info
                        .descriptor_var
                        .as_ref()
                        .expect("has_descriptor guard ensures descriptor_var is Some")
                ));
            }
        }

        // The ctor_ref determines what goes in Object.defineProperty/runInitializers:
        // - ES2022 with class decorators: `_classThis`
        // - ES2022 without class decorators: `this`
        // - ES2015 with class decorators: `_classThis`
        // - ES2015 without class decorators: the class alias `_a`
        let ctor_ref = if has_class_decorators {
            "_classThis".to_string()
        } else if self.use_static_blocks {
            "this".to_string()
        } else {
            class_alias.clone()
        };
        // For member __esDecorate calls in ES2022 static blocks, use `this` directly
        // (even with class decorators, since static blocks execute with class as `this`)
        let _member_ctor_ref = if self.use_static_blocks {
            "this".to_string()
        } else if has_class_decorators {
            "_classThis".to_string()
        } else {
            class_alias.clone()
        };

        // --- Class expression ---
        if has_class_decorators {
            // With class decorators: `var C = _classThis = class {` (ES2015) or `var C = class {` (ES2022)
            if self.use_static_blocks {
                out.push_str(&format!("{i1}var {class_name} = class"));
            } else {
                out.push_str(&format!("{i1}var {class_name} = _classThis = class"));
            }
        } else if self.use_static_blocks {
            if class_name.is_empty() {
                out.push_str(&format!("{i1}return class"));
            } else {
                out.push_str(&format!("{i1}return class {class_name}"));
            }
        } else if class_name.is_empty() {
            out.push_str(&format!("{i1}return {class_alias} = class"));
        } else {
            out.push_str(&format!("{i1}return {class_alias} = class {class_name}"));
        }
        if has_extends {
            if has_class_decorators {
                // When class decorators + extends, use the _classSuper alias
                out.push_str(" extends _classSuper");
            } else if let Some(extends_text) = self.get_extends_text(class_data) {
                out.push_str(&format!(" extends {extends_text}"));
            }
        }
        out.push_str(" {\n");

        if self.use_static_blocks {
            // ES2022: with class decorators, emit _classThis capture block first
            if has_class_decorators {
                out.push_str(&format!("{i2}static {{ _classThis = this; }}\n"));
                // For class expressions, emit __setFunctionName with the class name
                // or the externally-provided function name (from assignment context)
                if self.expression_mode {
                    // Use ONLY the externally-provided function name for __setFunctionName.
                    // The class's own name (e.g., `class C {}`) is NOT used — it's a
                    // self-reference, not the named evaluation target.
                    if let Some(fn_name) = self.function_name.clone() {
                        let set_fn = self.helper("__setFunctionName");
                        out.push_str(&format!(
                            "{i2}static {{ {set_fn}(_classThis, \"{fn_name}\"); }}\n"
                        ));
                    }
                }
            } else if self.expression_mode && self.function_name.is_some() {
                // Member-only decorators on class expression with a context name:
                // emit __setFunctionName(this, "name") in a static block
                let fn_name = self
                    .function_name
                    .as_ref()
                    .expect("guarded by function_name.is_some()");
                let set_fn = self.helper("__setFunctionName");
                out.push_str(&format!(
                    "{i2}static {{ {set_fn}(this, \"{fn_name}\"); }}\n"
                ));
            }

            // ES2022: for fields-in-constructor mode (!useDefineForClassFields),
            // emit assignment expressions in a separate static block as comma expression
            // when there are computed key assignments that need propKey.
            let has_computed_field_keys = !computed_key_vars.is_empty();
            if !self.use_define_for_class_fields && has_computed_field_keys {
                let mut assign_parts: Vec<String> = Vec::new();
                for (i, member) in decorated_members.iter().enumerate() {
                    let var_info = &member_vars[i];
                    let dec_exprs = member.decorator_exprs.join(", ");
                    assign_parts.push(format!("{} = [{}]", var_info.decorators_var, dec_exprs));
                }
                for (mi, var_name) in &computed_key_vars {
                    if let Some(member) = decorated_members.get(*mi)
                        && let MemberName::Computed(expr_idx) = &member.name
                    {
                        assign_parts.push(format!(
                            "{var_name} = {}({})",
                            self.helper("__propKey"),
                            self.node_text(*expr_idx)
                        ));
                    }
                }
                let assign_expr = assign_parts.join(", ");
                out.push_str(&format!("{i2}static {{ {assign_expr}; }}\n"));
            }
            out.push_str(&format!("{i2}static {{\n"));
            self.emit_decorator_application(
                &decorated_members,
                &member_vars,
                &class_decorators,
                &class_name,
                &ctor_ref,
                &computed_key_vars,
                has_extends,
                has_any_static,
                class_data,
                &i3,
                &mut out,
            );
            out.push_str(&format!("{i2}}}\n"));
        }

        // --- Emit class members ---
        // At ES2022, class is at indent+1, so members at indent+2.
        // At ES2015 + class decorators, class is at indent+1 (var C = class {}), members at indent+2.
        // At ES2015 without class decorators, class is at indent+2 (return _a = class), members at indent+3.
        let (member_indent, member_inner_indent) = if self.use_static_blocks || has_class_decorators
        {
            (&i2, &i3)
        } else {
            (&i3, &i4)
        };
        let (external_assignments, post_iife_assignments) = self.emit_class_body(
            class_node,
            class_data,
            &decorated_members,
            &member_vars,
            &computed_key_vars,
            has_any_instance,
            has_any_static,
            &ctor_ref,
            member_indent,
            member_inner_indent,
            &mut out,
        );

        if self.use_static_blocks {
            // ES2022: close class body
            out.push_str(&format!("{i1}}};\n"));
            if has_class_decorators {
                // With class decorators: return C = _classThis after the class
                out.push_str(&format!("{i1}return {class_name} = _classThis;\n"));
            }
        } else if has_class_decorators {
            // ES2015 + class decorators: separate statements pattern
            // Close class with semicolon (it's a var assignment, not a return)
            out.push_str(&format!("{i1}}};\n"));

            // __setFunctionName
            let set_fn_name = self.helper("__setFunctionName");
            out.push_str(&format!(
                "{i1}{set_fn_name}(_classThis, \"{class_name}\");\n"
            ));

            // Decorator application as separate IIFE
            out.push_str(&format!("{i1}(() => {{\n"));
            self.emit_decorator_application(
                &decorated_members,
                &member_vars,
                &class_decorators,
                &class_name,
                &ctor_ref,
                &computed_key_vars,
                has_extends,
                has_any_static,
                class_data,
                &i2,
                &mut out,
            );
            out.push_str(&format!("{i1}}})();\n"));

            // Static field initializations after decorator application
            for assign in &post_iife_assignments {
                if let Some(expr) = assign.strip_prefix("__EXTRA_INIT_IIFE__:") {
                    out.push_str(&format!("{i1}(() => {{\n{i2}{expr};\n{i1}}})();\n"));
                } else {
                    out.push_str(&format!("{i1}{assign};\n"));
                }
            }

            // Return C = _classThis
            out.push_str(&format!("{i1}return {class_name} = _classThis;\n"));
        } else {
            // ES2015 without class decorators: comma expression pattern
            out.push_str(&format!("{i2}}},\n"));

            // Pre-IIFE assignment expressions
            for assign in &external_assignments {
                out.push_str(&format!("{i2}{assign},\n"));
            }

            out.push_str(&format!("{i2}(() => {{\n"));
            self.emit_decorator_application(
                &decorated_members,
                &member_vars,
                &class_decorators,
                &class_name,
                &ctor_ref,
                &computed_key_vars,
                has_extends,
                has_any_static,
                class_data,
                &i3,
                &mut out,
            );
            out.push_str(&format!("{i2}}})(),\n"));

            // Post-IIFE static field initializations
            for assign in &post_iife_assignments {
                if let Some(expr) = assign.strip_prefix("__EXTRA_INIT_IIFE__:") {
                    out.push_str(&format!("{i2}(() => {{\n{i3}{expr};\n{i2}}})(),\n"));
                } else {
                    out.push_str(&format!("{i2}{assign},\n"));
                }
            }

            // Return class alias
            out.push_str(&format!("{i2}{class_alias};\n"));
        }

        // Close IIFE
        let i0 = indent_str(self.indent);
        if self.expression_mode {
            out.push_str(&i0);
            out.push_str("})()");
        } else {
            out.push_str("})();\n");
        }

        out
    }

    /// Emit the decorator application code (metadata, __esDecorate calls, etc.)
    #[allow(clippy::too_many_arguments)]
    fn emit_decorator_application(
        &self,
        decorated_members: &[DecoratedMember],
        member_vars: &[MemberVarInfo],
        class_decorators: &[String],
        class_name: &str,
        ctor_ref: &str,
        computed_key_vars: &[(usize, String)],
        has_extends: bool,
        _has_any_static: bool,
        class_data: &tsz_parser::parser::node::ClassData,
        indent: &str,
        out: &mut String,
    ) {
        // Metadata
        let has_class_decorators = !class_decorators.is_empty();
        if has_extends {
            // When class decorators are present, use _classSuper alias; otherwise use extends text directly
            let super_ref = if has_class_decorators {
                Some("_classSuper".to_string())
            } else {
                self.get_extends_text(class_data)
            };
            if let Some(super_ref) = super_ref {
                out.push_str(&format!("{indent}const _metadata = typeof Symbol === \"function\" && Symbol.metadata ? Object.create({super_ref}[Symbol.metadata] ?? null) : void 0;\n"));
            } else {
                out.push_str(&format!("{indent}const _metadata = typeof Symbol === \"function\" && Symbol.metadata ? Object.create(null) : void 0;\n"));
            }
        } else {
            out.push_str(&format!("{indent}const _metadata = typeof Symbol === \"function\" && Symbol.metadata ? Object.create(null) : void 0;\n"));
        }

        // Emit decorator assignment expressions before __esDecorate calls when
        // assignments can't go in a computed member sink:
        // - ES2022 static blocks without computed method sinks (field-only decorators)
        // - ES2015 + class decorators (assignments go in the IIFE, not a sink member)
        let has_computed_method_sink = computed_key_vars.iter().any(|(mi, _)| {
            decorated_members.get(*mi).is_some_and(|m| {
                matches!(
                    m.kind,
                    MemberKind::Method | MemberKind::Getter | MemberKind::Setter
                )
            })
        });
        let has_computed_field_keys_app = !computed_key_vars.is_empty();
        let emit_assignments_here = if self.use_static_blocks {
            // ES2022: emit here only when no computed keys and no computed method sinks
            !has_computed_field_keys_app
                && !has_computed_method_sink
                && !decorated_members.is_empty()
        } else {
            // ES2015 + class decorators: always put assignments in IIFE
            !class_decorators.is_empty()
        };
        if emit_assignments_here {
            for (i, member) in decorated_members.iter().enumerate() {
                let var_info = &member_vars[i];
                let dec_exprs = member.decorator_exprs.join(", ");
                out.push_str(&format!(
                    "{indent}{} = [{}];\n",
                    var_info.decorators_var, dec_exprs
                ));
            }
        }

        // __esDecorate calls for each member
        // In ES2022 static blocks, use `this` for the class ref (it IS the class in static blocks)
        let member_class_ref = if self.use_static_blocks {
            "this"
        } else {
            ctor_ref
        };
        for (i, member) in decorated_members.iter().enumerate() {
            let var_info = &member_vars[i];
            self.emit_es_decorate_call(
                member,
                var_info,
                member_class_ref,
                computed_key_vars,
                i,
                indent,
                out,
            );
        }

        // Class-level __esDecorate if needed
        let es_decorate = self.helper("__esDecorate");
        let run_initializers = self.helper("__runInitializers");
        if !class_decorators.is_empty() {
            out.push_str(&format!("{indent}{es_decorate}(null, _classDescriptor = {{ value: _classThis }}, _classDecorators, {{ kind: \"class\", name: _classThis.name, metadata: _metadata }}, null, _classExtraInitializers);\n"));
            out.push_str(&format!(
                "{indent}{class_name} = _classThis = _classDescriptor.value;\n"
            ));
        }

        // Metadata assignment
        out.push_str(&format!("{indent}if (_metadata) Object.defineProperty({ctor_ref}, Symbol.metadata, {{ enumerable: true, configurable: true, writable: true, value: _metadata }});\n"));

        // Static extra initializers — only for static method/getter/setter decorators
        let has_static_method_decorators = decorated_members
            .iter()
            .any(|m| m.is_static && !matches!(m.kind, MemberKind::Field | MemberKind::Accessor));
        if has_static_method_decorators {
            out.push_str(&format!(
                "{indent}{run_initializers}({ctor_ref}, _staticExtraInitializers);\n"
            ));
        }

        // Class extra initializers (when class decorators exist)
        if !class_decorators.is_empty() {
            out.push_str(&format!(
                "{indent}{run_initializers}({ctor_ref}, _classExtraInitializers);\n"
            ));
        }
    }

    /// Emit class body members with field decorator rewriting.
    ///
    /// Returns (`pre_iife_assignments`, `post_iife_assignments`) for ES2015 comma expression placement.
    #[allow(clippy::too_many_arguments)]
    fn emit_class_body(
        &self,
        class_node: &tsz_parser::parser::node::Node,
        class_data: &tsz_parser::parser::node::ClassData,
        decorated_members: &[DecoratedMember],
        member_vars: &[MemberVarInfo],
        computed_key_vars: &[(usize, String)],
        has_any_instance: bool,
        _has_any_static: bool,
        _class_alias: &str,
        indent: &str,
        inner_indent: &str,
        out: &mut String,
    ) -> (Vec<String>, Vec<String>) {
        let run_init = self.helper("__runInitializers");
        let fields_in_class_body = self.use_static_blocks && self.use_define_for_class_fields;

        let propkey_map: std::collections::HashMap<NodeIndex, &str> = computed_key_vars
            .iter()
            .filter_map(|(mi, var)| {
                decorated_members
                    .get(*mi)
                    .map(|m| (m.member_idx, var.as_str()))
            })
            .collect();

        let decorated_field_idx_set: std::collections::HashSet<NodeIndex> = decorated_members
            .iter()
            .filter(|m| m.kind == MemberKind::Field)
            .map(|m| m.member_idx)
            .collect();

        let field_infos = self.collect_decorated_field_info(decorated_members, computed_key_vars);

        let all_members: Vec<_> = class_data
            .members
            .nodes
            .iter()
            .filter_map(|&idx| self.arena.get(idx).map(|n| (idx, n)))
            .collect();

        // Build assignment injection map
        let mut assignment_queue: Vec<String> = Vec::new();
        let mut injected_assignments: std::collections::HashMap<NodeIndex, Vec<String>> =
            std::collections::HashMap::new();

        for (i, member) in decorated_members.iter().enumerate() {
            let var_info = &member_vars[i];
            let dec_exprs = member.decorator_exprs.join(", ");
            assignment_queue.push(format!("{} = [{}]", var_info.decorators_var, dec_exprs));

            let is_field_being_removed = !fields_in_class_body && member.kind == MemberKind::Field;
            if propkey_map.contains_key(&member.member_idx) {
                if let MemberName::Computed(expr_idx) = &member.name
                    && let Some((_, var_name)) = computed_key_vars.iter().find(|(mi, _)| *mi == i)
                {
                    assignment_queue.push(format!(
                        "{var_name} = {}({})",
                        self.helper("__propKey"),
                        self.node_text(*expr_idx)
                    ));
                }
                if !is_field_being_removed {
                    injected_assignments
                        .insert(member.member_idx, std::mem::take(&mut assignment_queue));
                }
            }
        }
        let remaining_assignments = assignment_queue;

        // Emittable members: exclude constructors, index sigs, semicolons, and removed fields
        let emittable: Vec<usize> = all_members
            .iter()
            .enumerate()
            .filter(|(_, (idx, node))| {
                node.kind != syntax_kind_ext::CONSTRUCTOR
                    && node.kind != syntax_kind_ext::INDEX_SIGNATURE
                    && node.kind != syntax_kind_ext::SEMICOLON_CLASS_ELEMENT
                    && (fields_in_class_body || !decorated_field_idx_set.contains(idx))
            })
            .map(|(i, _)| i)
            .collect();

        let class_close = self.find_class_close_brace(class_node);
        for &emit_i in &emittable {
            let (member_idx, member_node) = all_members[emit_i];
            let next_boundary = if emit_i + 1 < all_members.len() {
                all_members[emit_i + 1].1.pos as usize
            } else {
                class_close
            };
            let member_text = self.emit_member_bounded(member_node, next_boundary.min(class_close));

            let is_decorated_field =
                fields_in_class_body && decorated_field_idx_set.contains(&member_idx);

            if is_decorated_field {
                if let Some(fi) = field_infos
                    .iter()
                    .find(|f| decorated_members[f.member_var_index].member_idx == member_idx)
                {
                    let is_static = decorated_members[fi.member_var_index].is_static;
                    let static_prefix = if is_static { "static " } else { "" };
                    let var_info = &member_vars[fi.member_var_index];
                    let init_var = var_info
                        .initializers_var
                        .as_deref()
                        .unwrap_or("_initializers");

                    // Group by static/instance for chaining
                    let same_group: Vec<usize> = field_infos
                        .iter()
                        .enumerate()
                        .filter(|(_, f)| {
                            decorated_members[f.member_var_index].is_static == is_static
                        })
                        .map(|(idx, _)| idx)
                        .collect();
                    let group_idx = same_group
                        .iter()
                        .position(|&idx| {
                            decorated_members[field_infos[idx].member_var_index].member_idx
                                == member_idx
                        })
                        .unwrap_or(0);

                    let init_arg = if fi.initializer_text.is_empty() {
                        ", void 0".to_string()
                    } else {
                        format!(", {}", fi.initializer_text)
                    };

                    let run_init_expr = if group_idx == 0 {
                        format!("{run_init}(this, {init_var}{init_arg})")
                    } else {
                        let prev_fi = &field_infos[same_group[group_idx - 1]];
                        let prev_extra = member_vars[prev_fi.member_var_index]
                            .extra_initializers_var
                            .as_deref()
                            .unwrap_or("_extra");
                        format!(
                            "({run_init}(this, {prev_extra}), {run_init}(this, {init_var}{init_arg}))"
                        )
                    };

                    if let Some(assignments) = injected_assignments.get(&member_idx) {
                        let injected = assignments.join(", ");
                        out.push_str(&format!(
                            "{indent}{static_prefix}[({injected})] = {run_init_expr};\n"
                        ));
                    } else if fi.is_bracket_access {
                        out.push_str(&format!(
                            "{indent}{static_prefix}[{}] = {run_init_expr};\n",
                            fi.access_expr
                        ));
                    } else {
                        out.push_str(&format!(
                            "{indent}{static_prefix}{} = {run_init_expr};\n",
                            fi.access_expr
                        ));
                    }
                } else {
                    out.push_str(&format!("{indent}{member_text}\n"));
                }
            } else if let Some(assignments) = injected_assignments.get(&member_idx) {
                let injected = assignments.join(", ");
                if let Some(bracket_start) = member_text.find('[') {
                    let before = &member_text[..bracket_start + 1];
                    let after = &member_text[bracket_start + 1..];
                    if let Some(bracket_end) = self.find_matching_bracket(after) {
                        let rest = &after[bracket_end + 1..];
                        out.push_str(&format!("{indent}{before}({injected})]{rest}\n"));
                    } else {
                        out.push_str(&format!("{indent}{before}({injected})]() {{ }}\n"));
                    }
                } else {
                    out.push_str(&format!("{indent}{member_text}\n"));
                }
            } else {
                out.push_str(&format!("{indent}{member_text}\n"));
            }
        }

        // Handle remaining assignments
        let mut external_assignments: Vec<String> = Vec::new();
        let mut post_iife_assignments: Vec<String> = Vec::new();
        let has_computed_method_sink = computed_key_vars.iter().any(|(mi, _)| {
            decorated_members.get(*mi).is_some_and(|m| {
                matches!(
                    m.kind,
                    MemberKind::Method | MemberKind::Getter | MemberKind::Setter
                )
            })
        });
        let es2015_class_decorators = !self.use_static_blocks && _class_alias == "_classThis";
        let skip_sink = if self.use_static_blocks {
            !has_computed_method_sink && !decorated_members.is_empty()
        } else if es2015_class_decorators {
            true
        } else {
            if !remaining_assignments.is_empty() {
                external_assignments = remaining_assignments.clone();
            }
            true
        };
        if !remaining_assignments.is_empty() && !skip_sink {
            let sink_expr = remaining_assignments.join(", ");
            let sink_is_static = decorated_members.iter().any(|m| m.is_static);
            let static_prefix = if sink_is_static { "static " } else { "" };
            out.push_str(&format!("{indent}{static_prefix}[({sink_expr})]() {{ }}\n"));
        }

        // Static field initialization
        let static_fields: Vec<&DecoratedFieldInfo> = field_infos
            .iter()
            .filter(|fi| decorated_members[fi.member_var_index].is_static)
            .collect();

        if !static_fields.is_empty() {
            if self.use_static_blocks && !self.use_define_for_class_fields {
                // ES2022 + useDefine=false: each static field in its own static block
                for (sf_idx, fi) in static_fields.iter().enumerate() {
                    let var_info = &member_vars[fi.member_var_index];
                    let init_var = var_info.initializers_var.as_deref().unwrap_or("_init");
                    let init_arg = if fi.initializer_text.is_empty() {
                        ", void 0".to_string()
                    } else {
                        format!(", {}", fi.initializer_text)
                    };
                    let rhs = if sf_idx == 0 {
                        format!("{run_init}(this, {init_var}{init_arg})")
                    } else {
                        let prev_extra = member_vars[static_fields[sf_idx - 1].member_var_index]
                            .extra_initializers_var
                            .as_deref()
                            .unwrap_or("_extra");
                        format!(
                            "({run_init}(this, {prev_extra}), {run_init}(this, {init_var}{init_arg}))"
                        )
                    };
                    let lhs = if fi.is_bracket_access {
                        format!("this[{}]", fi.access_expr)
                    } else {
                        format!("this.{}", fi.access_expr)
                    };
                    out.push_str(&format!("{indent}static {{ {lhs} = {rhs}; }}\n"));
                }
                if let Some(last_fi) = static_fields.last()
                    && let Some(ref extra_var) =
                        member_vars[last_fi.member_var_index].extra_initializers_var
                {
                    out.push_str(&format!("{indent}static {{\n{inner_indent}{run_init}(this, {extra_var});\n{indent}}}\n"));
                }
            } else if self.use_static_blocks && self.use_define_for_class_fields {
                // ES2022 + useDefine=true: last static field's extra-initializers in static block
                if let Some(last_fi) = static_fields.last()
                    && let Some(ref extra_var) =
                        member_vars[last_fi.member_var_index].extra_initializers_var
                {
                    out.push_str(&format!("{indent}static {{\n{inner_indent}{run_init}(this, {extra_var});\n{indent}}}\n"));
                }
            } else {
                // ES2015: static field inits as comma expressions (post-IIFE)
                let class_ref = _class_alias;
                for (sf_idx, fi) in static_fields.iter().enumerate() {
                    let var_info = &member_vars[fi.member_var_index];
                    let init_var = var_info.initializers_var.as_deref().unwrap_or("_init");
                    let init_arg = if fi.initializer_text.is_empty() {
                        ", void 0".to_string()
                    } else {
                        format!(", {}", fi.initializer_text)
                    };
                    let rhs = if sf_idx == 0 {
                        format!("{run_init}({class_ref}, {init_var}{init_arg})")
                    } else {
                        let prev_extra = member_vars[static_fields[sf_idx - 1].member_var_index]
                            .extra_initializers_var
                            .as_deref()
                            .unwrap_or("_extra");
                        format!(
                            "({run_init}({class_ref}, {prev_extra}), {run_init}({class_ref}, {init_var}{init_arg}))"
                        )
                    };
                    if self.use_define_for_class_fields {
                        let key_expr = if fi.is_bracket_access {
                            fi.access_expr.clone()
                        } else {
                            format!("\"{}\"", fi.access_expr)
                        };
                        post_iife_assignments.push(format!(
                            "Object.defineProperty({class_ref}, {key_expr}, {{\n{indent}    enumerable: true,\n{indent}    configurable: true,\n{indent}    writable: true,\n{indent}    value: {rhs}\n{indent}}})"
                        ));
                    } else {
                        let lhs = if fi.is_bracket_access {
                            format!("{class_ref}[{}]", fi.access_expr)
                        } else {
                            format!("{class_ref}.{}", fi.access_expr)
                        };
                        post_iife_assignments.push(format!("{lhs} = {rhs}"));
                    }
                }
                if let Some(last_fi) = static_fields.last()
                    && let Some(ref extra_var) =
                        member_vars[last_fi.member_var_index].extra_initializers_var
                {
                    post_iife_assignments.push(format!(
                        "__EXTRA_INIT_IIFE__:{run_init}({class_ref}, {extra_var})"
                    ));
                }
            }
        }

        // Constructor
        let source_ctor = self.get_constructor_info(class_data);
        let has_instance_fields = field_infos
            .iter()
            .any(|fi| !decorated_members[fi.member_var_index].is_static);
        let has_instance_method = decorated_members
            .iter()
            .any(|m| !m.is_static && !matches!(m.kind, MemberKind::Field | MemberKind::Accessor));
        let needs_ctor = source_ctor.is_some() || has_any_instance;

        if needs_ctor {
            let mut ctor_init_calls: Vec<String> = Vec::new();

            if !fields_in_class_body && has_instance_fields {
                // Fields move to constructor
                for (fi_idx, fi) in field_infos.iter().enumerate() {
                    if decorated_members[fi.member_var_index].is_static {
                        continue;
                    }
                    let var_info = &member_vars[fi.member_var_index];
                    let init_var = var_info.initializers_var.as_deref().unwrap_or("_init");
                    let init_arg = if fi.initializer_text.is_empty() {
                        ", void 0".to_string()
                    } else {
                        format!(", {}", fi.initializer_text)
                    };
                    let instance_field_idx = field_infos[..fi_idx]
                        .iter()
                        .filter(|f| !decorated_members[f.member_var_index].is_static)
                        .count();

                    let rhs = if instance_field_idx == 0 {
                        format!("{run_init}(this, {init_var}{init_arg})")
                    } else {
                        let prev_fi = field_infos[..fi_idx]
                            .iter()
                            .rev()
                            .find(|f| !decorated_members[f.member_var_index].is_static)
                            .unwrap();
                        let prev_extra = member_vars[prev_fi.member_var_index]
                            .extra_initializers_var
                            .as_deref()
                            .unwrap_or("_extra");
                        format!(
                            "({run_init}(this, {prev_extra}), {run_init}(this, {init_var}{init_arg}))"
                        )
                    };

                    if self.use_define_for_class_fields && !self.use_static_blocks {
                        let key_expr = if fi.is_bracket_access {
                            fi.access_expr.clone()
                        } else {
                            format!("\"{}\"", fi.access_expr)
                        };
                        ctor_init_calls.push(format!(
                            "{inner_indent}Object.defineProperty(this, {key_expr}, {{\n{inner_indent}    enumerable: true,\n{inner_indent}    configurable: true,\n{inner_indent}    writable: true,\n{inner_indent}    value: {rhs}\n{inner_indent}}});\n"
                        ));
                    } else {
                        let lhs = if fi.is_bracket_access {
                            format!("this[{}]", fi.access_expr)
                        } else {
                            format!("this.{}", fi.access_expr)
                        };
                        ctor_init_calls.push(format!("{inner_indent}{lhs} = {rhs};\n"));
                    }
                }
                // Last instance field's extra-initializers
                if let Some(last_fi) = field_infos
                    .iter()
                    .rev()
                    .find(|f| !decorated_members[f.member_var_index].is_static)
                    && let Some(ref extra_var) =
                        member_vars[last_fi.member_var_index].extra_initializers_var
                {
                    ctor_init_calls.push(format!("{inner_indent}{run_init}(this, {extra_var});\n"));
                }
            } else if fields_in_class_body && has_instance_fields {
                // Fields in class body: only last instance field's extra-initializers in constructor
                if let Some(last_fi) = field_infos
                    .iter()
                    .rev()
                    .find(|f| !decorated_members[f.member_var_index].is_static)
                    && let Some(ref extra_var) =
                        member_vars[last_fi.member_var_index].extra_initializers_var
                {
                    ctor_init_calls.push(format!("{inner_indent}{run_init}(this, {extra_var});\n"));
                }
            } else if has_instance_method {
                ctor_init_calls.push(format!(
                    "{inner_indent}{run_init}(this, _instanceExtraInitializers);\n"
                ));
            }

            out.push_str(&format!("{indent}constructor("));
            if let Some(ctor) = source_ctor {
                out.push_str(&ctor.params);
                out.push_str(") {\n");
                for line in &ctor.body_lines {
                    out.push_str(&format!("{inner_indent}{}\n", line.trim()));
                }
                for call in &ctor_init_calls {
                    out.push_str(call);
                }
                out.push_str(&format!("{indent}}}\n"));
            } else {
                out.push_str(") {\n");
                for call in &ctor_init_calls {
                    out.push_str(call);
                }
                out.push_str(&format!("{indent}}}\n"));
            }
        }

        (external_assignments, post_iife_assignments)
    }

    /// Find the position of the class closing brace by scanning forward from the
    /// class body opening `{`, tracking brace depth.
    fn find_class_close_brace(&self, class_node: &tsz_parser::parser::node::Node) -> usize {
        let Some(source) = self.source_text else {
            return class_node.end as usize;
        };
        let bytes = source.as_bytes();
        let start = class_node.pos as usize;
        let end = source.len().min(class_node.end as usize + 100); // generous bound

        // Find the opening `{` of the class body
        let mut pos = start;
        while pos < end && bytes[pos] != b'{' {
            pos += 1;
        }
        if pos >= end {
            return class_node.end as usize;
        }

        // Track brace depth from the opening `{`
        let mut depth: u32 = 0;
        let mut in_string = false;
        let mut string_char: u8 = 0;
        let mut in_template = false;
        let mut template_depth: u32 = 0;

        while pos < end {
            let ch = bytes[pos];
            if in_string {
                if ch == b'\\' {
                    pos += 1; // skip escape
                } else if ch == string_char {
                    in_string = false;
                }
            } else if in_template {
                if ch == b'\\' {
                    pos += 1;
                } else if ch == b'`' {
                    in_template = false;
                } else if ch == b'$' && pos + 1 < end && bytes[pos + 1] == b'{' {
                    template_depth += 1;
                    pos += 1;
                }
            } else {
                match ch {
                    b'\'' | b'"' => {
                        in_string = true;
                        string_char = ch;
                    }
                    b'`' => in_template = true,
                    b'{' => depth += 1,
                    b'}' => {
                        if template_depth > 0 {
                            template_depth -= 1;
                        } else {
                            depth -= 1;
                            if depth == 0 {
                                return pos; // position of the closing `}`
                            }
                        }
                    }
                    _ => {}
                }
            }
            pos += 1;
        }
        class_node.end as usize
    }

    /// Emit a single member with decorators stripped, bounded by the next member's start.
    /// Uses AST positions for the clean start and the next member's position as end boundary.
    fn emit_member_bounded(
        &self,
        member_node: &tsz_parser::parser::node::Node,
        next_boundary: usize,
    ) -> String {
        let Some(source) = self.source_text else {
            return String::new();
        };

        let clean_start = self.find_member_clean_start(member_node);
        // Use member.end as the primary boundary, clamped by next_boundary
        let raw_end = std::cmp::min(member_node.end as usize, next_boundary);

        if clean_start < source.len() && raw_end <= source.len() && clean_start < raw_end {
            let mut text = source[clean_start..raw_end].trim();
            // Strip class closing brace that may leak into last member's text.
            // The parser sets member.end to include trailing trivia up to the class `}`.
            // Detect: a trailing `}` separated from member content by whitespace containing newline.
            if text.ends_with('}') {
                let before = &text[..text.len() - 1];
                let trimmed = before.trim_end();
                if trimmed.ends_with('}') && before.contains('\n') {
                    text = trimmed;
                }
            }
            // Strip TS type annotations from setter/method params: `(v: number)` → `(v)`
            let text = strip_param_types(text);
            let text = text.as_str();
            // Normalize empty method bodies: `{}` -> `{ }`
            if let Some(stripped) = text.strip_suffix("{}") {
                format!("{stripped}{{ }}")
            } else {
                text.to_string()
            }
        } else {
            String::new()
        }
    }

    /// Find the position in source text where the "clean" (non-decorator, non-TS-modifier)
    /// part of a class member begins.
    fn find_member_clean_start(&self, member_node: &tsz_parser::parser::node::Node) -> usize {
        let (modifiers, name_idx) = match member_node.kind {
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let data = self.arena.get_method_decl(member_node);
                (
                    data.as_ref().and_then(|m| m.modifiers.clone()),
                    data.map(|m| m.name),
                )
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                let data = self.arena.get_property_decl(member_node);
                (
                    data.as_ref().and_then(|p| p.modifiers.clone()),
                    data.map(|p| p.name),
                )
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                let data = self.arena.get_accessor(member_node);
                (
                    data.as_ref().and_then(|a| a.modifiers.clone()),
                    data.map(|a| a.name),
                )
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                let data = self.arena.get_accessor(member_node);
                (
                    data.as_ref().and_then(|a| a.modifiers.clone()),
                    data.map(|a| a.name),
                )
            }
            _ => (None, None),
        };

        let Some(mods) = modifiers else {
            return member_node.pos as usize;
        };

        let ts_only_kinds: &[u16] = &[
            SyntaxKind::AbstractKeyword as u16,
            SyntaxKind::DeclareKeyword as u16,
            SyntaxKind::ReadonlyKeyword as u16,
            SyntaxKind::OverrideKeyword as u16,
            SyntaxKind::PublicKeyword as u16,
            SyntaxKind::PrivateKeyword as u16,
            SyntaxKind::ProtectedKeyword as u16,
            SyntaxKind::AccessorKeyword as u16,
        ];

        // Find the first JS-visible modifier (static, async, etc.)
        for &mod_idx in &mods.nodes {
            let Some(mod_node) = self.arena.get(mod_idx) else {
                continue;
            };
            if mod_node.kind != syntax_kind_ext::DECORATOR
                && !ts_only_kinds.contains(&mod_node.kind)
            {
                // JS-visible modifier - start from its position
                return mod_node.pos as usize;
            }
        }

        // All modifiers are decorators/TS-only.
        // Use the name node position as the reliable anchor, but for GET_ACCESSOR
        // and SET_ACCESSOR we must include the `get`/`set` keyword which precedes
        // the name in the source text and is NOT stored as a modifier.
        if let Some(idx) = name_idx
            && let Some(name_node) = self.arena.get(idx)
        {
            let name_pos = name_node.pos as usize;
            let is_accessor = member_node.kind == syntax_kind_ext::GET_ACCESSOR
                || member_node.kind == syntax_kind_ext::SET_ACCESSOR;
            if is_accessor && let Some(source) = self.source_text {
                // Scan backwards from name position to find 'get' or 'set' keyword
                let keyword = if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
                    "get"
                } else {
                    "set"
                };
                // Allow generous whitespace between keyword and name
                let search_start = name_pos.saturating_sub(keyword.len() + 20);
                // Look for the keyword in the text before the name
                if let Some(kw_offset) = source[search_start..name_pos].rfind(keyword) {
                    return search_start + kw_offset;
                }
            }
            return name_pos;
        }

        member_node.pos as usize
    }

    /// Find the position of the matching `]` for a string starting after `[`.
    /// Returns the index of `]` within the input string, handling nested brackets.
    fn find_matching_bracket(&self, s: &str) -> Option<usize> {
        let mut depth = 1;
        for (i, ch) in s.char_indices() {
            match ch {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn get_identifier_text(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            self.arena
                .get_identifier(node)
                .map(|id| id.escaped_text.clone())
        } else {
            None
        }
    }

    fn node_text(&self, idx: NodeIndex) -> String {
        let Some(node) = self.arena.get(idx) else {
            return String::new();
        };
        let Some(source) = self.source_text else {
            return String::new();
        };
        let start = node.pos as usize;
        let end = node.end as usize;
        if start < source.len() && end <= source.len() && start < end {
            source[start..end].trim().to_string()
        } else {
            String::new()
        }
    }

    fn collect_class_decorator_exprs(&self, modifiers: &Option<NodeList>) -> Vec<String> {
        let Some(mods) = modifiers else {
            return Vec::new();
        };
        let mut result = Vec::new();
        for &idx in &mods.nodes {
            let Some(node) = self.arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::DECORATOR
                && let Some(dec) = self.arena.get_decorator(node)
            {
                result.push(self.node_text(dec.expression));
            }
        }
        result
    }

    fn collect_decorated_members(&self, members: &NodeList) -> Vec<DecoratedMember> {
        let mut result = Vec::new();

        for &member_idx in &members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            let (modifiers, name_idx, kind) = match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    (method.modifiers.clone(), method.name, MemberKind::Method)
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    let kind = if self
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
                    {
                        MemberKind::Accessor
                    } else {
                        MemberKind::Field
                    };
                    (prop.modifiers.clone(), prop.name, kind)
                }
                k if k == syntax_kind_ext::GET_ACCESSOR => {
                    let Some(acc) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    (acc.modifiers.clone(), acc.name, MemberKind::Getter)
                }
                k if k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(acc) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    (acc.modifiers.clone(), acc.name, MemberKind::Setter)
                }
                _ => continue,
            };

            // Abstract and declare members have no runtime representation — skip them entirely.
            // tsc strips abstract/ambient decorated members from the decorator transform output.
            if self
                .arena
                .has_modifier(&modifiers, SyntaxKind::AbstractKeyword)
                || self
                    .arena
                    .has_modifier(&modifiers, SyntaxKind::DeclareKeyword)
            {
                continue;
            }

            // Collect decorator expressions
            let mut decorator_exprs = Vec::new();
            if let Some(ref mods) = modifiers {
                for &mod_idx in &mods.nodes {
                    let Some(mod_node) = self.arena.get(mod_idx) else {
                        continue;
                    };
                    if mod_node.kind == syntax_kind_ext::DECORATOR
                        && let Some(dec) = self.arena.get_decorator(mod_node)
                    {
                        decorator_exprs.push(self.node_text(dec.expression));
                    }
                }
            }
            if decorator_exprs.is_empty() {
                continue;
            }

            let is_static = self
                .arena
                .has_modifier(&modifiers, SyntaxKind::StaticKeyword);
            let (name, is_private) = self.resolve_member_name(name_idx);

            result.push(DecoratedMember {
                member_idx,
                kind,
                name,
                is_static,
                is_private,
                decorator_exprs,
            });
        }

        result
    }

    fn resolve_member_name(&self, name_idx: NodeIndex) -> (MemberName, bool) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return (MemberName::Identifier(String::new()), false);
        };

        match name_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                let text = self
                    .arena
                    .get_identifier(name_node)
                    .map(|id| id.escaped_text.clone())
                    .unwrap_or_default();
                (MemberName::Identifier(text), false)
            }
            k if k == SyntaxKind::PrivateIdentifier as u16 => {
                let text = self
                    .arena
                    .get_identifier(name_node)
                    .map(|id| id.escaped_text.clone())
                    .unwrap_or_default();
                (MemberName::Private(text), true)
            }
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                let Some(computed) = self.arena.get_computed_property(name_node) else {
                    return (MemberName::Identifier(String::new()), false);
                };
                // Check if computed expression is a string literal
                if let Some(expr_node) = self.arena.get(computed.expression)
                    && expr_node.kind == SyntaxKind::StringLiteral as u16
                    && let Some(lit) = self.arena.get_literal(expr_node)
                {
                    return (MemberName::StringLiteral(lit.text.clone()), false);
                }
                (MemberName::Computed(computed.expression), false)
            }
            _ => (MemberName::Identifier(String::new()), false),
        }
    }

    fn has_extends_clause(&self, heritage: &Option<NodeList>) -> bool {
        let Some(clauses) = heritage else {
            return false;
        };
        for &clause_idx in &clauses.nodes {
            let Some(clause_node) = self.arena.get(clause_idx) else {
                continue;
            };
            if let Some(h) = self.arena.get_heritage_clause(clause_node)
                && h.token == SyntaxKind::ExtendsKeyword as u16
            {
                return true;
            }
        }
        false
    }

    fn get_extends_text(&self, class_data: &tsz_parser::parser::node::ClassData) -> Option<String> {
        let clauses = class_data.heritage_clauses.as_ref()?;
        for &clause_idx in &clauses.nodes {
            let clause_node = self.arena.get(clause_idx)?;
            let heritage = self.arena.get_heritage_clause(clause_node)?;
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let first_type = heritage.types.nodes.first()?;
            let type_node = self.arena.get(*first_type)?;
            if let Some(expr_data) = self.arena.get_expr_type_args(type_node) {
                return Some(self.node_text(expr_data.expression));
            }
            return Some(self.node_text(*first_type));
        }
        None
    }

    fn compute_all_member_vars(&self, members: &[DecoratedMember]) -> Vec<MemberVarInfo> {
        let mut counter: u32 = 0;
        // Track the last seen computed/string member name to group getter/setter pairs.
        // tsc only increments the suffix counter between different member names.
        let mut last_computed_name: Option<String> = None;
        members
            .iter()
            .map(|m| self.compute_member_var_info(m, &mut counter, &mut last_computed_name))
            .collect()
    }

    fn compute_member_var_info(
        &self,
        member: &DecoratedMember,
        counter: &mut u32,
        last_computed_name: &mut Option<String>,
    ) -> MemberVarInfo {
        let base_name = match &member.name {
            MemberName::Identifier(name) => name.clone(),
            MemberName::Private(name) => format!("private_{}", name.trim_start_matches('#')),
            MemberName::StringLiteral(_) | MemberName::Computed(_) => "member".to_string(),
        };

        let prefix = if member.is_static { "static_" } else { "" };
        let kind_prefix = match member.kind {
            MemberKind::Getter => "get_",
            MemberKind::Setter => "set_",
            _ => "",
        };

        let var_base = format!("_{prefix}{kind_prefix}{base_name}");

        // For computed/string members, only increment counter on NEW member names.
        // Getter/setter pairs with the same name share the same suffix.
        let is_computed_or_string = matches!(
            member.name,
            MemberName::StringLiteral(_) | MemberName::Computed(_)
        );

        if is_computed_or_string {
            let current_name = match &member.name {
                MemberName::StringLiteral(s) => s.clone(),
                MemberName::Computed(idx) => self.node_text(*idx),
                _ => unreachable!(),
            };
            let is_new_name = last_computed_name
                .as_ref()
                .is_none_or(|prev| *prev != current_name);
            if is_new_name {
                if last_computed_name.is_some() {
                    *counter += 1;
                }
                *last_computed_name = Some(current_name);
            }
        }

        let suffix = if *counter > 0 && is_computed_or_string {
            format!("_{}", *counter)
        } else {
            String::new()
        };

        let decorators_var = format!("{var_base}_decorators{suffix}");
        let has_field_inits = matches!(member.kind, MemberKind::Field | MemberKind::Accessor);
        let has_descriptor = member.is_private && matches!(member.kind, MemberKind::Method);

        MemberVarInfo {
            decorators_var,
            has_initializers: has_field_inits,
            initializers_var: if has_field_inits {
                Some(format!("{var_base}_initializers{suffix}"))
            } else {
                None
            },
            extra_initializers_var: if has_field_inits {
                Some(format!("{var_base}_extraInitializers{suffix}"))
            } else {
                None
            },
            has_descriptor,
            descriptor_var: if has_descriptor {
                Some(format!("{var_base}_descriptor{suffix}"))
            } else {
                None
            },
        }
    }

    fn emit_es_decorate_call(
        &self,
        member: &DecoratedMember,
        var_info: &MemberVarInfo,
        class_alias: &str,
        computed_key_vars: &[(usize, String)],
        member_index: usize,
        indent: &str,
        out: &mut String,
    ) {
        let kind_str = match member.kind {
            MemberKind::Method => "method",
            MemberKind::Getter => "getter",
            MemberKind::Setter => "setter",
            MemberKind::Field => "field",
            MemberKind::Accessor => "accessor",
        };

        let name_str = self.member_name_for_context(member, computed_key_vars, member_index);
        let access_str = self.member_access_for_context(member, computed_key_vars, member_index);

        let is_field_like = matches!(member.kind, MemberKind::Field | MemberKind::Accessor);

        // For methods/getters/setters, first arg is the class reference.
        // For fields/accessors, first arg is always null.
        let ctor_arg = if is_field_like || member.is_private {
            "null".to_string()
        } else {
            class_alias.to_string()
        };

        // For fields/accessors, pass per-field initializer and extra-initializer arrays.
        // For methods/getters/setters, pass null + instance/static extra initializers.
        let (init_arg, extra_init_arg) = if is_field_like {
            let init = var_info.initializers_var.as_deref().unwrap_or("null");
            let extra = var_info.extra_initializers_var.as_deref().unwrap_or("null");
            (init.to_string(), extra.to_string())
        } else {
            let extra = if member.is_static {
                "_staticExtraInitializers"
            } else {
                "_instanceExtraInitializers"
            };
            ("null".to_string(), extra.to_string())
        };

        let es_decorate = self.helper("__esDecorate");
        out.push_str(&format!(
            "{indent}{es_decorate}({ctor_arg}, null, {}, {{ kind: \"{kind_str}\", name: {name_str}, static: {}, private: {}, access: {{ {access_str} }}, metadata: _metadata }}, {init_arg}, {extra_init_arg});\n",
            var_info.decorators_var,
            member.is_static,
            member.is_private,
        ));
    }

    fn member_name_for_context(
        &self,
        member: &DecoratedMember,
        computed_key_vars: &[(usize, String)],
        member_index: usize,
    ) -> String {
        match &member.name {
            MemberName::Identifier(name)
            | MemberName::StringLiteral(name)
            | MemberName::Private(name) => format!("\"{name}\""),
            MemberName::Computed(_) => computed_key_vars
                .iter()
                .find(|(i, _)| *i == member_index)
                .map(|(_, var)| var.clone())
                .unwrap_or_else(|| "undefined".to_string()),
        }
    }

    fn member_access_for_context(
        &self,
        member: &DecoratedMember,
        computed_key_vars: &[(usize, String)],
        member_index: usize,
    ) -> String {
        let key_expr = match &member.name {
            MemberName::Identifier(name) | MemberName::StringLiteral(name) => {
                format!("\"{name}\"")
            }
            MemberName::Private(name) => name.clone(),
            MemberName::Computed(_) => computed_key_vars
                .iter()
                .find(|(i, _)| *i == member_index)
                .map(|(_, var)| var.clone())
                .unwrap_or_else(|| "undefined".to_string()),
        };

        // Private fields use dot notation (obj.#field), same as regular identifiers
        let prop_access = match &member.name {
            MemberName::Identifier(name) | MemberName::Private(name) => format!("obj.{name}"),
            _ => format!("obj[{key_expr}]"),
        };

        let has_in = format!("{key_expr} in obj");

        match member.kind {
            MemberKind::Method | MemberKind::Getter => {
                format!("has: obj => {has_in}, get: obj => {prop_access}")
            }
            MemberKind::Setter => {
                format!("has: obj => {has_in}, set: (obj, value) => {{ {prop_access} = value; }}")
            }
            MemberKind::Field | MemberKind::Accessor => {
                format!(
                    "has: obj => {has_in}, get: obj => {prop_access}, set: (obj, value) => {{ {prop_access} = value; }}"
                )
            }
        }
    }

    fn collect_decorated_field_info(
        &self,
        decorated_members: &[DecoratedMember],
        computed_key_vars: &[(usize, String)],
    ) -> Vec<DecoratedFieldInfo> {
        let mut result = Vec::new();
        for (i, member) in decorated_members.iter().enumerate() {
            if member.kind != MemberKind::Field {
                continue;
            }
            let (access_expr, is_bracket) = match &member.name {
                MemberName::Identifier(name) | MemberName::Private(name) => (name.clone(), false),
                MemberName::StringLiteral(name) => (format!("\"{name}\""), true),
                MemberName::Computed(_) => {
                    let var = computed_key_vars
                        .iter()
                        .find(|(mi, _)| *mi == i)
                        .map(|(_, v)| v.clone())
                        .unwrap_or_else(|| "undefined".to_string());
                    (var, true)
                }
            };
            let initializer_text = self.get_field_initializer_text(member.member_idx);
            result.push(DecoratedFieldInfo {
                access_expr,
                is_bracket_access: is_bracket,
                initializer_text,
                member_var_index: i,
            });
        }
        result
    }

    fn get_field_initializer_text(&self, member_idx: NodeIndex) -> String {
        let Some(member_node) = self.arena.get(member_idx) else {
            return String::new();
        };
        let Some(prop) = self.arena.get_property_decl(member_node) else {
            return String::new();
        };
        if prop.initializer == NodeIndex::NONE {
            return String::new();
        }
        self.node_text(prop.initializer)
    }

    fn get_constructor_info(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> Option<ConstructorInfo> {
        for &member_idx in &class_data.members.nodes {
            let member_node = self.arena.get(member_idx)?;
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let ctor = self.arena.get_constructor(member_node)?;
            let source = self.source_text?;

            let params = if ctor.parameters.nodes.is_empty() {
                String::new()
            } else {
                let mut param_texts = Vec::new();
                for &param_idx in &ctor.parameters.nodes {
                    let param_node = self.arena.get(param_idx)?;
                    let param_data = self.arena.get_parameter(param_node)?;
                    let name_text = self.node_text(param_data.name);
                    if param_data.initializer.is_some() {
                        let init_text = self.node_text(param_data.initializer);
                        param_texts.push(format!("{name_text} = {init_text}"));
                    } else if param_data.dot_dot_dot_token {
                        param_texts.push(format!("...{name_text}"));
                    } else {
                        param_texts.push(name_text);
                    }
                }
                param_texts.join(", ")
            };

            if ctor.body == NodeIndex::NONE {
                return Some(ConstructorInfo {
                    params,
                    body_lines: Vec::new(),
                });
            }
            let body_node = self.arena.get(ctor.body)?;
            let block = self.arena.get_block(body_node)?;
            let mut body_lines = Vec::new();
            for &stmt_idx in &block.statements.nodes {
                let stmt_node = self.arena.get(stmt_idx)?;
                let start = stmt_node.pos as usize;
                let end = stmt_node.end as usize;
                if start < source.len() && end <= source.len() && start < end {
                    body_lines.push(source[start..end].trim().to_string());
                }
            }
            return Some(ConstructorInfo { params, body_lines });
        }
        None
    }
}

fn indent_str(level: usize) -> String {
    "    ".repeat(level)
}

fn next_temp_var(counter: &mut u32) -> String {
    let name = format!("_{}", (b'a' + (*counter % 26) as u8) as char);
    *counter += 1;
    name
}

struct MemberVarInfo {
    decorators_var: String,
    has_initializers: bool,
    initializers_var: Option<String>,
    extra_initializers_var: Option<String>,
    has_descriptor: bool,
    descriptor_var: Option<String>,
}

struct ConstructorInfo {
    params: String,
    body_lines: Vec<String>,
}

#[cfg(test)]
#[path = "../../tests/es_decorators.rs"]
mod tests;
