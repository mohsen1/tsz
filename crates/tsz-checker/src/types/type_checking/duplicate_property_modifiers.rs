use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext::PROPERTY_SIGNATURE;
use tsz_solver::TypeId;

/// The `readonly` / optional modifier signature of a single property
/// declaration. Two declarations of the same property must carry identical
/// flags or `tsc` reports TS2687.
#[derive(Clone, Copy, PartialEq, Eq)]
struct PropertyModifierFlags {
    readonly: bool,
    optional: bool,
}

impl<'a> CheckerState<'a> {
    /// Check for duplicate property names in type literals (TS2300), differing
    /// member types (TS2717), and modifier disagreements (TS2687).
    ///
    /// e.g. `{ a: string; a: number; }` has duplicate property `a`. Method
    /// signatures (overloads) with the same name are allowed — only property
    /// signatures are checked for duplicates.
    pub(crate) fn check_type_literal_duplicate_properties(&mut self, members: &[NodeIndex]) {
        use crate::diagnostics::diagnostic_codes;

        // Track (member_idx, type_annotation, is_syntactic_name) for TS2717 comparison.
        // `is_syntactic_name` is true when the name was determined from syntax alone
        // (literal property name), false when it required evaluating a computed expression.
        let mut seen: rustc_hash::FxHashMap<String, (NodeIndex, NodeIndex, bool)> =
            rustc_hash::FxHashMap::default();
        // Canonical name -> property-signature member nodes (source order) for
        // names that occur more than once. Only populated on a duplicate hit, so
        // the common (no-duplicate) type literal allocates nothing extra. Feeds
        // the TS2687 modifier-agreement check after the duplicate scan.
        let mut duplicate_groups: rustc_hash::FxHashMap<String, Vec<NodeIndex>> =
            rustc_hash::FxHashMap::default();

        for &member_idx in members {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            // Only check property signatures for duplicates.
            // Method signatures with the same name are valid overloads.
            if member_node.kind != PROPERTY_SIGNATURE {
                continue;
            }

            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                continue;
            };
            // Try syntactic name first; fall back to resolved computed property name.
            // This handles cases like `[c0]` where c0 is a const variable — the
            // property name can only be determined by evaluating the expression type.
            let (name, is_syntactic) = if let Some(n) = self.get_member_name(member_idx) {
                (n, true)
            } else if let Some(n) = self.get_property_name_resolved(sig.name) {
                (n, false)
            } else {
                continue;
            };
            let type_ann = sig.type_annotation;

            if let Some(&(prev_idx, prev_type_ann, prev_syntactic)) = seen.get(&name) {
                let name_idx = sig.name;

                // Record the full duplicate group (first declaration once, then
                // each subsequent one) for the TS2687 modifier check below.
                duplicate_groups
                    .entry(name.clone())
                    .or_insert_with(|| vec![prev_idx])
                    .push(member_idx);

                // TS2300 "Duplicate identifier" only when both declarations use
                // syntactic (literal) names. Computed property names that resolve
                // to the same value (e.g., `[c0]` and `[c1]` where c0="1", c1=1)
                // get only TS2717, matching tsc behavior.
                if is_syntactic && prev_syntactic {
                    self.error_at_node(
                        name_idx,
                        &format!("Duplicate identifier '{name}'."),
                        diagnostic_codes::DUPLICATE_IDENTIFIER,
                    );
                    // Also mark the first occurrence
                    if let Some(prev_node) = self.ctx.arena.get(prev_idx) {
                        let prev_name_idx =
                            if let Some(prev_sig) = self.ctx.arena.get_signature(prev_node) {
                                prev_sig.name
                            } else {
                                prev_idx
                            };
                        self.error_at_node(
                            prev_name_idx,
                            &format!("Duplicate identifier '{name}'."),
                            diagnostic_codes::DUPLICATE_IDENTIFIER,
                        );
                    }
                }

                // TS2717 on the subsequent declaration when types differ.
                // Use display text for the property name to match TSC's
                // declarationNameToString (e.g., "1.0" not "1").
                let first_type = if prev_type_ann.is_some() {
                    self.get_type_from_type_node(prev_type_ann)
                } else {
                    TypeId::ANY
                };
                let this_type = if type_ann.is_some() {
                    self.get_type_from_type_node(type_ann)
                } else {
                    TypeId::ANY
                };
                if !self.type_contains_error(first_type)
                    && !self.type_contains_error(this_type)
                    && first_type != this_type
                {
                    let display_name = self
                        .get_member_name_display_text(name_idx)
                        .unwrap_or_else(|| name.clone());
                    let first_type_str = self.format_type(first_type);
                    let this_type_str = self.format_type(this_type);
                    self.error_at_node_msg(
                        name_idx,
                        diagnostic_codes::SUBSEQUENT_PROPERTY_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_PROPERTY_MUST_BE_OF_TYP,
                        &[&display_name, &first_type_str, &this_type_str],
                    );
                }
            } else {
                seen.insert(name, (member_idx, type_ann, is_syntactic));
            }
        }

