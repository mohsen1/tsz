//! The Judge: Query-based type algebra engine.
//!
//! This module implements the "Judge" layer - pure set-theoretic computations
//! for TypeScript's type system. The Judge knows nothing about TypeScript-specific
//! quirks; those are handled by the "Lawyer" layer (compat.rs).
//!
//! ## Architecture
//!
//! ```text
//!     Checker (AST traversal, diagnostics)
//!         |
//!         v
//!     Lawyer (TypeScript compatibility rules)
//!         |
//!         v
//!     Judge (Pure type algebra - THIS MODULE)
//!         |
//!         v
//!     TypeInterner (Type storage, deduplication)
//! ```
//!
//! ## Key Design Principles
//!
//! 1. **Pure Queries**: All Judge methods are pure functions that can be memoized.
//!    Side effects (like diagnostics) are handled by the Lawyer's "explain slow" pattern.
//!
//! 2. **Coinductive Cycle Handling**: Recursive types use greatest fixed point semantics.
//!    When a cycle is detected, we assume `true` for subtype checks.
//!
//! 3. **Classifier APIs**: Instead of exposing type internals, the Judge provides
//!    high-level classifiers (e.g., `classify_iterable`, `classify_callable`).
//!
//! 4. **Configuration as Input**: Compiler options like `strictNullChecks` are
//!    explicit parameters, enabling proper caching and invalidation.
//!
//! ## Usage
//!
//! ```ignore
//! let judge = Judge::new(&interner);
//!
//! // Pure subtype check (cached)
//! let is_subtype = judge.is_subtype(source, target);
//!
//! // Evaluate meta-types (cached)
//! let evaluated = judge.evaluate(conditional_type_id);
//!
//! // High-level classifiers
//! let iterable_kind = judge.classify_iterable(type_id);
//! ```

use crate::interner::Atom;
use crate::solver::TypeDatabase;
use crate::solver::evaluate::TypeEvaluator;
use crate::solver::index_signatures::IndexKind;
use crate::solver::subtype::{SubtypeChecker, TypeEnvironment};
use crate::solver::types::*;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::sync::Arc;

// =============================================================================
// Classification Enums
// =============================================================================

/// Classification of how a type can be iterated.
///
/// Used by the Checker to determine valid for-of targets, spread behavior, etc.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IterableKind {
    /// Array<T> - provides element type
    Array(TypeId),
    /// Tuple [T, U, V] - provides element union or individual types
    Tuple(Vec<TypeId>),
    /// string - iterates chars
    String,
    /// Has [Symbol.iterator] method returning an iterator
    SyncIterator {
        /// The iterator type returned by [Symbol.iterator]
        iterator_type: TypeId,
        /// The yielded element type (from Iterator.next().value)
        element_type: TypeId,
    },
    /// Has [Symbol.asyncIterator] method returning an async iterator
    AsyncIterator {
        /// The async iterator type
        iterator_type: TypeId,
        /// The yielded element type
        element_type: TypeId,
    },
    /// Not iterable
    NotIterable,
}

/// Classification of how a type can be called.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallableKind {
    /// Regular function with a single call signature
    Function {
        params: Vec<ParamInfo>,
        return_type: TypeId,
        type_params: Vec<TypeParamInfo>,
    },
    /// Constructor (new-able)
    Constructor {
        params: Vec<ParamInfo>,
        return_type: TypeId,
        type_params: Vec<TypeParamInfo>,
    },
    /// Overloaded function with multiple call signatures
    Overloaded {
        call_signatures: Vec<CallSignature>,
        construct_signatures: Vec<CallSignature>,
    },
    /// Not callable
    NotCallable,
}

bitflags::bitflags! {
    /// Flags indicating primitive-like behavior.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct PrimitiveFlags: u32 {
        const STRING_LIKE = 1 << 0;
        const NUMBER_LIKE = 1 << 1;
        const BOOLEAN_LIKE = 1 << 2;
        const BIGINT_LIKE = 1 << 3;
        const SYMBOL_LIKE = 1 << 4;
        const VOID_LIKE = 1 << 5;
        const NULLABLE = 1 << 6;
        const UNDEFINED = 1 << 7;
        const NULL = 1 << 8;
        const NEVER = 1 << 9;
        const UNKNOWN = 1 << 10;
        const ANY = 1 << 11;
    }
}

/// Classification of a type's truthiness behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TruthinessKind {
    /// Always truthy (non-empty object, non-zero number, etc.)
    AlwaysTruthy,
    /// Always falsy (0, "", null, undefined, false)
    AlwaysFalsy,
    /// Could be either (union of truthy and falsy)
    Sometimes,
    /// Unknown truthiness (any, unknown)
    Unknown,
}

/// Property access result from the Judge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PropertyResult {
    /// Property found with this type
    Found {
        type_id: TypeId,
        optional: bool,
        readonly: bool,
    },
    /// Property not found
    NotFound,
    /// Type has index signature that could provide the property
    IndexSignature { value_type: TypeId, readonly: bool },
    /// Type is any - property access always succeeds with any
    IsAny,
    /// Type is unknown - property access requires narrowing
    IsUnknown,
    /// Type is error - propagate error silently
    IsError,
}

