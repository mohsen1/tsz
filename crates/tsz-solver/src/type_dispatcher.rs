//! Type Dispatcher - Systematic Type Operation Dispatch Pattern
//!
//! This module provides a dispatcher pattern for handling different type
//! categories with specialized logic, demonstrating how to avoid repetitive
//! type matching and implement clean type-specific behavior.
//!
//! # Problem This Solves
//!
//! Many type operations require different behavior depending on the type category:
//!
//! ```ignore
//! // BEFORE: Repetitive pattern matching
//! match db.lookup(type_id) {
//!     Some(TypeKey::Object(shape_id)) => { /* object logic */ }
//!     Some(TypeKey::Union(list_id)) => { /* union logic */ }
//!     Some(TypeKey::Callable(shape_id)) => { /* callable logic */ }
//!     Some(TypeKey::Array(elem_type)) => { /* array logic */ }
//!     _ => { /* default */ }
//! }
//! ```
//!
//! # Solution: Dispatcher Pattern
//!
//! ```ignore
//! // AFTER: Type-safe dispatch
//! TypeDispatcher::new(db, type_id)
//!     .on_object(|shape_id| { /* handle object */ })
//!     .on_union(|list_id| { /* handle union */ })
//!     .on_callable(|shape_id| { /* handle callable */ })
//!     .dispatch()
//! ```

use crate::type_classifier::{TypeClassification, classify_type};
use crate::{TypeDatabase, TypeId};

/// Handler for object types
pub type ObjectHandler = fn(crate::ObjectShapeId) -> DispatchResult;

/// Handler for union types
pub type UnionHandler = fn(crate::TypeListId) -> DispatchResult;

/// Handler for intersection types
pub type IntersectionHandler = fn(crate::TypeListId) -> DispatchResult;

/// Handler for callable types
pub type CallableHandler = fn(crate::CallableShapeId) -> DispatchResult;

/// Handler for function types
pub type FunctionHandler = fn(crate::FunctionShapeId) -> DispatchResult;

/// Handler for array types
pub type ArrayHandler = fn(TypeId) -> DispatchResult;

/// Handler for tuple types
pub type TupleHandler = fn(crate::TupleListId) -> DispatchResult;

/// Handler for literal types
pub type LiteralHandler = fn(&crate::LiteralValue) -> DispatchResult;

/// Handler for intrinsic types
pub type IntrinsicHandler = fn(crate::IntrinsicKind) -> DispatchResult;

/// Handler for lazy types
pub type LazyHandler = fn(crate::def::DefId) -> DispatchResult;

/// Handler for unhandled types
pub type DefaultHandler = fn() -> DispatchResult;

/// Result of a dispatch operation
#[derive(Debug, Clone)]
pub enum DispatchResult {
    /// Operation succeeded with no result
    Ok,

    /// Operation succeeded with a TypeId result
    OkType(TypeId),

    /// Operation failed
    Error(String),

    /// Skip to default handler
    Skip,
}

/// Type dispatcher for systematic type operation handling.
///
/// This builder pattern implementation allows clean, type-safe handling of
/// different type categories without repetitive pattern matching.
///
/// # Example
///
/// ```ignore
/// let result = TypeDispatcher::new(&db, type_id)
///     .on_object(|shape_id| {
///         // Handle object type
///         DispatchResult::Ok
///     })
///     .on_union(|list_id| {
///         // Handle union type
///         DispatchResult::Ok
///     })
///     .on_default(|| {
///         // Fallback for unhandled types
///         DispatchResult::Ok
///     })
///     .dispatch();
/// ```
pub struct TypeDispatcher<'db> {
    #[allow(dead_code)]
    db: &'db dyn TypeDatabase,
    #[allow(dead_code)]
    type_id: TypeId,
    classification: Option<TypeClassification>,

    object_handler: Option<ObjectHandler>,
    union_handler: Option<UnionHandler>,
    intersection_handler: Option<IntersectionHandler>,
    callable_handler: Option<CallableHandler>,
    function_handler: Option<FunctionHandler>,
    array_handler: Option<ArrayHandler>,
    tuple_handler: Option<TupleHandler>,
    literal_handler: Option<LiteralHandler>,
    intrinsic_handler: Option<IntrinsicHandler>,
    lazy_handler: Option<LazyHandler>,
    default_handler: Option<DefaultHandler>,
}

impl<'db> TypeDispatcher<'db> {
    /// Create a new dispatcher for a type.
    pub fn new(db: &'db dyn TypeDatabase, type_id: TypeId) -> Self {
        let classification = Some(classify_type(db, type_id));

        Self {
            db,
            type_id,
            classification,

            object_handler: None,
            union_handler: None,
            intersection_handler: None,
            callable_handler: None,
            function_handler: None,
            array_handler: None,
            tuple_handler: None,
            literal_handler: None,
            intrinsic_handler: None,
            lazy_handler: None,
            default_handler: None,
        }
    }

