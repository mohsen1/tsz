//! TC39 (non-legacy) Decorator Transform
//!
//! Transforms decorated classes using the TC39 decorator protocol.
//! For ES2015 targets, outputs an IIFE with comma-separated decorator application.
//! For ES2022+ targets, uses static initializer blocks.

use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;

#[path = "es_decorators_helpers.rs"]
mod helpers;
use helpers::*;

#[path = "es_decorators_application.rs"]
mod application;
#[path = "es_decorators_class_body.rs"]
mod class_body;
#[path = "es_decorators_collect_fields.rs"]
mod collect_fields;
#[path = "es_decorators_collection.rs"]
mod collection;
#[path = "es_decorators_member_emit.rs"]
mod member_emit;
#[path = "es_decorators_node_utils.rs"]
mod node_utils;
#[path = "es_decorators_private_members.rs"]
mod private_members;

use crate::transforms::emit_utils::hygienic_temp_name;

struct ClassDecoratorVars<'a> {
    class_descriptor: &'a str,
    class_this_var: &'a str,
    class_super_var: &'a str,
    class_decorators_var: &'a str,
    class_extra_initializers_var: &'a str,
    instance_extra_initializers_var: &'a str,
    static_extra_initializers_var: &'a str,
    metadata_var: &'a str,
    metadata_super_temp_var: &'a str,
}

struct DecoratorApplicationCtx<'a> {
    decorated_members: &'a [DecoratedMember],
    member_vars: &'a [MemberVarInfo],
    source_order_decorator_members: &'a std::collections::HashSet<NodeIndex>,
    class_decorators: &'a [String],
    class_name: &'a str,
    ctor_ref: &'a str,
    computed_key_vars: &'a [(usize, String)],
    class_decorator_static_private_methods: &'a [ClassDecoratorStaticPrivateMethodInfo],
    class_decorator_auto_accessor_infos: &'a [ClassDecoratorAutoAccessorInfo],
    class_decorator_static_private_fields: &'a [ClassDecoratorStaticPrivateFieldInfo],
    has_extends: bool,
}

struct ClassBodyCtx<'a> {
    class_node: &'a tsz_parser::parser::node::Node,
    class_data: &'a tsz_parser::parser::node::ClassData,
    decorated_members: &'a [DecoratedMember],
    member_vars: &'a [MemberVarInfo],
    source_order_decorator_members: &'a std::collections::HashSet<NodeIndex>,
    computed_key_vars: &'a [(usize, String)],
    plain_computed_instance_fields: &'a [PlainComputedInstanceFieldInfo],
    class_decorator_instance_private_fields: &'a [ClassDecoratorInstancePrivateFieldInfo],
    class_decorator_static_private_methods: &'a [ClassDecoratorStaticPrivateMethodInfo],
    class_decorator_auto_accessor_infos: &'a [ClassDecoratorAutoAccessorInfo],
    class_decorator_static_private_fields: &'a [ClassDecoratorStaticPrivateFieldInfo],
}

struct ClassBodyFlags<'a> {
    has_any_instance: bool,
    class_alias: &'a str,
    class_name: &'a str,
    defer_class_extra_init: bool,
    class_this_var: &'a str,
    class_extra_initializers_var: &'a str,
    instance_extra_initializers_var: &'a str,
    static_extra_initializers_var: &'a str,
    has_static_method: bool,
    instance_private_brand_var: Option<&'a str>,
}

struct EsDecorateMemberCtx<'a> {
    member_index: usize,
    class_alias: &'a str,
    class_private_ref: &'a str,
    class_name: &'a str,
    computed_key_vars: &'a [(usize, String)],
    class_decorator_static_private_methods: &'a [ClassDecoratorStaticPrivateMethodInfo],
    class_decorator_auto_accessor_infos: &'a [ClassDecoratorAutoAccessorInfo],
    class_decorator_static_private_fields: &'a [ClassDecoratorStaticPrivateFieldInfo],
}

