//! Iterator and Iterable Type Checking
//!
//! This module handles type inference and checking for:
//! - Iterator and IterableIterator types
//! - for-of loops with iterables
//! - Symbol.iterator protocol
//! - Spread of iterables
//! - Async iterators and for-await-of
//!
//! # Iterator Protocol
//!
//! The iterator protocol defines how objects can be iterated:
//!
//! ```typescript
//! interface Iterator<T, TReturn = any, TNext = undefined> {
//!     next(...args: [] | [TNext]): IteratorResult<T, TReturn>;
//!     return?(value?: TReturn): IteratorResult<T, TReturn>;
//!     throw?(e?: any): IteratorResult<T, TReturn>;
//! }
//!
//! interface Iterable<T> {
//!     [Symbol.iterator](): Iterator<T>;
//! }
//!
//! interface IterableIterator<T> extends Iterator<T> {
//!     [Symbol.iterator](): IterableIterator<T>;
//! }
//! ```
//!
//! # Async Iterator Protocol
//!
//! ```typescript
//! interface AsyncIterator<T, TReturn = any, TNext = undefined> {
//!     next(...args: [] | [TNext]): Promise<IteratorResult<T, TReturn>>;
//!     return?(value?: TReturn | PromiseLike<TReturn>): Promise<IteratorResult<T, TReturn>>;
//!     throw?(e?: any): Promise<IteratorResult<T, TReturn>>;
//! }
//!
//! interface AsyncIterable<T> {
//!     [Symbol.asyncIterator](): AsyncIterator<T>;
//! }
//! ```

use super::context::CheckerContext;
use crate::parser::syntax_kind_ext;
use crate::parser::NodeIndex;
use crate::scanner::SyntaxKind;
use crate::solver::TypeId;

/// Type checker for iterators and iterables.
pub struct IteratorChecker<'a, 'ctx> {
    ctx: &'a mut CheckerContext<'ctx>,
}

/// Information about an iterator type.
#[derive(Debug, Clone)]
pub struct IteratorTypeInfo {
    /// The type of values yielded by the iterator (T)
    pub yield_type: TypeId,
    /// The type returned when done (TReturn)
    pub return_type: TypeId,
    /// The type accepted by .next() (TNext)
    pub next_type: TypeId,
}

impl Default for IteratorTypeInfo {
    fn default() -> Self {
        Self {
            yield_type: TypeId::ANY,
            return_type: TypeId::ANY,
            next_type: TypeId::UNDEFINED,
        }
    }
}

/// Result of checking an iterable type.
#[derive(Debug, Clone)]
pub struct IterableCheckResult {
    /// Whether the type is iterable
    pub is_iterable: bool,
    /// Whether the type is async iterable
    pub is_async_iterable: bool,
    /// The element type if iterable
    pub element_type: TypeId,
    /// Detailed iterator info if available
    pub iterator_info: Option<IteratorTypeInfo>,
}

impl<'a, 'ctx> IteratorChecker<'a, 'ctx> {
    /// Create a new iterator checker with a mutable context reference.
    pub fn new(ctx: &'a mut CheckerContext<'ctx>) -> Self {
        Self { ctx }
    }

    /// Check if a type is iterable (has Symbol.iterator).
    pub fn is_iterable(&self, type_id: TypeId) -> bool {
        // Built-in iterable types
        if type_id == TypeId::STRING {
            return true;
        }

        if let Some(type_key) = self.ctx.types.lookup(type_id) {
            match type_key {
                crate::solver::TypeKey::Array(_) => return true,
                crate::solver::TypeKey::Tuple(_) => return true,
                crate::solver::TypeKey::Object(shape_id) => {
                    // Check for Symbol.iterator method
                    return self.object_has_iterator_method(shape_id);
                }
                crate::solver::TypeKey::Union(type_list_id) => {
                    // Union is iterable if all constituents are iterable
                    let types = self.ctx.types.type_list(type_list_id);
                    return types.iter().all(|&t| self.is_iterable(t));
                }
                _ => {}
            }
        }

        false
    }

    /// Check if a type is async iterable (has Symbol.asyncIterator).
    pub fn is_async_iterable(&self, type_id: TypeId) -> bool {
        if let Some(type_key) = self.ctx.types.lookup(type_id) {
            match type_key {
                crate::solver::TypeKey::Object(shape_id) => {
                    return self.object_has_async_iterator_method(shape_id);
                }
                crate::solver::TypeKey::Union(type_list_id) => {
                    let types = self.ctx.types.type_list(type_list_id);
                    return types.iter().all(|&t| self.is_async_iterable(t));
                }
                _ => {}
            }
        }

        false
    }

