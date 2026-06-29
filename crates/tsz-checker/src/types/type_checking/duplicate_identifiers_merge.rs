use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{TypeId, Visibility};

/// A resolved instance member contributed by one declaration of a merged
/// class+interface symbol. Carries the comparison type and enough provenance to
/// anchor a diagnostic on the right declaration in source order.
struct MergedMemberType {
    /// Property name node — diagnostics anchor here (TSC points at the name).
    name_node: NodeIndex,
    /// Source position of the owning member, used to order the two conflicting
    /// declarations (the earlier one supplies the canonical type).
    pos: u32,
    /// Resolved comparison type: the property type, or the function type for a
    /// method.
    ty: TypeId,
}

impl<'a> CheckerState<'a> {
    /// Check diagnostics specific to merged class+interface declarations.
    ///
    /// - TS2687: All declarations of a merged member must have identical modifiers.
    /// - TS2717: A property declared on both halves with differing types.
    /// - TS2394: An interface method signature incompatible with the class
    ///   method implementation it merges with (overload-vs-implementation).
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

            // Borrow the member list rather than clone: the loop only reads from
            // `&self` (member-name and visibility queries), so the arena borrow
            // persists across iterations instead of allocating a fresh `Vec` per
            // merged class / interface declaration.
            let member_nodes: &[NodeIndex] = match node.kind {
                syntax_kind_ext::CLASS_DECLARATION => self
                    .ctx
                    .arena
                    .get_class(node)
                    .map(|class_data| class_data.members.nodes.as_slice())
                    .unwrap_or_default(),
                syntax_kind_ext::INTERFACE_DECLARATION => self
                    .ctx
                    .arena
                    .get_interface(node)
                    .map(|interface_data| interface_data.members.nodes.as_slice())
                    .unwrap_or_default(),
                _ => &[],
            };

            for &member_idx in member_nodes {
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

        self.check_merged_class_interface_member_type_conflicts(&declarations_by_position);
    }

