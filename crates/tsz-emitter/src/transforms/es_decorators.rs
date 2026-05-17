//! TC39 (non-legacy) Decorator Transform
//!
//! Transforms decorated classes using the TC39 decorator protocol.
//! For ES2015 targets, outputs an IIFE with comma-separated decorator application.
//! For ES2022+ targets, uses static initializer blocks.

use rustc_hash::FxHashMap;
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

#[path = "es_decorators_helpers.rs"]
mod helpers;
use helpers::*;

use crate::transforms::emit_utils::hygienic_temp_name;

/// TC39 Decorator Emitter
pub struct TC39DecoratorEmitter<'a> {
    arena: &'a NodeArena,
    source_text: Option<&'a str>,
    indent: usize,
    /// When true, uses `static { }` blocks (ES2022+) instead of IIFE pattern (ES2015).
    use_static_blocks: bool,
    /// When true, prefix helper calls with `tslib_1.` (importHelpers + commonjs).
    tslib_prefix: bool,
    tslib_import_binding: String,
    /// When true, emit as an expression (no `let C = ` wrapper) for class expressions.
    expression_mode: bool,
    /// Function name for class expression named evaluation (__setFunctionName).
    function_name: Option<String>,
    /// Runtime temp used for anonymous decorated class expressions.
    anonymous_class_name: Option<String>,
    /// Function body text rendered by the main emitter before this transform
    /// assembles descriptor/externalized function expressions.
    function_body_texts: FxHashMap<NodeIndex, String>,
    /// When true, decorated fields stay as class field declarations (ES2022+).
    /// When false, decorated fields move to constructor assignments.
    use_define_for_class_fields: bool,
}