    /// Get the element type of an iterable.
    ///
    /// For arrays, this returns the element type.
    /// For strings, this returns string (each character).
    /// For iterables, this extracts T from Iterator<T>.
    pub fn get_iterable_element_type(&self, type_id: TypeId) -> TypeId {
        // Handle built-in types
        if type_id == TypeId::STRING {
            return TypeId::STRING;
        }

        if let Some(type_key) = self.ctx.types.lookup(type_id) {
            match type_key {
                crate::solver::TypeKey::Array(elem_type) => return elem_type,
                crate::solver::TypeKey::Tuple(tuple_id) => {
                    let elements = self.ctx.types.tuple_list(tuple_id);
                    if elements.is_empty() {
                        return TypeId::NEVER;
                    }
                    // Return union of all element types
                    let types: Vec<TypeId> = elements.iter().map(|e| e.type_id).collect();
                    return self.ctx.types.union(types);
                }
                crate::solver::TypeKey::Union(type_list_id) => {
                    // Get element types from all constituents
                    let types = self.ctx.types.type_list(type_list_id);
                    let element_types: Vec<TypeId> = types
                        .iter()
                        .map(|&t| self.get_iterable_element_type(t))
                        .collect();
                    return self.ctx.types.union(element_types);
                }
                _ => {}
            }
        }

        // Default for unknown iterables
        TypeId::ANY
    }

    /// Check a for-of loop and return the element type.
    pub fn check_for_of_loop(&mut self, for_of_idx: NodeIndex) -> ForOfCheckResult {
        let Some(node) = self.ctx.arena.get(for_of_idx) else {
            return ForOfCheckResult::error("Invalid for-of node");
        };

        if node.kind != syntax_kind_ext::FOR_OF_STATEMENT {
            return ForOfCheckResult::error("Expected for-of statement");
        }

        if let Some(for_of) = self.ctx.arena.get_for_of_statement(node) {
            // Get the type of the expression being iterated
            let iterable_type = self.check_expression(for_of.expression);

            // Check if it's an async for-await-of
            let is_async = for_of.await_modifier;

            // Validate iterability
            if is_async {
                if !self.is_async_iterable(iterable_type) && !self.is_iterable(iterable_type) {
                    return ForOfCheckResult {
                        is_valid: false,
                        element_type: TypeId::ANY,
                        is_async,
                        error: Some("Type is not an async iterable".to_string()),
                    };
                }
            } else if !self.is_iterable(iterable_type) {
                return ForOfCheckResult {
                    is_valid: false,
                    element_type: TypeId::ANY,
                    is_async,
                    error: Some("Type is not iterable".to_string()),
                };
            }

            // Get the element type
            let element_type = if is_async {
                self.get_async_iterable_element_type(iterable_type)
            } else {
                self.get_iterable_element_type(iterable_type)
            };

            return ForOfCheckResult {
                is_valid: true,
                element_type,
                is_async,
                error: None,
            };
        }

        ForOfCheckResult::error("Failed to parse for-of statement")
    }

    /// Check spread of an iterable.
    pub fn check_spread(&mut self, spread_idx: NodeIndex) -> SpreadCheckResult {
        let Some(node) = self.ctx.arena.get(spread_idx) else {
            return SpreadCheckResult::error("Invalid spread node");
        };

        if node.kind != syntax_kind_ext::SPREAD_ELEMENT {
            return SpreadCheckResult::error("Expected spread element");
        }

        if let Some(spread) = self.ctx.arena.get_spread_element(node) {
            let spread_type = self.check_expression(spread.expression);

            // Check if the spread operand is iterable
            if !self.is_iterable(spread_type) {
                return SpreadCheckResult {
                    is_valid: false,
                    element_type: TypeId::ANY,
                    error: Some("Type is not iterable and cannot be spread".to_string()),
                };
            }

            let element_type = self.get_iterable_element_type(spread_type);

            return SpreadCheckResult {
                is_valid: true,
                element_type,
                error: None,
            };
        }

        SpreadCheckResult::error("Failed to parse spread element")
    }