// =============================================================================
// Judge Configuration
// =============================================================================

/// Configuration options for the Judge.
///
/// These correspond to TypeScript compiler options that affect type checking.
/// Making them explicit enables proper caching and invalidation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct JudgeConfig {
    /// strictNullChecks - whether null/undefined are separate types
    pub strict_null_checks: bool,
    /// strictFunctionTypes - contravariant function parameters
    pub strict_function_types: bool,
    /// exactOptionalPropertyTypes - optional properties don't include undefined implicitly
    pub exact_optional_property_types: bool,
    /// noUncheckedIndexedAccess - index signatures include undefined
    pub no_unchecked_indexed_access: bool,
}

impl Default for JudgeConfig {
    fn default() -> Self {
        JudgeConfig {
            strict_null_checks: true,
            strict_function_types: true,
            exact_optional_property_types: false,
            no_unchecked_indexed_access: false,
        }
    }
}

// =============================================================================
// Judge Trait
// =============================================================================

/// The Judge trait: pure type algebra queries.
///
/// This trait defines the query interface for type checking operations.
/// Implementations can provide different caching strategies (e.g., Salsa).
///
/// ## Coinductive Semantics
///
/// Subtype checks use coinductive semantics for recursive types:
/// - When a cycle is detected, assume `true` (greatest fixed point)
/// - This correctly handles types like `type List<T> = { head: T; tail: List<T> }`
///
/// ## Memoization
///
/// All methods are designed to be memoizable:
/// - No side effects (diagnostics handled separately)
/// - Deterministic results for same inputs
/// - Configuration is explicit (not implicit state)
pub trait Judge {
    // =========================================================================
    // Core Type Relations
    // =========================================================================

    /// Check if `source` is a subtype of `target`.
    ///
    /// Uses coinductive semantics: cycles assume `true`.
    ///
    /// # Example
    /// ```ignore
    /// // number <: number | string
    /// assert!(judge.is_subtype(TypeId::NUMBER, union_type));
    /// ```
    fn is_subtype(&self, source: TypeId, target: TypeId) -> bool;

    /// Check if two types are identical (stricter than subtyping).
    ///
    /// Identity requires both `A <: B` and `B <: A`.
    fn are_identical(&self, a: TypeId, b: TypeId) -> bool {
        a == b || (self.is_subtype(a, b) && self.is_subtype(b, a))
    }

    // =========================================================================
    // Type Evaluation
    // =========================================================================

    /// Evaluate a type, resolving meta-types (conditional, mapped, keyof, etc.).
    ///
    /// Returns the evaluated type (may be the same if no evaluation needed).
    ///
    /// # Cycle Recovery
    /// Returns the input type on cycle (identity recovery).
    fn evaluate(&self, type_id: TypeId) -> TypeId;

    /// Instantiate a generic type with type arguments.
    ///
    /// # Example
    /// ```ignore
    /// // Array<number> from Array<T> with T=number
    /// let array_number = judge.instantiate(array_generic, &[TypeId::NUMBER]);
    /// ```
    fn instantiate(&self, generic: TypeId, args: &[TypeId]) -> TypeId;

    // =========================================================================
    // Type Classifiers
    // =========================================================================

    /// Classify how a type can be iterated.
    ///
    /// Used for:
    /// - for-of loop targets
    /// - Spread operators
    /// - Array.from() arguments
    fn classify_iterable(&self, type_id: TypeId) -> IterableKind;

    /// Classify how a type can be called.
    ///
    /// Used for:
    /// - Call expressions
    /// - new expressions
    /// - Overload resolution
    fn classify_callable(&self, type_id: TypeId) -> CallableKind;

    /// Get primitive-like behavior flags for a type.
    ///
    /// Used for:
    /// - Binary operator resolution
    /// - Type coercion rules
    fn classify_primitive(&self, type_id: TypeId) -> PrimitiveFlags;

    /// Classify a type's truthiness behavior.
    ///
    /// Used for:
    /// - Control flow narrowing
    /// - Conditional expressions
    fn classify_truthiness(&self, type_id: TypeId) -> TruthinessKind;

    // =========================================================================
    // Property Access
    // =========================================================================

    /// Get the apparent type (unwrap type params, resolve constraints).
    fn apparent_type(&self, type_id: TypeId) -> TypeId;

    /// Get a specific property's type from a type.
    ///
    /// Returns `PropertyResult` which distinguishes between:
    /// - Property found
    /// - Property not found
    /// - Index signature match
    /// - Special types (any, unknown, error)
    fn get_property(&self, type_id: TypeId, name: Atom) -> PropertyResult;

    /// Get all members of a type as (name, type) pairs.
    fn get_members(&self, type_id: TypeId) -> Arc<Vec<(Atom, TypeId)>>;

    /// Get call signatures of a type.
    fn get_call_signatures(&self, type_id: TypeId) -> Arc<Vec<CallSignature>>;

    /// Get construct signatures of a type.
    fn get_construct_signatures(&self, type_id: TypeId) -> Arc<Vec<CallSignature>>;