    /// Register a handler for object types
    pub fn on_object(mut self, handler: ObjectHandler) -> Self {
        self.object_handler = Some(handler);
        self
    }

    /// Register a handler for union types
    pub fn on_union(mut self, handler: UnionHandler) -> Self {
        self.union_handler = Some(handler);
        self
    }

    /// Register a handler for intersection types
    pub fn on_intersection(mut self, handler: IntersectionHandler) -> Self {
        self.intersection_handler = Some(handler);
        self
    }

    /// Register a handler for callable types
    pub fn on_callable(mut self, handler: CallableHandler) -> Self {
        self.callable_handler = Some(handler);
        self
    }

    /// Register a handler for function types
    pub fn on_function(mut self, handler: FunctionHandler) -> Self {
        self.function_handler = Some(handler);
        self
    }

    /// Register a handler for array types
    pub fn on_array(mut self, handler: ArrayHandler) -> Self {
        self.array_handler = Some(handler);
        self
    }

    /// Register a handler for tuple types
    pub fn on_tuple(mut self, handler: TupleHandler) -> Self {
        self.tuple_handler = Some(handler);
        self
    }

    /// Register a handler for literal types
    pub fn on_literal(mut self, handler: LiteralHandler) -> Self {
        self.literal_handler = Some(handler);
        self
    }

    /// Register a handler for intrinsic types
    pub fn on_intrinsic(mut self, handler: IntrinsicHandler) -> Self {
        self.intrinsic_handler = Some(handler);
        self
    }

    /// Register a handler for lazy types
    pub fn on_lazy(mut self, handler: LazyHandler) -> Self {
        self.lazy_handler = Some(handler);
        self
    }

    /// Register a default handler for unhandled types
    pub fn on_default(mut self, handler: DefaultHandler) -> Self {
        self.default_handler = Some(handler);
        self
    }

    /// Execute the dispatch based on registered handlers.
    ///
    /// Calls the appropriate handler based on the type's classification,
    /// or the default handler if no specific handler is registered.
    pub fn dispatch(self) -> DispatchResult {
        match self.classification {
            None => self.call_default(),

            Some(TypeClassification::Object(shape_id)) => {
                if let Some(handler) = self.object_handler {
                    handler(shape_id)
                } else {
                    self.call_default()
                }
            }

            Some(TypeClassification::ObjectWithIndex(shape_id)) => {
                if let Some(handler) = self.object_handler {
                    handler(shape_id)
                } else {
                    self.call_default()
                }
            }

            Some(TypeClassification::Union(list_id)) => {
                if let Some(handler) = self.union_handler {
                    handler(list_id)
                } else {
                    self.call_default()
                }
            }

            Some(TypeClassification::Intersection(list_id)) => {
                if let Some(handler) = self.intersection_handler {
                    handler(list_id)
                } else {
                    self.call_default()
                }
            }

            Some(TypeClassification::Callable(shape_id)) => {
                if let Some(handler) = self.callable_handler {
                    handler(shape_id)
                } else {
                    self.call_default()
                }
            }

            Some(TypeClassification::Function(shape_id)) => {
                if let Some(handler) = self.function_handler {
                    handler(shape_id)
                } else {
                    self.call_default()
                }
            }

            Some(TypeClassification::Array(elem_type)) => {
                if let Some(handler) = self.array_handler {
                    handler(elem_type)
                } else {
                    self.call_default()
                }
            }

            Some(TypeClassification::Tuple(list_id)) => {
                if let Some(handler) = self.tuple_handler {
                    handler(list_id)
                } else {
                    self.call_default()
                }
            }

            Some(TypeClassification::Literal(ref value)) => {
                if let Some(handler) = self.literal_handler {
                    handler(value)
                } else {
                    self.call_default()
                }
            }

            Some(TypeClassification::Intrinsic(kind)) => {
                if let Some(handler) = self.intrinsic_handler {
                    handler(kind)
                } else {
                    self.call_default()
                }
            }

            Some(TypeClassification::Lazy(def_id)) => {
                if let Some(handler) = self.lazy_handler {
                    handler(def_id)
                } else {
                    self.call_default()
                }
            }

            _ => self.call_default(),
        }
    }

    fn call_default(&self) -> DispatchResult {
        if let Some(handler) = self.default_handler {
            handler()
        } else {
            DispatchResult::Ok
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dispatch_result_variants() {
        let ok = DispatchResult::Ok;
        let error = DispatchResult::Error("test".to_string());

        assert!(matches!(ok, DispatchResult::Ok));
        assert!(matches!(error, DispatchResult::Error(_)));
    }
}