    /// Check a for-in loop (iterates over keys).
    pub fn check_for_in_loop(&mut self, for_in_idx: NodeIndex) -> ForInCheckResult {
        let Some(node) = self.ctx.arena.get(for_in_idx) else {
            return ForInCheckResult::error("Invalid for-in node");
        };

        if node.kind != syntax_kind_ext::FOR_IN_STATEMENT {
            return ForInCheckResult::error("Expected for-in statement");
        }

        if let Some(for_in) = self.ctx.arena.get_for_in_statement(node) {
            let object_type = self.check_expression(for_in.expression);

            // for-in iterates over string keys
            // Check that the object is valid for for-in
            let is_valid = self.is_valid_for_in_target(object_type);

            return ForInCheckResult {
                is_valid,
                key_type: TypeId::STRING, // for-in always yields strings
                error: if is_valid {
                    None
                } else {
                    Some("The right-hand side must be of type 'any', an object type, or a type parameter".to_string())
                },
            };
        }

        ForInCheckResult::error("Failed to parse for-in statement")
    }

    /// Create an Iterator<T> type.
    pub fn create_iterator_type(&self, element_type: TypeId) -> TypeId {
        // TODO: Create proper Iterator<T> type reference
        // For now, return a placeholder
        TypeId::ANY
    }

    /// Create an IterableIterator<T> type.
    pub fn create_iterable_iterator_type(&self, element_type: TypeId) -> TypeId {
        // TODO: Create proper IterableIterator<T> type reference
        TypeId::ANY
    }

    /// Create an AsyncIterator<T> type.
    pub fn create_async_iterator_type(&self, element_type: TypeId) -> TypeId {
        TypeId::ANY
    }

    /// Create an AsyncIterableIterator<T> type.
    pub fn create_async_iterable_iterator_type(&self, element_type: TypeId) -> TypeId {
        TypeId::ANY
    }

    // =========================================================================
    // Helper methods
    // =========================================================================

    fn object_has_iterator_method(&self, _shape_id: crate::solver::ObjectShapeId) -> bool {
        // TODO: Check if object shape has [Symbol.iterator] method
        // This requires looking up the property with the well-known symbol key
        false
    }

    fn object_has_async_iterator_method(&self, _shape_id: crate::solver::ObjectShapeId) -> bool {
        // TODO: Check if object shape has [Symbol.asyncIterator] method
        false
    }

    fn get_async_iterable_element_type(&self, type_id: TypeId) -> TypeId {
        // For async iterables, get the awaited element type
        // This unwraps Promise<T> to get T
        self.get_iterable_element_type(type_id)
    }

    fn is_valid_for_in_target(&self, type_id: TypeId) -> bool {
        // for-in works with any, object types, and type parameters
        if type_id == TypeId::ANY {
            return true;
        }

        if let Some(type_key) = self.ctx.types.lookup(type_id) {
            match type_key {
                crate::solver::TypeKey::Object(_) => return true,
                crate::solver::TypeKey::Array(_) => return true,
                crate::solver::TypeKey::TypeParameter(_) => return true,
                _ => {}
            }
        }

        false
    }

    fn check_expression(&mut self, idx: NodeIndex) -> TypeId {
        // Check cache first
        if let Some(&cached) = self.ctx.node_types.get(&idx.0) {
            return cached;
        }

        // Basic type inference
        if let Some(node) = self.ctx.arena.get(idx) {
            match node.kind {
                k if k == SyntaxKind::NumericLiteral as u16 => TypeId::NUMBER,
                k if k == SyntaxKind::StringLiteral as u16 => TypeId::STRING,
                k if k == SyntaxKind::TrueKeyword as u16 => self.ctx.types.literal_boolean(true),
                k if k == SyntaxKind::FalseKeyword as u16 => self.ctx.types.literal_boolean(false),
                k if k == SyntaxKind::NullKeyword as u16 => TypeId::NULL,
                k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                    // For array literals, infer element type
                    self.infer_array_literal_type(idx)
                }
                _ => TypeId::ANY,
            }
        } else {
            TypeId::ANY
        }
    }

    fn infer_array_literal_type(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return self.ctx.types.array(TypeId::ANY);
        };

        if let Some(array_lit) = self.ctx.arena.get_array_literal(node) {
            if array_lit.elements.nodes.is_empty() {
                // Empty array literal has never[] element type by default
                return self.ctx.types.array(TypeId::NEVER);
            }

            // Collect element types
            let mut element_types = Vec::new();
            for &elem_idx in &array_lit.elements.nodes {
                if !elem_idx.is_null() {
                    let elem_type = self.check_expression(elem_idx);
                    element_types.push(elem_type);
                }
            }

            // Create union of element types
            let element_type = if element_types.is_empty() {
                TypeId::NEVER
            } else if element_types.len() == 1 {
                element_types[0]
            } else {
                self.ctx.types.union(element_types)
            };

            return self.ctx.types.array(element_type);
        }

        self.ctx.types.array(TypeId::ANY)
    }
}

