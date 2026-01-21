//! JSX Type Checking
//!
//! This module provides type checking for JSX elements and expressions.
//!
//! JSX type checking involves:
//! - Resolving the element type (intrinsic HTML elements vs components)
//! - Checking props against expected types
//! - Validating children
//! - Handling JSX namespace configuration

use crate::parser::NodeIndex;
use crate::parser::node::NodeArena;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;

/// JSX intrinsic element types that map to HTML/DOM elements
pub const INTRINSIC_ELEMENTS: &[&str] = &[
    "a",
    "abbr",
    "address",
    "area",
    "article",
    "aside",
    "audio",
    "b",
    "base",
    "bdi",
    "bdo",
    "big",
    "blockquote",
    "body",
    "br",
    "button",
    "canvas",
    "caption",
    "cite",
    "code",
    "col",
    "colgroup",
    "data",
    "datalist",
    "dd",
    "del",
    "details",
    "dfn",
    "dialog",
    "div",
    "dl",
    "dt",
    "em",
    "embed",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hgroup",
    "hr",
    "html",
    "i",
    "iframe",
    "img",
    "input",
    "ins",
    "kbd",
    "keygen",
    "label",
    "legend",
    "li",
    "link",
    "main",
    "map",
    "mark",
    "menu",
    "menuitem",
    "meta",
    "meter",
    "nav",
    "noindex",
    "noscript",
    "object",
    "ol",
    "optgroup",
    "option",
    "output",
    "p",
    "param",
    "picture",
    "pre",
    "progress",
    "q",
    "rp",
    "rt",
    "ruby",
    "s",
    "samp",
    "script",
    "section",
    "select",
    "slot",
    "small",
    "source",
    "span",
    "strong",
    "style",
    "sub",
    "summary",
    "sup",
    "svg",
    "table",
    "tbody",
    "td",
    "template",
    "textarea",
    "tfoot",
    "th",
    "thead",
    "time",
    "title",
    "tr",
    "track",
    "u",
    "ul",
    "var",
    "video",
    "wbr",
    "webview",
];

/// SVG intrinsic element types
pub const SVG_INTRINSIC_ELEMENTS: &[&str] = &[
    "svg",
    "animate",
    "animateMotion",
    "animateTransform",
    "circle",
    "clipPath",
    "defs",
    "desc",
    "ellipse",
    "feBlend",
    "feColorMatrix",
    "feComponentTransfer",
    "feComposite",
    "feConvolveMatrix",
    "feDiffuseLighting",
    "feDisplacementMap",
    "feDistantLight",
    "feDropShadow",
    "feFlood",
    "feFuncA",
    "feFuncB",
    "feFuncG",
    "feFuncR",
    "feGaussianBlur",
    "feImage",
    "feMerge",
    "feMergeNode",
    "feMorphology",
    "feOffset",
    "fePointLight",
    "feSpecularLighting",
    "feSpotLight",
    "feTile",
    "feTurbulence",
    "filter",
    "foreignObject",
    "g",
    "image",
    "line",
    "linearGradient",
    "marker",
    "mask",
    "metadata",
    "mpath",
    "path",
    "pattern",
    "polygon",
    "polyline",
    "radialGradient",
    "rect",
    "set",
    "stop",
    "switch",
    "symbol",
    "text",
    "textPath",
    "tspan",
    "use",
    "view",
];

/// JSX Checker for type-checking JSX elements
pub struct JsxChecker<'a> {
    arena: &'a NodeArena,
    /// JSX factory function name (default: React.createElement)
    jsx_factory: String,
    /// JSX fragment factory (default: React.Fragment)
    jsx_fragment_factory: String,
}

