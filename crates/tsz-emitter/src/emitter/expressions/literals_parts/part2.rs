impl<'a> Printer<'a> {
    /// Emit object literal with spread elements as `Object.assign()` for pre-ES2018 targets.
    ///
    /// TypeScript's object spread lowering for ES2015-ES2017:
    /// - `{ ...a }` → `Object.assign({}, a)`
    /// - `{ x: 1, ...a }` → `Object.assign({ x: 1 }, a)`
    /// - `{ ...a, x: 1 }` → `Object.assign(Object.assign({}, a), { x: 1 })`
    /// - `{ ...a, x: 1, ...b }` → `Object.assign(Object.assign(Object.assign({}, a), { x: 1 }), b)`
    ///
    /// The pattern left-folds: each spread/segment adds one more `Object.assign` wrapping.
    fn emit_object_literal_with_object_assign(&mut self, node: &Node, elements: &[NodeIndex]) {
        // Segment elements into alternating spans of regular props and spread elements.
        // Each segment is either a slice of regular properties or a single spread node.
        #[derive(Clone)]
        enum Seg<'a> {
            Props(&'a [NodeIndex]),
            Spread(NodeIndex),
        }

        // A trailing line comment on the object literal's last element (e.g.
        // `{ a: 1, ...b } // c`) must survive the Object.assign lowering. tsc
        // emits it after the final argument and moves the closing `)` to the
        // next line. The argument span and the literal's closing brace bound
        // the comment scan so we never steal comments belonging to outer code.
        let last_element_trailing = (!self.ctx.options.remove_comments)
            .then(|| elements.last().copied())
            .flatten()
            .and_then(|last_idx| self.arena.get(last_idx).map(|n| (n.pos, n.end)))
            .map(|(last_pos, last_end_raw)| {
                let token_end = self.find_token_end_before_trivia(last_pos, last_end_raw);
                (token_end, node.end)
            });

        let mut segs: Vec<Seg<'_>> = Vec::new();
        let mut seg_start = 0usize;
        for (i, &idx) in elements.iter().enumerate() {
            let is_spread = self
                .arena
                .get(idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::SPREAD_ASSIGNMENT);
            if is_spread {
                if seg_start < i {
                    segs.push(Seg::Props(&elements[seg_start..i]));
                }
                segs.push(Seg::Spread(idx));
                seg_start = i + 1;
            }
        }
        if seg_start < elements.len() {
            segs.push(Seg::Props(&elements[seg_start..]));
        }

        // Count how many Object.assign calls we need:
        // one for each spread + one if the first segment is a spread (needs empty {} seed).
        let num_assign = segs.len();
        // Opening parens for left-folding: (num_assign - 1) calls wrapping the first.
        // Write the opening Object.assign( calls.
        for _ in 0..num_assign.saturating_sub(1) {
            self.write("Object.assign(");
        }

        // Emit the first segment (the "seed" accumulator).
        let first_seg = segs.first().cloned();
        match &first_seg {
            Some(Seg::Props(props)) => {
                self.emit_inline_object_props(props);
            }
            Some(Seg::Spread(spread_idx)) => {
                // When the spread expression is a *simple* object literal (no nested
                // spreads), tsc optimizes away the empty `{}` seed:
                //   `{ ...{x: 0} }` → `Object.assign({x: 0})`
                // But if the literal itself contains spreads, it will be lowered to
                // an Object.assign() chain, and using that as the seed would mutate
                // the intermediate result. In that case, wrap with `{}`:
                //   `{ ...{a: 3, ...b}, c: 1 }` → `Object.assign(Object.assign({}, Object.assign({a: 3}, b)), {c: 1})`
                let spread_is_simple_literal = self.arena.get(*spread_idx).is_some_and(|n| {
                    self.arena
                        .get_spread(n)
                        .and_then(|s| self.arena.get(s.expression))
                        .is_some_and(|e| {
                            if e.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                                return false;
                            }
                            // Check that the inner literal has no nested spreads
                            let Some(inner_obj) = self.arena.get_literal_expr(e) else {
                                return false;
                            };
                            !inner_obj.elements.nodes.iter().any(|&idx| {
                                self.arena
                                    .get(idx)
                                    .is_some_and(|n| n.kind == syntax_kind_ext::SPREAD_ASSIGNMENT)
                            })
                        })
                });
                if spread_is_simple_literal {
                    if segs.len() == 1 {
                        // Single spread of simple literal: Object.assign(expr)
                        self.write("Object.assign(");
                        self.emit_spread_expression_node(*spread_idx);
                        self.emit_object_assign_last_element_trailing(last_element_trailing);
                        self.write(")");
                    } else {
                        // Multiple segments, simple literal first: use expr as seed
                        self.emit_spread_expression_node(*spread_idx);
                    }
                } else {
                    // Non-literal spread: seed is {}
                    self.write("Object.assign({}, ");
                    self.emit_spread_expression_node(*spread_idx);
                    if segs.len() == 1 {
                        self.emit_object_assign_last_element_trailing(last_element_trailing);
                    }
                    self.write(")");
                }
            }
            None => {
                self.write("{}");
                return;
            }
        }

        // Emit remaining segments, each adding `, seg)` to close one Object.assign.
        let last_seg_i = segs.len().saturating_sub(1);
        for (i, seg) in segs.iter().enumerate().skip(1) {
            self.write(", ");
            match seg {
                Seg::Props(props) => {
                    self.emit_inline_object_props(props);
                }
                Seg::Spread(spread_idx) => {
                    self.emit_spread_expression_node(*spread_idx);
                }
            }
            // The outermost (last) Object.assign call wraps the final source
            // element, so its trailing comment belongs right before this `)`.
            if i == last_seg_i {
                self.emit_object_assign_last_element_trailing(last_element_trailing);
            }
            self.write(")");
        }
    }

    /// Emit a trailing comment captured from the last element of an object
    /// literal that was lowered to `Object.assign(...)`. tsc renders it after
    /// the final argument with the closing `)` moved onto the next line, e.g.
    /// `Object.assign({ a: 1 }, b // c\n)`.
    fn emit_object_assign_last_element_trailing(
        &mut self,
        last_element_trailing: Option<(u32, u32)>,
    ) {
        if let Some((token_end, max_pos)) = last_element_trailing
            && self.has_trailing_comment_on_same_line(token_end, max_pos)
        {
            // `emit_trailing_comments_before` writes its own leading space, so
            // do not add one here.
            self.emit_trailing_comments_before(token_end, max_pos);
            self.write_line();
        }
    }

    /// Emit `{ prop, prop, ... }` as an inline object literal (no lowering).
    fn emit_inline_object_props(&mut self, props: &[NodeIndex]) {
        self.write("{ ");
        for (i, &prop) in props.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.emit_object_property(prop);
        }
        self.write(" }");
    }

    /// Emit the expression part of a `SPREAD_ASSIGNMENT` node (the `x` in `...x`).
    fn emit_spread_expression_node(&mut self, spread_idx: NodeIndex) {
        if let Some(spread_node) = self.arena.get(spread_idx)
            && let Some(spread) = self.arena.get_spread(spread_node)
        {
            self.emit_expression(spread.expression);
        }
    }

    fn object_spread_has_recovered_trailing_empty_object(
        &self,
        node: &Node,
        elements: &[NodeIndex],
    ) -> bool {
        if elements.len() != 1 {
            return false;
        }
        let Some(spread_node) = self.arena.get(elements[0]) else {
            return false;
        };
        if spread_node.kind != syntax_kind_ext::SPREAD_ASSIGNMENT {
            return false;
        }
        let Some(spread) = self.arena.get_spread(spread_node) else {
            return false;
        };
        let Some(source) = self.source_text else {
            return false;
        };
        let start = std::cmp::min(
            self.arena
                .get(spread.expression)
                .map_or(spread_node.end, |expr| expr.end) as usize,
            source.len(),
        );
        let end = std::cmp::min(node.end as usize, source.len());
        if start >= end {
            return false;
        }
        source[start..end].trim_start().starts_with('{')
    }
}