    /// Compare the resolved member *types* across a merged class+interface
    /// symbol, the gap that left tsz silent where tsc reports TS2717/TS2394.
    ///
    /// Structural rule: when a class and a merging interface (any declaration
    /// order) both declare an instance member of the same name, tsc folds them
    /// into one symbol and enforces type agreement.
    /// - Two property-like members whose widened types differ -> TS2717,
    ///   anchored on the *later* declaration, naming the earlier (canonical)
    ///   type. This mirrors `checkVariableLikeDeclaration`, where the symbol's
    ///   `valueDeclaration` is the first-bound declaration and every subsequent
    ///   one must match it.
    /// - An interface method signature is an *overload* of the class method
    ///   *implementation*; an overload incompatible with the implementation ->
    ///   TS2394, anchored on the interface signature. This reuses the same
    ///   `isImplementationCompatibleWithOverload` relation the intra-class
    ///   overload path uses.
    ///
    /// Only instance members participate: `static` class members live on the
    /// constructor side and never merge with the interface. Interface+interface
    /// conflicts are owned by `check_merged_interface_declaration_diagnostics`,
    /// so this path compares strictly across the class/interface boundary.
    fn check_merged_class_interface_member_type_conflicts(
        &mut self,
        declarations_sorted: &[NodeIndex],
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        // Snapshot the member lists (owned) so the arena borrow is released
        // before `&mut self` type resolution runs below.
        struct DeclMembers {
            is_class: bool,
            type_params: Option<tsz_parser::parser::NodeList>,
            members: Vec<NodeIndex>,
        }
        let mut decl_members: Vec<DeclMembers> = Vec::new();
        for &decl_idx in declarations_sorted {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            match node.kind {
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    if let Some(class) = self.ctx.arena.get_class(node) {
                        decl_members.push(DeclMembers {
                            is_class: true,
                            type_params: class.type_parameters.clone(),
                            members: class.members.nodes.clone(),
                        });
                    }
                }
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                    if let Some(iface) = self.ctx.arena.get_interface(node) {
                        decl_members.push(DeclMembers {
                            is_class: false,
                            type_params: iface.type_parameters.clone(),
                            members: iface.members.nodes.clone(),
                        });
                    }
                }
                _ => {}
            }
        }

        // Earliest-by-position property/method per name on each side. Methods
        // are kept as a list because an interface may contribute several
        // overload signatures for one name.
        let mut interface_props: FxHashMap<String, MergedMemberType> = FxHashMap::default();
        let mut interface_methods: FxHashMap<String, Vec<MergedMemberType>> = FxHashMap::default();
        let mut class_props: FxHashMap<String, MergedMemberType> = FxHashMap::default();
        let mut class_method_impls: FxHashMap<String, MergedMemberType> = FxHashMap::default();

        // The annotated type node is `NONE` for class fields inferred from an
        // initializer and for accessors; both route through the member helper.
        enum Shape {
            Property {
                name_idx: NodeIndex,
                type_annotation: NodeIndex,
                initializer: NodeIndex,
            },
            Method {
                name_idx: NodeIndex,
                has_body: bool,
            },
            Skip,
        }

        for dm in &decl_members {
            let (_, updates) = self.push_type_parameters(&dm.type_params);

            for &member_idx in &dm.members {
                // Phase 1: read the member shape and position under the
                // immutable arena borrow only.
                let mut pos = u32::MAX;
                let shape = {
                    let Some(member_node) = self.ctx.arena.get(member_idx) else {
                        continue;
                    };
                    pos = member_node.pos;
                    let instance = |modifiers: &Option<tsz_parser::parser::NodeList>| {
                        !self.has_static_modifier(modifiers)
                    };
                    match member_node.kind {
                        syntax_kind_ext::PROPERTY_DECLARATION if dm.is_class => self
                            .ctx
                            .arena
                            .get_property_decl(member_node)
                            .filter(|prop| instance(&prop.modifiers))
                            .map(|prop| Shape::Property {
                                name_idx: prop.name,
                                type_annotation: prop.type_annotation,
                                initializer: prop.initializer,
                            })
                            .unwrap_or(Shape::Skip),
                        syntax_kind_ext::GET_ACCESSOR if dm.is_class => self
                            .ctx
                            .arena
                            .get_accessor(member_node)
                            .filter(|acc| instance(&acc.modifiers))
                            .map(|acc| Shape::Property {
                                name_idx: acc.name,
                                type_annotation: NodeIndex::NONE,
                                initializer: NodeIndex::NONE,
                            })
                            .unwrap_or(Shape::Skip),
                        syntax_kind_ext::METHOD_DECLARATION if dm.is_class => self
                            .ctx
                            .arena
                            .get_method_decl(member_node)
                            .filter(|method| instance(&method.modifiers))
                            .map(|method| Shape::Method {
                                name_idx: method.name,
                                has_body: method.body.is_some(),
                            })
                            .unwrap_or(Shape::Skip),
                        syntax_kind_ext::PROPERTY_SIGNATURE if !dm.is_class => self
                            .ctx
                            .arena
                            .get_signature(member_node)
                            .map(|sig| Shape::Property {
                                name_idx: sig.name,
                                type_annotation: sig.type_annotation,
                                initializer: NodeIndex::NONE,
                            })
                            .unwrap_or(Shape::Skip),
                        syntax_kind_ext::METHOD_SIGNATURE if !dm.is_class => self
                            .ctx
                            .arena
                            .get_signature(member_node)
                            .map(|sig| Shape::Method {
                                name_idx: sig.name,
                                has_body: false,
                            })
                            .unwrap_or(Shape::Skip),
                        _ => Shape::Skip,
                    }
                };

                // Phase 2: resolve the comparison type with `&mut self`.
                match shape {
                    Shape::Skip => {}
                    Shape::Property {
                        name_idx,
                        type_annotation,
                        initializer,
                    } => {
                        let Some(name) = self.get_property_name(name_idx) else {
                            continue;
                        };
                        let ty = if type_annotation.is_some() {
                            self.get_type_from_type_node(type_annotation)
                        } else if dm.is_class {
                            // Class field/accessor with no annotation: the type
                            // comes from the initializer, widened the way
                            // `getWidenedTypeForVariableLikeDeclaration` widens
                            // it (`p = "x"` -> `string`, not `"x"`). Accessors
                            // (no initializer) route through the member helper.
                            if initializer.is_some() {
                                let init = self.get_type_of_node(initializer);
                                self.widen_literal_type(init)
                            } else {
                                self.get_type_of_class_member(member_idx)
                            }
                        } else {
                            TypeId::ANY
                        };
                        let entry = MergedMemberType {
                            name_node: name_idx,
                            pos,
                            ty,
                        };
                        if dm.is_class {
                            class_props.entry(name).or_insert(entry);
                        } else {
                            interface_props.entry(name).or_insert(entry);
                        }
                    }
                    Shape::Method { name_idx, has_body } => {
                        let Some(name) = self.get_property_name(name_idx) else {
                            continue;
                        };
                        let ty = if dm.is_class {
                            self.get_type_of_class_member(member_idx)
                        } else {
                            self.get_type_of_interface_member_simple(member_idx)
                        };
                        let entry = MergedMemberType {
                            name_node: name_idx,
                            pos,
                            ty,
                        };
                        if dm.is_class {
                            // Only the implementation (the one with a body) is a
                            // valid target for the overload-compatibility check.
                            if has_body {
                                class_method_impls.entry(name).or_insert(entry);
                            }
                        } else {
                            interface_methods.entry(name).or_default().push(entry);
                        }
                    }
                }
            }

            self.pop_type_parameters(updates);
        }

        // TS2717: a property declared on both halves with differing widened
        // types. Final diagnostics are position-sorted downstream, so emit
        // directly. The earlier declaration is the symbol's value declaration
        // in tsc; it supplies the canonical "must be of type" target, and the
        // error anchors on the later one.
        for (name, class_member) in &class_props {
            let Some(iface_member) = interface_props.get(name) else {
                continue;
            };
            if self.type_contains_error(class_member.ty)
                || self.type_contains_error(iface_member.ty)
            {
                continue;
            }
            let (first, second) = if iface_member.pos <= class_member.pos {
                (iface_member, class_member)
            } else {
                (class_member, iface_member)
            };
            if !self.duplicate_decl_types_match(first.ty, second.ty) {
                let expected = self.format_type(first.ty);
                let actual = self.format_type(second.ty);
                self.error_at_node_msg(
                    second.name_node,
                    diagnostic_codes::SUBSEQUENT_PROPERTY_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_PROPERTY_MUST_BE_OF_TYP,
                    &[name, &expected, &actual],
                );
            }
        }

        // TS2394: an interface method signature incompatible with the class
        // method implementation it merges into.
        for (name, overloads) in &interface_methods {
            let Some(class_impl) = class_method_impls.get(name) else {
                continue;
            };
            if self.type_contains_error(class_impl.ty) {
                continue;
            }
            for overload in overloads {
                if self.type_contains_error(overload.ty) {
                    continue;
                }
                if !self.is_implementation_compatible_with_overload_inner(
                    class_impl.ty,
                    overload.ty,
                    /* bivariant_params */ true,
                ) {
                    self.error_at_node(
                        overload.name_node,
                        diagnostic_messages::THIS_OVERLOAD_SIGNATURE_IS_NOT_COMPATIBLE_WITH_ITS_IMPLEMENTATION_SIGNATURE,
                        diagnostic_codes::THIS_OVERLOAD_SIGNATURE_IS_NOT_COMPATIBLE_WITH_ITS_IMPLEMENTATION_SIGNATURE,
                    );
                    // tsc reports only the first incompatible overload.
                    break;
                }
            }
        }
    }
}
