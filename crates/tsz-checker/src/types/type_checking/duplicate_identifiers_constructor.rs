use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    /// Check for constructor parameter property conflicts with explicit class properties.
    ///
    /// When a constructor parameter has a visibility modifier (`public`, `private`, `protected`)
    /// or `readonly`, it implicitly declares a class property. If the class also has an explicit
    /// property with the same name, tsc reports:
    /// - TS2300 ("Duplicate identifier") on the parameter property
    /// - TS2687 ("All declarations must have identical modifiers") on both declarations
    ///   when the access modifiers differ
    pub(crate) fn check_constructor_parameter_property_conflicts(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_parser::parser::syntax_kind_ext;

        // Iterate over all class scopes in the binder
        for scope in &self.ctx.binder.scopes {
            if scope.kind != tsz_binder::ContainerKind::Class {
                continue;
            }

            let class_node_idx = scope.container_node;
            let Some(class_node) = self.ctx.arena.get(class_node_idx) else {
                continue;
            };
            let Some(class_data) = self.ctx.arena.get_class(class_node) else {
                continue;
            };

            // Collect all constructor declarations in source order so overload signatures
            // are seen before the implementation, matching tsc's duplicate reporting.
            let mut constructor_indices = Vec::new();
            for &member_idx in &class_data.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind == syntax_kind_ext::CONSTRUCTOR {
                    constructor_indices.push(member_idx);
                }
            }
            if constructor_indices.is_empty() {
                continue;
            }

            // Collect explicit property declarations with their names
            // Map: property name -> (property NodeIndex, has_private, has_protected)
            let mut explicit_props: Vec<(String, NodeIndex, u8)> = Vec::new();
            for &member_idx in &class_data.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                    continue;
                }
                let Some(prop_data) = self.ctx.arena.get_property_decl(member_node) else {
                    continue;
                };
                // Skip static properties — they don't conflict with instance parameter properties
                if self
                    .ctx
                    .arena
                    .has_modifier_ref(prop_data.modifiers.as_ref(), SyntaxKind::StaticKeyword)
                {
                    continue;
                }
                let Some(name_node) = self.ctx.arena.get(prop_data.name) else {
                    continue;
                };
                let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                    continue;
                };
                let access = self.parameter_access_modifier(prop_data.modifiers.as_ref());
                explicit_props.push((ident.escaped_text.clone(), prop_data.name, access));
            }

            let mut seen_param_props: FxHashSet<String> = FxHashSet::default();
            for constructor_idx in constructor_indices {
                let Some(ctor_node) = self.ctx.arena.get(constructor_idx) else {
                    continue;
                };
                let Some(ctor_data) = self.ctx.arena.get_constructor(ctor_node) else {
                    continue;
                };

                // Check each constructor parameter for parameter property modifiers
                for &param_idx in &ctor_data.parameters.nodes {
                    let Some(param_node) = self.ctx.arena.get(param_idx) else {
                        continue;
                    };
                    let Some(param_data) = self.ctx.arena.get_parameter(param_node) else {
                        continue;
                    };

                    // Check if this parameter has an accessibility modifier (public/private/protected/readonly)
                    let has_accessibility =
                        self.ctx.arena.has_modifier_ref(
                            param_data.modifiers.as_ref(),
                            SyntaxKind::PublicKeyword,
                        ) || self.ctx.arena.has_modifier_ref(
                            param_data.modifiers.as_ref(),
                            SyntaxKind::PrivateKeyword,
                        ) || self.ctx.arena.has_modifier_ref(
                            param_data.modifiers.as_ref(),
                            SyntaxKind::ProtectedKeyword,
                        ) || self.ctx.arena.has_modifier_ref(
                            param_data.modifiers.as_ref(),
                            SyntaxKind::ReadonlyKeyword,
                        );

                    if !has_accessibility {
                        continue;
                    }

                    // Get the parameter name
                    let Some(param_name_node) = self.ctx.arena.get(param_data.name) else {
                        continue;
                    };
                    let Some(param_ident) = self.ctx.arena.get_identifier(param_name_node) else {
                        continue;
                    };

                    let param_access =
                        self.parameter_access_modifier(param_data.modifiers.as_ref());

                    // Check if an explicit property with the same name exists
                    for (prop_name, prop_name_idx, prop_access) in &explicit_props {
                        if param_ident.escaped_text != *prop_name {
                            continue;
                        }

                        // TS2300: Duplicate identifier on the parameter
                        let dup_msg =
                            format_message(diagnostic_messages::DUPLICATE_IDENTIFIER, &[prop_name]);
                        self.error_at_node(
                            param_data.name,
                            &dup_msg,
                            diagnostic_codes::DUPLICATE_IDENTIFIER,
                        );

                        // TS2687: All declarations must have identical modifiers
                        // Report on both the property and the parameter when modifiers differ
                        if param_access != *prop_access {
                            let mod_msg = format_message(
                                diagnostic_messages::ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_MODIFIERS,
                                &[prop_name],
                            );
                            // Report on the explicit property declaration
                            self.error_at_node(
                                *prop_name_idx,
                                &mod_msg,
                                diagnostic_codes::ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_MODIFIERS,
                            );
                            // Report on the constructor parameter
                            self.error_at_node(
                                param_data.name,
                                &mod_msg,
                                diagnostic_codes::ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_MODIFIERS,
                            );
                        }

                        break;
                    }
                }

                self.check_duplicate_constructor_parameter_properties(
                    &ctor_data.parameters.nodes,
                    &mut seen_param_props,
                );
            }
        }
    }

    fn check_duplicate_constructor_parameter_properties(
        &mut self,
        parameter_nodes: &[NodeIndex],
        seen_param_props: &mut FxHashSet<String>,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        for &param_idx in parameter_nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param_data) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            if !self.has_parameter_property_modifier(&param_data.modifiers) {
                continue;
            }
            let Some(param_name_node) = self.ctx.arena.get(param_data.name) else {
                continue;
            };
            let Some(param_ident) = self.ctx.arena.get_identifier(param_name_node) else {
                continue;
            };

            if !seen_param_props.insert(param_ident.escaped_text.clone()) {
                let dup_msg = format_message(
                    diagnostic_messages::DUPLICATE_IDENTIFIER,
                    &[&param_ident.escaped_text],
                );
                self.error_at_node(
                    param_data.name,
                    &dup_msg,
                    diagnostic_codes::DUPLICATE_IDENTIFIER,
                );
            }
        }
    }

    /// Get an access modifier level for a declaration's modifier list.
    /// Returns: 0 = public (default), 1 = private, 2 = protected
    fn parameter_access_modifier(&self, modifiers: Option<&tsz_parser::parser::NodeList>) -> u8 {
        if self
            .ctx
            .arena
            .has_modifier_ref(modifiers, SyntaxKind::PrivateKeyword)
        {
            1
        } else if self
            .ctx
            .arena
            .has_modifier_ref(modifiers, SyntaxKind::ProtectedKeyword)
        {
            2
        } else {
            0 // public or no modifier
        }
    }
}
