impl<'a> ES5ClassTransformer<'a> {
    /// Transform a class declaration to IR with an optional override name
    pub fn transform_class_to_ir_with_name(
        &mut self,
        class_idx: NodeIndex,
        override_name: Option<&str>,
    ) -> Option<IRNode> {
        let class_node = self.arena.get(class_idx)?;
        let class_data = self.arena.get_class(class_node)?;

        // Skip ambient/declare classes
        if self
            .arena
            .has_modifier(&class_data.modifiers, SyntaxKind::DeclareKeyword)
        {
            return None;
        }

        // Get class name
        let class_name = if let Some(name) = override_name {
            name.to_string()
        } else {
            get_identifier_text(self.arena, class_data.name)?
        };

        if class_name.is_empty() {
            return None;
        }

        self.class_name = class_name;
        self.tc39_es5_member_decorators = if self.tc39_decorators {
            self.collect_tc39_es5_member_decorators(class_data)
        } else {
            Vec::new()
        };
        self.tc39_has_instance_member_decorators = self
            .tc39_es5_member_decorators
            .iter()
            .any(|member| !member.is_static && !member.is_field());

        // Collect private fields, methods, and accessors.
        let mut used_private_names = collect_enclosing_source_binding_names(self.arena, class_idx);
        self.private_fields = collect_private_fields_with_reserved(
            self.arena,
            class_idx,
            &self.class_name,
            &mut used_private_names,
        );
        self.private_methods = collect_private_methods_with_reserved(
            self.arena,
            class_idx,
            &self.class_name,
            &mut used_private_names,
        );
        self.private_accessors = collect_private_accessors_with_reserved(
            self.arena,
            class_idx,
            &self.class_name,
            &mut used_private_names,
        );
        let has_instance_private_brand =
            self.private_methods.iter().any(|method| !method.is_static)
                || self
                    .private_accessors
                    .iter()
                    .any(|accessor| !accessor.is_static);
        self.private_instances_weakset_name = has_instance_private_brand.then(|| {
            make_unique_private_name(
                &private_helper_base(&self.class_name, "instances"),
                &mut used_private_names,
            )
        });
        self.auto_accessors = collect_auto_accessor_fields(self.arena, class_idx, &self.class_name);

        // Check for extends clause
        let base_class = self.get_extends_class(&class_data.heritage_clauses);
        self.has_extends = base_class.is_some();
        self.extends_null = crate::transforms::emit_utils::extends_null_literal(
            self.arena,
            &class_data.heritage_clauses,
        );
        self.super_name = if self.has_extends {
            self.fresh_super_name()
        } else {
            "_super".to_string()
        };

        // Scan property declarations for computed names that need hoisting.
        // This must happen before constructor/member IR emission so that temps
        // are available when building property assignment IR nodes.
        self.computed_prop_temp_map.clear();
        let has_static_private_lowering = self.private_fields.iter().any(|field| field.is_static)
            || self.private_methods.iter().any(|method| method.is_static)
            || self
                .private_accessors
                .iter()
                .any(|accessor| accessor.is_static);
        self.current_static_class_alias = if self
            .static_members_need_class_alias(&class_data.members)
            || has_static_private_lowering
        {
            Some(self.generate_temp_name())
        } else if self
            .auto_accessors
            .iter()
            .any(|accessor| accessor.is_static)
        {
            Some(generated_auto_accessor_name(1))
        } else {
            None
        };
        // Each entry: (Option<temp_name>, expr_idx, member_idx) for the comma expression.
        let mut computed_prop_entries: Vec<(Option<String>, NodeIndex, NodeIndex)> = Vec::new();
        // When a static field uses a computed key the IIFE body emits `C[_x] = v`
        // which must see `_x` already assigned. Since `C` is the IIFE function name
        // (not the outer `var C` binding), all key temps must be co-located inside
        // the IIFE in declaration-before-use order. Instance-only classes use a
        // closure over an outer `var _a` instead, matching tsc's canonical form.
        let mut static_computed_iife_value_exists = false;
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
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
            if let Some(Tc39Es5MemberDecorator {
                name: Tc39Es5MemberName::Computed { key_var, .. },
                ..
            }) = self.tc39_es5_decorated_field(member_idx)
            {
                self.computed_prop_temp_map
                    .insert(computed.expression, key_var.clone());
                continue;
            }
            let Some(expr_node) = self.arena.get(computed.expression) else {
                continue;
            };
            // Skip constant expressions
            let is_constant = expr_node.kind == SyntaxKind::StringLiteral as u16
                || expr_node.kind == SyntaxKind::NumericLiteral as u16
                || expr_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16;
            if is_constant {
                continue;
            }
            // Check if this property is erased
            // `declare` fields have no runtime effect even when an
            // initializer is present, so the computed expression must
            // emit only as a side-effect statement (no temp). Mirrors
            // the ES2015+ path in `emit_es6.rs`. Without this, ES5
            // emission allocated `var _a; _a = field3;` for ambient
            // declared static decorated fields.
            let is_erased = if self
                .arena
                .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                || self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
            {
                true
            } else {
                let is_private = self
                    .arena
                    .get(prop.name)
                    .is_some_and(|n| n.kind == SyntaxKind::PrivateIdentifier as u16);
                let has_accessor = self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword);
                !self.property_initializer_has_equals(member_node, prop)
                    && !self.use_define_for_class_fields
                    && !is_private
                    && !has_accessor
            };
            if is_erased {
                // Side-effect only: emit expression for effects but no temp.
                // Check if the expression (possibly wrapped in type assertions) is
                // a simple identifier or keyword literal.
                let is_side_effect_free =
                    Self::is_expr_side_effect_free(self.arena, computed.expression);
                if !is_side_effect_free {
                    computed_prop_entries.push((None, computed.expression, member_idx));
                }
            } else {
                let temp = self.generate_temp_name();
                self.computed_prop_temp_map
                    .insert(computed.expression, temp.clone());
                computed_prop_entries.push((Some(temp), computed.expression, member_idx));
                if self.arena.is_static(&prop.modifiers)
                    && (self.property_initializer_has_equals(member_node, prop)
                        || self
                            .arena
                            .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword))
                {
                    static_computed_iife_value_exists = true;
                }
            }
        }
        let consumed_computed_auto_accessor_entries: Vec<usize> =
            if let Some(first_accessor) = self.first_computed_instance_auto_accessor() {
                computed_prop_entries
                    .iter()
                    .enumerate()
                    .filter_map(|(entry_idx, (_, _, member_idx))| {
                        (*member_idx == first_accessor.member_idx).then_some(entry_idx)
                    })
                    .collect()
            } else {
                Vec::new()
            };
        let consumed_computed_auto_accessor_temps: Vec<String> =
            consumed_computed_auto_accessor_entries
                .iter()
                .filter_map(|entry_idx| computed_prop_entries[*entry_idx].0.clone())
                .collect();

        let computed_prop_temp_decls: Vec<String> = computed_prop_entries
            .iter()
            .enumerate()
            .filter_map(|(entry_idx, (temp, _, _))| {
                (!consumed_computed_auto_accessor_entries.contains(&entry_idx))
                    .then(|| temp.clone())
                    .flatten()
            })
            .collect();
        let mut computed_prop_init_entries = Vec::new();
        if !computed_prop_entries.is_empty() {
            let mut comma_parts: Vec<IRNode> = Vec::new();
            for (entry_idx, (temp_name, expr_idx, _)) in computed_prop_entries.iter().enumerate() {
                if consumed_computed_auto_accessor_entries.contains(&entry_idx) {
                    continue;
                }
                let expr_ir = if let Some(alias) = self.inherited_computed_name_this.as_ref() {
                    self.convert_expression_static_with_raw_this_substitution(*expr_idx, alias)
                } else {
                    self.convert_expression(*expr_idx)
                };
                if let Some(temp) = temp_name {
                    comma_parts.push(IRNode::assign(IRNode::id(temp.clone()), expr_ir));
                } else {
                    comma_parts.push(expr_ir);
                }
            }
            if !comma_parts.is_empty() {
                let result = comma_parts
                    .into_iter()
                    .reduce(|left, right| IRNode::BinaryExpr {
                        left: Box::new(left),
                        operator: std::borrow::Cow::Borrowed(","),
                        right: Box::new(right),
                    })
                    .unwrap();
                computed_prop_init_entries.push(IRNode::ExpressionStatement(Box::new(result)));
            }
        }

        // Build IIFE body
        let mut body = Vec::new();

        // __extends(ClassName, _super);
        if self.has_extends {
            body.push(IRNode::ExtendsHelper {
                class_name: self.class_name.clone().into(),
                super_name: self.super_name.clone().into(),
            });
        }

        // Constructor function
        if let Some(ctor_ir) = self.emit_constructor_ir(class_idx) {
            body.push(ctor_ir);
        }
        if let Some(alias) = self.class_self_reference_alias.as_ref() {
            body.push(IRNode::expr_stmt(IRNode::assign(
                IRNode::id(alias.clone()),
                IRNode::id(self.class_name.clone()),
            )));
        }
        // When emitting a class expression that needs the comma pattern
        // (_classTemp = IIFE, _propTemp = expr, _classTemp), the caller owns
        // the temp hoisting and inline initialization.  In that mode we leave
        // the body clean and carry the data in the ES5ClassIIFE node fields.
        let (ir_computed_prop_temp_decls, ir_computed_prop_temp_inits) =
            if self.emit_computed_props_outside.get() {
                (computed_prop_temp_decls, computed_prop_init_entries)
            } else if static_computed_iife_value_exists {
                if !computed_prop_temp_decls.is_empty() {
                    let var_decls: Vec<IRNode> = computed_prop_temp_decls
                        .into_iter()
                        .map(|name| IRNode::var_decl(name, None))
                        .collect();
                    body.push(IRNode::VarDeclList(var_decls));
                }
                body.extend(computed_prop_init_entries);
                (Vec::new(), Vec::new())
            } else {
                (computed_prop_temp_decls, computed_prop_init_entries)
            };
        // Prototype methods and static members interleaved in source order
        let deferred_static_blocks = self.emit_all_members_ir(&mut body, class_idx);

        // Legacy decorator __decorate calls (inside IIFE, before return)
        if self.legacy_decorators {
            self.emit_member_decorator_ir(&mut body, class_idx);
        }
        if !self.class_decorators.is_empty() {
            if let Some(alias) = self.class_self_reference_alias.as_ref()
                && !self.has_static_property_initializer(&class_data.members)
            {
                body.push(IRNode::VarDecl {
                    name: alias.clone().into(),
                    initializer: None,
                });
            }
            self.emit_class_decorator_ir(&mut body, class_idx);
        } else if self.legacy_decorators {
            // Even without class-level decorators, constructor parameter decorators
            // need a class-level __decorate call: C = __decorate([__param(0, dec)], C)
            self.emit_ctor_param_decorator_ir(&mut body, class_idx);
        }

        // Emit var declarations for hoisted temp variables collected during
        // member expression conversion (e.g., from computed property lowering
        // inside object literals like `{ [expr]: val }` → `(_a = {}, _a[expr] = val, _a)`).
        let extra_temps: Vec<String> = std::mem::take(&mut *self.extra_hoisted_temps.borrow_mut());
        if !extra_temps.is_empty() {
            let var_decls: Vec<IRNode> = extra_temps
                .into_iter()
                .map(|name| IRNode::VarDecl {
                    name: name.into(),
                    initializer: None,
                })
                .collect();
            // tsc puts `var _a;` at the very top of the IIFE body, before __extends.
            body.insert(0, IRNode::VarDeclList(var_decls));
        }

        if self.auto_accessor_storage_decls_in_iife() {
            self.emit_auto_accessor_storage_decls_and_static_inits(&mut body);
        }

        // Build private storage declarations and instantiations.
        let private_storage_decls = self.private_storage_declarations_in_tsc_order(class_data);
        let mut weakmap_decls: Vec<String> = Vec::new();
        let auto_accessor_decls_in_iife = self.auto_accessor_storage_decls_in_iife();
        for accessor in &self.auto_accessors {
            if !accessor.is_static && !auto_accessor_decls_in_iife {
                weakmap_decls.push(accessor.weakmap_name.clone());
            }
        }
        weakmap_decls.extend(consumed_computed_auto_accessor_temps);

        // Private helper instantiations and extracted method/accessor functions.
        let mut weakmap_inits: Vec<String> = Vec::new();
        if has_static_private_lowering && let Some(alias) = self.current_static_class_alias.as_ref()
        {
            weakmap_inits.push(format!("{alias} = {}", self.class_name));
        }
        weakmap_inits.extend(
            self.private_fields
                .iter()
                .filter(|f| !f.is_static)
                .map(|f| format!("{} = new WeakMap()", f.weakmap_name)),
        );
        if let Some(instances) = self.private_instances_weakset_name.as_ref() {
            weakmap_inits.push(format!("{instances} = new WeakSet()"));
        }
        weakmap_inits.extend(self.private_method_and_accessor_init_strings());
        let post_weakmap_statements = self.static_private_field_init_strings();
        if !private_storage_decls.is_empty() {
            body.push(IRNode::VarDeclList(
                private_storage_decls
                    .into_iter()
                    .map(|name| IRNode::VarDecl {
                        name: name.into(),
                        initializer: None,
                    })
                    .collect(),
            ));
            if !weakmap_inits.is_empty() {
                body.push(IRNode::expr_stmt(IRNode::Raw(
                    weakmap_inits.join(", ").into(),
                )));
            }
            body.extend(
                post_weakmap_statements
                    .into_iter()
                    .map(|statement| IRNode::expr_stmt(IRNode::Raw(statement.into()))),
            );
            weakmap_inits = Vec::new();
        } else {
            weakmap_decls.extend(private_storage_decls);
        }
        let auto_accessor_instance_inits_in_computed_key =
            self.first_computed_instance_auto_accessor().is_some();
        for accessor in &self.auto_accessors {
            if !accessor.is_static
                && !auto_accessor_decls_in_iife
                && !auto_accessor_instance_inits_in_computed_key
            {
                weakmap_inits.push(format!("{} = new WeakMap()", accessor.weakmap_name));
            }
        }
        let post_weakmap_statements = Vec::new();

        // return ClassName;
        body.push(IRNode::ret(Some(IRNode::id(self.class_name.clone()))));

        // When the class has auto-accessor members, the statement-level comment
        // handler in source_file.rs intentionally skips leading comments (to
        // avoid emitting them before the WeakMap storage declarations). In that
        // case we extract the comment here so the IR printer can place it
        // between the storage declarations and the class IIFE.
        // For classes without auto-accessors the source_file handler emits the
        // comment normally, so we pass None to avoid duplicates.
        let leading_comment = if !self.auto_accessors.is_empty() {
            self.extract_leading_comment(class_node)
        } else {
            None
        };
        // The deferred static block IIFEs (rendered after the class IIFE) only
        // need an outside class-value alias when lowering actually referenced
        // that alias. Recovered `super()` calls in invalid static blocks, for
        // example, still lower through `_super.call(this)` and should not create
        // a dead class alias.
        let deferred_block_class_alias = self
            .current_static_class_alias
            .as_ref()
            .filter(|alias| {
                deferred_static_blocks
                    .iter()
                    .any(|block| block.contains_identifier(alias))
            })
            .cloned();
        Some(IRNode::ES5ClassIIFE {
            name: self.class_name.clone().into(),
            binding_name: None,
            base_class: base_class.map(Box::new),
            super_param: self.has_extends.then(|| self.super_name.clone().into()),
            body,
            weakmap_decls,
            computed_prop_temp_decls: ir_computed_prop_temp_decls,
            computed_prop_temp_inits: ir_computed_prop_temp_inits,
            weakmap_inits,
            post_weakmap_statements,
            leading_comment,
            deferred_static_blocks,
            deferred_block_class_alias,
        })
    }
}
