//! ES2015+ CommonJS export-destructuring lowering.
//!
//! `tsc` lowers `export const <pattern> = <init>` through
//! `flattenDestructuringAssignment`: an identifier source is reused inline at
//! every access, any other source is cached in a temp (or emitted inline when
//! used exactly once), default initializers become `=== void 0` checks, and
//! nested patterns recurse against the corresponding access. This module
//! reproduces that form byte-for-byte. The ES5 path keeps its own inline-comma
//! shape in `exports.rs`.

use super::super::Printer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Source value for the recursive ES2015+ CommonJS export-destructuring lowering.
///
/// `tsc` reuses an identifier source inline at every access but emits any other
/// source exactly once; `DestructBase` carries whichever form applies and grows
/// member-access suffixes as the walk descends into the pattern.
enum DestructBase {
    /// A reusable string expression — an identifier or a hoisted temp, plus any
    /// trailing member accesses. Safe to repeat at every access.
    Str(String),
    /// A non-identifier source emitted inline, with any pending member-access
    /// suffix. Used exactly once (guaranteed by the single-element invariant).
    Inline { node: NodeIndex, suffix: String },
}

impl DestructBase {
    /// Extend the source with one more member access (`.prop` / `[i]`).
    fn with_access(&self, suffix: &str) -> Self {
        match self {
            Self::Str(text) => Self::Str(format!("{text}{suffix}")),
            Self::Inline {
                node,
                suffix: existing,
            } => Self::Inline {
                node: *node,
                suffix: format!("{existing}{suffix}"),
            },
        }
    }
}

