//! JS/checkJs-specific scanning of constructor bodies for `this.prop = value`
//! assignments that serve as implicit property declarations.

use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{PropertyInfo, TypeId, Visibility};

impl CheckerState<'_> {
    pub(crate) fn enclosing_jsdoc_class_template_types(
        &mut self,
        node_idx: NodeIndex,
    ) -> FxHashMap<String, TypeId> {
        let mut template_types = FxHashMap::default();
        if !self.is_js_file() {
            return template_types;
        }

        let mut current = node_idx;
        for _ in 0..12 {
            let Some(parent_idx) = self.ctx.arena.get_extended(current).map(|ext| ext.parent)
            else {
                break;
            };
            if parent_idx.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };
            if (parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION)
                && let Some(class) = self.ctx.arena.get_class(parent_node)
            {
                if class
                    .type_parameters
                    .as_ref()
                    .is_some_and(|params| !params.nodes.is_empty())
                {
                    return template_types;
                }
                let Some(source_file) = self.ctx.arena.source_files.first() else {
                    return template_types;
                };
                let Some(jsdoc) = self.try_leading_jsdoc(
                    &source_file.comments,
                    parent_node.pos,
                    &source_file.text,
                ) else {
                    return template_types;
                };

                for name in Self::jsdoc_template_type_params(&jsdoc) {
                    let atom = self.ctx.types.intern_string(&name);
                    template_types.entry(name).or_insert_with(|| {
                        self.ctx
                            .types
                            .factory()
                            .type_param(tsz_solver::TypeParamInfo {
                                name: atom,
                                constraint: None,
                                default: None,
                                is_const: false,
                            })
                    });
                }
                return template_types;
            }
            current = parent_idx;
        }

        template_types
    }

    fn js_class_body_param_type_map(&mut self, body_idx: NodeIndex) -> FxHashMap<String, TypeId> {
        let mut param_type_map = FxHashMap::default();
        let Some(parent_idx) = self.ctx.arena.get_extended(body_idx).map(|ext| ext.parent) else {
            return param_type_map;
        };
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return param_type_map;
        };

        let parameters = match parent_node.kind {
            k if k == syntax_kind_ext::CONSTRUCTOR => self
                .ctx
                .arena
                .get_constructor(parent_node)
                .map(|ctor| &ctor.parameters),
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .ctx
                .arena
                .get_method_decl(parent_node)
                .map(|method| &method.parameters),
            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION =>
            {
                self.ctx
                    .arena
                    .get_function(parent_node)
                    .map(|func| &func.parameters)
            }
            _ => None,
        };
        let Some(parameters) = parameters else {
            return param_type_map;
        };
        let class_template_types = self.enclosing_jsdoc_class_template_types(parent_idx);

        let jsdoc = self.get_jsdoc_for_function(parent_idx);
        let jsdoc_param_names: Vec<String> = jsdoc
            .as_ref()
            .map(|jsdoc| {
                Self::extract_jsdoc_param_names(jsdoc)
                    .into_iter()
                    .map(|(name, _)| name)
                    .collect()
            })
            .unwrap_or_default();
        let comment_start = self.get_jsdoc_comment_pos_for_function(parent_idx);

        for (pi, &param_idx) in parameters.nodes.iter().enumerate() {
            let Some(param) = self.ctx.arena.get_parameter_at(param_idx) else {
                continue;
            };
            let Some(name_ident) = self.ctx.arena.get_identifier_at(param.name) else {
                continue;
            };
            let param_type = if param.type_annotation.is_some() {
                Some(self.get_type_from_type_node(param.type_annotation))
            } else if let (Some(jsdoc), Some(comment_start)) = (jsdoc.as_ref(), comment_start) {
                let pname = self.effective_jsdoc_param_name(param.name, &jsdoc_param_names, pi);
                self.resolve_jsdoc_param_type_with_pos(jsdoc, &pname, Some(comment_start))
                    .or_else(|| {
                        Self::extract_jsdoc_param_type_string(jsdoc, &pname).and_then(|type_expr| {
                            let normalized = type_expr
                                .trim()
                                .trim_end_matches('=')
                                .trim_start_matches("...")
                                .trim();
                            class_template_types.get(normalized).copied()
                        })
                    })
                    .or_else(|| self.jsdoc_type_annotation_for_node(param_idx))
            } else {
                self.jsdoc_type_annotation_for_node(param_idx)
            };
            if let Some(param_type) = param_type {
                param_type_map.insert(name_ident.escaped_text.clone(), param_type);
            }
        }

        param_type_map
    }

    fn js_constructor_assignment_rhs_type(
        &mut self,
        rhs_idx: NodeIndex,
        param_type_map: &FxHashMap<String, TypeId>,
    ) -> TypeId {
        if let Some(rhs_node) = self.ctx.arena.get(rhs_idx)
            && rhs_node.kind == SyntaxKind::Identifier as u16
            && let Some(rhs_ident) = self.ctx.arena.get_identifier(rhs_node)
            && let Some(&param_type) = param_type_map.get(rhs_ident.escaped_text.as_str())
        {
            return param_type;
        }
        if let Some(rhs_node) = self.ctx.arena.get(rhs_idx)
            && rhs_node.kind == SyntaxKind::Identifier as u16
            && let Some(sym_id) = self.ctx.binder.resolve_identifier(self.ctx.arena, rhs_idx)
        {
            let symbol_type = self.get_type_of_symbol(sym_id);
            if symbol_type != TypeId::ERROR && symbol_type != TypeId::UNDEFINED {
                return symbol_type;
            }
        }

        self.get_type_of_node(rhs_idx)
    }

    pub(crate) fn js_assignment_rhs_is_void_zero(&self, rhs_idx: NodeIndex) -> bool {
        let rhs_idx = self.ctx.arena.skip_parenthesized(rhs_idx);
        let Some(rhs_node) = self.ctx.arena.get(rhs_idx) else {
            return false;
        };

        if rhs_node.kind != syntax_kind_ext::VOID_EXPRESSION
            && rhs_node.kind != syntax_kind_ext::PREFIX_UNARY_EXPRESSION
        {
            return false;
        }

        let Some(unary) = self.ctx.arena.get_unary_expr(rhs_node) else {
            return false;
        };
        if unary.operator != SyntaxKind::VoidKeyword as u16 {
            return false;
        }

        let operand_idx = self.ctx.arena.skip_parenthesized(unary.operand);
        let Some(operand_node) = self.ctx.arena.get(operand_idx) else {
            return false;
        };
        operand_node.kind == SyntaxKind::NumericLiteral as u16
            && self
                .ctx
                .arena
                .get_literal(operand_node)
                .is_some_and(|lit| lit.text == "0")
    }

    pub(crate) fn js_statement_declared_type(&mut self, stmt_idx: NodeIndex) -> Option<TypeId> {
        self.jsdoc_type_annotation_for_node(stmt_idx).or_else(|| {
            let stmt_node = self.ctx.arena.get(stmt_idx)?;
            let expr_stmt = self.ctx.arena.get_expression_statement(stmt_node)?;
            self.jsdoc_type_annotation_for_node_direct(expr_stmt.expression)
        })
    }

    /// Scan a body (constructor or method) for `this.prop = value` assignments
    /// and add them as instance properties. This implements the JS/checkJs
    /// pattern where assignments serve as implicit property declarations.
    ///
    /// Also handles the `var self = this; self.prop = value` alias pattern.
    ///
    /// Only top-level expression statements in the body are scanned.
    /// Properties that already exist (from explicit declarations or parameter
    /// properties) are skipped — explicit declarations take precedence.
    pub(crate) fn collect_js_constructor_this_properties(
        &mut self,
        body_idx: NodeIndex,
        properties: &mut FxHashMap<Atom, PropertyInfo>,
        parent_sym: Option<SymbolId>,
        emit_implicit_any: bool,
    ) {
        let top_level_stmts: Vec<NodeIndex> = {
            let Some(body_node) = self.ctx.arena.get(body_idx) else {
                return;
            };
            let Some(block) = self.ctx.arena.get_block(body_node) else {
                return;
            };
            block.statements.nodes.clone()
        };
        let mut stmts = Vec::new();
        for stmt_idx in top_level_stmts {
            self.collect_nested_js_this_assignment_statements(stmt_idx, &mut stmts);
        }
        let param_type_map = self.js_class_body_param_type_map(body_idx);

        // Check if the enclosing function has a JSDoc @this tag.
        // When @this provides an explicit type for `this`, all properties
        // from `this.x` assignments inherit their types from the @this type,
        // so TS7008 should be suppressed.
        let enclosing_has_this_tag = self
            .ctx
            .arena
            .get_extended(body_idx)
            .map(|ext| ext.parent)
            .and_then(|func_idx| self.get_jsdoc_for_function(func_idx))
            .is_some_and(|jsdoc| jsdoc.contains("@this"));

        // Phase 1: Detect `var/let/const alias = this` patterns
        let this_aliases = self.collect_this_aliases(&stmts);
        let mut constructor_collected_props = FxHashSet::default();

        for &stmt_idx in &stmts {
            let Some((prop_name, rhs_idx, is_private, report_idx)) = self
                .extract_this_property_assignment(stmt_idx, &this_aliases)
                .or_else(|| {
                    self.extract_jsdoc_this_property_declaration(stmt_idx, &this_aliases)
                        .map(|(prop_name, is_private, report_idx)| {
                            (prop_name, NodeIndex::NONE, is_private, report_idx)
                        })
                })
            else {
                continue;
            };

            // Skip private identifiers — they have separate handling
            if is_private {
                continue;
            }

            let name_atom = self.ctx.types.intern_string(&prop_name);

            // Don't override explicit declarations or properties collected by
            // earlier scans (e.g. class members/method passes). Within this scan,
            // allow later concrete assignments to refine earlier `null`/`undefined`
            // placeholders from branchy constructor code.
            if properties.contains_key(&name_atom)
                && !constructor_collected_props.contains(&name_atom)
            {
                continue;
            }
            let is_readonly = self.jsdoc_has_readonly_tag(stmt_idx);

            if !rhs_idx.is_none() && self.js_assignment_rhs_is_void_zero(rhs_idx) {
                if let Some(parent_sym) = parent_sym
                    && let Some(symbol) = self.ctx.binder.get_symbol(parent_sym)
                {
                    self.error_at_node(
                        report_idx,
                        &format!(
                            "Property '{prop_name}' does not exist on type '{}'.",
                            symbol.escaped_name
                        ),
                        crate::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                    );
                }
                continue;
            }

            // Determine type: JSDoc @type annotation > inferred from RHS
            // Track whether the resulting `any` type came from an explicit source
            // (JSDoc @type, or a parameter with @param {any}) vs a truly implicit one
            // (no RHS, or RHS is null/undefined without annotation).
            let mut any_is_explicit = false;
            let mut provisional_open = false;
            let type_id = if let Some(jsdoc_type) = self.js_statement_declared_type(stmt_idx) {
                any_is_explicit = true;
                jsdoc_type
            } else if !rhs_idx.is_none() {
                let mut rhs_type =
                    self.js_constructor_assignment_rhs_type(rhs_idx, &param_type_map);
                let rhs_is_direct_empty_array =
                    self.ctx.arena.get(rhs_idx).is_some_and(|rhs_node| {
                        rhs_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                            && self
                                .ctx
                                .arena
                                .get_literal_expr(rhs_node)
                                .is_some_and(|lit| lit.elements.nodes.is_empty())
                    });
                if rhs_is_direct_empty_array
                    && tsz_solver::type_queries::get_array_element_type(self.ctx.types, rhs_type)
                        == Some(TypeId::NEVER)
                {
                    rhs_type = self.ctx.types.factory().array(TypeId::ANY);
                    provisional_open = true;
                }
                if rhs_type == TypeId::NULL || rhs_type == TypeId::UNDEFINED {
                    provisional_open = true;
                } else if rhs_type == TypeId::ANY
                    || tsz_solver::type_queries::get_array_element_type(self.ctx.types, rhs_type)
                        == Some(TypeId::ANY)
                {
                    // RHS evaluates to `any` or `any[]` — check if it came from
                    // an explicitly-typed source (e.g., a parameter with @param {any}).
                    // If so, the member's `any` type is explicit, not implicit.
                    //
                    // Also treat expressions rooted on `this` as explicit:
                    // `this.z = this.y`, `this[x] = this[y].bind(this)`, etc.
                    // The property type comes from the class/constructor, so the
                    // `any` is due to incomplete `this` context during class building,
                    // not a genuinely untyped source. tsc does not emit TS7008 here.
                    if self.expression_roots_in_this(rhs_idx) {
                        any_is_explicit = true;
                    }
                    if let Some(rhs_node) = self.ctx.arena.get(rhs_idx)
                        && rhs_node.kind == SyntaxKind::Identifier as u16
                        && let Some(sym_id) =
                            self.ctx.binder.resolve_identifier(self.ctx.arena, rhs_idx)
                        && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                    {
                        let decl = symbol.value_declaration;
                        if !decl.is_none() {
                            // Check if the declaration has an inline type annotation
                            let has_inline_type = self.ctx.arena.get(decl).is_some_and(|d| {
                                self.ctx
                                    .arena
                                    .get_parameter(d)
                                    .is_some_and(|p| !p.type_annotation.is_none())
                            });
                            // Check if the enclosing function's JSDoc has
                            // a @param {type} tag for this parameter
                            let has_jsdoc_param_type = if !has_inline_type {
                                let param_name = self
                                    .ctx
                                    .arena
                                    .get(decl)
                                    .and_then(|d| self.ctx.arena.get_parameter(d))
                                    .and_then(|p| self.ctx.arena.get(p.name))
                                    .and_then(|n| self.ctx.arena.get_identifier(n))
                                    .map(|id| id.escaped_text.as_str());
                                if let Some(pname) = param_name {
                                    // Walk to enclosing function via extended node parent
                                    let func_idx =
                                        self.ctx.arena.get_extended(decl).map(|ext| ext.parent);
                                    func_idx
                                        .and_then(|fidx| self.get_jsdoc_for_function(fidx))
                                        .is_some_and(|jsdoc| {
                                            Self::jsdoc_has_param_type(&jsdoc, pname)
                                        })
                                } else {
                                    false
                                }
                            } else {
                                false
                            };
                            if has_inline_type || has_jsdoc_param_type {
                                any_is_explicit = true;
                            }
                        }
                    }
                }
                if is_readonly {
                    rhs_type
                } else {
                    self.widen_literal_type(rhs_type)
                }
            } else {
                TypeId::ANY
            };

            if emit_implicit_any
                && self.ctx.no_implicit_any()
                && !any_is_explicit
                && !enclosing_has_this_tag
            {
                let implicit_type = if type_id == TypeId::ANY {
                    Some("any")
                } else if tsz_solver::type_queries::get_array_element_type(self.ctx.types, type_id)
                    == Some(TypeId::ANY)
                {
                    Some("any[]")
                } else {
                    None
                };
                if let Some(implicit_type) = implicit_type {
                    let message =
                        format!("Member '{prop_name}' implicitly has an '{implicit_type}' type.");
                    let implicit_anchor = stmt_idx;
                    let already_emitted = self.ctx.diagnostics.iter().any(|d| {
                        d.code
                            == crate::diagnostics::diagnostic_codes::MEMBER_IMPLICITLY_HAS_AN_TYPE
                            && d.start == self.ctx.arena.get(implicit_anchor).map_or(0, |n| n.pos)
                            && d.message_text == message
                    });
                    if !already_emitted {
                        self.error_at_node_msg(
                            implicit_anchor,
                            crate::diagnostics::diagnostic_codes::MEMBER_IMPLICITLY_HAS_AN_TYPE,
                            &[&prop_name, implicit_type],
                        );
                    }
                }
            }

            if type_id == TypeId::VOID {
                if let Some(parent_sym) = parent_sym
                    && let Some(symbol) = self.ctx.binder.get_symbol(parent_sym)
                {
                    self.error_at_node(
                        report_idx,
                        &format!(
                            "Property '{prop_name}' does not exist on type '{}'.",
                            symbol.escaped_name
                        ),
                        crate::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                    );
                }
                continue;
            }
            if let Some(existing) = properties.get_mut(&name_atom) {
                if existing.write_type == TypeId::ANY {
                    if existing.type_id == TypeId::UNDEFINED && !provisional_open {
                        existing.type_id = type_id;
                        existing.write_type = type_id;
                    } else {
                        let merged = self.ctx.types.factory().union2(existing.type_id, type_id);
                        existing.type_id = merged;
                        existing.write_type = if provisional_open {
                            TypeId::ANY
                        } else {
                            merged
                        };
                    }
                    existing.readonly &= is_readonly;
                }
                continue;
            }

            constructor_collected_props.insert(name_atom);
            properties.insert(
                name_atom,
                PropertyInfo {
                    name: name_atom,
                    type_id,
                    write_type: if provisional_open {
                        TypeId::ANY
                    } else {
                        type_id
                    },
                    optional: false,
                    readonly: is_readonly,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: parent_sym,
                    declaration_order: 0,
                },
            );
        }
    }

    /// Scan statements for `var/let/const X = this` patterns and return
    /// the set of alias identifier names.
    fn collect_this_aliases(&self, stmts: &[NodeIndex]) -> Vec<String> {
        let mut aliases = Vec::new();
        for &stmt_idx in stmts {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                continue;
            }
            let Some(var_stmt) = self.ctx.arena.get_variable(stmt_node) else {
                continue;
            };
            // VariableStatement -> declarations (NodeList of VARIABLE_DECLARATION_LIST)
            for &decl_list_idx in &var_stmt.declarations.nodes {
                let Some(decl_list_node) = self.ctx.arena.get(decl_list_idx) else {
                    continue;
                };
                let Some(decl_list) = self.ctx.arena.get_variable(decl_list_node) else {
                    continue;
                };
                for &decl_idx in &decl_list.declarations.nodes {
                    let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                        continue;
                    };
                    let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                        continue;
                    };
                    // Check initializer is `this`
                    if let Some(init_node) = self.ctx.arena.get(var_decl.initializer)
                        && init_node.kind == SyntaxKind::ThisKeyword as u16
                    {
                        // Get the name identifier
                        if let Some(name_node) = self.ctx.arena.get(var_decl.name)
                            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                        {
                            aliases.push(ident.escaped_text.clone());
                        }
                    }
                }
            }
        }
        aliases
    }

    fn collect_nested_js_this_assignment_statements(
        &self,
        stmt_idx: NodeIndex,
        out: &mut Vec<NodeIndex>,
    ) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => out.push(stmt_idx),
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &nested_idx in &block.statements.nodes {
                        self.collect_nested_js_this_assignment_statements(nested_idx, out);
                    }
                }
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.ctx.arena.get_if_statement(node) {
                    self.collect_nested_js_this_assignment_statements(if_stmt.then_statement, out);
                    if !if_stmt.else_statement.is_none() {
                        self.collect_nested_js_this_assignment_statements(
                            if_stmt.else_statement,
                            out,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    /// Check if an expression is rooted in `this` — i.e., is `this`,
    /// `this.x`, `this[x]`, `this.x.bind(...)`, `this[x].call(...)`, etc.
    /// Used to suppress TS7008 when the `any` type comes from incomplete
    /// `this` context during class type building, not a genuinely untyped source.
    fn expression_roots_in_this(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        match node.kind {
            k if k == SyntaxKind::ThisKeyword as u16 => true,
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                self.ctx
                    .arena
                    .get_access_expr(node)
                    .is_some_and(|a| self.expression_roots_in_this(a.expression))
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => self
                .ctx
                .arena
                .get_call_expr(node)
                .is_some_and(|c| self.expression_roots_in_this(c.expression)),
            _ => false,
        }
    }

    /// Extract a `this.propName = rhs`, `alias.propName = rhs`,
    /// `this[computed] = rhs`, or `alias[computed] = rhs` pattern
    /// from an expression statement. The `this_aliases` parameter contains
    /// names of variables known to alias `this` (e.g., `var self = this`).
    /// Returns `(property_name, rhs_node_index, is_private, report_node_index)` if matched.
    fn extract_this_property_assignment(
        &mut self,
        stmt_idx: NodeIndex,
        this_aliases: &[String],
    ) -> Option<(String, NodeIndex, bool, NodeIndex)> {
        let stmt_node = self.ctx.arena.get(stmt_idx)?;
        if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
            return None;
        }
        let expr_stmt = self.ctx.arena.get_expression_statement(stmt_node)?;
        let expr_node = self.ctx.arena.get(expr_stmt.expression)?;
        if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let binary = self.ctx.arena.get_binary_expr(expr_node)?;
        if binary.operator_token != SyntaxKind::EqualsToken as u16 {
            return None;
        }

        // Check LHS is this.propName, alias.propName, this[key], or alias[key]
        let lhs_node = self.ctx.arena.get(binary.left)?;
        let is_element_access = lhs_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION;
        if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION && !is_element_access {
            return None;
        }
        let access = self.ctx.arena.get_access_expr(lhs_node)?;
        let obj_node = self.ctx.arena.get(access.expression)?;

        let is_this_or_alias = if obj_node.kind == SyntaxKind::ThisKeyword as u16 {
            true
        } else if obj_node.kind == SyntaxKind::Identifier as u16 {
            // Check if the identifier is a known `this` alias
            if let Some(ident) = self.ctx.arena.get_identifier(obj_node) {
                this_aliases.iter().any(|a| a == &ident.escaped_text)
            } else {
                false
            }
        } else {
            false
        };

        if !is_this_or_alias {
            return None;
        }

        if is_element_access {
            // For element access (this[key] = value), evaluate the key expression's
            // type to get a property name. Handles Symbol keys, string literal keys,
            // and const variable references.
            let arg_idx = access.name_or_argument;
            let prev = self.ctx.preserve_literal_types;
            self.ctx.preserve_literal_types = true;
            let key_type = self.get_type_of_node(arg_idx);
            self.ctx.preserve_literal_types = prev;
            let prop_name =
                crate::query_boundaries::type_computation::access::literal_property_name(
                    self.ctx.types,
                    key_type,
                )
                .map(|atom| self.ctx.types.resolve_atom(atom))?;
            Some((prop_name, binary.right, false, access.name_or_argument))
        } else {
            let name_node = self.ctx.arena.get(access.name_or_argument)?;
            let ident = self.ctx.arena.get_identifier(name_node)?;
            let is_private = name_node.kind == SyntaxKind::PrivateIdentifier as u16;
            Some((
                ident.escaped_text.clone(),
                binary.right,
                is_private,
                access.name_or_argument,
            ))
        }
    }

    fn extract_jsdoc_this_property_declaration(
        &mut self,
        stmt_idx: NodeIndex,
        this_aliases: &[String],
    ) -> Option<(String, bool, NodeIndex)> {
        self.js_statement_declared_type(stmt_idx)?;

        let stmt_node = self.ctx.arena.get(stmt_idx)?;
        if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
            return None;
        }
        let expr_stmt = self.ctx.arena.get_expression_statement(stmt_node)?;
        let expr_node = self.ctx.arena.get(expr_stmt.expression)?;
        let is_element_access = expr_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION;
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION && !is_element_access {
            return None;
        }

        let access = self.ctx.arena.get_access_expr(expr_node)?;
        let obj_node = self.ctx.arena.get(access.expression)?;
        let is_this_or_alias = if obj_node.kind == SyntaxKind::ThisKeyword as u16 {
            true
        } else if obj_node.kind == SyntaxKind::Identifier as u16 {
            self.ctx
                .arena
                .get_identifier(obj_node)
                .is_some_and(|ident| {
                    this_aliases
                        .iter()
                        .any(|alias| alias == &ident.escaped_text)
                })
        } else {
            false
        };
        if !is_this_or_alias {
            return None;
        }

        if is_element_access {
            let prev_preserve = self.ctx.preserve_literal_types;
            self.ctx.preserve_literal_types = true;
            let key_type = self.get_type_of_node(access.name_or_argument);
            self.ctx.preserve_literal_types = prev_preserve;
            let prop_name =
                crate::query_boundaries::type_computation::access::literal_property_name(
                    self.ctx.types,
                    key_type,
                )
                .map(|atom| self.ctx.types.resolve_atom(atom))?;
            return Some((prop_name, false, access.name_or_argument));
        }

        let name_node = self.ctx.arena.get(access.name_or_argument)?;
        if name_node.kind == SyntaxKind::PrivateIdentifier as u16 {
            return Some((String::new(), true, access.name_or_argument));
        }
        let ident = self.ctx.arena.get_identifier(name_node)?;
        Some((ident.escaped_text.clone(), false, access.name_or_argument))
    }

    /// Build a quick partial type from a class's declared members without
    /// recursing into the full class instance type resolution.
    ///
    /// This is used when a nested class expression extends its enclosing class
    /// which is currently being resolved. We extract only syntactically-visible
    /// property types (annotated properties, constructor parameter properties,
    /// and properties with simple initializers) to avoid triggering recursive
    /// resolution while still providing enough type information for property
    /// access within the nested class.
    pub(crate) fn quick_prescan_class_members(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) -> TypeId {
        let factory = self.ctx.types.factory();
        let class_sym = self.ctx.binder.get_node_symbol(class_idx);
        let mut props: Vec<PropertyInfo> = Vec::new();

        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&prop.modifiers) {
                        continue;
                    }
                    let Some(name) = self.get_property_name_resolved(prop.name) else {
                        continue;
                    };
                    let name_atom = self.ctx.types.intern_string(&name);
                    let is_readonly = self.has_readonly_modifier(&prop.modifiers)
                        || self.jsdoc_has_readonly_tag(member_idx);
                    let visibility = self.get_member_visibility(&prop.modifiers, prop.name);

                    // Try annotation first, then fall back to initializer inference
                    let type_id = if let Some(declared) =
                        self.effective_class_property_declared_type(member_idx, prop)
                    {
                        declared
                    } else if prop.initializer.is_some() {
                        let prev = self.ctx.preserve_literal_types;
                        self.ctx.preserve_literal_types = true;
                        let init_type = self.get_type_of_node(prop.initializer);
                        self.ctx.preserve_literal_types = prev;
                        if is_readonly {
                            init_type
                        } else {
                            self.widen_literal_type(init_type)
                        }
                    } else {
                        TypeId::ANY
                    };

                    props.push(PropertyInfo {
                        name: name_atom,
                        type_id,
                        write_type: type_id,
                        optional: prop.question_token,
                        readonly: is_readonly,
                        is_method: false,
                        is_class_prototype: false,
                        visibility,
                        parent_id: class_sym,
                        declaration_order: 0,
                    });
                }
                k if k == syntax_kind_ext::CONSTRUCTOR => {
                    let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                        continue;
                    };
                    for &param_idx in &ctor.parameters.nodes {
                        let Some(param_node) = self.ctx.arena.get(param_idx) else {
                            continue;
                        };
                        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                            continue;
                        };
                        if !self.has_parameter_property_modifier(&param.modifiers) {
                            continue;
                        }
                        let Some(name) = self.get_property_name(param.name) else {
                            continue;
                        };
                        let name_atom = self.ctx.types.intern_string(&name);
                        let is_readonly = self.has_readonly_modifier(&param.modifiers);
                        let type_id = if param.type_annotation.is_some() {
                            self.get_type_from_type_node(param.type_annotation)
                        } else if param.initializer.is_some() {
                            let init_type = self.get_type_of_node(param.initializer);
                            if is_readonly {
                                init_type
                            } else {
                                self.widen_literal_type(init_type)
                            }
                        } else {
                            TypeId::ANY
                        };
                        let visibility = self.get_visibility_from_modifiers(&param.modifiers);
                        props.push(PropertyInfo {
                            name: name_atom,
                            type_id,
                            write_type: type_id,
                            optional: param.question_token,
                            readonly: is_readonly,
                            is_method: false,
                            is_class_prototype: false,
                            visibility,
                            parent_id: class_sym,
                            declaration_order: 0,
                        });
                    }
                }
                _ => {}
            }
        }

        if props.is_empty() {
            return TypeId::ERROR;
        }

        let result = factory.object(props);
        // Cache the partial type so subsequent nested class expressions can use it
        self.ctx.class_instance_type_cache.insert(class_idx, result);
        result
    }
}