struct EsDecorateVars<'a> {
    instance_extra_initializers_var: &'a str,
    static_extra_initializers_var: &'a str,
    metadata_var: &'a str,
}

struct AutoAccessorClassCtx<'a> {
    class_name: &'a str,
    class_alias: &'a str,
}

struct AutoAccessorMemberEmitCtx<'a> {
    previous_extra_initializers: Option<&'a str>,
    injected_assignments: Option<&'a [String]>,
    class: AutoAccessorClassCtx<'a>,
    indent: &'a str,
}

struct CtorMembersCtx<'a> {
    parameter_properties: &'a [ParameterPropertyInfo],
    field_infos: &'a [DecoratedFieldInfo],
    auto_accessor_infos: &'a [DecoratedAutoAccessorInfo],
    plain_computed_instance_fields: &'a [PlainComputedInstanceFieldInfo],
    class_decorator_instance_private_fields: &'a [ClassDecoratorInstancePrivateFieldInfo],
    computed_key_sink_value_initializers: &'a std::collections::HashMap<NodeIndex, Vec<String>>,
    decorated_members: &'a [DecoratedMember],
    member_vars: &'a [MemberVarInfo],
    source_order_decorator_members: &'a std::collections::HashSet<NodeIndex>,
}

struct CtorInitFlags {
    fields_in_class_body: bool,
    has_instance_fields: bool,
    has_instance_auto_accessors: bool,
    has_instance_method: bool,
    has_extends: bool,
}

struct CtorOutputCtx<'a> {
    class_name: &'a str,
    indent: &'a str,
    inner_indent: &'a str,
    instance_extra_initializers_var: &'a str,
    instance_private_brand_var: Option<&'a str>,
}

struct PlainComputedInstanceFieldInfo {
    member_idx: NodeIndex,
    key_var: String,
    key_expr: NodeIndex,
    initializer_text: String,
}

struct ClassDecoratorInstancePrivateFieldInfo {
    member_idx: NodeIndex,
    storage_name: String,
    initializer_text: String,
}

