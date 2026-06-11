use super::super::Printer;
use tsz_common::interner::AstAtom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in super::super) fn jsx_child_stole_parent_closer(
        &self,
        child: NodeIndex,
        parent_tag_name: NodeIndex,
    ) -> bool {
        let Some(child_node) = self.arena.get(child) else {
            return false;
        };
        if child_node.kind != syntax_kind_ext::JSX_ELEMENT {
            return false;
        }
        let Some(child_element) = self.arena.get_jsx_element(child_node) else {
            return false;
        };
        let Some(child_open_tag) = self
            .arena
            .get(child_element.opening_element)
            .and_then(|opening_node| self.arena.get_jsx_opening(opening_node))
            .map(|opening| opening.tag_name)
        else {
            return false;
        };
        let Some(child_close_tag) = self
            .arena
            .get(child_element.closing_element)
            .and_then(|closing_node| self.arena.get_jsx_closing(closing_node))
            .map(|closing| closing.tag_name)
        else {
            return false;
        };

        !self.jsx_tag_names_match(child_open_tag, child_close_tag)
            && self.jsx_tag_names_match(child_close_tag, parent_tag_name)
    }

    fn jsx_tag_names_match(&self, a: NodeIndex, b: NodeIndex) -> bool {
        if a == b {
            return true;
        }
        let Some(node_a) = self.arena.get(a) else {
            return false;
        };
        let Some(node_b) = self.arena.get(b) else {
            return false;
        };
        if node_a.kind != node_b.kind {
            return false;
        }

        if node_a.is_identifier() {
            if let (Some(id_a), Some(id_b)) = (
                self.arena.get_identifier(node_a),
                self.arena.get_identifier(node_b),
            ) {
                if id_a.atom != AstAtom::NONE && id_b.atom != AstAtom::NONE {
                    return id_a.atom == id_b.atom;
                }
                return id_a.escaped_text == id_b.escaped_text;
            }
        } else if node_a.kind == SyntaxKind::ThisKeyword as u16 {
            return true;
        } else if node_a.kind == syntax_kind_ext::JSX_NAMESPACED_NAME {
            if let (Some(ns_a), Some(ns_b)) = (
                self.arena.get_jsx_namespaced_name(node_a),
                self.arena.get_jsx_namespaced_name(node_b),
            ) {
                return self.jsx_tag_names_match(ns_a.namespace, ns_b.namespace)
                    && self.jsx_tag_names_match(ns_a.name, ns_b.name);
            }
        } else if node_a.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let (Some(acc_a), Some(acc_b)) = (
                self.arena.get_access_expr(node_a),
                self.arena.get_access_expr(node_b),
            )
        {
            return self.jsx_tag_names_match(acc_a.expression, acc_b.expression)
                && self.jsx_tag_names_match(acc_a.name_or_argument, acc_b.name_or_argument);
        }

        false
    }
}
