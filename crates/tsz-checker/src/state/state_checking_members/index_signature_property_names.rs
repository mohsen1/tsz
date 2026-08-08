//! TS2411 property-name rendering.
//!
//! `tsc` names the offending property in a TS2411
//! ("Property '{0}' ... is not assignable to '{1}' index type ...") from the
//! property's *declaration* name node (`getNameOfSymbolAsWritten` ->
//! `declarationNameToString`), not from its resolved symbol name: a computed
//! key keeps its brackets (`["get1"]`, `[42]`) and a string-literal key keeps
//! its quotes. This holds even when the diagnostic anchors at a *derived*
//! type's index signature and the offending property is inherited from a base.
//!
//! The own-member index-constraint path renders directly from the member's own
//! name node; the inherited path only has the resolved atom on the derived-side
//! object shape, so it recovers the base declaration's name node through the
//! property's declaring class/interface symbol. Both share
//! [`CheckerState::ts2411_written_member_name`].

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl CheckerState<'_> {
    /// The property name as `tsc` writes it in a TS2411 diagnostic.
    ///
    /// A computed property name keeps its bracketed source text (`["get1"]`,
    /// `[42]`) and a string-literal name keeps its original quotes. Returns
    /// `None` for a plain identifier name, so callers fall back to the resolved
    /// property name — which for an identifier is the identical text.
    pub(crate) fn ts2411_written_member_name(&self, name_idx: NodeIndex) -> Option<String> {
        let name_node = self.ctx.arena.get(name_idx)?;
        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            // The node range runs one token past the `]` (a trailing `(`, `:`,
            // `=` leaks in); truncate at the closing bracket so the printed
            // name is exactly the bracketed text.
            let text = self.node_text(name_idx)?;
            Some(match text.rfind(']') {
                Some(end) => text[..=end].to_string(),
                None => text.trim_end_matches(':').to_string(),
            })
        } else if name_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16 {
            self.node_text(name_idx)
        } else {
            None
        }
    }

    /// The declaration name node of an inherited property `prop_name`, found on
    /// its declaring class/interface (`parent_id`).
    ///
    /// A TS2411 reported against a *derived* index signature names an inherited
    /// property, and `tsc` renders that name from the property symbol's own
    /// declaration — so a computed name written on the base
    /// (`get ["get1"]() {}`) must still print as `["get1"]`, not the resolved
    /// `get1`. The derived-side shape only carries the resolved atom, so recover
    /// the base declaration's name node here. Returns `None` when no declaration
    /// carries a member with a matching resolved name (e.g. a plain identifier
    /// key, for which the resolved name is already the written form).
    fn inherited_member_name_node(
        &self,
        parent_id: tsz_binder::SymbolId,
        prop_name: &str,
    ) -> Option<NodeIndex> {
        let decls = self.ctx.binder.symbols.get(parent_id)?.declarations.clone();
        for decl_idx in decls {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let members = if decl_node.kind == syntax_kind_ext::CLASS_DECLARATION {
                self.ctx
                    .arena
                    .get_class(decl_node)
                    .map(|c| c.members.nodes.clone())
            } else if decl_node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                self.ctx
                    .arena
                    .get_interface(decl_node)
                    .map(|i| i.members.nodes.clone())
            } else {
                None
            };
            let Some(members) = members else {
                continue;
            };
            for member_idx in members {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let Some(name_node_idx) = self.get_member_name_node(member_node) else {
                    continue;
                };
                if self.get_member_name(member_idx).as_deref() == Some(prop_name) {
                    return Some(name_node_idx);
                }
            }
        }
        None
    }

    /// The TS2411 written name for an inherited property: the base declaration's
    /// computed/string-literal name text when there is one, else the resolved
    /// `prop_name`. Mirrors the own-member path so both report an inherited and
    /// an own computed key identically.
    pub(crate) fn ts2411_inherited_prop_name(
        &self,
        parent_id: Option<tsz_binder::SymbolId>,
        prop_name: &str,
    ) -> String {
        parent_id
            .and_then(|pid| self.inherited_member_name_node(pid, prop_name))
            .and_then(|name_idx| self.ts2411_written_member_name(name_idx))
            .unwrap_or_else(|| prop_name.to_string())
    }
}