        for (name, member_nodes) in &duplicate_groups {
            self.report_property_modifier_disagreements(name, member_nodes);
        }
    }

    /// TS2687: "All declarations of '{0}' must have identical modifiers."
    ///
    /// `tsc` raises this whenever two or more property declarations resolve to
    /// the same member name but disagree on the `readonly` or optional (`?`)
    /// modifier. It is independent of the duplicate-identifier (TS2300) and
    /// same-type (TS2717) diagnostics: it fires even when the names are computed
    /// (so TS2300 is suppressed) and even when the declared types are identical
    /// (so TS2717 is absent).
    ///
    /// Targeting mirrors `tsc`: the first declaration is the reference; every
    /// later declaration whose flags differ from it is flagged, and the
    /// reference itself is flagged once if any later declaration differs. So
    /// `readonly a; a; readonly a` flags the first two (the third matches the
    /// reference) while `a; readonly a; readonly a` flags all three.
    ///
    /// `member_nodes` is the property-signature member nodes that share the
    /// canonical `name`, in source order. Callers pass the same group of
    /// declarations they already detected as duplicates, so computed names that
    /// resolve to the same value share a group.
    pub(crate) fn report_property_modifier_disagreements(
        &mut self,
        name: &str,
        member_nodes: &[NodeIndex],
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        if member_nodes.len() < 2 {
            return;
        }

        let Some(reference) = self.property_signature_modifier_flags(member_nodes[0]) else {
            return;
        };

        // Collect the declarations that disagree with the reference before
        // emitting, so the flag reads (`&self`) don't interleave with the error
        // emission (`&mut self`). The reference is flagged once if any later
        // declaration differs.
        let mut nodes_to_flag: Vec<NodeIndex> = Vec::new();
        for &member_idx in &member_nodes[1..] {
            let Some(flags) = self.property_signature_modifier_flags(member_idx) else {
                continue;
            };
            if flags != reference {
                if nodes_to_flag.is_empty() {
                    nodes_to_flag.push(member_nodes[0]);
                }
                nodes_to_flag.push(member_idx);
            }
        }
        if nodes_to_flag.is_empty() {
            return;
        }

        let message = crate::diagnostics::format_message(
            diagnostic_messages::ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_MODIFIERS,
            &[name],
        );
        for member_idx in nodes_to_flag {
            let name_node = self
                .property_signature_name_node(member_idx)
                .unwrap_or(member_idx);
            self.error_at_node(
                name_node,
                &message,
                diagnostic_codes::ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_MODIFIERS,
            );
        }
    }

    /// Extract the `readonly` / optional modifier flags of a property signature,
    /// or `None` when `member_idx` is not a property signature.
    fn property_signature_modifier_flags(
        &self,
        member_idx: NodeIndex,
    ) -> Option<PropertyModifierFlags> {
        let node = self.ctx.arena.get(member_idx)?;
        if node.kind != PROPERTY_SIGNATURE {
            return None;
        }
        let sig = self.ctx.arena.get_signature(node)?;
        Some(PropertyModifierFlags {
            readonly: self.has_readonly_modifier(&sig.modifiers),
            optional: sig.question_token,
        })
    }

    /// The name node of a property signature, used to anchor the diagnostic.
    fn property_signature_name_node(&self, member_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(member_idx)?;
        if node.kind != PROPERTY_SIGNATURE {
            return None;
        }
        let sig = self.ctx.arena.get_signature(node)?;
        sig.name.is_some().then_some(sig.name)
    }
}