impl<'a> TC39DecoratorEmitter<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            source_text: None,
            indent: 0,
            use_static_blocks: false,
            tslib_prefix: false,
            tslib_import_binding: "tslib_1".to_string(),
            expression_mode: false,
            function_name: None,
            anonymous_class_name: None,
            function_body_texts: FxHashMap::default(),
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

    pub fn set_tslib_import_binding(&mut self, binding: String) {
        self.tslib_import_binding = binding;
    }

    pub const fn set_expression_mode(&mut self, expr: bool) {
        self.expression_mode = expr;
    }

    /// Set the function name for class expression named evaluation.
    /// Used for `__setFunctionName(_classThis, name)` in ES2022 mode.
    pub fn set_function_name(&mut self, name: String) {
        self.function_name = Some(name);
    }

    pub fn set_anonymous_class_name(&mut self, name: String) {
        self.anonymous_class_name = Some(name);
    }

    pub fn set_function_body_text(&mut self, body_idx: NodeIndex, text: String) {
        self.function_body_texts.insert(body_idx, text);
    }

    pub const fn set_use_define_for_class_fields(&mut self, use_define: bool) {
        self.use_define_for_class_fields = use_define;
    }

    /// Returns the helper function name with optional tslib prefix.
    fn helper(&self, name: &str) -> String {
        if self.tslib_prefix {
            format!("{}.{name}", self.tslib_import_binding)
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
        let class_name_was_empty = class_name.is_empty();
        // For anonymous class expressions WITH class decorators, generate a temp name
        // (needed for the var assignment pattern). Without class decorators, keep anonymous.
        let class_name = if class_name.is_empty() && !class_decorators.is_empty() {
            self.anonymous_class_name
                .clone()
                .unwrap_or_else(|| "class_1".to_string())
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

        // tsc avoids shadowing user bindings inside the transformed class wrapper
        // by suffixing decorator temporaries that collide with identifiers used
        // anywhere in the class span (decorators, name, extends, body). Without
        // this rename, e.g. a class body referring to a user `const _classDescriptor`
        // would resolve to the generated temp instead. See issue #3091.
        let class_span_text = self
            .source_text
            .map(|src| {
                let start = class_node.pos as usize;
                let end = (class_node.end as usize).min(src.len());
                if start <= end { &src[start..end] } else { "" }
            })
            .unwrap_or("");
        let class_descriptor_var = hygienic_temp_name("_classDescriptor", class_span_text);
        let class_extra_initializers_var =
            hygienic_temp_name("_classExtraInitializers", class_span_text);
        let class_this_var = hygienic_temp_name("_classThis", class_span_text);
        let class_super_var = hygienic_temp_name("_classSuper", class_span_text);
        let class_decorators_var = hygienic_temp_name("_classDecorators", class_span_text);
        let metadata_var = hygienic_temp_name("_metadata", class_span_text);
        let instance_extra_initializers_var =
            hygienic_temp_name("_instanceExtraInitializers", class_span_text);
        let static_extra_initializers_var =
            hygienic_temp_name("_staticExtraInitializers", class_span_text);

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
        let decorated_auto_accessor_infos =
            self.collect_decorated_auto_accessor_info(&decorated_members, &computed_key_vars);
        let class_decorator_static_private_methods =
            if has_class_decorators && self.use_static_blocks {
                self.collect_class_decorator_static_private_methods(
                    class_data,
                    &class_name,
                    &decorated_members,
                    class_span_text,
                )
            } else {
                Vec::new()
            };
        let auto_accessor_storage_decls: Vec<String> = if self.use_static_blocks {
            Vec::new()
        } else {
            decorated_auto_accessor_infos
                .iter()
                .map(|info| format!("_{class_name}_{}_accessor_storage", info.storage_base))
                .collect()
        };

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
            let mut var_names = vec![class_alias.as_str()];
            var_names.extend(auto_accessor_storage_decls.iter().map(String::as_str));
            out.push_str(&format!("{i1}var {};\n", var_names.join(", ")));
        } else if !auto_accessor_storage_decls.is_empty() {
            out.push_str(&format!(
                "{i1}var {};\n",
                auto_accessor_storage_decls.join(", ")
            ));
        }
        if !computed_key_vars.is_empty() {
            let key_names: Vec<&str> = computed_key_vars.iter().map(|(_, v)| v.as_str()).collect();
            out.push_str(&format!("{i1}var {};\n", key_names.join(", ")));
        }
        if !class_decorator_static_private_methods.is_empty() {
            let method_names: Vec<&str> = class_decorator_static_private_methods
                .iter()
                .map(|info| info.temp_var.as_str())
                .collect();
            out.push_str(&format!("{i1}var {};\n", method_names.join(", ")));
        }

        // Class decorator variables
        if !class_decorators.is_empty() {
            out.push_str(&format!(
                "{i1}let {class_decorators_var} = [{}];\n",
                class_decorators.join(", ")
            ));
            out.push_str(&format!("{i1}let {class_descriptor_var};\n"));
            out.push_str(&format!("{i1}let {class_extra_initializers_var} = [];\n"));
            out.push_str(&format!("{i1}let {class_this_var};\n"));
            // When a decorated class extends a base class, tsc captures the super class
            // in a _classSuper variable so it can be used for metadata and super access.
            if has_extends && let Some(extends_text) = self.get_extends_text(class_data) {
                out.push_str(&format!("{i1}let {class_super_var} = {extends_text};\n"));
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
            out.push_str(&format!(
                "{i1}let {instance_extra_initializers_var} = [];\n"
            ));
        }
        if has_static_method {
            out.push_str(&format!("{i1}let {static_extra_initializers_var} = [];\n"));
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
            class_this_var.clone()
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
            class_this_var.clone()
        } else {
            class_alias.clone()
        };

        // --- Class expression ---
        if has_class_decorators {
            // With class decorators: `var C = _classThis = class {` (ES2015) or `var C = class {` (ES2022)
            if self.use_static_blocks {
                out.push_str(&format!("{i1}var {class_name} = class"));
            } else {
                out.push_str(&format!("{i1}var {class_name} = {class_this_var} = class"));
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
                out.push_str(&format!(" extends {class_super_var}"));
            } else if let Some(extends_text) = self.get_extends_text(class_data) {
                out.push_str(&format!(" extends {extends_text}"));
            }
        }
        out.push_str(" {\n");

        if self.use_static_blocks {
            // ES2022: with class decorators, emit _classThis capture block first
            if has_class_decorators {
                out.push_str(&format!("{i2}static {{ {class_this_var} = this; }}\n"));
                // For class expressions, emit `__setFunctionName(_classThis, ...)`
                // only when the source class was *anonymous*. A named class
                // expression (`class C { ... }`) carries its own name through
                // to the engine — tsc does not emit the helper in that case
                // (e.g. `export const C = @dec class C {}` round-trips to a
                // bare `var C = class { static { _classThis = this; } ... }`
                // with no `__setFunctionName` static block).
                if self.expression_mode && class_name_was_empty {
                    let fn_name = self.function_name.clone().unwrap_or_default();
                    let set_fn = self.helper("__setFunctionName");
                    out.push_str(&format!(
                        "{i2}static {{ {set_fn}({class_this_var}, \"{fn_name}\"); }}\n"
                    ));
                } else if !self.expression_mode
                    && !class_name.is_empty()
                    && (class_name_was_empty || !class_decorator_static_private_methods.is_empty())
                {
                    let set_fn = self.helper("__setFunctionName");
                    out.push_str(&format!(
                        "{i2}static {{ {set_fn}(this, \"{class_name}\"); }}\n"
                    ));
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
            for info in &class_decorator_static_private_methods {
                out.push_str(&format!(
                    "{i2}static {{ {} = function {}({}) {}; }}\n",
                    info.temp_var, info.function_name, info.params, info.body
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
            let defer_class_init_inner =
                has_class_decorators && self.has_user_static_members(&class_data.members);
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
                defer_class_init_inner,
                &class_descriptor_var,
                &class_this_var,
                &class_super_var,
                &class_decorators_var,
                &class_extra_initializers_var,
                &instance_extra_initializers_var,
                &static_extra_initializers_var,
                &metadata_var,
            );
            out.push_str(&format!("{i2}}}\n"));
        }

        let defer_class_init =
            has_class_decorators && self.has_user_static_members(&class_data.members);

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
            &class_name,
            member_indent,
            member_inner_indent,
            &mut out,
            defer_class_init,
            &class_this_var,
            &class_extra_initializers_var,
            &instance_extra_initializers_var,
            &class_decorator_static_private_methods,
        );

        if self.use_static_blocks {
            // ES2022: close class body
            out.push_str(&format!("{i1}}};\n"));
            if has_class_decorators {
                // With class decorators: return C = _classThis after the class
                out.push_str(&format!("{i1}return {class_name} = {class_this_var};\n"));
            }
        } else if has_class_decorators {
            // ES2015 + class decorators: separate statements pattern
            // Close class with semicolon (it's a var assignment, not a return)
            out.push_str(&format!("{i1}}};\n"));

            // __setFunctionName
            let set_fn_name = self.helper("__setFunctionName");
            let set_function_name = if self.expression_mode && class_name_was_empty {
                self.function_name.as_deref().unwrap_or(&class_name)
            } else {
                &class_name
            };
            out.push_str(&format!(
                "{i1}{set_fn_name}({class_this_var}, \"{set_function_name}\");\n"
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
                defer_class_init,
                &class_descriptor_var,
                &class_this_var,
                &class_super_var,
                &class_decorators_var,
                &class_extra_initializers_var,
                &instance_extra_initializers_var,
                &static_extra_initializers_var,
                &metadata_var,
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
            out.push_str(&format!("{i1}return {class_name} = {class_this_var};\n"));
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
                false,
                &class_descriptor_var,
                &class_this_var,
                &class_super_var,
                &class_decorators_var,
                &class_extra_initializers_var,
                &instance_extra_initializers_var,
                &static_extra_initializers_var,
                &metadata_var,
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
        defer_class_extra_init: bool,
        class_descriptor: &str,
        class_this_var: &str,
        class_super_var: &str,
        class_decorators_var: &str,
        class_extra_initializers_var: &str,
        instance_extra_initializers_var: &str,
        static_extra_initializers_var: &str,
        metadata_var: &str,
    ) {
        // Metadata
        let has_class_decorators = !class_decorators.is_empty();
        if has_extends {
            // When class decorators are present, use _classSuper alias; otherwise use extends text directly
            let super_ref = if has_class_decorators {
                Some(class_super_var.to_string())
            } else {
                self.get_extends_text(class_data)
            };
            if let Some(super_ref) = super_ref {
                out.push_str(&format!("{indent}const {metadata_var} = typeof Symbol === \"function\" && Symbol.metadata ? Object.create({super_ref}[Symbol.metadata] ?? null) : void 0;\n"));
            } else {
                out.push_str(&format!("{indent}const {metadata_var} = typeof Symbol === \"function\" && Symbol.metadata ? Object.create(null) : void 0;\n"));
            }
        } else {
            out.push_str(&format!("{indent}const {metadata_var} = typeof Symbol === \"function\" && Symbol.metadata ? Object.create(null) : void 0;\n"));
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
            // ES2015: assignments without a computed-name sink go in the IIFE.
            !class_decorators.is_empty()
                || (!has_computed_field_keys_app && !decorated_members.is_empty())
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
                instance_extra_initializers_var,
                static_extra_initializers_var,
                metadata_var,
            );
        }

        // Class-level __esDecorate if needed
        let es_decorate = self.helper("__esDecorate");
        let run_initializers = self.helper("__runInitializers");
        if !class_decorators.is_empty() {
            out.push_str(&format!("{indent}{es_decorate}(null, {class_descriptor} = {{ value: {class_this_var} }}, {class_decorators_var}, {{ kind: \"class\", name: {class_this_var}.name, metadata: {metadata_var} }}, null, {class_extra_initializers_var});\n"));
            out.push_str(&format!(
                "{indent}{class_name} = {class_this_var} = {class_descriptor}.value;\n"
            ));
        }

        // Metadata assignment
        out.push_str(&format!("{indent}if ({metadata_var}) Object.defineProperty({ctor_ref}, Symbol.metadata, {{ enumerable: true, configurable: true, writable: true, value: {metadata_var} }});\n"));

        // Static extra initializers — only for static method/getter/setter decorators
        let has_static_method_decorators = decorated_members
            .iter()
            .any(|m| m.is_static && !matches!(m.kind, MemberKind::Field | MemberKind::Accessor));
        if has_static_method_decorators {
            out.push_str(&format!(
                "{indent}{run_initializers}({ctor_ref}, {static_extra_initializers_var});\n"
            ));
        }

        // Class extra initializers: deferred when user static members exist.
        if !class_decorators.is_empty() && !defer_class_extra_init {
            out.push_str(&format!(
                "{indent}{run_initializers}({ctor_ref}, {class_extra_initializers_var});\n"
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
        class_name: &str,
        indent: &str,
        inner_indent: &str,
        out: &mut String,
        defer_class_extra_init: bool,
        class_this_var: &str,
        class_extra_initializers_var: &str,
        instance_extra_initializers_var: &str,
        class_decorator_static_private_methods: &[ClassDecoratorStaticPrivateMethodInfo],
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
        let decorated_auto_accessor_idx_set: std::collections::HashSet<NodeIndex> =
            decorated_members
                .iter()
                .filter(|m| m.kind == MemberKind::Accessor)
                .map(|m| m.member_idx)
                .collect();
        let class_decorator_static_private_method_map: std::collections::HashMap<
            NodeIndex,
            &ClassDecoratorStaticPrivateMethodInfo,
        > = class_decorator_static_private_methods
            .iter()
            .map(|info| (info.member_idx, info))
            .collect();
        let field_infos = self.collect_decorated_field_info(decorated_members, computed_key_vars);
        let auto_accessor_infos =
            self.collect_decorated_auto_accessor_info(decorated_members, computed_key_vars);
        let parameter_properties = self.collect_constructor_parameter_properties(class_data);
        let has_parameter_properties = !parameter_properties.is_empty();
        let source_ctor = self.get_constructor_info(class_data);
        let has_instance_fields = field_infos
            .iter()
            .any(|fi| !decorated_members[fi.member_var_index].is_static);
        let has_instance_auto_accessors = auto_accessor_infos
            .iter()
            .any(|info| !decorated_members[info.member_var_index].is_static);
        let has_instance_method = decorated_members
            .iter()
            .any(|m| !m.is_static && !matches!(m.kind, MemberKind::Field | MemberKind::Accessor));
        let needs_ctor = source_ctor.is_some() || has_any_instance || has_parameter_properties;
        let constructor_output = if needs_ctor {
            Some(self.render_decorated_constructor(
                source_ctor.as_ref(),
                &parameter_properties,
                &field_infos,
                &auto_accessor_infos,
                decorated_members,
                member_vars,
                fields_in_class_body,
                has_instance_fields,
                has_instance_auto_accessors,
                has_instance_method,
                class_name,
                self.has_extends_clause(&class_data.heritage_clauses),
                indent,
                inner_indent,
                instance_extra_initializers_var,
            ))
        } else {
            None
        };

        let all_members: Vec<_> = class_data
            .members
            .nodes
            .iter()
            .filter_map(|&idx| self.arena.get(idx).map(|n| (idx, n)))
            .collect();

        let mut plain_static_field_idx_set: std::collections::HashSet<NodeIndex> =
            std::collections::HashSet::new();
        let mut plain_static_field_assignments: Vec<String> = Vec::new();
        if !self.use_static_blocks {
            for (member_idx, member_node) in &all_members {
                if decorated_field_idx_set.contains(member_idx) {
                    continue;
                }
                let Some(assignment) =
                    self.plain_static_field_assignment(member_node, _class_alias, indent)
                else {
                    continue;
                };
                plain_static_field_idx_set.insert(*member_idx);
                plain_static_field_assignments.push(assignment);
            }
        }

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
                node.kind != syntax_kind_ext::INDEX_SIGNATURE
                    && node.kind != syntax_kind_ext::SEMICOLON_CLASS_ELEMENT
                    && (fields_in_class_body || !decorated_field_idx_set.contains(idx))
                    && !plain_static_field_idx_set.contains(idx)
            })
            .map(|(i, _)| i)
            .collect();

        let class_close = self.find_class_close_brace(class_node);
        for &emit_i in &emittable {
            let (member_idx, member_node) = all_members[emit_i];
            if member_node.kind == syntax_kind_ext::CONSTRUCTOR {
                if let Some(output) = &constructor_output {
                    out.push_str(output);
                }
                continue;
            }
            if let Some(info) = class_decorator_static_private_method_map.get(&member_idx) {
                if info.needs_wrapper {
                    out.push_str(&format!(
                        "{indent}static get {}() {{ return {}; }}\n",
                        info.member_name, info.temp_var
                    ));
                }
                continue;
            }
            let next_boundary = if emit_i + 1 < all_members.len() {
                all_members[emit_i + 1].1.pos as usize
            } else {
                class_close
            };
            let member_text = self.emit_member_bounded(member_node, next_boundary.min(class_close));

            let is_decorated_field =
                fields_in_class_body && decorated_field_idx_set.contains(&member_idx);
            let is_decorated_auto_accessor = decorated_auto_accessor_idx_set.contains(&member_idx);
            let private_decorated_member_index = decorated_members.iter().position(|member| {
                member.member_idx == member_idx
                    && member.is_private
                    && self.use_static_blocks
                    && matches!(
                        member.kind,
                        MemberKind::Method | MemberKind::Getter | MemberKind::Setter
                    )
            });

            if let Some(member_var_index) = private_decorated_member_index {
                let member = &decorated_members[member_var_index];
                let var_info = &member_vars[member_var_index];
                if let Some(assignments) = injected_assignments.get(&member_idx) {
                    let injected = assignments.join(", ");
                    out.push_str(&format!("{indent}static {{ {injected}; }}\n"));
                }
                self.emit_private_decorated_member_wrapper(member, var_info, indent, out);
            } else if is_decorated_auto_accessor {
                if let Some(info) = auto_accessor_infos
                    .iter()
                    .find(|info| decorated_members[info.member_var_index].member_idx == member_idx)
                {
                    let member = &decorated_members[info.member_var_index];
                    let var_info = &member_vars[info.member_var_index];
                    self.emit_decorated_auto_accessor_member(
                        member,
                        info,
                        var_info,
                        injected_assignments.get(&member_idx).map(Vec::as_slice),
                        class_name,
                        _class_alias,
                        indent,
                        out,
                    );
                }
            } else if is_decorated_field {
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
                    push_indented_lines(out, indent, &member_text);
                }
            } else if let Some(assignments) = injected_assignments.get(&member_idx) {
                let injected = assignments.join(", ");
                if let Some(bracket_start) = member_text.find('[') {
                    let before = &member_text[..bracket_start + 1];
                    let after = &member_text[bracket_start + 1..];
                    if let Some(bracket_end) = self.find_matching_bracket(after) {
                        let rest = &after[bracket_end + 1..];
                        push_indented_lines(out, indent, &format!("{before}({injected})]{rest}"));
                    } else {
                        push_indented_lines(out, indent, &format!("{before}({injected})]() {{ }}"));
                    }
                } else {
                    push_indented_lines(out, indent, &member_text);
                }
            } else {
                push_indented_lines(out, indent, &member_text);
            }
        }
        if source_ctor.is_none()
            && let Some(output) = &constructor_output
        {
            out.push_str(output);
        }

        // Handle remaining assignments
        let mut external_assignments: Vec<String> = Vec::new();
        let mut post_iife_assignments: Vec<String> = Vec::new();
        post_iife_assignments.extend(plain_static_field_assignments);
        if !self.use_static_blocks {
            for info in auto_accessor_infos
                .iter()
                .filter(|info| !decorated_members[info.member_var_index].is_static)
            {
                external_assignments.push(format!(
                    "{} = new WeakMap()",
                    self.auto_accessor_weakmap_storage_name(class_name, info)
                ));
            }
        }
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
            if !remaining_assignments.is_empty() && !computed_key_vars.is_empty() {
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

        if !self.use_static_blocks {
            for info in auto_accessor_infos
                .iter()
                .filter(|info| decorated_members[info.member_var_index].is_static)
            {
                let member = &decorated_members[info.member_var_index];
                let var_info = &member_vars[info.member_var_index];
                let init_var = var_info.initializers_var.as_deref().unwrap_or("_init");
                let init_arg = self.auto_accessor_initializer_arg(info);
                let storage_name = self.auto_accessor_weakmap_storage_name(class_name, info);
                let mut assignment = String::new();
                if let Some(comment) = self.leading_member_comments(member.member_idx, indent) {
                    assignment.push_str(&comment);
                    assignment.push('\n');
                    assignment.push_str(indent);
                }
                assignment.push_str(&format!(
                    "{storage_name} = {{ value: {run_init}({_class_alias}, {init_var}{init_arg}) }}"
                ));
                post_iife_assignments.push(assignment);
                if let Some(extra_var) = var_info.extra_initializers_var.as_deref() {
                    post_iife_assignments.push(format!(
                        "__EXTRA_INIT_IIFE__:{run_init}({_class_alias}, {extra_var})"
                    ));
                }
            }
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

        // ES2022 + class decorators: deferred __runInitializers static block
        if defer_class_extra_init {
            if self.use_static_blocks {
                out.push_str(&format!(
                    "{indent}static {{\n{inner_indent}{run_init}({class_this_var}, {class_extra_initializers_var});\n{indent}}}\n"
                ));
            } else {
                post_iife_assignments.push(format!(
                    "__EXTRA_INIT_IIFE__:{run_init}({class_this_var}, {class_extra_initializers_var})"
                ));
            }
        }

        if self.use_static_blocks
            && let Some(info) = auto_accessor_infos
                .iter()
                .rev()
                .find(|info| decorated_members[info.member_var_index].is_static)
            && let Some(extra_var) = member_vars[info.member_var_index]
                .extra_initializers_var
                .as_deref()
        {
            out.push_str(&format!(
                "{indent}static {{\n{inner_indent}{run_init}(this, {extra_var});\n{indent}}}\n"
            ));
        }

        (external_assignments, post_iife_assignments)
    }

    #[allow(clippy::too_many_arguments)]
    fn render_decorated_constructor(
        &self,
        source_ctor: Option<&ConstructorInfo>,
        parameter_properties: &[ParameterPropertyInfo],
        field_infos: &[DecoratedFieldInfo],
        auto_accessor_infos: &[DecoratedAutoAccessorInfo],
        decorated_members: &[DecoratedMember],
        member_vars: &[MemberVarInfo],
        fields_in_class_body: bool,
        has_instance_fields: bool,
        has_instance_auto_accessors: bool,
        has_instance_method: bool,
        class_name: &str,
        has_extends: bool,
        indent: &str,
        inner_indent: &str,
        instance_extra_initializers_var: &str,
    ) -> String {
        let run_init = self.helper("__runInitializers");
        let parameter_properties_run_instance_initializers =
            has_instance_method && !parameter_properties.is_empty();
        let mut output = String::new();
        let mut ctor_init_calls: Vec<String> = Vec::new();

        if self.use_static_blocks && self.use_define_for_class_fields {
            for (idx, prop) in parameter_properties.iter().enumerate() {
                if idx == 0 && parameter_properties_run_instance_initializers {
                    output.push_str(&format!(
                        "{indent}{} = {run_init}(this, {instance_extra_initializers_var});\n",
                        prop.name
                    ));
                } else {
                    output.push_str(&format!("{indent}{};\n", prop.name));
                }
                ctor_init_calls.push(format!("{inner_indent}this.{0} = {0};\n", prop.name));
            }
        } else {
            for (idx, prop) in parameter_properties.iter().enumerate() {
                let value = if idx == 0 && parameter_properties_run_instance_initializers {
                    format!(
                        "({run_init}(this, {instance_extra_initializers_var}), {})",
                        prop.name
                    )
                } else {
                    prop.name.clone()
                };
                if self.use_define_for_class_fields {
                    ctor_init_calls.push(format!(
                        "{inner_indent}Object.defineProperty(this, \"{}\", {{\n{inner_indent}    enumerable: true,\n{inner_indent}    configurable: true,\n{inner_indent}    writable: true,\n{inner_indent}    value: {value}\n{inner_indent}}});\n",
                        prop.name
                    ));
                } else {
                    ctor_init_calls.push(format!("{inner_indent}this.{0} = {value};\n", prop.name));
                }
            }
        }

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
        } else if has_instance_method && !parameter_properties_run_instance_initializers {
            ctor_init_calls.push(format!(
                "{inner_indent}{run_init}(this, {instance_extra_initializers_var});\n"
            ));
        }

        if has_instance_auto_accessors {
            if self.use_static_blocks {
                if let Some(info) = auto_accessor_infos
                    .iter()
                    .rev()
                    .find(|info| !decorated_members[info.member_var_index].is_static)
                    && let Some(extra_var) = member_vars[info.member_var_index]
                        .extra_initializers_var
                        .as_deref()
                {
                    ctor_init_calls.push(format!("{inner_indent}{run_init}(this, {extra_var});\n"));
                }
            } else {
                for info in auto_accessor_infos
                    .iter()
                    .filter(|info| !decorated_members[info.member_var_index].is_static)
                {
                    let var_info = &member_vars[info.member_var_index];
                    let init_var = var_info.initializers_var.as_deref().unwrap_or("_init");
                    let init_arg = self.auto_accessor_initializer_arg(info);
                    let storage_name = self.auto_accessor_weakmap_storage_name(class_name, info);
                    ctor_init_calls.push(format!(
                        "{inner_indent}{storage_name}.set(this, {run_init}(this, {init_var}{init_arg}));\n"
                    ));
                }
                if let Some(info) = auto_accessor_infos
                    .iter()
                    .rev()
                    .find(|info| !decorated_members[info.member_var_index].is_static)
                    && let Some(extra_var) = member_vars[info.member_var_index]
                        .extra_initializers_var
                        .as_deref()
                {
                    ctor_init_calls.push(format!("{inner_indent}{run_init}(this, {extra_var});\n"));
                }
            }
        }

        output.push_str(&format!("{indent}constructor("));
        if let Some(ctor) = source_ctor {
            output.push_str(&ctor.params);
            output.push_str(") {\n");
            let split_at = if has_extends {
                ctor.body_lines
                    .iter()
                    .position(|line| line.contains("super("))
                    .map_or(0, |idx| idx + 1)
            } else {
                0
            };
            for line in &ctor.body_lines[..split_at] {
                output.push_str(&format!("{inner_indent}{}\n", line.trim()));
            }
            for call in &ctor_init_calls {
                output.push_str(call);
            }
            for line in &ctor.body_lines[split_at..] {
                output.push_str(&format!("{inner_indent}{}\n", line.trim()));
            }
            output.push_str(&format!("{indent}}}\n"));
        } else {
            output.push_str(") {\n");
            for call in &ctor_init_calls {
                output.push_str(call);
            }
            output.push_str(&format!("{indent}}}\n"));
        }
        output
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_decorated_auto_accessor_member(
        &self,
        member: &DecoratedMember,
        info: &DecoratedAutoAccessorInfo,
        var_info: &MemberVarInfo,
        injected_assignments: Option<&[String]>,
        class_name: &str,
        class_alias: &str,
        indent: &str,
        out: &mut String,
    ) {
        let run_init = self.helper("__runInitializers");
        let init_var = var_info
            .initializers_var
            .as_deref()
            .unwrap_or("_initializers");
        let init_arg = self.auto_accessor_initializer_arg(info);
        let getter_name = self.auto_accessor_member_name(member, info, injected_assignments);
        let setter_name = self.auto_accessor_member_name(member, info, None);
        let static_prefix = if member.is_static { "static " } else { "" };

        if self.use_static_blocks {
            let storage_name = self.native_auto_accessor_storage_name(info);
            out.push_str(&format!(
                "{indent}{static_prefix}{storage_name} = {run_init}(this, {init_var}{init_arg});\n"
            ));

            if let Some(comment) = self.leading_member_comments(member.member_idx, indent) {
                out.push_str(&comment);
                out.push('\n');
            }

            if member.is_private {
                let descriptor_var = var_info.descriptor_var.as_deref().unwrap_or("_descriptor");
                out.push_str(&format!(
                    "{indent}{static_prefix}get {getter_name}() {{ return {descriptor_var}.get.call(this); }}\n"
                ));
                out.push_str(&format!(
                    "{indent}{static_prefix}set {setter_name}(value) {{ return {descriptor_var}.set.call(this, value); }}\n"
                ));
                return;
            }

            if member.is_static {
                let class_ref = if class_name.is_empty() {
                    "this"
                } else {
                    class_name
                };
                out.push_str(&format!(
                    "{indent}static get {getter_name}() {{ return {class_ref}.{storage_name}; }}\n"
                ));
                out.push_str(&format!(
                    "{indent}static set {setter_name}(value) {{ {class_ref}.{storage_name} = value; }}\n"
                ));
            } else {
                out.push_str(&format!(
                    "{indent}get {getter_name}() {{ return this.{storage_name}; }}\n"
                ));
                out.push_str(&format!(
                    "{indent}set {setter_name}(value) {{ this.{storage_name} = value; }}\n"
                ));
            }
            return;
        }

        if let Some(comment) = self.leading_member_comments(member.member_idx, indent) {
            out.push_str(&comment);
            out.push('\n');
        }

        let storage_name = self.auto_accessor_weakmap_storage_name(class_name, info);
        let get_helper = self.helper("__classPrivateFieldGet");
        let set_helper = self.helper("__classPrivateFieldSet");
        if member.is_static {
            out.push_str(&format!(
                "{indent}static get {getter_name}() {{ return {get_helper}({class_alias}, {class_alias}, \"f\", {storage_name}); }}\n"
            ));
            out.push_str(&format!(
                "{indent}static set {setter_name}(value) {{ {set_helper}({class_alias}, {class_alias}, value, \"f\", {storage_name}); }}\n"
            ));
        } else {
            out.push_str(&format!(
                "{indent}get {getter_name}() {{ return {get_helper}(this, {storage_name}, \"f\"); }}\n"
            ));
            out.push_str(&format!(
                "{indent}set {setter_name}(value) {{ {set_helper}(this, {storage_name}, value, \"f\"); }}\n"
            ));
        }
    }

    fn auto_accessor_member_name(
        &self,
        member: &DecoratedMember,
        info: &DecoratedAutoAccessorInfo,
        injected_assignments: Option<&[String]>,
    ) -> String {
        match &member.name {
            MemberName::Computed(_) => {
                if let Some(assignments) = injected_assignments
                    && !assignments.is_empty()
                {
                    return format!("[({})]", assignments.join(", "));
                }
                format!("[{}]", info.name)
            }
            _ => info.name.clone(),
        }
    }

    fn native_auto_accessor_storage_name(&self, info: &DecoratedAutoAccessorInfo) -> String {
        format!("#{}_accessor_storage", info.storage_base)
    }

    fn auto_accessor_weakmap_storage_name(
        &self,
        class_name: &str,
        info: &DecoratedAutoAccessorInfo,
    ) -> String {
        let class_prefix = if class_name.is_empty() {
            "class"
        } else {
            class_name
        };
        format!("_{class_prefix}_{}_accessor_storage", info.storage_base)
    }

    fn auto_accessor_initializer_arg(&self, info: &DecoratedAutoAccessorInfo) -> String {
        if info.initializer_text.is_empty() {
            ", void 0".to_string()
        } else {
            format!(", {}", info.initializer_text)
        }
    }

    fn leading_member_comments(&self, member_idx: NodeIndex, indent: &str) -> Option<String> {
        let member_node = self.arena.get(member_idx)?;
        let source = self.source_text?;
        let start = member_node.pos as usize;
        if start >= source.len() {
            return None;
        }

        let mut comments: Vec<String> = Vec::new();
        for line in source[..start].lines().rev() {
            let line = line.trim();
            if line.is_empty() {
                if comments.is_empty() {
                    continue;
                }
                break;
            }
            if is_comment_line(line) {
                comments.push(line.to_string());
                continue;
            }
            break;
        }
        if !comments.is_empty() {
            comments.reverse();
            return Some(comments.join(&format!("\n{indent}")));
        }

        let end = self.find_member_clean_start(member_node).min(source.len());
        if start >= end {
            return None;
        }

        let comments: Vec<String> = source[start..end]
            .lines()
            .map(str::trim)
            .filter(|line| is_comment_line(line))
            .map(ToOwned::to_owned)
            .collect();
        if comments.is_empty() {
            None
        } else {
            Some(comments.join(&format!("\n{indent}")))
        }
    }

    fn has_user_static_members(&self, members: &NodeList) -> bool {
        for &idx in &members.nodes {
            let Some(node) = self.arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                return true;
            }
            if node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                && let Some(prop) = self.arena.get_property_decl(node)
                && self.arena.is_static(&prop.modifiers)
            {
                return true;
            }
        }
        false
    }

    fn plain_static_field_assignment(
        &self,
        member_node: &tsz_parser::parser::node::Node,
        class_ref: &str,
        indent: &str,
    ) -> Option<String> {
        if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
            return None;
        }
        let prop = self.arena.get_property_decl(member_node)?;
        if !self.arena.is_static(&prop.modifiers)
            || self
                .arena
                .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
            || self
                .arena
                .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
            || self
                .arena
                .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
        {
            return None;
        }

        let (property_access, property_key) = self.static_field_assignment_name(prop.name)?;
        let value = if prop.initializer.is_some() {
            self.node_text(prop.initializer)
        } else {
            "void 0".to_string()
        };

        if self.use_define_for_class_fields {
            Some(format!(
                "Object.defineProperty({class_ref}, {property_key}, {{\n{indent}    enumerable: true,\n{indent}    configurable: true,\n{indent}    writable: true,\n{indent}    value: {value}\n{indent}}})"
            ))
        } else {
            Some(format!("{class_ref}{property_access} = {value}"))
        }
    }

    fn static_field_assignment_name(&self, name_idx: NodeIndex) -> Option<(String, String)> {
        let name_node = self.arena.get(name_idx)?;
        match name_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                let name = self
                    .arena
                    .get_identifier(name_node)
                    .map(|id| id.escaped_text.clone())?;
                Some((format!(".{name}"), format!("\"{name}\"")))
            }
            k if k == SyntaxKind::PrivateIdentifier as u16 => None,
            k if k == SyntaxKind::StringLiteral as u16 => {
                let name_text = self.node_text(name_idx);
                if name_text.is_empty() {
                    None
                } else {
                    Some((format!("[{name_text}]"), name_text))
                }
            }
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                let computed = self.arena.get_computed_property(name_node)?;
                let key = self.node_text(computed.expression);
                if key.is_empty() {
                    None
                } else {
                    Some((format!("[{key}]"), key))
                }
            }
            _ => {
                let key = self.node_text(name_idx);
                if key.is_empty() {
                    None
                } else {
                    Some((format!("[{key}]"), key))
                }
            }
        }
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
            let text = normalize_member_indentation(&text);
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
                result.push(normalize_decorator_expr_text(
                    &self.node_text(dec.expression),
                ));
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
                        decorator_exprs.push(normalize_decorator_expr_text(
                            &self.node_text(dec.expression),
                        ));
                    }
                }
            }
            if decorator_exprs.is_empty() {
                continue;
            }

            let is_static = self.arena.is_static(&modifiers);
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
        let prefix = if member.is_static { "static_" } else { "" };
        let (kind_prefix, base_name) = match &member.name {
            MemberName::Private(name) => {
                let private_name = name.trim_start_matches('#');
                let base_name = match member.kind {
                    MemberKind::Getter => format!("private_get_{private_name}"),
                    MemberKind::Setter => format!("private_set_{private_name}"),
                    _ => format!("private_{private_name}"),
                };
                ("", base_name)
            }
            MemberName::Identifier(name) => {
                let kind_prefix = match member.kind {
                    MemberKind::Getter => "get_",
                    MemberKind::Setter => "set_",
                    _ => "",
                };
                (kind_prefix, name.clone())
            }
            MemberName::StringLiteral(_) | MemberName::Computed(_) => {
                let kind_prefix = match member.kind {
                    MemberKind::Getter => "get_",
                    MemberKind::Setter => "set_",
                    _ => "",
                };
                (kind_prefix, "member".to_string())
            }
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
        let has_descriptor = member.is_private
            && self.use_static_blocks
            && matches!(
                member.kind,
                MemberKind::Method | MemberKind::Getter | MemberKind::Setter | MemberKind::Accessor
            );

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

    #[allow(clippy::too_many_arguments)]
    fn emit_es_decorate_call(
        &self,
        member: &DecoratedMember,
        var_info: &MemberVarInfo,
        class_alias: &str,
        computed_key_vars: &[(usize, String)],
        member_index: usize,
        indent: &str,
        out: &mut String,
        instance_extra_initializers_var: &str,
        static_extra_initializers_var: &str,
        metadata_var: &str,
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

        let descriptor_arg = if self.use_static_blocks && member.is_private {
            self.private_member_descriptor_arg(member, var_info, &name_str)
        } else {
            "null".to_string()
        };

        // For methods/getters/setters/accessors/private, first arg is the class ref.
        // For plain fields, first arg is null.
        let ctor_arg = if member.kind == MemberKind::Field {
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
                static_extra_initializers_var.to_string()
            } else {
                instance_extra_initializers_var.to_string()
            };
            ("null".to_string(), extra)
        };

        let es_decorate = self.helper("__esDecorate");
        out.push_str(&format!(
            "{indent}{es_decorate}({ctor_arg}, {descriptor_arg}, {}, {{ kind: \"{kind_str}\", name: {name_str}, static: {}, private: {}, access: {{ {access_str} }}, metadata: {metadata_var} }}, {init_arg}, {extra_init_arg});\n",
            var_info.decorators_var,
            member.is_static,
            member.is_private,
        ));
    }

    fn private_member_descriptor_arg(
        &self,
        member: &DecoratedMember,
        var_info: &MemberVarInfo,
        name_str: &str,
    ) -> String {
        let descriptor_var = var_info.descriptor_var.as_deref().unwrap_or("_descriptor");
        let set_function_name = self.helper("__setFunctionName");
        match member.kind {
            MemberKind::Method => {
                let function_expr = self.private_method_function_expr(member);
                format!(
                    "{descriptor_var} = {{ value: {set_function_name}({function_expr}, {name_str}) }}"
                )
            }
            MemberKind::Getter => {
                let function_expr = self.private_getter_function_expr(member);
                format!(
                    "{descriptor_var} = {{ get: {set_function_name}({function_expr}, {name_str}, \"get\") }}"
                )
            }
            MemberKind::Setter => {
                let function_expr = self.private_setter_function_expr(member);
                format!(
                    "{descriptor_var} = {{ set: {set_function_name}({function_expr}, {name_str}, \"set\") }}"
                )
            }
            MemberKind::Accessor => {
                let storage_name = self.private_auto_accessor_storage_name(member);
                format!(
                    "{descriptor_var} = {{ get: {set_function_name}(function () {{ return this.{storage_name}; }}, {name_str}, \"get\"), set: {set_function_name}(function (value) {{ this.{storage_name} = value; }}, {name_str}, \"set\") }}"
                )
            }
            MemberKind::Field => "null".to_string(),
        }
    }

    fn emit_private_decorated_member_wrapper(
        &self,
        member: &DecoratedMember,
        var_info: &MemberVarInfo,
        indent: &str,
        out: &mut String,
    ) {
        let Some(member_name) = self.private_member_name(member) else {
            return;
        };
        let descriptor_var = var_info.descriptor_var.as_deref().unwrap_or("_descriptor");
        let static_prefix = if member.is_static { "static " } else { "" };
        match member.kind {
            MemberKind::Method => {
                out.push_str(&format!(
                    "{indent}{static_prefix}get {member_name}() {{ return {descriptor_var}.value; }}\n"
                ));
            }
            MemberKind::Getter => {
                out.push_str(&format!(
                    "{indent}{static_prefix}get {member_name}() {{ return {descriptor_var}.get.call(this); }}\n"
                ));
            }
            MemberKind::Setter => {
                let params = self.private_member_parameter_list(member);
                let param = params.split(',').next().map(str::trim).unwrap_or("value");
                let param = if param.is_empty() { "value" } else { param };
                out.push_str(&format!(
                    "{indent}{static_prefix}set {member_name}({param}) {{ return {descriptor_var}.set.call(this, {param}); }}\n"
                ));
            }
            MemberKind::Field | MemberKind::Accessor => {}
        }
    }

    fn private_method_function_expr(&self, member: &DecoratedMember) -> String {
        let Some(member_node) = self.arena.get(member.member_idx) else {
            return "function () { }".to_string();
        };
        let Some(method) = self.arena.get_method_decl(member_node) else {
            return "function () { }".to_string();
        };
        let async_prefix = if self
            .arena
            .has_modifier(&method.modifiers, SyntaxKind::AsyncKeyword)
        {
            "async "
        } else {
            ""
        };
        let star = if method.asterisk_token { "*" } else { "" };
        let params = self.parameter_list_text(&method.parameters);
        let body = self.function_body_text(method.body);
        format!("{async_prefix}function{star} ({params}) {body}")
    }

    fn private_getter_function_expr(&self, member: &DecoratedMember) -> String {
        let Some(member_node) = self.arena.get(member.member_idx) else {
            return "function () { }".to_string();
        };
        let Some(accessor) = self.arena.get_accessor(member_node) else {
            return "function () { }".to_string();
        };
        let body = self.function_body_text(accessor.body);
        format!("function () {body}")
    }

    fn private_setter_function_expr(&self, member: &DecoratedMember) -> String {
        let params = self.private_member_parameter_list(member);
        let Some(member_node) = self.arena.get(member.member_idx) else {
            return format!("function ({params}) {{ }}");
        };
        let Some(accessor) = self.arena.get_accessor(member_node) else {
            return format!("function ({params}) {{ }}");
        };
        let body = self.function_body_text(accessor.body);
        format!("function ({params}) {body}")
    }

    fn private_member_parameter_list(&self, member: &DecoratedMember) -> String {
        let Some(member_node) = self.arena.get(member.member_idx) else {
            return "value".to_string();
        };
        if let Some(method) = self.arena.get_method_decl(member_node) {
            return self.parameter_list_text(&method.parameters);
        }
        if let Some(accessor) = self.arena.get_accessor(member_node) {
            return self.parameter_list_text(&accessor.parameters);
        }
        "value".to_string()
    }

    fn parameter_list_text(&self, parameters: &NodeList) -> String {
        parameters
            .nodes
            .iter()
            .filter_map(|&param_idx| {
                let param_node = self.arena.get(param_idx)?;
                let param_data = self.arena.get_parameter(param_node)?;
                let name_text = self.node_text(param_data.name);
                let param_text = if param_data.initializer != NodeIndex::NONE {
                    let init_text = self.node_text(param_data.initializer);
                    format!("{name_text} = {init_text}")
                } else if param_data.dot_dot_dot_token {
                    format!("...{name_text}")
                } else {
                    name_text
                };
                Some(param_text)
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn function_body_text(&self, body_idx: NodeIndex) -> String {
        if body_idx == NodeIndex::NONE {
            return "{ }".to_string();
        }
        if let Some(body) = self.function_body_texts.get(&body_idx) {
            body.clone()
        } else {
            "{ }".to_string()
        }
    }

    fn private_member_name(&self, member: &DecoratedMember) -> Option<String> {
        match &member.name {
            MemberName::Private(name) => Some(name.clone()),
            _ => None,
        }
    }

    fn private_auto_accessor_storage_name(&self, member: &DecoratedMember) -> String {
        match &member.name {
            MemberName::Private(name) => {
                format!("#{}_accessor_storage", name.trim_start_matches('#'))
            }
            _ => "#accessor_storage".to_string(),
        }
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

    fn collect_constructor_parameter_properties(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> Vec<ParameterPropertyInfo> {
        let mut result = Vec::new();
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let Some(ctor) = self.arena.get_constructor(member_node) else {
                continue;
            };
            for &param_idx in &ctor.parameters.nodes {
                let Some(param_node) = self.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.arena.get_parameter(param_node) else {
                    continue;
                };
                if !has_parameter_property_modifier(self.arena, &param.modifiers) {
                    continue;
                }
                let name = crate::transforms::emit_utils::identifier_emit_text_or_empty(
                    self.arena, param.name,
                );
                if !name.is_empty() {
                    result.push(ParameterPropertyInfo { name });
                }
            }
        }
        result
    }

    fn collect_decorated_auto_accessor_info(
        &self,
        decorated_members: &[DecoratedMember],
        computed_key_vars: &[(usize, String)],
    ) -> Vec<DecoratedAutoAccessorInfo> {
        let mut result = Vec::new();
        let mut generated_name_index = 0u32;

        for (i, member) in decorated_members.iter().enumerate() {
            if member.kind != MemberKind::Accessor {
                continue;
            }

            let (name, storage_base) = match &member.name {
                MemberName::Identifier(name) => (name.clone(), name.clone()),
                MemberName::Private(name) => {
                    let name = name.trim_start_matches('#').to_string();
                    (format!("#{name}"), name)
                }
                MemberName::StringLiteral(name) => {
                    let storage_base = generated_auto_accessor_name(generated_name_index);
                    generated_name_index += 1;
                    (format!("\"{name}\""), storage_base)
                }
                MemberName::Computed(_) => {
                    let access_name = computed_key_vars
                        .iter()
                        .find(|(mi, _)| *mi == i)
                        .map(|(_, v)| v.clone())
                        .unwrap_or_else(|| "undefined".to_string());
                    let storage_base = generated_auto_accessor_name(generated_name_index);
                    generated_name_index += 1;
                    (access_name, storage_base)
                }
            };

            result.push(DecoratedAutoAccessorInfo {
                name,
                initializer_text: self.get_field_initializer_text(member.member_idx),
                storage_base,
                member_var_index: i,
            });
        }

        result
    }

    fn collect_class_decorator_static_private_methods(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
        class_name: &str,
        decorated_members: &[DecoratedMember],
        class_span_text: &str,
    ) -> Vec<ClassDecoratorStaticPrivateMethodInfo> {
        let decorated_member_indices: std::collections::HashSet<NodeIndex> = decorated_members
            .iter()
            .map(|member| member.member_idx)
            .collect();
        let mut result = Vec::new();
        for &member_idx in &class_data.members.nodes {
            if decorated_member_indices.contains(&member_idx) {
                continue;
            }
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::METHOD_DECLARATION {
                continue;
            }
            let Some(method) = self.arena.get_method_decl(member_node) else {
                continue;
            };
            if !self.arena.is_static(&method.modifiers) {
                continue;
            }
            let Some(name_node) = self.arena.get(method.name) else {
                continue;
            };
            if name_node.kind != SyntaxKind::PrivateIdentifier as u16 {
                continue;
            }
            let Some(private_name) = self.arena.get_identifier(name_node) else {
                continue;
            };
            let member_name = private_name.escaped_text.to_string();
            let private_name = member_name.trim_start_matches('#');
            let temp_base = if class_name.is_empty() {
                "class".to_string()
            } else {
                class_name.to_string()
            };
            let temp_var =
                hygienic_temp_name(&format!("_{temp_base}_{private_name}"), class_span_text);
            let needs_wrapper = self
                .node_tree_contains_private_identifier(method.body, &member_name)
                || self.class_body_references_private_name(class_data, member_idx, &member_name);
            result.push(ClassDecoratorStaticPrivateMethodInfo {
                member_idx,
                member_name,
                needs_wrapper,
                function_name: temp_var.clone(),
                temp_var,
                params: self.parameter_list_text(&method.parameters),
                body: self.function_body_text(method.body),
            });
        }
        result
    }

    fn class_body_references_private_name(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
        owner_member_idx: NodeIndex,
        private_name: &str,
    ) -> bool {
        class_data.members.nodes.iter().any(|&member_idx| {
            member_idx != owner_member_idx
                && self.node_tree_contains_private_identifier(member_idx, private_name)
        })
    }

    fn node_tree_contains_private_identifier(&self, root: NodeIndex, private_name: &str) -> bool {
        let mut stack = vec![root];
        while let Some(idx) = stack.pop() {
            let Some(node) = self.arena.get(idx) else {
                continue;
            };
            if node.kind == SyntaxKind::PrivateIdentifier as u16
                && let Some(ident) = self.arena.get_identifier(node)
                && ident.escaped_text == private_name
            {
                return true;
            }
            stack.extend(self.arena.get_children(idx));
        }
        false
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

#[cfg(test)]
#[path = "../../tests/es_decorators.rs"]
mod tests;