impl Printer<'_> {
    /// Lower an ES2015+ CommonJS `export const <pattern> = <init>` through the
    /// recursive destructuring form `tsc` uses (`flattenDestructuringAssignment`).
    ///
    /// The whole declaration is emitted as one comma-sequenced expression
    /// statement. A simple identifier source is reused inline at every access;
    /// any other source is cached in a hoisted temp when the pattern has more
    /// than one element, or emitted inline when it has exactly one. Default
    /// initializers become `temp = <access>, exports.x = temp === void 0 ?
    /// <default> : temp`, and nested patterns recurse against the corresponding
    /// access (or the defaulted value). This mirrors `tsc` byte-for-byte; the
    /// ES5 path keeps its own inline-comma shape.
    pub(in crate::emitter) fn emit_cjs_destructuring_export_flattened(
        &mut self,
        pattern_idx: NodeIndex,
        initializer: NodeIndex,
    ) {
        let element_count = self.binding_pattern_element_count(pattern_idx);

        let init_is_identifier = initializer.is_some()
            && self
                .arena
                .get(initializer)
                .is_some_and(|node| node.kind == SyntaxKind::Identifier as u16)
            && !self.get_identifier_text(initializer).is_empty();

        let mut first = true;
        let base = if init_is_identifier {
            DestructBase::Str(self.get_identifier_text(initializer))
        } else if element_count != 1 {
            // Source used more than once (or zero times): cache it once.
            let temp = self.make_unique_name_cjs_destructuring();
            self.emit_assignment_separator(&mut first);
            self.write(&temp);
            self.write(" = ");
            if initializer.is_none() {
                self.write("void 0");
            } else {
                self.emit(initializer);
            }
            DestructBase::Str(temp)
        } else {
            // Exactly one access: emit the source inline at that access.
            DestructBase::Inline {
                node: initializer,
                suffix: String::new(),
            }
        };

        self.flatten_cjs_destructuring_export(pattern_idx, &base, &mut first);
        self.write(";");
    }

    /// Recursively emit the assignments for one binding pattern against the
    /// already-prepared source `base`.
    fn flatten_cjs_destructuring_export(
        &mut self,
        pattern_idx: NodeIndex,
        base: &DestructBase,
        first: &mut bool,
    ) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };
        let pattern_is_array = pattern_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN;
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return;
        };
        let element_indices: Vec<NodeIndex> = pattern.elements.nodes.clone();

        let mut excluded_props: Vec<String> = Vec::new();
        let mut rest: Option<(String, usize, u32)> = None;

        for (element_index, &elem_idx) in element_indices.iter().enumerate() {
            let Some(elem_node) = self.arena.get(elem_idx) else {
                continue;
            };
            if elem_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }
            let Some(elem) = self.arena.get_binding_element(elem_node) else {
                continue;
            };

            if elem.dot_dot_dot_token {
                rest = Some((
                    self.get_identifier_text(elem.name),
                    element_index,
                    elem_node.pos,
                ));
                continue;
            }

            let access_suffix = if pattern_is_array {
                format!("[{element_index}]")
            } else {
                let (prop_text, prop_kind) = if elem.property_name.is_some() {
                    let kind = self.arena.get(elem.property_name).map(|n| n.kind);
                    let text = if kind == Some(SyntaxKind::StringLiteral as u16) {
                        self.get_string_literal_text(elem.property_name)
                            .unwrap_or_default()
                    } else if kind == Some(SyntaxKind::NumericLiteral as u16) {
                        self.get_numeric_literal_text(elem.property_name)
                            .unwrap_or_default()
                    } else {
                        self.get_identifier_text_idx(elem.property_name)
                    };
                    (text, kind)
                } else {
                    (self.get_identifier_text(elem.name), None)
                };
                excluded_props.push(prop_text.clone());
                Self::destructuring_export_property_suffix(&prop_text, prop_kind)
            };
            let access_base = base.with_access(&access_suffix);
            let leading_comment_pos = if elem.property_name.is_some() {
                self.arena
                    .get(elem.name)
                    .map_or(elem_node.pos, |name_node| name_node.pos)
            } else {
                elem_node.pos
            };

            let target_is_pattern = self.arena.get(elem.name).is_some_and(|name_node| {
                name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                    || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            });

            if elem.initializer.is_some() {
                // `temp = <access>` then a `=== void 0 ? <default> : temp` check.
                let access_temp = self.make_unique_name_cjs_destructuring();
                self.emit_assignment_separator(first);
                self.write(&access_temp);
                self.write(" = ");
                self.write_destructuring_export_base(&access_base);

                if target_is_pattern {
                    // The defaulted value feeds a nested pattern; bind it to a
                    // temp so the nested accesses read a reusable identifier.
                    let value_temp = self.make_unique_name_cjs_destructuring();
                    self.emit_assignment_separator(first);
                    self.write(&value_temp);
                    self.write(" = ");
                    self.write_destructuring_export_default(&access_temp, elem.initializer);
                    self.flatten_cjs_destructuring_export(
                        elem.name,
                        &DestructBase::Str(value_temp),
                        first,
                    );
                } else {
                    let export_name = self.get_identifier_text(elem.name);
                    self.write_export_assignment_start_with_comments(
                        first,
                        &export_name,
                        leading_comment_pos,
                    );
                    self.write_destructuring_export_default(&access_temp, elem.initializer);
                }
            } else if target_is_pattern {
                let sub_count = self.binding_pattern_element_count(elem.name);
                if sub_count == 1 {
                    // Single nested access: reuse the access expression directly.
                    self.flatten_cjs_destructuring_export(elem.name, &access_base, first);
                } else {
                    let temp = self.make_unique_name_cjs_destructuring();
                    self.emit_assignment_separator(first);
                    self.write(&temp);
                    self.write(" = ");
                    self.write_destructuring_export_base(&access_base);
                    self.flatten_cjs_destructuring_export(
                        elem.name,
                        &DestructBase::Str(temp),
                        first,
                    );
                }
            } else {
                let export_name = self.get_identifier_text(elem.name);
                self.write_export_assignment_start_with_comments(
                    first,
                    &export_name,
                    leading_comment_pos,
                );
                self.write_destructuring_export_base(&access_base);
            }
        }

        if let Some((rest_name, rest_index, rest_comment_pos)) = rest {
            self.write_export_assignment_start_with_comments(first, &rest_name, rest_comment_pos);
            if pattern_is_array {
                // Array rest: `source.slice(<index>)`.
                self.write_destructuring_export_base(base);
                self.write(".slice(");
                self.write(&rest_index.to_string());
                self.write(")");
            } else {
                // Object rest: `__rest(source, [<excluded keys>])`.
                self.write_helper("__rest");
                self.write("(");
                self.write_destructuring_export_base(base);
                self.write(", [");
                for (i, prop) in excluded_props.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.write("\"");
                    self.write(prop);
                    self.write("\"");
                }
                self.write("])");
            }
        }
    }

    /// Emit `<access_temp> === void 0 ? <default> : <access_temp>`.
    fn write_destructuring_export_default(&mut self, access_temp: &str, initializer: NodeIndex) {
        self.write(access_temp);
        self.write(" === void 0 ? ");
        self.emit(initializer);
        self.write(" : ");
        self.write(access_temp);
    }

    /// Write a [`DestructBase`] into the output buffer.
    fn write_destructuring_export_base(&mut self, base: &DestructBase) {
        match base {
            DestructBase::Str(text) => self.write(text),
            DestructBase::Inline { node, suffix } => {
                self.emit(*node);
                // `1.foo` is a parse error (the `.` reads as a decimal point);
                // `tsc` emits a second dot for a numeric-literal receiver.
                if suffix.starts_with('.')
                    && self.arena.get(*node).is_some_and(Node::is_numeric_literal)
                {
                    self.write(".");
                }
                self.write(suffix);
            }
        }
    }

    /// Build the property-access suffix for an object binding element, choosing
    /// dotted, numeric-indexed, or quoted-bracket form to match `tsc`.
    fn destructuring_export_property_suffix(prop_text: &str, prop_kind: Option<u16>) -> String {
        if prop_kind == Some(SyntaxKind::NumericLiteral as u16) {
            format!("[{prop_text}]")
        } else if prop_kind == Some(SyntaxKind::StringLiteral as u16)
            || !super::super::is_valid_identifier_name(prop_text)
        {
            format!("[\"{prop_text}\"]")
        } else {
            format!(".{prop_text}")
        }
    }

    /// Emit a CJS export assignment head while preserving comments attached to
    /// the corresponding binding element.
    fn write_export_assignment_start_with_comments(
        &mut self,
        first: &mut bool,
        name: &str,
        leading_comment_pos: u32,
    ) {
        self.emit_assignment_separator(first);
        self.emit_comments_before_pos(leading_comment_pos);
        self.write("exports.");
        self.write(name);
        self.write(" = ");
    }

    /// Number of elements in a binding pattern node (0 when it is not one).
    fn binding_pattern_element_count(&self, pattern_idx: NodeIndex) -> usize {
        self.arena
            .get(pattern_idx)
            .and_then(|node| self.arena.get_binding_pattern(node))
            .map_or(0, |pattern| pattern.elements.nodes.len())
    }
}
