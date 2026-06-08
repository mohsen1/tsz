//! TS2687 modifier-agreement checking for class members.
//!
//! `tsc` raises TS2687 ("All declarations of '{0}' must have identical
//! modifiers.") from `checkVariableLikeDeclaration` whenever a class **property**
//! declaration's modifier flags disagree with the other declarations of the same
//! member symbol. The compared flags are the optional (`?`) token plus the
//! `getSelectedEffectiveModifierFlags` mask
//! `{Private, Protected, Async, Abstract, Readonly, Static}` — notably *not*
//! `public`, `override`, `declare`, or the `accessor` keyword.
//!
//! This mirrors the merged type-literal / interface path in
//! `duplicate_property_modifiers.rs`, but classes need the broader flag set and
//! must distinguish member kinds: accessors and methods participate in the
//! member group (and can be the reference declaration), yet TS2687 is only ever
//! emitted on property declarations because it originates from
//! `checkVariableLikeDeclaration`, which never runs on accessors or methods.
//! (The existing `check_duplicate_class_members` grouping cannot be reused here
//! because it tracks accessors in a separate map, whereas the reference model
//! below needs properties, methods, and accessors interleaved in source order.)
//!
//! Targeting follows `tsc`: the value declaration (the first declaration in
//! source order) is the reference. Every later property declaration whose flags
//! differ from the reference is flagged, and the reference property itself is
//! flagged once if any other property declaration disagrees with it (`tsc`'s
//! `isVariableLike` restricts the "some other declaration differs" probe to
//! property declarations).

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

/// The modifier signature of a single class member declaration. Two
/// declarations of the same member must carry identical flags or `tsc` reports
/// TS2687. Matches `tsc`'s `areDeclarationFlagsIdentical`: the optional token
/// plus the `{Private, Protected, Async, Abstract, Readonly, Static}` mask.
#[derive(Clone, Copy, PartialEq, Eq)]
struct ClassMemberModifierFlags {
    readonly: bool,
    optional: bool,
    private: bool,
    protected: bool,
    is_abstract: bool,
    is_async: bool,
    is_static: bool,
}

impl CheckerState<'_> {
    /// Report TS2687 for class member declarations that share a name but
    /// disagree on modifiers.
    ///
    /// Instance and static members are grouped separately (they resolve to
    /// distinct symbols in `tsc`), and only groups with more than one
    /// declaration can disagree — so this allocates nothing extra for the common
    /// no-duplicate class body. The groups are visited in hash order;
    /// diagnostics are sorted by source position downstream (as in the sibling
    /// type-literal path), so iteration order does not affect output.
    pub(crate) fn check_class_member_modifier_disagreements(&mut self, members: &[NodeIndex]) {
        use rustc_hash::FxHashMap;

        // Canonical group key (`"static:name"` / `"name"`) -> member declaration
        // nodes in source order.
        let mut groups: FxHashMap<String, Vec<NodeIndex>> = FxHashMap::default();
        for &member_idx in members {
            let Some(key) = self.class_member_modifier_group_key(member_idx) else {
                continue;
            };
            groups.entry(key).or_default().push(member_idx);
        }

        for member_nodes in groups.into_values() {
            if member_nodes.len() < 2 {
                continue;
            }
            self.report_class_member_modifier_disagreements(&member_nodes);
        }
    }

    /// The canonical grouping key for a participating class member, or `None`
    /// when the member does not participate (constructor, index signature,
    /// static block) or carries a computed/late-bound name we cannot
    /// canonicalize syntactically.
    ///
    /// Reuses [`Self::get_class_member_name_info`] (the same name + static-ness
    /// extraction the duplicate-member scan uses) and skips late-bound names,
    /// which `tsc` keys on the resolved symbol.
    fn class_member_modifier_group_key(&mut self, member_idx: NodeIndex) -> Option<String> {
        let (name, name_node, is_static) = self.get_class_member_name_info(member_idx)?;
        if name.is_empty() || self.is_late_bound_member_name(name_node) {
            return None;
        }
        Some(if is_static {
            format!("static:{name}")
        } else {
            name
        })
    }

    /// Emit TS2687 for a single name group (members in source order).
    fn report_class_member_modifier_disagreements(&mut self, member_nodes: &[NodeIndex]) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        if member_nodes.len() < 2 {
            return;
        }

        // The reference (`tsc`'s value declaration) is the first declaration,
        // and may be a property, method, or accessor.
        let reference_idx = member_nodes[0];
        let Some((reference_is_property, reference_flags)) =
            self.class_member_modifier_info(reference_idx)
        else {
            return;
        };

        // Collect every later property declaration that disagrees with the
        // reference. Methods and accessors are never flagged and never count as
        // a disagreeing "other declaration" (`tsc`'s `isVariableLike` probe is
        // restricted to property declarations).
        let mut nodes_to_flag: Vec<NodeIndex> = member_nodes[1..]
            .iter()
            .copied()
            .filter(|&idx| {
                self.class_member_modifier_info(idx)
                    .is_some_and(|(is_property, flags)| is_property && flags != reference_flags)
            })
            .collect();

        // The reference property is flagged once if any later property differs.
        if reference_is_property && !nodes_to_flag.is_empty() {
            nodes_to_flag.push(reference_idx);
        }

        for member_idx in nodes_to_flag {
            // Every flagged node was grouped through `get_class_member_name_info`,
            // so its name (and name node) resolve here too.
            let Some((name, name_node, _)) = self.get_class_member_name_info(member_idx) else {
                continue;
            };
            // `tsc` renders the name with `declarationNameToString`; reuse the
            // shared member-name display text so string/numeric literal names
            // match (e.g. `"1.0"` not `1`).
            let display_name = self.get_member_name_display_text(name_node).unwrap_or(name);
            let message = format_message(
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

    /// Whether `member_idx` is a property declaration, paired with the modifier
    /// flags `tsc` compares for TS2687. Returns `None` for non-participating
    /// member kinds (constructor, index signature, static block). A single arena
    /// lookup serves both the property-kind test and the flag extraction.
    fn class_member_modifier_info(
        &self,
        member_idx: NodeIndex,
    ) -> Option<(bool, ClassMemberModifierFlags)> {
        let member_node = self.ctx.arena.get(member_idx)?;
        let (modifiers, optional, is_property) = match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                let prop = self.ctx.arena.get_property_decl(member_node)?;
                (&prop.modifiers, prop.question_token, true)
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.ctx.arena.get_method_decl(member_node)?;
                (&method.modifiers, method.question_token, false)
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                let accessor = self.ctx.arena.get_accessor(member_node)?;
                (&accessor.modifiers, false, false)
            }
            _ => return None,
        };

        let flags = ClassMemberModifierFlags {
            readonly: self.has_readonly_modifier(modifiers),
            optional,
            private: self.has_private_modifier(modifiers),
            protected: self.has_protected_modifier(modifiers),
            is_abstract: self.has_abstract_modifier(modifiers),
            is_async: self.has_async_modifier(modifiers),
            is_static: self.has_static_modifier(modifiers),
        };
        Some((is_property, flags))
    }
}