impl<'a> JsxChecker<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            jsx_factory: "React.createElement".to_string(),
            jsx_fragment_factory: "React.Fragment".to_string(),
        }
    }

    /// Set the JSX factory function
    pub fn set_jsx_factory(&mut self, factory: &str) {
        self.jsx_factory = factory.to_string();
    }

    /// Set the JSX fragment factory
    pub fn set_fragment_factory(&mut self, factory: &str) {
        self.jsx_fragment_factory = factory.to_string();
    }

    /// Check if a tag name is an intrinsic HTML element
    pub fn is_intrinsic_element(tag_name: &str) -> bool {
        // Intrinsic elements start with lowercase
        if tag_name.is_empty() {
            return false;
        }
        let first_char = tag_name.chars().next().unwrap();
        first_char.is_ascii_lowercase()
    }

    /// Check if a tag name is a known HTML intrinsic element
    pub fn is_known_intrinsic(tag_name: &str) -> bool {
        INTRINSIC_ELEMENTS.contains(&tag_name) || SVG_INTRINSIC_ELEMENTS.contains(&tag_name)
    }

    /// Get the tag name from a JSX element
    pub fn get_tag_name(&self, tag_name_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(tag_name_idx)?;

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                let ident = self.arena.get_identifier(node)?;
                Some(ident.escaped_text.clone())
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                // Handle qualified names like React.Component
                self.get_qualified_name(tag_name_idx)
            }
            k if k == syntax_kind_ext::JSX_NAMESPACED_NAME => {
                let ns = self.arena.get_jsx_namespaced_name(node)?;
                let ns_name = self.get_tag_name(ns.namespace)?;
                let name = self.get_tag_name(ns.name)?;
                Some(format!("{}:{}", ns_name, name))
            }
            _ => None,
        }
    }

    /// Get a qualified name from a property access expression
    fn get_qualified_name(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            let ident = self.arena.get_identifier(node)?;
            return Some(ident.escaped_text.clone());
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node)?;
            let left = self.get_qualified_name(access.expression)?;
            let right = self.get_tag_name(access.name_or_argument)?;
            return Some(format!("{}.{}", left, right));
        }

        None
    }

    /// Check a JSX element for type errors
    pub fn check_jsx_element(&self, element_idx: NodeIndex) -> Vec<JsxError> {
        let mut errors = Vec::new();

        let Some(node) = self.arena.get(element_idx) else {
            return errors;
        };

        match node.kind {
            k if k == syntax_kind_ext::JSX_ELEMENT => {
                self.check_jsx_full_element(element_idx, &mut errors);
            }
            k if k == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT => {
                self.check_jsx_self_closing(element_idx, &mut errors);
            }
            k if k == syntax_kind_ext::JSX_FRAGMENT => {
                self.check_jsx_fragment(element_idx, &mut errors);
            }
            _ => {}
        }

        errors
    }

    fn check_jsx_full_element(&self, idx: NodeIndex, errors: &mut Vec<JsxError>) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };
        let Some(jsx) = self.arena.get_jsx_element(node) else {
            return;
        };

        // Get opening element tag name
        if let Some(opening_node) = self.arena.get(jsx.opening_element) {
            if let Some(opening) = self.arena.get_jsx_opening(opening_node) {
                let opening_tag = self.get_tag_name(opening.tag_name);
                let opening_tag_clone = opening_tag.clone();

                // Check closing element tag matches
                if let Some(closing_node) = self.arena.get(jsx.closing_element) {
                    if let Some(closing) = self.arena.get_jsx_closing(closing_node) {
                        let closing_tag = self.get_tag_name(closing.tag_name);

                        if opening_tag != closing_tag {
                            errors.push(JsxError::MismatchedClosingTag {
                                opening: opening_tag.unwrap_or_default(),
                                closing: closing_tag.unwrap_or_default(),
                                pos: closing_node.pos,
                            });
                        }
                    }
                }

                // Check attributes
                self.check_jsx_attributes(
                    opening.attributes,
                    &opening_tag_clone.unwrap_or_default(),
                    errors,
                );
            }
        }

        // Check children
        for &child_idx in &jsx.children.nodes {
            self.check_jsx_child(child_idx, errors);
        }
    }

    fn check_jsx_self_closing(&self, idx: NodeIndex, errors: &mut Vec<JsxError>) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };
        let Some(jsx) = self.arena.get_jsx_opening(node) else {
            return;
        };

        let tag_name = self.get_tag_name(jsx.tag_name).unwrap_or_default();

        // Check attributes
        self.check_jsx_attributes(jsx.attributes, &tag_name, errors);
    }

    fn check_jsx_fragment(&self, idx: NodeIndex, errors: &mut Vec<JsxError>) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };
        let Some(jsx) = self.arena.get_jsx_fragment(node) else {
            return;
        };

        // Check children
        for &child_idx in &jsx.children.nodes {
            self.check_jsx_child(child_idx, errors);
        }
    }

    fn check_jsx_attributes(
        &self,
        attrs_idx: NodeIndex,
        _tag_name: &str,
        errors: &mut Vec<JsxError>,
    ) {
        let Some(attrs_node) = self.arena.get(attrs_idx) else {
            return;
        };
        let Some(attrs) = self.arena.get_jsx_attributes(attrs_node) else {
            return;
        };

        let mut seen_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

        for &attr_idx in &attrs.properties.nodes {
            let Some(attr_node) = self.arena.get(attr_idx) else {
                continue;
            };

            if attr_node.kind == syntax_kind_ext::JSX_ATTRIBUTE {
                if let Some(attr) = self.arena.get_jsx_attribute(attr_node) {
                    if let Some(name) = self.get_tag_name(attr.name) {
                        // Check for duplicate attributes
                        if seen_keys.contains(&name) {
                            errors.push(JsxError::DuplicateAttribute {
                                name: name.clone(),
                                pos: attr_node.pos,
                            });
                        }
                        seen_keys.insert(name);
                    }
                }
            }
        }
    }

    fn check_jsx_child(&self, child_idx: NodeIndex, errors: &mut Vec<JsxError>) {
        let Some(child_node) = self.arena.get(child_idx) else {
            return;
        };

        match child_node.kind {
            k if k == syntax_kind_ext::JSX_ELEMENT => {
                self.check_jsx_full_element(child_idx, errors);
            }
            k if k == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT => {
                self.check_jsx_self_closing(child_idx, errors);
            }
            k if k == syntax_kind_ext::JSX_FRAGMENT => {
                self.check_jsx_fragment(child_idx, errors);
            }
            k if k == syntax_kind_ext::JSX_EXPRESSION => {
                // Expression children - recursively check
            }
            k if k == SyntaxKind::JsxText as u16 => {
                // Text children are always valid
            }
            _ => {}
        }
    }

    /// Get the element type for a JSX tag
    pub fn get_element_type(&self, tag_name: &str) -> JsxElementType {
        if Self::is_intrinsic_element(tag_name) {
            if Self::is_known_intrinsic(tag_name) {
                JsxElementType::IntrinsicElement(tag_name.to_string())
            } else {
                JsxElementType::UnknownIntrinsic(tag_name.to_string())
            }
        } else {
            JsxElementType::Component(tag_name.to_string())
        }
    }
}