    /// Get the result of indexing: T[K]
    fn get_index_type(&self, object: TypeId, key: TypeId) -> TypeId;

    /// Get index signature type (string or number indexer).
    fn get_index_signature(&self, type_id: TypeId, kind: IndexKind) -> Option<TypeId>;

    /// Get keyof: keyof T
    fn get_keyof(&self, type_id: TypeId) -> TypeId;

    // =========================================================================
    // Configuration
    // =========================================================================

    /// Get the current configuration.
    fn config(&self) -> &JudgeConfig;
}

// =============================================================================
// Default Judge Implementation
// =============================================================================

/// Default implementation of the Judge trait.
///
/// Uses basic caching with FxHashMap. For production use with incremental
/// compilation, consider a Salsa-based implementation.
pub struct DefaultJudge<'a> {
    db: &'a dyn TypeDatabase,
    config: JudgeConfig,
    /// Type environment for resolving references
    env: &'a TypeEnvironment,
    /// Cache for subtype results
    subtype_cache: RefCell<FxHashMap<(TypeId, TypeId), bool>>,
    /// Cache for evaluated types
    eval_cache: RefCell<FxHashMap<TypeId, TypeId>>,
}

impl<'a> DefaultJudge<'a> {
    /// Create a new Judge with the given database and configuration.
    pub fn new(db: &'a dyn TypeDatabase, env: &'a TypeEnvironment, config: JudgeConfig) -> Self {
        DefaultJudge {
            db,
            config,
            env,
            subtype_cache: RefCell::new(FxHashMap::default()),
            eval_cache: RefCell::new(FxHashMap::default()),
        }
    }

    /// Create a Judge with default configuration.
    pub fn with_defaults(db: &'a dyn TypeDatabase, env: &'a TypeEnvironment) -> Self {
        Self::new(db, env, JudgeConfig::default())
    }

    /// Clear all caches.
    pub fn clear_caches(&self) {
        self.subtype_cache.borrow_mut().clear();
        self.eval_cache.borrow_mut().clear();
    }

    /// Get the underlying database.
    pub fn db(&self) -> &'a dyn TypeDatabase {
        self.db
    }
}

impl<'a> Judge for DefaultJudge<'a> {
    fn is_subtype(&self, source: TypeId, target: TypeId) -> bool {
        // Fast path: identity
        if source == target {
            return true;
        }

        // Check cache
        let key = (source, target);
        if let Some(&cached) = self.subtype_cache.borrow().get(&key) {
            return cached;
        }

        // Create a SubtypeChecker and perform the check
        let mut checker = SubtypeChecker::with_resolver(self.db, self.env);
        checker.strict_function_types = self.config.strict_function_types;
        checker.strict_null_checks = self.config.strict_null_checks;
        checker.exact_optional_property_types = self.config.exact_optional_property_types;
        checker.no_unchecked_indexed_access = self.config.no_unchecked_indexed_access;

        let result = checker.is_subtype_of(source, target);

        // Cache the result
        self.subtype_cache.borrow_mut().insert(key, result);

        result
    }

    fn evaluate(&self, type_id: TypeId) -> TypeId {
        // Fast path: intrinsics don't need evaluation
        if type_id.is_intrinsic() {
            return type_id;
        }

        // Check cache
        if let Some(&cached) = self.eval_cache.borrow().get(&type_id) {
            return cached;
        }

        // Create evaluator and evaluate
        let mut evaluator = TypeEvaluator::with_resolver(self.db, self.env);
        let result = evaluator.evaluate(type_id);

        // Cache the result
        self.eval_cache.borrow_mut().insert(type_id, result);

        result
    }

    fn instantiate(&self, generic: TypeId, args: &[TypeId]) -> TypeId {
        use crate::solver::instantiate::instantiate_generic;

        // Get type params from the generic type
        let key = match self.db.lookup(generic) {
            Some(k) => k,
            None => return TypeId::ERROR,
        };

        // Try to get type params from Lazy - use DefId directly
        if let TypeKey::Lazy(def_id) = &key {
            if let Some(params) = self.env.get_def_params(*def_id) {
                if let Some(resolved) = self.env.get_def(*def_id) {
                    return instantiate_generic(self.db, resolved, params, args);
                }
            }
        }

        // Fallback: can't instantiate
        generic
    }

