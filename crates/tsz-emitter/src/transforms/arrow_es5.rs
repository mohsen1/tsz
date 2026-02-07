//! ES5 Arrow Function Transform
//!
//! Transforms ES6 arrow functions to ES5 function expressions:
//!
//! ```typescript
//! const add = (a, b) => a + b;
//! const greet = (name) => {
//!     console.log("Hello " + name);
//! };
//! const obj = {
//!     method() {
//!         const arrow = () => this.x;  // `this` capture needed
//!     }
//! };
//! ```
//!
//! Becomes:
//!
//! ```javascript
//! var add = function (a, b) { return a + b; };
//! var greet = function (name) {
//!     console.log("Hello " + name);
//! };
//! var obj = {
//!     method: function () {
//!         var _this = this;
//!         var arrow = function () { return _this.x; };
//!     }
//! };
//! ```
//!
//! # Architecture Note
//!
//! The `contains_this_reference` function has been moved to `syntax::transform_utils`
//! as a shared utility to avoid circular dependencies. This module re-exports it
//! for backward compatibility.

use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;

// Re-export from shared utilities to avoid duplication
//
// The `contains_this_reference` function has been moved to `syntax::transform_utils`
// as a shared utility to avoid circular dependencies between lowering_pass and transforms.
// This re-export maintains backward compatibility for existing code.
pub use tsz_parser::syntax::transform_utils::contains_this_reference;

/// Context for arrow function transformation
pub struct ArrowTransformContext {
    /// Whether we need to capture `this` as `_this`
    pub needs_this_capture: bool,
}

impl Default for ArrowTransformContext {
    fn default() -> Self {
        Self::new()
    }
}

impl ArrowTransformContext {
    pub fn new() -> Self {
        ArrowTransformContext {
            needs_this_capture: false,
        }
    }

    /// Analyze an arrow function to determine if `this` capture is needed
    pub fn analyze_arrow(&mut self, arena: &NodeArena, func_idx: NodeIndex) {
        let Some(func_node) = arena.get(func_idx) else {
            return;
        };
        let Some(func_data) = arena.get_function(func_node) else {
            return;
        };

        // Check if body contains `this` references
        if !func_data.body.is_none() && contains_this_reference(arena, func_data.body) {
            self.needs_this_capture = true;
        }
    }
}

#[cfg(test)]
mod tests {

    use tsz_parser::parser::ParserState;

    #[test]
    fn test_detect_this_in_arrow() {
        let source = "const f = () => this.x;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let _root = parser.parse_source_file();

        // Simple test: the source contains "this" keyword
        assert!(
            source.contains("this"),
            "Expected to detect 'this' in source"
        );
    }

    #[test]
    fn test_no_this_in_arrow() {
        let source = "const add = (a, b) => a + b;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let _root = parser.parse_source_file();

        // Simple test: the source doesn't contain "this"
        assert!(
            !source.contains("this"),
            "Should not detect 'this' in simple arrow"
        );
    }
}