struct DecoratorReceiverState<'a> {
    temp_counter: &'a mut u32,
    receiver_temp_vars: &'a mut Vec<String>,
    needs_outer_this_capture: &'a mut bool,
    outer_this_var: &'a str,
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
    tslib_import_binding: String,
    /// When true, emit as an expression (no `let C = ` wrapper) for class expressions.
    expression_mode: bool,
    /// Function name for class expression named evaluation (__setFunctionName).
    function_name: Option<String>,
    /// When true, `function_name` is emitted as a runtime expression instead of
    /// a string literal.
    function_name_is_expression: bool,
    /// Runtime temp used for anonymous decorated class expressions.
    anonymous_class_name: Option<String>,
    /// Function body text rendered by the main emitter before this transform
    /// assembles descriptor/externalized function expressions.
    function_body_texts: FxHashMap<NodeIndex, String>,
    /// Field initializer text rendered by the main emitter when raw source text
    /// would miss nested transforms.
    field_initializer_texts: FxHashMap<NodeIndex, String>,
    /// Member decorator expression text rendered by the main emitter for
    /// generated static/IIFE application sites.
    decorator_expression_texts: FxHashMap<NodeIndex, String>,
    /// Static block text rendered by the main emitter when raw source text
    /// would miss scoped static `super` rewrites.
    static_block_texts: FxHashMap<NodeIndex, String>,
    /// Static member text rendered by the main emitter when raw source text
    /// would miss scoped static `super` rewrites in field initializers.
    static_member_texts: FxHashMap<NodeIndex, String>,
    /// Extends expression text rendered by the main emitter when raw source
    /// text would preserve type-only syntax or named-evaluation-sensitive forms.
    extends_text: Option<String>,
    /// File-level reserved temp used when member decorators capture lexical `this`.
    outer_this_var: Option<String>,
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
            function_name_is_expression: false,
            anonymous_class_name: None,
            function_body_texts: FxHashMap::default(),
            field_initializer_texts: FxHashMap::default(),
            decorator_expression_texts: FxHashMap::default(),
            static_block_texts: FxHashMap::default(),
            static_member_texts: FxHashMap::default(),
            extends_text: None,
            outer_this_var: None,
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
        self.function_name_is_expression = false;
    }

    pub fn set_function_name_expression(&mut self, expression: String) {
        self.function_name = Some(expression);
        self.function_name_is_expression = true;
    }

    pub fn set_anonymous_class_name(&mut self, name: String) {
        self.anonymous_class_name = Some(name);
    }

    pub fn set_function_body_text(&mut self, body_idx: NodeIndex, text: String) {
        self.function_body_texts.insert(body_idx, text);
    }

    pub fn set_field_initializer_text(&mut self, member_idx: NodeIndex, text: String) {
        self.field_initializer_texts.insert(member_idx, text);
    }

    pub fn set_decorator_expression_text(&mut self, expr_idx: NodeIndex, text: String) {
        self.decorator_expression_texts.insert(expr_idx, text);
    }

    pub fn set_static_block_text(&mut self, member_idx: NodeIndex, text: String) {
        self.static_block_texts.insert(member_idx, text);
    }

    pub fn set_static_member_text(&mut self, member_idx: NodeIndex, text: String) {
        self.static_member_texts.insert(member_idx, text);
    }

    pub fn set_extends_text(&mut self, text: String) {
        self.extends_text = Some(text);
    }

    pub fn set_outer_this_var(&mut self, name: String) {
        self.outer_this_var = Some(name);
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

    fn function_name_arg(&self, fallback: &str) -> String {
        let name = self.function_name.as_deref().unwrap_or(fallback);
        if self.function_name_is_expression {
            name.to_string()
        } else {
            format!("\"{name}\"")
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
        let class_span_text = self
            .source_text
            .map(|src| {
                let start = class_node.pos as usize;
                let end = (class_node.end as usize).min(src.len());
                if start <= end { &src[start..end] } else { "" }
            })
            .unwrap_or("");
        let mut temp_counter: u32 = 0;
        let outer_this_var = self
            .outer_this_var
            .clone()
            .unwrap_or_else(|| hygienic_temp_name("_outerThis", class_span_text));
        let mut decorator_receiver_temp_vars = Vec::new();
        let mut needs_outer_this_capture = false;
        let class_decorators = self.collect_class_decorator_exprs(
            &class_data.modifiers,
            &mut DecoratorReceiverState {
                temp_counter: &mut temp_counter,
                receiver_temp_vars: &mut decorator_receiver_temp_vars,
                needs_outer_this_capture: &mut needs_outer_this_capture,
                outer_this_var: &outer_this_var,
            },
        );
        let has_class_decorators = !class_decorators.is_empty();
        let class_alias = if self.use_static_blocks || has_class_decorators {
            String::new()
        } else {
            next_temp_var(&mut temp_counter)
        };
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
        let source_order_decorator_members = if self.use_define_for_class_fields
            || (!self.use_static_blocks && has_class_decorators)
        {
            self.source_order_decorator_assignment_members(&class_data.members)
        } else {
            std::collections::HashSet::new()
        };
        let decorated_members = self.collect_decorated_members(
            &class_data.members,
            &mut DecoratorReceiverState {
                temp_counter: &mut temp_counter,
                receiver_temp_vars: &mut decorator_receiver_temp_vars,
                needs_outer_this_capture: &mut needs_outer_this_capture,
                outer_this_var: &outer_this_var,
            },
            &source_order_decorator_members,
        );
        let has_extends = self.has_extends_clause(&class_data.heritage_clauses);

        // If there are no class decorators and no decorated members (e.g., all members
        // are abstract), return empty to signal that no transform is needed.
        if !has_class_decorators && decorated_members.is_empty() {
            return String::new();
        }

        let has_any_instance = decorated_members.iter().any(|m| !m.is_static);
        let _has_any_static = decorated_members.iter().any(|m| m.is_static);

        // tsc avoids shadowing user bindings inside the transformed class wrapper
        // by suffixing decorator temporaries that collide with identifiers used
        // anywhere in the class span (decorators, name, extends, body). Without
        // this rename, e.g. a class body referring to a user `const _classDescriptor`
        // would resolve to the generated temp instead. See issue #3091.
        let class_descriptor_var = hygienic_temp_name("_classDescriptor", class_span_text);
        let class_extra_initializers_var =
            hygienic_temp_name("_classExtraInitializers", class_span_text);
        let class_this_var = hygienic_temp_name("_classThis", class_span_text);
        let class_super_var = hygienic_temp_name("_classSuper", class_span_text);
        let class_decorators_var = hygienic_temp_name("_classDecorators", class_span_text);
        let metadata_var = hygienic_temp_name("_metadata", class_span_text);
        let metadata_super_temp_base = if !self.use_static_blocks && has_class_decorators {
            "_a"
        } else {
            "_b"
        };
        let metadata_super_temp_var = hygienic_temp_name(metadata_super_temp_base, class_span_text);
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
        let plain_computed_instance_fields = if !self.use_static_blocks && has_class_decorators {
            self.collect_plain_computed_instance_fields(
                class_data,
                &decorated_members,
                &mut temp_counter,
            )
        } else {
            Vec::new()
        };
        // Compute member variable names
        let member_vars = self.compute_all_member_vars(&decorated_members);
        let class_decorator_static_private_methods = if has_class_decorators {
            self.collect_class_decorator_static_private_methods(
                class_data,
                &class_name,
                &decorated_members,
                class_span_text,
            )
        } else {
            Vec::new()
        };
        let class_decorator_auto_accessor_infos = if has_class_decorators {
            self.collect_class_decorator_auto_accessor_info(
                class_data,
                &decorated_members,
                &class_name,
                class_span_text,
            )
        } else {
            Vec::new()
        };
        let class_decorator_static_private_auto_accessor_indices: std::collections::HashSet<
            NodeIndex,
        > = class_decorator_auto_accessor_infos
            .iter()
            .filter(|info| info.is_decorated && info.member.is_static && info.member.is_private)
            .map(|info| info.member.member_idx)
            .collect();
        let reserved_auto_accessor_storage_bases: Vec<String> = class_decorator_auto_accessor_infos
            .iter()
            .filter(|info| info.is_decorated && info.member.is_static && info.member.is_private)
            .filter_map(|info| match &info.member.name {
                MemberName::Private(name) | MemberName::Identifier(name) => {
                    Some(name.trim_start_matches('#').to_string())
                }
                MemberName::StringLiteral(_) | MemberName::Computed(_) => None,
            })
            .collect();
        let decorated_auto_accessor_infos = self.collect_decorated_auto_accessor_info(
            &decorated_members,
            &computed_key_vars,
            &reserved_auto_accessor_storage_bases,
            &class_decorator_static_private_auto_accessor_indices,
        );
        let class_decorator_static_private_fields = if has_class_decorators {
            self.collect_class_decorator_static_private_field_info(
                class_data,
                &decorated_members,
                &class_name,
                class_span_text,
            )
        } else {
            Vec::new()
        };
        let class_decorator_instance_private_fields =
            if has_class_decorators && !self.use_static_blocks {
                self.collect_class_decorator_instance_private_field_info(
                    class_data,
                    &decorated_members,
                    &class_name,
                )
            } else {
                Vec::new()
            };
        let decorated_static_private_field_storage_decls: Vec<String> = if !self.use_static_blocks {
            let class_decorator_field_indices: std::collections::HashSet<NodeIndex> =
                class_decorator_static_private_fields
                    .iter()
                    .map(|info| info.member_idx)
                    .collect();
            decorated_members
                .iter()
                .filter(|member| {
                    member.is_static
                        && member.is_private
                        && member.kind == MemberKind::Field
                        && !class_decorator_field_indices.contains(&member.member_idx)
                })
                .map(|member| {
                    self.static_private_field_storage_name(&class_name, member, class_span_text)
                })
                .collect()
        } else {
            Vec::new()
        };
        let decorated_instance_private_field_storage_decls: Vec<String> = if !self.use_static_blocks
        {
            decorated_members
                .iter()
                .filter(|member| {
                    !member.is_static && member.is_private && member.kind == MemberKind::Field
                })
                .map(|member| self.private_field_storage_name(&class_name, member))
                .collect()
        } else {
            Vec::new()
        };
        let class_decorator_instance_private_field_storage_decls: Vec<&str> =
            class_decorator_instance_private_fields
                .iter()
                .map(|info| info.storage_name.as_str())
                .collect();
        let decorated_private_method_access_decls: Vec<String> = if !self.use_static_blocks {
            let class_decorator_method_indices: std::collections::HashSet<NodeIndex> =
                class_decorator_static_private_methods
                    .iter()
                    .map(|info| info.member_idx)
                    .collect();
            let class_decorator_auto_accessor_indices: std::collections::HashSet<NodeIndex> =
                class_decorator_auto_accessor_infos
                    .iter()
                    .map(|info| info.member.member_idx)
                    .collect();
            decorated_members
                .iter()
                .filter(|member| {
                    self.needs_es2015_private_descriptor(member)
                        && !class_decorator_method_indices.contains(&member.member_idx)
                        && !class_decorator_auto_accessor_indices.contains(&member.member_idx)
                })
                .flat_map(|member| {
                    self.private_decorated_member_access_temp_names(&class_name, member)
                })
                .collect()
        } else {
            Vec::new()
        };
        let instance_private_brand_var = if !self.use_static_blocks
            && decorated_members
                .iter()
                .any(|member| self.needs_es2015_instance_private_brand(member))
        {
            Some(self.instance_private_brand_name(&class_name))
        } else {
            None
        };
        let auto_accessor_storage_decls: Vec<String> = if self.use_static_blocks {
            Vec::new()
        } else {
            decorated_auto_accessor_infos
                .iter()
                .map(|info| self.auto_accessor_weakmap_storage_name(&class_name, info))
                .collect()
        };
        let class_decorator_private_temp_decls: Vec<String> =
            class_decorator_static_private_methods
                .iter()
                .map(|info| info.temp_var.clone())
                .chain(
                    class_decorator_static_private_fields
                        .iter()
                        .map(|info| info.storage_name.clone()),
                )
                .chain(class_decorator_auto_accessor_infos.iter().flat_map(|info| {
                    let mut vars = Vec::new();
                    if info.is_decorated {
                        vars.push(info.storage_name.clone());
                    }
                    vars.extend(info.getter_temp_var.iter().cloned());
                    vars.extend(info.setter_temp_var.iter().cloned());
                    if !info.is_decorated {
                        vars.push(info.storage_name.clone());
                    }
                    vars
                }))
                .collect();

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
            let mut var_names = Vec::new();
            if let Some(brand_var) = instance_private_brand_var.as_deref() {
                var_names.push(brand_var);
            }
            var_names.push(class_alias.as_str());
            var_names.extend(
                class_decorator_private_temp_decls
                    .iter()
                    .map(String::as_str),
            );
            var_names.extend(auto_accessor_storage_decls.iter().map(String::as_str));
            var_names.extend(
                class_decorator_instance_private_field_storage_decls
                    .iter()
                    .copied(),
            );
            var_names.extend(
                decorated_instance_private_field_storage_decls
                    .iter()
                    .map(String::as_str),
            );
            var_names.extend(
                decorated_static_private_field_storage_decls
                    .iter()
                    .map(String::as_str),
            );
            var_names.extend(
                decorated_private_method_access_decls
                    .iter()
                    .map(String::as_str),
            );
            out.push_str(&format!("{i1}var {};\n", var_names.join(", ")));
        } else if !auto_accessor_storage_decls.is_empty()
            || !class_decorator_private_temp_decls.is_empty()
            || !class_decorator_instance_private_field_storage_decls.is_empty()
            || !decorated_instance_private_field_storage_decls.is_empty()
            || !decorated_static_private_field_storage_decls.is_empty()
            || !decorated_private_method_access_decls.is_empty()
            || instance_private_brand_var.is_some()
        {
            let mut var_names: Vec<&str> = Vec::new();
            if let Some(brand_var) = instance_private_brand_var.as_deref() {
                var_names.push(brand_var);
            }
            var_names.extend(
                class_decorator_private_temp_decls
                    .iter()
                    .map(String::as_str),
            );
            var_names.extend(auto_accessor_storage_decls.iter().map(String::as_str));
            var_names.extend(
                class_decorator_instance_private_field_storage_decls
                    .iter()
                    .copied(),
            );
            var_names.extend(
                decorated_instance_private_field_storage_decls
                    .iter()
                    .map(String::as_str),
            );
            var_names.extend(
                decorated_static_private_field_storage_decls
                    .iter()
                    .map(String::as_str),
            );
            var_names.extend(
                decorated_private_method_access_decls
                    .iter()
                    .map(String::as_str),
            );
            out.push_str(&format!("{i1}var {};\n", var_names.join(", ")));
        }
        if !computed_key_vars.is_empty() {
            let key_names: Vec<&str> = computed_key_vars.iter().map(|(_, v)| v.as_str()).collect();
            out.push_str(&format!("{i1}var {};\n", key_names.join(", ")));
        }
        if !plain_computed_instance_fields.is_empty() {
            let key_names: Vec<&str> = plain_computed_instance_fields
                .iter()
                .map(|info| info.key_var.as_str())
                .collect();
            out.push_str(&format!("{i1}var {};\n", key_names.join(", ")));
        }
        if !decorator_receiver_temp_vars.is_empty() {
            let receiver_names: Vec<&str> = decorator_receiver_temp_vars
                .iter()
                .map(String::as_str)
                .collect();
            out.push_str(&format!("{i1}var {};\n", receiver_names.join(", ")));
        }
        if needs_outer_this_capture {
            out.push_str(&format!("{i1}let {outer_this_var} = this;\n"));
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
        }
        // When a transformed decorated class extends a base class, tsc captures
        // the super class so metadata inheritance and `extends` share one value.
        if has_extends && let Some(extends_text) = self.get_extends_text(class_data) {
            out.push_str(&format!("{i1}let {class_super_var} = {extends_text};\n"));
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
        if has_static_method {
            out.push_str(&format!("{i1}let {static_extra_initializers_var} = [];\n"));
        }
        if has_instance_method {
            out.push_str(&format!(
                "{i1}let {instance_extra_initializers_var} = [];\n"
            ));
        }

        // Per-member decorator and initializer variables
        for member_index in self.member_var_declaration_order(&decorated_members) {
            let var_info = &member_vars[member_index];
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
            out.push_str(&format!(" extends {class_super_var}"));
        }
        out.push_str(" {\n");

        if self.use_static_blocks {
            // ES2022: with class decorators, emit _classThis capture block first
            if has_class_decorators {
                out.push_str(&format!("{i2}static {{ {class_this_var} = this; }}\n"));
                if self.expression_mode && class_name_was_empty {
                    let fn_name = self.function_name_arg("");
                    let set_fn = self.helper("__setFunctionName");
                    out.push_str(&format!(
                        "{i2}static {{ {set_fn}({class_this_var}, {fn_name}); }}\n"
                    ));
                } else if ((self.expression_mode && self.function_name.is_none())
                    || (!self.expression_mode && !class_name.is_empty()))
                    && (class_name_was_empty
                        || !class_decorator_static_private_methods.is_empty()
                        || !class_decorator_auto_accessor_infos.is_empty()
                        || !class_decorator_static_private_fields.is_empty())
                {
                    let set_fn = self.helper("__setFunctionName");
                    out.push_str(&format!(
                        "{i2}static {{ {set_fn}(this, \"{class_name}\"); }}\n"
                    ));
                }
            } else if self.expression_mode && self.function_name.is_some() {
                // Member-only decorators on class expression with a context name:
                // emit __setFunctionName(this, "name") in a static block
                let fn_name = self.function_name_arg("");
                let set_fn = self.helper("__setFunctionName");
                out.push_str(&format!("{i2}static {{ {set_fn}(this, {fn_name}); }}\n"));
            }
            let assignments = self.class_decorator_static_private_temp_assignments(
                &class_decorator_static_private_methods,
                &class_decorator_auto_accessor_infos,
                &decorated_members,
                &member_vars,
                &class_this_var,
            );
            if !assignments.is_empty() {
                out.push_str(&format!("{i2}static {{ {assignments}; }}\n"));
            }

            // ES2022: for fields-in-constructor mode (!useDefineForClassFields),
            // emit assignment expressions in a separate static block as comma expression
            // when there are computed key assignments that need propKey.
            let has_computed_field_keys = !computed_key_vars.is_empty();
            if !self.use_define_for_class_fields && has_computed_field_keys {
                let mut assign_parts: Vec<String> = Vec::new();
                for (i, member) in decorated_members.iter().enumerate() {
                    if source_order_decorator_members.contains(&member.member_idx) {
                        continue;
                    }
                    let var_info = &member_vars[i];
                    let dec_exprs = member.captured_decorator_exprs.join(", ");
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
                &DecoratorApplicationCtx {
                    decorated_members: &decorated_members,
                    member_vars: &member_vars,
                    source_order_decorator_members: &source_order_decorator_members,
                    class_decorators: &class_decorators,
                    class_name: &class_name,
                    ctor_ref: &ctor_ref,
                    computed_key_vars: &computed_key_vars,
                    class_decorator_static_private_methods: &class_decorator_static_private_methods,
                    class_decorator_auto_accessor_infos: &class_decorator_auto_accessor_infos,
                    class_decorator_static_private_fields: &class_decorator_static_private_fields,
                    has_extends,
                },
                &i3,
                &mut out,
                defer_class_init_inner,
                &ClassDecoratorVars {
                    class_descriptor: &class_descriptor_var,
                    class_this_var: &class_this_var,
                    class_super_var: &class_super_var,
                    class_decorators_var: &class_decorators_var,
                    class_extra_initializers_var: &class_extra_initializers_var,
                    instance_extra_initializers_var: &instance_extra_initializers_var,
                    static_extra_initializers_var: &static_extra_initializers_var,
                    metadata_var: &metadata_var,
                    metadata_super_temp_var: &metadata_super_temp_var,
                },
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
            &ClassBodyCtx {
                class_node,
                class_data,
                decorated_members: &decorated_members,
                member_vars: &member_vars,
                source_order_decorator_members: &source_order_decorator_members,
                computed_key_vars: &computed_key_vars,
                plain_computed_instance_fields: &plain_computed_instance_fields,
                class_decorator_instance_private_fields: &class_decorator_instance_private_fields,
                class_decorator_static_private_methods: &class_decorator_static_private_methods,
                class_decorator_auto_accessor_infos: &class_decorator_auto_accessor_infos,
                class_decorator_static_private_fields: &class_decorator_static_private_fields,
            },
            &ClassBodyFlags {
                has_any_instance,
                class_alias: &ctor_ref,
                class_name: &class_name,
                defer_class_extra_init: defer_class_init,
                class_this_var: &class_this_var,
                class_extra_initializers_var: &class_extra_initializers_var,
                instance_extra_initializers_var: &instance_extra_initializers_var,
                static_extra_initializers_var: &static_extra_initializers_var,
                has_static_method,
                instance_private_brand_var: instance_private_brand_var.as_deref(),
            },
            member_indent,
            member_inner_indent,
            &mut out,
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

            for assign in external_assignments.iter().filter(|assign| {
                self.is_es2015_storage_setup_assignment(assign)
                    || plain_computed_instance_fields
                        .iter()
                        .any(|info| assign.starts_with(&format!("{} = ", info.key_var)))
            }) {
                out.push_str(&format!("{i1}{assign};\n"));
            }

            for assignment in self.class_decorator_static_private_temp_assignment_list(
                &class_decorator_static_private_methods,
                &class_decorator_auto_accessor_infos,
                &decorated_members,
                &member_vars,
                &class_this_var,
            ) {
                out.push_str(&format!("{i1}{assignment};\n"));
            }

            // __setFunctionName
            let set_fn_name = self.helper("__setFunctionName");
            let set_function_name = if self.expression_mode && class_name_was_empty {
                self.function_name_arg(&class_name)
            } else {
                format!("\"{class_name}\"")
            };
            out.push_str(&format!(
                "{i1}{set_fn_name}({class_this_var}, {set_function_name});\n"
            ));

            // Decorator application as separate IIFE
            out.push_str(&format!("{i1}(() => {{\n"));
            self.emit_decorator_application(
                &DecoratorApplicationCtx {
                    decorated_members: &decorated_members,
                    member_vars: &member_vars,
                    source_order_decorator_members: &source_order_decorator_members,
                    class_decorators: &class_decorators,
                    class_name: &class_name,
                    ctor_ref: &ctor_ref,
                    computed_key_vars: &computed_key_vars,
                    class_decorator_static_private_methods: &class_decorator_static_private_methods,
                    class_decorator_auto_accessor_infos: &class_decorator_auto_accessor_infos,
                    class_decorator_static_private_fields: &class_decorator_static_private_fields,
                    has_extends,
                },
                &i2,
                &mut out,
                defer_class_init,
                &ClassDecoratorVars {
                    class_descriptor: &class_descriptor_var,
                    class_this_var: &class_this_var,
                    class_super_var: &class_super_var,
                    class_decorators_var: &class_decorators_var,
                    class_extra_initializers_var: &class_extra_initializers_var,
                    instance_extra_initializers_var: &instance_extra_initializers_var,
                    static_extra_initializers_var: &static_extra_initializers_var,
                    metadata_var: &metadata_var,
                    metadata_super_temp_var: &metadata_super_temp_var,
                },
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
                &DecoratorApplicationCtx {
                    decorated_members: &decorated_members,
                    member_vars: &member_vars,
                    source_order_decorator_members: &source_order_decorator_members,
                    class_decorators: &class_decorators,
                    class_name: &class_name,
                    ctor_ref: &ctor_ref,
                    computed_key_vars: &computed_key_vars,
                    class_decorator_static_private_methods: &class_decorator_static_private_methods,
                    class_decorator_auto_accessor_infos: &class_decorator_auto_accessor_infos,
                    class_decorator_static_private_fields: &class_decorator_static_private_fields,
                    has_extends,
                },
                &i3,
                &mut out,
                false,
                &ClassDecoratorVars {
                    class_descriptor: &class_descriptor_var,
                    class_this_var: &class_this_var,
                    class_super_var: &class_super_var,
                    class_decorators_var: &class_decorators_var,
                    class_extra_initializers_var: &class_extra_initializers_var,
                    instance_extra_initializers_var: &instance_extra_initializers_var,
                    static_extra_initializers_var: &static_extra_initializers_var,
                    metadata_var: &metadata_var,
                    metadata_super_temp_var: &metadata_super_temp_var,
                },
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
}

#[cfg(test)]
#[path = "../../tests/es_decorators.rs"]
mod tests;