    fn classify_iterable(&self, type_id: TypeId) -> IterableKind {
        let evaluated = self.evaluate(type_id);

        // Check for special types
        if evaluated == TypeId::ANY || evaluated == TypeId::ERROR {
            return IterableKind::NotIterable;
        }
        if evaluated == TypeId::STRING {
            return IterableKind::String;
        }

        let key = match self.db.lookup(evaluated) {
            Some(k) => k,
            None => return IterableKind::NotIterable,
        };

        match key {
            TypeKey::Array(elem) => IterableKind::Array(elem),
            TypeKey::Tuple(list_id) => {
                let elements = self.db.tuple_list(list_id);
                let types: Vec<TypeId> = elements.iter().map(|e| e.type_id).collect();
                IterableKind::Tuple(types)
            }
            TypeKey::Literal(LiteralValue::String(_)) => IterableKind::String,
            TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                // Check for Symbol.iterator
                let shape = self.db.object_shape(shape_id);
                let iterator_name = self.db.intern_string("[Symbol.iterator]");
                for prop in &shape.properties {
                    if prop.name == iterator_name {
                        // Found iterator - extract element type
                        return IterableKind::SyncIterator {
                            iterator_type: prop.type_id,
                            element_type: self.extract_iterator_element_type(prop.type_id),
                        };
                    }
                }
                // Check for Symbol.asyncIterator
                let async_iterator_name = self.db.intern_string("[Symbol.asyncIterator]");
                for prop in &shape.properties {
                    if prop.name == async_iterator_name {
                        return IterableKind::AsyncIterator {
                            iterator_type: prop.type_id,
                            element_type: self.extract_iterator_element_type(prop.type_id),
                        };
                    }
                }
                IterableKind::NotIterable
            }
            TypeKey::Union(members_id) => {
                // All members must be iterable with compatible element types
                let members = self.db.type_list(members_id);
                let mut element_types = Vec::new();
                for &member in members.iter() {
                    match self.classify_iterable(member) {
                        IterableKind::Array(elem) => element_types.push(elem),
                        IterableKind::Tuple(elems) => element_types.extend(elems),
                        IterableKind::String => element_types.push(TypeId::STRING),
                        IterableKind::SyncIterator { element_type, .. } => {
                            element_types.push(element_type);
                        }
                        IterableKind::AsyncIterator { element_type, .. } => {
                            element_types.push(element_type);
                        }
                        IterableKind::NotIterable => return IterableKind::NotIterable,
                    }
                }
                if element_types.is_empty() {
                    IterableKind::NotIterable
                } else {
                    let union = self.db.union(element_types);
                    IterableKind::Array(union)
                }
            }
            _ => IterableKind::NotIterable,
        }
    }

    fn classify_callable(&self, type_id: TypeId) -> CallableKind {
        let evaluated = self.evaluate(type_id);

        if evaluated == TypeId::ANY {
            return CallableKind::NotCallable;
        }

        let key = match self.db.lookup(evaluated) {
            Some(k) => k,
            None => return CallableKind::NotCallable,
        };

        match key {
            TypeKey::Function(fn_id) => {
                let shape = self.db.function_shape(fn_id);
                if shape.is_constructor {
                    CallableKind::Constructor {
                        params: shape.params.clone(),
                        return_type: shape.return_type,
                        type_params: shape.type_params.clone(),
                    }
                } else {
                    CallableKind::Function {
                        params: shape.params.clone(),
                        return_type: shape.return_type,
                        type_params: shape.type_params.clone(),
                    }
                }
            }
            TypeKey::Callable(callable_id) => {
                let shape = self.db.callable_shape(callable_id);
                CallableKind::Overloaded {
                    call_signatures: shape.call_signatures.clone(),
                    construct_signatures: shape.construct_signatures.clone(),
                }
            }
            _ => CallableKind::NotCallable,
        }
    }

    fn classify_primitive(&self, type_id: TypeId) -> PrimitiveFlags {
        let mut flags = PrimitiveFlags::empty();

        // Handle intrinsics directly
        match type_id {
            TypeId::ANY => return PrimitiveFlags::ANY,
            TypeId::UNKNOWN => return PrimitiveFlags::UNKNOWN,
            TypeId::NEVER => return PrimitiveFlags::NEVER,
            TypeId::VOID => return PrimitiveFlags::VOID_LIKE,
            TypeId::UNDEFINED => {
                return PrimitiveFlags::UNDEFINED | PrimitiveFlags::NULLABLE;
            }
            TypeId::NULL => return PrimitiveFlags::NULL | PrimitiveFlags::NULLABLE,
            TypeId::BOOLEAN | TypeId::BOOLEAN_TRUE | TypeId::BOOLEAN_FALSE => {
                return PrimitiveFlags::BOOLEAN_LIKE;
            }
            TypeId::NUMBER => return PrimitiveFlags::NUMBER_LIKE,
            TypeId::STRING => return PrimitiveFlags::STRING_LIKE,
            TypeId::BIGINT => return PrimitiveFlags::BIGINT_LIKE,
            TypeId::SYMBOL => return PrimitiveFlags::SYMBOL_LIKE,
            _ => {}
        }

        let key = match self.db.lookup(type_id) {
            Some(k) => k,
            None => return flags,
        };

        match key {
            TypeKey::Literal(LiteralValue::String(_)) => flags |= PrimitiveFlags::STRING_LIKE,
            TypeKey::Literal(LiteralValue::Number(_)) => flags |= PrimitiveFlags::NUMBER_LIKE,
            TypeKey::Literal(LiteralValue::Boolean(_)) => flags |= PrimitiveFlags::BOOLEAN_LIKE,
            TypeKey::Literal(LiteralValue::BigInt(_)) => flags |= PrimitiveFlags::BIGINT_LIKE,
            TypeKey::Union(members_id) => {
                let members = self.db.type_list(members_id);
                for &member in members.iter() {
                    flags |= self.classify_primitive(member);
                }
            }
            TypeKey::TemplateLiteral(_) => flags |= PrimitiveFlags::STRING_LIKE,
            _ => {}
        }

        flags
    }

    fn classify_truthiness(&self, type_id: TypeId) -> TruthinessKind {
        // Handle intrinsics
        match type_id {
            TypeId::ANY | TypeId::UNKNOWN => return TruthinessKind::Unknown,
            TypeId::NEVER => return TruthinessKind::AlwaysFalsy,
            TypeId::VOID | TypeId::UNDEFINED | TypeId::NULL => return TruthinessKind::AlwaysFalsy,
            TypeId::BOOLEAN => return TruthinessKind::Sometimes,
            TypeId::BOOLEAN_TRUE => return TruthinessKind::AlwaysTruthy,
            TypeId::BOOLEAN_FALSE => return TruthinessKind::AlwaysFalsy,
            _ => {}
        }

        let key = match self.db.lookup(type_id) {
            Some(k) => k,
            None => return TruthinessKind::Unknown,
        };

        match key {
            TypeKey::Literal(LiteralValue::String(s)) => {
                let s_str = self.db.resolve_atom(s);
                if s_str.is_empty() {
                    TruthinessKind::AlwaysFalsy
                } else {
                    TruthinessKind::AlwaysTruthy
                }
            }
            TypeKey::Literal(LiteralValue::Number(n)) => {
                if n.0 == 0.0 || n.0.is_nan() {
                    TruthinessKind::AlwaysFalsy
                } else {
                    TruthinessKind::AlwaysTruthy
                }
            }
            TypeKey::Literal(LiteralValue::Boolean(b)) => {
                if b {
                    TruthinessKind::AlwaysTruthy
                } else {
                    TruthinessKind::AlwaysFalsy
                }
            }
            TypeKey::Literal(LiteralValue::BigInt(s)) => {
                let s_str = self.db.resolve_atom(s);
                if s_str == "0" || s_str == "0n" {
                    TruthinessKind::AlwaysFalsy
                } else {
                    TruthinessKind::AlwaysTruthy
                }
            }
            TypeKey::Object(_)
            | TypeKey::ObjectWithIndex(_)
            | TypeKey::Array(_)
            | TypeKey::Tuple(_)
            | TypeKey::Function(_)
            | TypeKey::Callable(_) => TruthinessKind::AlwaysTruthy,
            TypeKey::Union(members_id) => {
                let members = self.db.type_list(members_id);
                let mut has_truthy = false;
                let mut has_falsy = false;
                for &member in members.iter() {
                    match self.classify_truthiness(member) {
                        TruthinessKind::AlwaysTruthy => has_truthy = true,
                        TruthinessKind::AlwaysFalsy => has_falsy = true,
                        TruthinessKind::Sometimes | TruthinessKind::Unknown => {
                            has_truthy = true;
                            has_falsy = true;
                        }
                    }
                }
                match (has_truthy, has_falsy) {
                    (true, true) => TruthinessKind::Sometimes,
                    (true, false) => TruthinessKind::AlwaysTruthy,
                    (false, true) => TruthinessKind::AlwaysFalsy,
                    (false, false) => TruthinessKind::Unknown,
                }
            }
            TypeKey::Intrinsic(IntrinsicKind::String)
            | TypeKey::Intrinsic(IntrinsicKind::Number)
            | TypeKey::Intrinsic(IntrinsicKind::Bigint) => {
                // Could be empty string, 0, or 0n
                TruthinessKind::Sometimes
            }
            _ => TruthinessKind::Unknown,
        }
    }

    fn apparent_type(&self, type_id: TypeId) -> TypeId {
        let key = match self.db.lookup(type_id) {
            Some(k) => k,
            None => return type_id,
        };

        match key {
            TypeKey::TypeParameter(ref info) => info.constraint.unwrap_or(type_id),
            TypeKey::Lazy(def_id) => self.env.get_def(def_id).unwrap_or(type_id),
            _ => type_id,
        }
    }

    fn get_property(&self, type_id: TypeId, name: Atom) -> PropertyResult {
        // Handle special types
        match type_id {
            TypeId::ANY => return PropertyResult::IsAny,
            TypeId::UNKNOWN => return PropertyResult::IsUnknown,
            TypeId::ERROR => return PropertyResult::IsError,
            _ => {}
        }

        let evaluated = self.evaluate(type_id);
        let key = match self.db.lookup(evaluated) {
            Some(k) => k,
            None => return PropertyResult::NotFound,
        };

        match key {
            TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.db.object_shape(shape_id);

                // Check named properties first
                for prop in &shape.properties {
                    if prop.name == name {
                        return PropertyResult::Found {
                            type_id: prop.type_id,
                            optional: prop.optional,
                            readonly: prop.readonly,
                        };
                    }
                }

                // Check index signatures
                if let Some(ref string_idx) = shape.string_index {
                    return PropertyResult::IndexSignature {
                        value_type: string_idx.value_type,
                        readonly: string_idx.readonly,
                    };
                }

                PropertyResult::NotFound
            }
            TypeKey::Array(_elem) => {
                let name_str = self.db.resolve_atom(name);
                if name_str == "length" {
                    PropertyResult::Found {
                        type_id: TypeId::NUMBER,
                        optional: false,
                        readonly: false,
                    }
                } else {
                    // Could check for array methods here
                    PropertyResult::NotFound
                }
            }
            TypeKey::Tuple(list_id) => {
                let name_str = self.db.resolve_atom(name);
                if name_str == "length" {
                    let elements = self.db.tuple_list(list_id);
                    let len_type = self.db.literal_number(elements.len() as f64);
                    PropertyResult::Found {
                        type_id: len_type,
                        optional: false,
                        readonly: true,
                    }
                } else if let Ok(idx) = name_str.parse::<usize>() {
                    let elements = self.db.tuple_list(list_id);
                    if let Some(elem) = elements.get(idx) {
                        PropertyResult::Found {
                            type_id: elem.type_id,
                            optional: elem.optional,
                            readonly: false,
                        }
                    } else {
                        PropertyResult::NotFound
                    }
                } else {
                    PropertyResult::NotFound
                }
            }
            TypeKey::Union(members_id) => {
                let members = self.db.type_list(members_id);
                let mut result_types = Vec::new();
                let mut all_optional = true;
                let mut any_readonly = false;

                for &member in members.iter() {
                    match self.get_property(member, name) {
                        PropertyResult::Found {
                            type_id,
                            optional,
                            readonly,
                        } => {
                            result_types.push(type_id);
                            if !optional {
                                all_optional = false;
                            }
                            if readonly {
                                any_readonly = true;
                            }
                        }
                        PropertyResult::IndexSignature {
                            value_type,
                            readonly,
                        } => {
                            result_types.push(value_type);
                            if readonly {
                                any_readonly = true;
                            }
                        }
                        PropertyResult::IsAny => return PropertyResult::IsAny,
                        PropertyResult::IsUnknown => return PropertyResult::IsUnknown,
                        PropertyResult::IsError => return PropertyResult::IsError,
                        PropertyResult::NotFound => {
                            // Property missing from at least one union member
                            return PropertyResult::NotFound;
                        }
                    }
                }

                if result_types.is_empty() {
                    PropertyResult::NotFound
                } else {
                    PropertyResult::Found {
                        type_id: self.db.union(result_types),
                        optional: all_optional,
                        readonly: any_readonly,
                    }
                }
            }
            TypeKey::Intersection(members_id) => {
                let members = self.db.type_list(members_id);
                let mut found_types = Vec::new();
                let mut optional = true;
                let mut readonly = false;

                for &member in members.iter() {
                    if let PropertyResult::Found {
                        type_id,
                        optional: opt,
                        readonly: ro,
                    } = self.get_property(member, name)
                    {
                        found_types.push(type_id);
                        if !opt {
                            optional = false;
                        }
                        if ro {
                            readonly = true;
                        }
                    }
                }

                if found_types.is_empty() {
                    PropertyResult::NotFound
                } else {
                    PropertyResult::Found {
                        type_id: self.db.intersection(found_types),
                        optional,
                        readonly,
                    }
                }
            }
            _ => PropertyResult::NotFound,
        }
    }

    fn get_members(&self, type_id: TypeId) -> Arc<Vec<(Atom, TypeId)>> {
        let evaluated = self.evaluate(type_id);
        let key = match self.db.lookup(evaluated) {
            Some(k) => k,
            None => return Arc::new(Vec::new()),
        };

        match key {
            TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.db.object_shape(shape_id);
                Arc::new(
                    shape
                        .properties
                        .iter()
                        .map(|p| (p.name, p.type_id))
                        .collect(),
                )
            }
            TypeKey::Callable(callable_id) => {
                let shape = self.db.callable_shape(callable_id);
                Arc::new(
                    shape
                        .properties
                        .iter()
                        .map(|p| (p.name, p.type_id))
                        .collect(),
                )
            }
            _ => Arc::new(Vec::new()),
        }
    }

    fn get_call_signatures(&self, type_id: TypeId) -> Arc<Vec<CallSignature>> {
        let evaluated = self.evaluate(type_id);
        let key = match self.db.lookup(evaluated) {
            Some(k) => k,
            None => return Arc::new(Vec::new()),
        };

        match key {
            TypeKey::Function(fn_id) => {
                let shape = self.db.function_shape(fn_id);
                if shape.is_constructor {
                    return Arc::new(Vec::new());
                }
                Arc::new(vec![CallSignature {
                    type_params: shape.type_params.clone(),
                    params: shape.params.clone(),
                    this_type: shape.this_type,
                    return_type: shape.return_type,
                    type_predicate: shape.type_predicate.clone(),
                    is_method: shape.is_method,
                }])
            }
            TypeKey::Callable(callable_id) => {
                let shape = self.db.callable_shape(callable_id);
                Arc::new(shape.call_signatures.clone())
            }
            _ => Arc::new(Vec::new()),
        }
    }

    fn get_construct_signatures(&self, type_id: TypeId) -> Arc<Vec<CallSignature>> {
        let evaluated = self.evaluate(type_id);
        let key = match self.db.lookup(evaluated) {
            Some(k) => k,
            None => return Arc::new(Vec::new()),
        };

        match key {
            TypeKey::Function(fn_id) => {
                let shape = self.db.function_shape(fn_id);
                if !shape.is_constructor {
                    return Arc::new(Vec::new());
                }
                Arc::new(vec![CallSignature {
                    type_params: shape.type_params.clone(),
                    params: shape.params.clone(),
                    this_type: shape.this_type,
                    return_type: shape.return_type,
                    type_predicate: shape.type_predicate.clone(),
                    is_method: false,
                }])
            }
            TypeKey::Callable(callable_id) => {
                let shape = self.db.callable_shape(callable_id);
                Arc::new(shape.construct_signatures.clone())
            }
            _ => Arc::new(Vec::new()),
        }
    }

    fn get_index_type(&self, object: TypeId, key: TypeId) -> TypeId {
        crate::solver::evaluate::evaluate_index_access_with_options(
            self.db,
            object,
            key,
            self.config.no_unchecked_indexed_access,
        )
    }

    fn get_index_signature(&self, type_id: TypeId, kind: IndexKind) -> Option<TypeId> {
        let evaluated = self.evaluate(type_id);
        let key = self.db.lookup(evaluated)?;

        match key {
            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.db.object_shape(shape_id);
                match kind {
                    IndexKind::String => shape.string_index.as_ref().map(|s| s.value_type),
                    IndexKind::Number => shape.number_index.as_ref().map(|s| s.value_type),
                }
            }
            TypeKey::Array(elem) => {
                if kind == IndexKind::Number {
                    Some(elem)
                } else {
                    None
                }
            }
            TypeKey::Tuple(list_id) => {
                if kind == IndexKind::Number {
                    let elements = self.db.tuple_list(list_id);
                    let types: Vec<TypeId> = elements.iter().map(|e| e.type_id).collect();
                    Some(self.db.union(types))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn get_keyof(&self, type_id: TypeId) -> TypeId {
        crate::solver::evaluate::evaluate_keyof(self.db, type_id)
    }

    fn config(&self) -> &JudgeConfig {
        &self.config
    }
}

impl<'a> DefaultJudge<'a> {
    /// Extract the element type from an iterator type.
    fn extract_iterator_element_type(&self, iterator_type: TypeId) -> TypeId {
        // Look for .next() method returning { value: T, done: boolean }
        let next_name = self.db.intern_string("next");
        if let PropertyResult::Found { type_id, .. } = self.get_property(iterator_type, next_name) {
            // Check if it's a function
            if let Some(TypeKey::Function(fn_id)) = self.db.lookup(type_id) {
                let shape = self.db.function_shape(fn_id);
                // Look for value property in return type
                let value_name = self.db.intern_string("value");
                if let PropertyResult::Found {
                    type_id: value_type,
                    ..
                } = self.get_property(shape.return_type, value_name)
                {
                    return value_type;
                }
            }
        }
        TypeId::UNKNOWN
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::TypeInterner;

    fn create_test_interner() -> TypeInterner {
        TypeInterner::new()
    }

    #[test]
    fn test_is_subtype_identity() {
        let interner = create_test_interner();
        let env = TypeEnvironment::new();
        let judge = DefaultJudge::with_defaults(&interner, &env);

        assert!(judge.is_subtype(TypeId::NUMBER, TypeId::NUMBER));
        assert!(judge.is_subtype(TypeId::STRING, TypeId::STRING));
        assert!(judge.is_subtype(TypeId::BOOLEAN, TypeId::BOOLEAN));
    }

    #[test]
    fn test_is_subtype_any_unknown() {
        let interner = create_test_interner();
        let env = TypeEnvironment::new();
        let judge = DefaultJudge::with_defaults(&interner, &env);

        // Everything is subtype of any
        assert!(judge.is_subtype(TypeId::NUMBER, TypeId::ANY));
        assert!(judge.is_subtype(TypeId::STRING, TypeId::ANY));

        // Everything is subtype of unknown
        assert!(judge.is_subtype(TypeId::NUMBER, TypeId::UNKNOWN));
        assert!(judge.is_subtype(TypeId::STRING, TypeId::UNKNOWN));

        // any is subtype of everything
        assert!(judge.is_subtype(TypeId::ANY, TypeId::NUMBER));
        assert!(judge.is_subtype(TypeId::ANY, TypeId::STRING));

        // never is subtype of everything
        assert!(judge.is_subtype(TypeId::NEVER, TypeId::NUMBER));
        assert!(judge.is_subtype(TypeId::NEVER, TypeId::STRING));
    }

    #[test]
    fn test_classify_primitive() {
        let interner = create_test_interner();
        let env = TypeEnvironment::new();
        let judge = DefaultJudge::with_defaults(&interner, &env);

        let flags = judge.classify_primitive(TypeId::NUMBER);
        assert!(flags.contains(PrimitiveFlags::NUMBER_LIKE));
        assert!(!flags.contains(PrimitiveFlags::STRING_LIKE));

        let flags = judge.classify_primitive(TypeId::STRING);
        assert!(flags.contains(PrimitiveFlags::STRING_LIKE));
        assert!(!flags.contains(PrimitiveFlags::NUMBER_LIKE));

        let flags = judge.classify_primitive(TypeId::NULL);
        assert!(flags.contains(PrimitiveFlags::NULLABLE));
        assert!(flags.contains(PrimitiveFlags::NULL));

        let flags = judge.classify_primitive(TypeId::UNDEFINED);
        assert!(flags.contains(PrimitiveFlags::NULLABLE));
        assert!(flags.contains(PrimitiveFlags::UNDEFINED));
    }

    #[test]
    fn test_classify_truthiness() {
        let interner = create_test_interner();
        let env = TypeEnvironment::new();
        let judge = DefaultJudge::with_defaults(&interner, &env);

        assert_eq!(
            judge.classify_truthiness(TypeId::BOOLEAN_TRUE),
            TruthinessKind::AlwaysTruthy
        );
        assert_eq!(
            judge.classify_truthiness(TypeId::BOOLEAN_FALSE),
            TruthinessKind::AlwaysFalsy
        );
        assert_eq!(
            judge.classify_truthiness(TypeId::BOOLEAN),
            TruthinessKind::Sometimes
        );
        assert_eq!(
            judge.classify_truthiness(TypeId::NULL),
            TruthinessKind::AlwaysFalsy
        );
        assert_eq!(
            judge.classify_truthiness(TypeId::UNDEFINED),
            TruthinessKind::AlwaysFalsy
        );
        assert_eq!(
            judge.classify_truthiness(TypeId::ANY),
            TruthinessKind::Unknown
        );
    }

    #[test]
    fn test_classify_iterable_array() {
        let interner = create_test_interner();
        let env = TypeEnvironment::new();
        let judge = DefaultJudge::with_defaults(&interner, &env);

        let array_number = interner.array(TypeId::NUMBER);
        match judge.classify_iterable(array_number) {
            IterableKind::Array(elem) => assert_eq!(elem, TypeId::NUMBER),
            _ => panic!("Expected Array iterable kind"),
        }
    }

    #[test]
    fn test_classify_iterable_string() {
        let interner = create_test_interner();
        let env = TypeEnvironment::new();
        let judge = DefaultJudge::with_defaults(&interner, &env);

        assert_eq!(
            judge.classify_iterable(TypeId::STRING),
            IterableKind::String
        );
    }

    #[test]
    fn test_classify_callable_function() {
        let interner = create_test_interner();
        let env = TypeEnvironment::new();
        let judge = DefaultJudge::with_defaults(&interner, &env);

        let fn_type = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: None,
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::STRING,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        match judge.classify_callable(fn_type) {
            CallableKind::Function {
                params,
                return_type,
                ..
            } => {
                assert_eq!(params.len(), 1);
                assert_eq!(params[0].type_id, TypeId::NUMBER);
                assert_eq!(return_type, TypeId::STRING);
            }
            _ => panic!("Expected Function callable kind"),
        }
    }

    #[test]
    fn test_get_property_object() {
        let interner = create_test_interner();
        let env = TypeEnvironment::new();
        let judge = DefaultJudge::with_defaults(&interner, &env);

        let foo_atom = interner.intern_string("foo");
        let obj = interner.object(vec![PropertyInfo {
            name: foo_atom,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

        match judge.get_property(obj, foo_atom) {
            PropertyResult::Found {
                type_id,
                optional,
                readonly,
            } => {
                assert_eq!(type_id, TypeId::NUMBER);
                assert!(!optional);
                assert!(!readonly);
            }
            _ => panic!("Expected property to be found"),
        }

        let bar_atom = interner.intern_string("bar");
        match judge.get_property(obj, bar_atom) {
            PropertyResult::NotFound => {}
            _ => panic!("Expected property not found"),
        }
    }

    #[test]
    fn test_get_property_special_types() {
        let interner = create_test_interner();
        let env = TypeEnvironment::new();
        let judge = DefaultJudge::with_defaults(&interner, &env);

        let foo_atom = interner.intern_string("foo");

        assert!(matches!(
            judge.get_property(TypeId::ANY, foo_atom),
            PropertyResult::IsAny
        ));
        assert!(matches!(
            judge.get_property(TypeId::UNKNOWN, foo_atom),
            PropertyResult::IsUnknown
        ));
        assert!(matches!(
            judge.get_property(TypeId::ERROR, foo_atom),
            PropertyResult::IsError
        ));
    }

    #[test]
    fn test_caching() {
        let interner = create_test_interner();
        let env = TypeEnvironment::new();
        let judge = DefaultJudge::with_defaults(&interner, &env);

        // First call - not cached
        let result1 = judge.is_subtype(TypeId::NUMBER, TypeId::ANY);
        assert!(result1);

        // Second call - should be cached
        let result2 = judge.is_subtype(TypeId::NUMBER, TypeId::ANY);
        assert!(result2);

        // Clear caches
        judge.clear_caches();

        // Third call - cache was cleared, should still work
        let result3 = judge.is_subtype(TypeId::NUMBER, TypeId::ANY);
        assert!(result3);
    }
}
