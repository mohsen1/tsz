use super::super::super::*;
use super::super::helpers::ArraySegment;
use crate::transforms::emit_utils;
use tsz_parser::parser::NodeIndex;

impl<'a> Printer<'a> {
    pub(super) fn emit_spread_args_array(&mut self, args: &[NodeIndex]) {
        // Build arguments array using __spreadArray for spread elements
        if args.is_empty() {
            self.write("[]");
            return;
        }

        // Check if there are any spread elements
        let has_spread = args
            .iter()
            .any(|&arg_idx| emit_utils::is_spread_element(self.arena, arg_idx));

        if !has_spread {
            // No spreads, just emit an array literal
            self.write("[");
            self.emit_comma_separated(args);
            self.write("]");
            return;
        }

        // Build segments by grouping consecutive non-spread and spread elements
        let mut segments: Vec<ArraySegment> = Vec::new();
        let mut current_start = 0;

        for (i, &arg_idx) in args.iter().enumerate() {
            if emit_utils::is_spread_element(self.arena, arg_idx) {
                // Add non-spread segment before this spread
                if current_start < i {
                    segments.push(ArraySegment::Elements(&args[current_start..i]));
                }
                // Add the spread element
                segments.push(ArraySegment::Spread(arg_idx));
                current_start = i + 1;
            }
        }

        // Add remaining elements after last spread
        if current_start < args.len() {
            segments.push(ArraySegment::Elements(&args[current_start..]));
        }

        // Emit using nested __spreadArray calls
        self.emit_spread_segments(&segments);
    }

    fn emit_spread_segments(&mut self, segments: &[ArraySegment]) {
        if segments.is_empty() {
            self.write("[]");
            return;
        }

        let wrap_spread_with_read = self.ctx.target_es5 && self.ctx.options.downlevel_iteration;

        if segments.len() == 1 {
            match &segments[0] {
                ArraySegment::Spread(spread_idx) => {
                    // Just a single spread with no other arguments:
                    // TypeScript optimization - pass arrays directly unless
                    // downlevelIteration requires __read for iterable inputs.
                    if let Some(spread_node) = self.arena.get(*spread_idx) {
                        if wrap_spread_with_read {
                            self.write_helper("__spreadArray");
                            self.write("([], ");
                            self.emit_spread_expression_with_read(spread_node, true);
                            self.write(", false)");
                        } else {
                            self.emit_spread_expression(spread_node);
                        }
                    }
                }
                ArraySegment::Elements(elems) => {
                    // Just elements: [1, 2, 3]
                    self.write("[");
                    self.emit_comma_separated(elems);
                    self.write("]");
                }
            }
            return;
        }

        // Multiple segments: build nested __spreadArray calls
        // Pattern: __spreadArray(__spreadArray(base, segment1, false), segment2, false)

        // Open __spreadArray calls for all but the last segment
        for _ in 0..segments.len() - 1 {
            self.write_helper("__spreadArray");
            self.write("(");
        }

        // Emit the first segment as a complete unit
        match &segments[0] {
            ArraySegment::Elements(elems) => {
                self.write("[");
                self.emit_comma_separated(elems);
                self.write("]");
            }
            ArraySegment::Spread(spread_idx) => {
                // First segment is spread: emit as __spreadArray([], spread, false)
                self.write_helper("__spreadArray");
                self.write("([], ");
                if let Some(spread_node) = self.arena.get(*spread_idx) {
                    self.emit_spread_expression_with_read(spread_node, wrap_spread_with_read);
                }
                self.write(", false)");
            }
        }

        // Emit remaining segments - each closes one __spreadArray call
        for segment in &segments[1..] {
            match segment {
                ArraySegment::Elements(elems) => {
                    self.write(", [");
                    self.emit_comma_separated(elems);
                    self.write("], false)");
                }
                ArraySegment::Spread(spread_idx) => {
                    self.write(", ");
                    if let Some(spread_node) = self.arena.get(*spread_idx) {
                        self.emit_spread_expression_with_read(spread_node, wrap_spread_with_read);
                    }
                    self.write(", false)");
                }
            }
        }
    }

    pub(super) fn emit_new_spread_args_array(&mut self, args: &[NodeIndex]) {
        let mut segments: Vec<ArraySegment> = Vec::new();
        let mut current_start = 0;

        for (i, &arg_idx) in args.iter().enumerate() {
            if emit_utils::is_spread_element(self.arena, arg_idx) {
                if current_start < i {
                    segments.push(ArraySegment::Elements(&args[current_start..i]));
                }
                segments.push(ArraySegment::Spread(arg_idx));
                current_start = i + 1;
            }
        }

        if current_start < args.len() {
            segments.push(ArraySegment::Elements(&args[current_start..]));
        }

        if segments.is_empty() {
            self.write("[void 0]");
            return;
        }

        if segments.len() == 1
            && let ArraySegment::Spread(spread_idx) = &segments[0]
        {
            self.write_helper("__spreadArray");
            self.write("([void 0], ");
            if let Some(spread_node) = self.arena.get(*spread_idx) {
                self.emit_spread_expression(spread_node);
            }
            self.write(", false)");
            return;
        }

        for _ in 0..segments.len() - 1 {
            self.write_helper("__spreadArray");
            self.write("(");
        }

        match &segments[0] {
            ArraySegment::Elements(elems) => {
                self.write("[void 0, ");
                self.emit_comma_separated(elems);
                self.write("]");
            }
            ArraySegment::Spread(spread_idx) => {
                self.write_helper("__spreadArray");
                self.write("([void 0], ");
                if let Some(spread_node) = self.arena.get(*spread_idx) {
                    self.emit_spread_expression(spread_node);
                }
                self.write(", false)");
            }
        }

        for segment in &segments[1..] {
            match segment {
                ArraySegment::Elements(elems) => {
                    self.write(", [");
                    self.emit_comma_separated(elems);
                    self.write("], false)");
                }
                ArraySegment::Spread(spread_idx) => {
                    self.write(", ");
                    if let Some(spread_node) = self.arena.get(*spread_idx) {
                        self.emit_spread_expression(spread_node);
                    }
                    self.write(", false)");
                }
            }
        }
    }
}
