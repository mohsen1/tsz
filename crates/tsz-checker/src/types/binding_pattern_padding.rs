//! `tsc`'s `padTupleType` / `padObjectLiteralType`: a binding pattern with an
//! initializer widens the declared parameter type with optional slots or
//! properties for pattern elements the initializer does not cover.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// `tsc`'s `padTupleType`, applied from `checkDeclarationInitializer`.
    ///
    /// An array binding pattern with more elements than its initializer tuple
    /// pads that tuple, so `([x, y = 0] = [0])` declares `[number, number?]`
    /// rather than `[number]` and `([x = 0, y = 0] = [])` declares
    /// `[number?, number?]` rather than `[]`. Each padded slot is optional and
    /// takes its type from the corresponding pattern element's default, or
    /// `any` when the element has none.
    ///
    /// A pattern that is not longer than the initializer is returned unchanged,
    /// which keeps `([z, , ,] = [1, 3, 4, 6, 7])` at the initializer's own five
    /// elements. Tuples that already carry a rest element are left alone —
    /// `tsc` skips padding for those, and a trailing rest pattern element
    /// contributes no slot of its own.
    pub(crate) fn pad_array_binding_pattern_tuple(
        &mut self,
        pattern_idx: NodeIndex,
        tuple_type: TypeId,
    ) -> TypeId {
        let Some(name_node) = self.ctx.arena.get(pattern_idx) else {
            return tuple_type;
        };
        if name_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN {
            return tuple_type;
        }
        let Some(pattern) = self.ctx.arena.get_binding_pattern(name_node) else {
            return tuple_type;
        };
        let Some(elements) =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, tuple_type)
        else {
            return tuple_type;
        };
        if elements.iter().any(|element| element.rest) {
            return tuple_type;
        }
        let pattern_len = pattern.elements.nodes.len();
        if elements.len() >= pattern_len {
            return tuple_type;
        }
        let pattern_elements = pattern.elements.nodes.clone();
        let mut padded = elements;
        let pad_start = padded.len();
        for (offset, &element_idx) in pattern_elements[pad_start..pattern_len].iter().enumerate() {
            let index = pad_start + offset;
            let binding = self
                .ctx
                .arena
                .get(element_idx)
                .and_then(|node| self.ctx.arena.get_binding_element(node));
            if index == pattern_len - 1 && binding.is_some_and(|element| element.dot_dot_dot_token)
            {
                continue;
            }
            let element_type = match binding.map(|element| element.initializer) {
                Some(initializer) if initializer.is_some() => {
                    let raw = self.get_type_of_node(initializer);
                    self.widen_mutable_binding_initializer_type(initializer, raw)
                }
                _ => TypeId::ANY,
            };
            padded.push(tsz_solver::TupleElement {
                type_id: element_type,
                name: None,
                optional: true,
                rest: false,
            });
        }
        crate::query_boundaries::checkers::parameters::tuple_type_from_elements(
            self.ctx.types,
            padded,
        )
    }

    /// `tsc`'s `padObjectLiteralType`: an object binding pattern whose
    /// defaulted elements name properties missing from the initializer's type
    /// adds each as an **optional** property typed from that element's
    /// default, so `({ x, y = 0 } = { x: 0 })` declares
    /// `{ x: number; y?: number }` rather than `{ x: number }`. Elements
    /// without defaults, rest elements, and computed keys contribute nothing;
    /// a type that is not a plain object shape is returned unchanged, and the
    /// rebuild preserves the shape's flags, index signatures, and nominal
    /// identity.
    pub(crate) fn pad_object_binding_pattern_type(
        &mut self,
        pattern_idx: NodeIndex,
        object_type: TypeId,
    ) -> TypeId {
        let Some(name_node) = self.ctx.arena.get(pattern_idx) else {
            return object_type;
        };
        if name_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN {
            return object_type;
        }
        let Some(pattern) = self.ctx.arena.get_binding_pattern(name_node) else {
            return object_type;
        };
        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, object_type)
        else {
            return object_type;
        };
        let mut missing: Vec<(tsz_common::interner::Atom, NodeIndex)> = Vec::new();
        for &element_idx in &pattern.elements.nodes {
            let Some(element) = self
                .ctx
                .arena
                .get(element_idx)
                .and_then(|node| self.ctx.arena.get_binding_element(node))
            else {
                continue;
            };
            if element.dot_dot_dot_token || element.initializer.is_none() {
                continue;
            }
            let name_idx = if element.property_name.is_some() {
                element.property_name
            } else {
                element.name
            };
            let Some(ident) = self.ctx.arena.get_identifier_at(name_idx) else {
                continue;
            };
            let atom = self.ctx.types.intern_string(&ident.escaped_text);
            if shape.properties.iter().any(|prop| prop.name == atom) {
                continue;
            }
            missing.push((atom, element.initializer));
        }
        if missing.is_empty() {
            return object_type;
        }
        let mut props = shape.properties.clone();
        for (atom, initializer) in missing {
            let raw = self.get_type_of_node(initializer);
            let element_type = self.widen_mutable_binding_initializer_type(initializer, raw);
            props.push(tsz_solver::PropertyInfo::opt(atom, element_type));
        }
        crate::query_boundaries::widening::rebuild_object_with_shape_metadata(
            self.ctx.types,
            object_type,
            &shape,
            props,
        )
    }
}
