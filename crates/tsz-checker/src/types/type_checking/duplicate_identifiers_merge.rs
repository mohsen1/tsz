use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::Visibility;

impl<'a> CheckerState<'a> {
    /// Check diagnostics specific to merged class+interface declarations.
    ///
    /// - TS2687: All declarations of a merged member must have identical modifiers.
    pub(crate) fn check_merged_class_interface_declaration_diagnostics(
        &mut self,
        declarations: &[NodeIndex],
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        if declarations.len() <= 1 {
            return;
        }

        let has_class = declarations.iter().any(|&decl_idx| {
            self.ctx
                .arena
                .get(decl_idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::CLASS_DECLARATION)
        });
        let has_interface = declarations.iter().any(|&decl_idx| {
            self.ctx
                .arena
                .get(decl_idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::INTERFACE_DECLARATION)
        });
        if !has_class || !has_interface {
            return;
        }

        let mut declarations_by_position = declarations.to_vec();
        declarations_by_position.sort_by_key(|&decl_idx| {
            self.ctx
                .arena
                .get(decl_idx)
                .map(|node| node.pos)
                .unwrap_or(u32::MAX)
        });

        let mut seen_members: FxHashMap<String, (Visibility, NodeIndex)> = FxHashMap::default();
        let mut seen_name_by_node: FxHashMap<NodeIndex, String> = FxHashMap::default();
        let mut error_nodes: FxHashSet<NodeIndex> = FxHashSet::default();

        for &decl_idx in &declarations_by_position {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };

            let member_nodes: Vec<NodeIndex> = match node.kind {
                syntax_kind_ext::CLASS_DECLARATION => self
                    .ctx
                    .arena
                    .get_class(node)
                    .map(|class_data| class_data.members.nodes.clone())
                    .unwrap_or_default(),
                syntax_kind_ext::INTERFACE_DECLARATION => self
                    .ctx
                    .arena
                    .get_interface(node)
                    .map(|interface_data| interface_data.members.nodes.clone())
                    .unwrap_or_default(),
                _ => Vec::new(),
            };

            for &member_idx in &member_nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };

                let Some((name_idx, visibility)) = (match member_node.kind {
                    syntax_kind_ext::PROPERTY_DECLARATION => {
                        self.ctx.arena.get_property_decl(member_node).map(|prop| {
                            (
                                prop.name,
                                self.get_visibility_from_modifiers(&prop.modifiers),
                            )
                        })
                    }
                    syntax_kind_ext::METHOD_DECLARATION => {
                        self.ctx.arena.get_method_decl(member_node).map(|method| {
                            (
                                method.name,
                                self.get_visibility_from_modifiers(&method.modifiers),
                            )
                        })
                    }
                    syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                        self.ctx.arena.get_accessor(member_node).map(|accessor| {
                            (
                                accessor.name,
                                self.get_visibility_from_modifiers(&accessor.modifiers),
                            )
                        })
                    }
                    syntax_kind_ext::PROPERTY_SIGNATURE | syntax_kind_ext::METHOD_SIGNATURE => {
                        self.ctx.arena.get_signature(member_node).map(|sig| {
                            (sig.name, self.get_visibility_from_modifiers(&sig.modifiers))
                        })
                    }
                    _ => None,
                }) else {
                    continue;
                };
                let Some(member_name) = self.get_property_name(name_idx) else {
                    continue;
                };
                seen_name_by_node.insert(name_idx, member_name.clone());

                if let Some((existing_visibility, existing_name_idx)) =
                    seen_members.get(&member_name)
                {
                    if *existing_visibility != visibility {
                        error_nodes.insert(*existing_name_idx);
                        error_nodes.insert(name_idx);
                    }
                    continue;
                }

                seen_members.insert(member_name.clone(), (visibility, name_idx));
            }
        }

        for error_node in error_nodes {
            let Some(member_name) = seen_name_by_node.get(&error_node) else {
                continue;
            };
            let message = crate::diagnostics::format_message(
                diagnostic_messages::ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_MODIFIERS,
                &[member_name],
            );
            self.error_at_node(
                error_node,
                &message,
                diagnostic_codes::ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_MODIFIERS,
            );
        }
    }
}