/// Types of JSX elements
#[derive(Debug, Clone, PartialEq)]
pub enum JsxElementType {
    /// Known intrinsic HTML/SVG element
    IntrinsicElement(String),
    /// Unknown intrinsic element (lowercase but not in known list)
    UnknownIntrinsic(String),
    /// Component (function or class)
    Component(String),
}

/// JSX type checking errors
#[derive(Debug, Clone)]
pub enum JsxError {
    /// Opening and closing tags don't match
    MismatchedClosingTag {
        opening: String,
        closing: String,
        pos: u32,
    },
    /// Duplicate attribute name
    DuplicateAttribute { name: String, pos: u32 },
    /// Unknown intrinsic element
    UnknownIntrinsicElement { name: String, pos: u32 },
    /// Missing required prop
    MissingRequiredProp {
        prop_name: String,
        element: String,
        pos: u32,
    },
    /// Invalid prop type
    InvalidPropType {
        prop_name: String,
        expected: String,
        actual: String,
        pos: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_intrinsic_element() {
        assert!(JsxChecker::is_intrinsic_element("div"));
        assert!(JsxChecker::is_intrinsic_element("span"));
        assert!(JsxChecker::is_intrinsic_element("custom-element"));
        assert!(!JsxChecker::is_intrinsic_element("MyComponent"));
        assert!(!JsxChecker::is_intrinsic_element("App"));
        assert!(!JsxChecker::is_intrinsic_element(""));
    }

    #[test]
    fn test_is_known_intrinsic() {
        assert!(JsxChecker::is_known_intrinsic("div"));
        assert!(JsxChecker::is_known_intrinsic("span"));
        assert!(JsxChecker::is_known_intrinsic("svg"));
        assert!(JsxChecker::is_known_intrinsic("circle"));
        assert!(!JsxChecker::is_known_intrinsic("foobar"));
        assert!(!JsxChecker::is_known_intrinsic("custom-element"));
    }

    #[test]
    fn test_element_type() {
        let arena = NodeArena::new();
        let checker = JsxChecker::new(&arena);

        assert_eq!(
            checker.get_element_type("div"),
            JsxElementType::IntrinsicElement("div".to_string())
        );
        assert_eq!(
            checker.get_element_type("customtag"),
            JsxElementType::UnknownIntrinsic("customtag".to_string())
        );
        assert_eq!(
            checker.get_element_type("MyComponent"),
            JsxElementType::Component("MyComponent".to_string())
        );
    }
}
