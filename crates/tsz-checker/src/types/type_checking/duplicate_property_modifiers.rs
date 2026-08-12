use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext::{COMPUTED_PROPERTY_NAME, PROPERTY_SIGNATURE};
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

        // A duplicate name (TS2300), a subsequent-declaration type mismatch
        // (TS2717), or a modifier disagreement (TS2687) all require at least two
        // property signatures that share a name, so a literal with fewer than two
        // members can never trip any of them. Bail before touching the heap — this
        // is the common case (most object types carry zero or one members) and the
        // check runs on every type literal in the program. (TS1170/TS2464 computed
        // -property checks live in a separate per-member pass in `core.rs` and are
        // unaffected by this early return.) (#11617)
        if members.len() < 2 {
            return;
        }

        // Canonical name -> (member_idx, type_annotation, is_eagerly_bound)
        // triples, in source order. Mirrors `check_duplicate_interface_members`
        // (`interface_checks.rs`) — the two containers apply one rule.
        let mut seen_properties: rustc_hash::FxHashMap<String, Vec<(NodeIndex, NodeIndex, bool)>> =
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
            // A computed name declares a member only when it resolves to a
            // literal or unique-symbol property name (`tsc`'s late-bound name
            // rule), so it must go through the resolving query. The syntactic
            // query returns the *expression's* source text for a computed name
            // (`k` for `[k]`), which both invents a member for a name that
            // declares none and collides with a sibling literal `k`.
            let is_computed = self
                .ctx
                .arena
                .get(sig.name)
                .is_some_and(|node| node.kind == COMPUTED_PROPERTY_NAME);
            let name = if is_computed {
                let Some(n) = self.get_property_name_resolved(sig.name) else {
                    continue;
                };
                n
            } else if let Some(n) = self.get_member_name(member_idx) {
                n
            } else {
                continue;
            };

            let is_eager = self.is_eagerly_bound_member_name(sig.name);
            seen_properties.entry(name).or_default().push((
                member_idx,
                sig.type_annotation,
                is_eager,
            ));
        }

        // Report errors for duplicates — tsc reports TS2300 on ALL occurrences
        // (both first and subsequent), not just the second+.
        for (name, entries) in &seen_properties {
            if entries.len() <= 1 {
                continue;
            }

            // tsc renders the duplicate name via `declarationNameToString` of
            // the group's first *eagerly bound* declaration's name node
            // (verbatim source spelling), falling back to the first
            // declaration only when every member of the group is late-bound:
            // `{ "artist"; artist }` reports `'"artist"'` at both, `{ 0.0;
            // '0' }` reports `'0.0'` (raw source, not the canonicalized `0`),
            // and `{ [c0]: number; 1: number }` (where `const c0 = "1"`)
            // reports `'1'` even though `[c0]` is written first, because a
            // computed name over an entity reference is late-bound and the
            // plain numeric-literal `1` is not (#16258 residual 1).
            // The group's first eagerly-bound declaration, if any. TS2300
            // requires at least one such member: tsc's binder only collides
            // eagerly-bound (syntactic / literal-spelled) member names; a
            // computed name over an entity reference (`[c0]` for `const c0 =
            // "a"`) is late-bound, and `lateBindMember` reports a duplicate
            // only on a genuine symbol-flags exclusion (method vs. accessor),
            // never for two plain property members that merely resolve to the
            // same key. So a group whose members are *all* late-bound (`{ [c0]:
            // T; [c1]: T }` with `c0 === c1`) merges silently — no TS2300 —
            // while the checker-level consistency checks (TS2687 modifiers,
            // TS2717 type) still fire. A mixed group (`{ [c0]: T; 1: T }`) has
            // an eager member, so it reports TS2300, rendered at that eager
            // declaration's spelling.
            let eager_entry = entries.iter().find(|entry| entry.2);
            let group_has_eager = eager_entry.is_some();
            let render_entry = *eager_entry.unwrap_or(&entries[0]);
            let render_idx = render_entry.0;
            let render_name_idx = self
                .property_signature_name_node(render_idx)
                .unwrap_or(render_idx);
            let display_name = self
                .declaration_name_to_string(render_name_idx)
                .unwrap_or_else(|| name.clone());

            // TS2687: duplicate property declarations must agree on
            // `readonly` / optional modifiers. Independent of TS2300/TS2717.
            // The comparison reference is the same eagerly-bound declaration
            // TS2300 renders, not source-order-first — see the doc comment on
            // `report_property_modifier_disagreements`.
            let member_nodes: Vec<NodeIndex> = entries.iter().map(|entry| entry.0).collect();
            self.report_property_modifier_disagreements(render_idx, &member_nodes);

            // The reference type for TS2717 is the eagerly-bound
            // declaration's own type, not source-order-first's — same
            // reference `render_idx` as TS2300/TS2687.
            let reference_type = if render_entry.1.is_some() {
                self.get_type_from_type_node(render_entry.1)
            } else {
                TypeId::ANY
            };

            for &(idx, type_ann, _) in entries.iter() {
                let Some(error_node) = self.property_signature_name_node(idx) else {
                    continue;
                };

                // TS2300 on every declaration in the group, but only when the
                // group has an eagerly-bound member (see `group_has_eager`).
                // How each name was spelled does not matter — a computed name
                // that reached this point resolved to a real member key, so it
                // names the same member as its group siblings.
                if group_has_eager {
                    self.error_at_node(
                        error_node,
                        &format!("Duplicate identifier '{display_name}'."),
                        diagnostic_codes::DUPLICATE_IDENTIFIER,
                    );
                }

                // TS2717 on every declaration OTHER than the reference, when
                // its type differs from the reference's. Use display text
                // for the property name to match TSC's
                // declarationNameToString (e.g., "1.0" not "1").
                if idx != render_idx {
                    let this_type = if type_ann.is_some() {
                        self.get_type_from_type_node(type_ann)
                    } else {
                        TypeId::ANY
                    };
                    if !self.type_contains_error(reference_type)
                        && !self.type_contains_error(this_type)
                        && reference_type != this_type
                    {
                        let display_name = self
                            .get_member_name_display_text(error_node)
                            .unwrap_or_else(|| name.clone());
                        let reference_type_str = self.format_type(reference_type);
                        let this_type_str = self.format_type(this_type);
                        self.error_at_node_msg(
                            error_node,
                            diagnostic_codes::SUBSEQUENT_PROPERTY_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_PROPERTY_MUST_BE_OF_TYP,
                            &[&display_name, &reference_type_str, &this_type_str],
                        );
                    }
                }
            }
        }
    }

    /// TS2687: "All declarations of '{0}' must have identical modifiers."
    ///
    /// `tsc` raises this whenever two or more property declarations resolve to
    /// the same member name but disagree on the `readonly` or optional (`?`)
    /// modifier. It is independent of the same-type (TS2717) diagnostic: it
    /// fires even when the declared types are identical (so TS2717 is absent).
    ///
    /// Targeting and naming both key off `reference_idx` — the group's first
    /// *eagerly bound* declaration (`is_eagerly_bound_member_name`), the same
    /// declaration TS2300 renders and TS2717 compares types against — not
    /// source-order-first. Every other declaration whose flags differ from
    /// the reference's is flagged, and the reference itself is flagged once
    /// if anything differs. Unlike TS2300 (one shared name for the whole
    /// group), each flagged declaration is named by **its own** spelling, not
    /// the reference's: oracle-pinned on `readonly [c0]: number; 1: string;`
    /// (`const c0 = "1"`, so `1` is the reference) — `tsc` reports `"All
    /// declarations of '[c0]' must have identical modifiers."` at `[c0]` and
    /// `"...of '1'..."` at `1`, not the same name at both (#16258 residual 1).
    ///
    /// `member_nodes` is the property-signature member nodes that share the
    /// canonical member name, in source order; `reference_idx` must be one of
    /// them. Callers pass the same group of declarations they already
    /// detected as duplicates, so computed names that resolve to the same
    /// value share a group.
    pub(crate) fn report_property_modifier_disagreements(
        &mut self,
        reference_idx: NodeIndex,
        member_nodes: &[NodeIndex],
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        if member_nodes.len() < 2 {
            return;
        }

        let Some(reference) = self.property_signature_modifier_flags(reference_idx) else {
            return;
        };

        // Collect the declarations that disagree with the reference before
        // emitting, so the flag reads (`&self`) don't interleave with the error
        // emission (`&mut self`). The reference is flagged once if any other
        // declaration differs.
        let mut nodes_to_flag: Vec<NodeIndex> = Vec::new();
        for &member_idx in member_nodes {
            if member_idx == reference_idx {
                continue;
            }
            let Some(flags) = self.property_signature_modifier_flags(member_idx) else {
                continue;
            };
            if flags != reference {
                nodes_to_flag.push(member_idx);
            }
        }
        if nodes_to_flag.is_empty() {
            return;
        }
        nodes_to_flag.insert(0, reference_idx);

        for member_idx in nodes_to_flag {
            let name_node = self
                .property_signature_name_node(member_idx)
                .unwrap_or(member_idx);
            let display_name = self
                .get_member_name_display_text(name_node)
                .unwrap_or_default();
            let message = crate::diagnostics::format_message(
                diagnostic_messages::ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_MODIFIERS,
                &[&display_name],
            );
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