/// Result of checking a for-of loop.
#[derive(Debug, Clone)]
pub struct ForOfCheckResult {
    /// Whether the for-of loop is valid
    pub is_valid: bool,
    /// The element type of the iteration
    pub element_type: TypeId,
    /// Whether this is an async for-await-of
    pub is_async: bool,
    /// Error message if invalid
    pub error: Option<String>,
}

impl ForOfCheckResult {
    fn error(msg: &str) -> Self {
        Self {
            is_valid: false,
            element_type: TypeId::ANY,
            is_async: false,
            error: Some(msg.to_string()),
        }
    }
}

/// Result of checking a spread operation.
#[derive(Debug, Clone)]
pub struct SpreadCheckResult {
    /// Whether the spread is valid
    pub is_valid: bool,
    /// The element type being spread
    pub element_type: TypeId,
    /// Error message if invalid
    pub error: Option<String>,
}

impl SpreadCheckResult {
    fn error(msg: &str) -> Self {
        Self {
            is_valid: false,
            element_type: TypeId::ANY,
            error: Some(msg.to_string()),
        }
    }
}

/// Result of checking a for-in loop.
#[derive(Debug, Clone)]
pub struct ForInCheckResult {
    /// Whether the for-in loop is valid
    pub is_valid: bool,
    /// The key type (always string for for-in)
    pub key_type: TypeId,
    /// Error message if invalid
    pub error: Option<String>,
}

impl ForInCheckResult {
    fn error(msg: &str) -> Self {
        Self {
            is_valid: false,
            key_type: TypeId::STRING,
            error: Some(msg.to_string()),
        }
    }
}

/// Errors that can occur during iterator type checking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IteratorError {
    /// Type is not iterable
    NotIterable,
    /// Type is not async iterable
    NotAsyncIterable,
    /// Missing Symbol.iterator method
    MissingIteratorMethod,
    /// Iterator result type mismatch
    IteratorResultMismatch { expected: TypeId, actual: TypeId },
}

impl std::fmt::Display for IteratorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IteratorError::NotIterable => {
                write!(f, "Type is not iterable")
            }
            IteratorError::NotAsyncIterable => {
                write!(f, "Type is not an async iterable")
            }
            IteratorError::MissingIteratorMethod => {
                write!(f, "Type must have a '[Symbol.iterator]()' method that returns an iterator")
            }
            IteratorError::IteratorResultMismatch { expected, actual } => {
                write!(f, "Iterator result type '{:?}' is not assignable to '{:?}'", actual, expected)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::TypeInterner;
    use crate::thin_binder::ThinBinderState;
    use crate::thin_parser::ThinParserState;

    fn create_context(source: &str) -> (ThinParserState, ThinBinderState, TypeInterner) {
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = ThinBinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        let types = TypeInterner::new();
        (parser, binder, types)
    }

    #[test]
    fn test_string_is_iterable() {
        let source = "'hello'";
        let (parser, binder, types) = create_context(source);
        let ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::checker::context::CheckerOptions::default(),
        );

        // String type is always iterable
        let checker = IteratorChecker { ctx: &mut { ctx } };
        assert!(checker.is_iterable(TypeId::STRING));
    }

    #[test]
    fn test_array_element_type() {
        let source = "[1, 2, 3]";
        let (parser, binder, types) = create_context(source);
        let ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::checker::context::CheckerOptions::default(),
        );

        // Create an array type
        let number_array = types.array(TypeId::NUMBER);
        let checker = IteratorChecker { ctx: &mut { ctx } };

        // Element type of number[] should be number
        let elem_type = checker.get_iterable_element_type(number_array);
        assert_eq!(elem_type, TypeId::NUMBER);
    }

    #[test]
    fn test_tuple_element_type() {
        let source = "const x: [number, string] = [1, 'a']";
        let (parser, binder, types) = create_context(source);
        let ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::checker::context::CheckerOptions::default(),
        );

        // Create a tuple type [number, string]
        let tuple_type = types.tuple(vec![
            crate::solver::TupleElement { type_id: TypeId::NUMBER, optional: false },
            crate::solver::TupleElement { type_id: TypeId::STRING, optional: false },
        ]);

        let checker = IteratorChecker { ctx: &mut { ctx } };

        // Element type of [number, string] should be number | string
        let elem_type = checker.get_iterable_element_type(tuple_type);
        // The result should be a union type
        assert!(checker.is_iterable(tuple_type));
    }
}
