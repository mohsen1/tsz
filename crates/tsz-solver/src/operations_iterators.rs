//! Iterator and async iterator type extraction.
//!
//! Extracts iterator protocol information from types, handling both sync
//! and async iterators. Used by the checker for `for..of` and `for await..of`
//! loop type checking.

use crate::TypeDatabase;
use crate::operations_property::PropertyAccessEvaluator;
use crate::types::{PropertyInfo, TypeData, TypeId};

/// Information about an iterator type extracted from a type.
///
/// This struct captures the key types needed for iterator/generator type checking:
/// - The iterator object type itself
/// - The type yielded by next().value (T in Iterator<T>)
/// - The type returned when done (`TReturn` in `IteratorResult`<T, `TReturn`>)
/// - The type accepted by `next()` (`TNext` in Iterator<T, `TReturn`, `TNext`>)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IteratorInfo {
    /// The iterator object type (has `next()` method)
    pub iterator_type: TypeId,
    /// The type yielded by the iterator (from `IteratorResult`<T, `TReturn`>)
    pub yield_type: TypeId,
    /// The return type when iteration completes
    pub return_type: TypeId,
    /// The type accepted by next(val) (contravariant)
    pub next_type: TypeId,
}

/// Extract iterator information from a type.
///
/// This function handles both sync and async iterators by finding the
/// appropriate symbol property and extracting the relevant types.
///
/// # Arguments
///
/// * `db` - The type database/interner
/// * `type_id` - The type to extract iterator info from
/// * `is_async` - If true, look for [Symbol.asyncIterator], otherwise [Symbol.iterator]
///
/// # Returns
///
/// * `Some(IteratorInfo)` - If the type is iterable
/// * `None` - If the type is not iterable or doesn't have a valid `next()` method
pub fn get_iterator_info(
    db: &dyn crate::db::QueryDatabase,
    type_id: TypeId,
    is_async: bool,
) -> Option<IteratorInfo> {
    use crate::type_queries::is_callable_type;

    // Fast path: Handle intrinsics that are always iterable
    // The 'any' black hole: any iterates to any
    if type_id == TypeId::ANY {
        return Some(IteratorInfo {
            iterator_type: TypeId::ANY,
            yield_type: TypeId::ANY,
            return_type: TypeId::ANY,
            next_type: TypeId::ANY,
        });
    }

    // Fast path: Handle Array and Tuple types
    if let Some(key) = db.lookup(type_id) {
        match key {
            TypeData::Array(elem_type) => {
                return get_array_iterator_info(type_id, elem_type);
            }
            TypeData::Tuple(_) => {
                return get_tuple_iterator_info(db, type_id);
            }
            _ => {}
        }
    }

    // Step 1: Find the iterator-producing method
    let symbol_name = if is_async {
        "[Symbol.asyncIterator]"
    } else {
        "[Symbol.iterator]"
    };

    let evaluator = PropertyAccessEvaluator::new(db);
    let iterator_method_type = evaluator
        .resolve_property_access(type_id, symbol_name)
        .success_type()?;

    // Step 2: Get the iterator type by "calling" the method
    // The [Symbol.iterator] property is a method that returns the iterator
    use crate::type_queries::get_return_type;
    let iterator_type = if is_callable_type(db, iterator_method_type) {
        // The symbol is a method - extract its return type
        // For [Symbol.iterator], the return type is Iterator<T>
        get_return_type(db, iterator_method_type).unwrap_or(TypeId::ANY)
    } else {
        // The symbol property IS the iterator type (non-callable)
        iterator_method_type
    };

    // Step 3: Find the next() method on the iterator
    let next_method_type = evaluator
        .resolve_property_access(iterator_type, "next")
        .success_type()?;

    // Step 4: Extract types from the IteratorResult
    extract_iterator_result_types(db, iterator_type, next_method_type, is_async)
}

/// Get iterator info for Array types.
const fn get_array_iterator_info(array_type: TypeId, elem_type: TypeId) -> Option<IteratorInfo> {
    // Arrays yield their element type
    // The iterator type for Array<T> has:
    // - yield: T
    // - return: undefined
    // - next: accepts undefined (TNext = undefined)
    Some(IteratorInfo {
        iterator_type: array_type,
        yield_type: elem_type,
        return_type: TypeId::UNDEFINED,
        next_type: TypeId::UNDEFINED,
    })
}

/// Get iterator info for Tuple types.
fn get_tuple_iterator_info(db: &dyn TypeDatabase, tuple_type: TypeId) -> Option<IteratorInfo> {
    // Tuples yield the union of their element types
    match db.lookup(tuple_type) {
        Some(TypeData::Tuple(list_id)) => {
            let elements = db.tuple_list(list_id);
            let elem_types: Vec<TypeId> = elements.iter().map(|e| e.type_id).collect();

            // Union of all element types (or Never if empty)
            let yield_type = if elem_types.is_empty() {
                TypeId::NEVER
            } else {
                elem_types
                    .into_iter()
                    .reduce(|acc, elem| db.union2(acc, elem))
                    .unwrap_or(TypeId::NEVER)
            };

            Some(IteratorInfo {
                iterator_type: tuple_type,
                yield_type,
                return_type: TypeId::UNDEFINED,
                next_type: TypeId::UNDEFINED,
            })
        }
        _ => None,
    }
}

/// Extract T from a Promise<T> type.
///
/// Handles two representations:
/// 1. `Application(base=PROMISE_BASE`, args=[T]) — synthetic promise
/// 2. Object types with a `then` callback — structurally promise-like
///
/// Returns the inner type T, or None if not a promise type.
fn extract_promise_inner_type(
    db: &dyn crate::db::QueryDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    match db.lookup(type_id) {
        // Application: Promise<T> where base is PROMISE_BASE
        Some(TypeData::Application(app_id)) => {
            let app = db.type_application(app_id);
            if app.base == TypeId::PROMISE_BASE {
                return app.args.first().copied();
            }
            // For other applications, the first arg is typically T
            // e.g. PromiseLike<T>, custom Promise subclasses
            app.args.first().copied()
        }
        // Object type: look for then(onfulfilled: (value: T) => any) => any
        Some(TypeData::Object(shape_id)) => {
            let shape = db.object_shape(shape_id);
            let then_atom = db.intern_string("then");
            let then_prop = PropertyInfo::find_in_slice(&shape.properties, then_atom)?;
            // then is a function: (onfulfilled: (value: T) => any) => any
            // Extract T from the first parameter of the first parameter
            match db.lookup(then_prop.type_id) {
                Some(TypeData::Function(fn_id)) => {
                    let fn_shape = db.function_shape(fn_id);
                    let onfulfilled = fn_shape.params.first()?;
                    // onfulfilled: (value: T) => any — extract T from its first param
                    match db.lookup(onfulfilled.type_id) {
                        Some(TypeData::Function(inner_fn_id)) => {
                            let inner_shape = db.function_shape(inner_fn_id);
                            inner_shape.params.first().map(|p| p.type_id)
                        }
                        _ => None,
                    }
                }
                Some(TypeData::Callable(callable_id)) => {
                    let callable = db.callable_shape(callable_id);
                    let sig = callable.call_signatures.first()?;
                    let onfulfilled = sig.params.first()?;
                    match db.lookup(onfulfilled.type_id) {
                        Some(TypeData::Function(inner_fn_id)) => {
                            let inner_shape = db.function_shape(inner_fn_id);
                            inner_shape.params.first().map(|p| p.type_id)
                        }
                        _ => None,
                    }
                }
                // If then is itself a type (e.g. structural shorthand), just return it
                _ => Some(then_prop.type_id),
            }
        }
        _ => None,
    }
}

/// Extract yield/return/next types from the `next()` method's return type.
///
/// For sync iterators: `next()` returns `IteratorResult`<T, `TReturn`>
/// For async iterators: `next()` returns Promise<`IteratorResult`<T, `TReturn`>>
fn extract_iterator_result_types(
    db: &dyn crate::db::QueryDatabase,
    iterator_type: TypeId,
    next_method_type: TypeId,
    is_async: bool,
) -> Option<IteratorInfo> {
    use crate::type_queries::is_promise_like;

    // Get the return type and parameter types of next()
    let (next_return_type, next_params) = match db.lookup(next_method_type) {
        Some(TypeData::Function(shape_id)) => {
            let shape = db.function_shape(shape_id);
            (shape.return_type, shape.params.clone())
        }
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            let sig = shape.call_signatures.first()?;
            (sig.return_type, sig.params.clone())
        }
        _ => return None,
    };

    // For async iterators, unwrap the Promise wrapper
    let iterator_result_type = if is_async {
        if is_promise_like(db, next_return_type) {
            extract_promise_inner_type(db, next_return_type).unwrap_or(next_return_type)
        } else {
            return None;
        }
    } else {
        next_return_type
    };

    // Extract yield_type and return_type from IteratorResult<T, TReturn>
    // IteratorResult = { value: T, done: false } | { value: TReturn, done: true }
    let (yield_type, return_type) = extract_iterator_result_value_types(db, iterator_result_type);

    // Extract next_type from the first parameter of next()
    let next_type = next_params.first().map_or(TypeId::UNDEFINED, |p| p.type_id);

    Some(IteratorInfo {
        iterator_type,
        yield_type,
        return_type,
        next_type,
    })
}

/// Extract yield and return types from an `IteratorResult` type.
///
/// `IteratorResult`<T, `TReturn`> is typically:
///   { value: T, done: false } | { value: `TReturn`, done: true }
///
/// Returns (`yield_type`, `return_type`). Yield comes from done:false branches,
/// return comes from done:true branches.
fn extract_iterator_result_value_types(
    db: &dyn crate::db::QueryDatabase,
    iterator_result_type: TypeId,
) -> (TypeId, TypeId) {
    let done_atom = db.intern_string("done");
    let value_atom = db.intern_string("value");

    match db.lookup(iterator_result_type) {
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            let mut yield_types = Vec::new();
            let mut return_types = Vec::new();

            for &member_id in members.iter() {
                if let Some(TypeData::Object(shape_id)) = db.lookup(member_id) {
                    let shape = db.object_shape(shape_id);
                    let value_type = shape
                        .properties
                        .iter()
                        .find(|p| p.name == value_atom)
                        .map(|p| p.type_id);
                    let done_type = shape
                        .properties
                        .iter()
                        .find(|p| p.name == done_atom)
                        .map(|p| p.type_id);

                    match done_type {
                        // done: true branch → return_type
                        Some(t) if t == TypeId::BOOLEAN_TRUE => {
                            if let Some(v) = value_type {
                                return_types.push(v);
                            }
                        }
                        // done: false branch → yield_type
                        Some(t) if t == TypeId::BOOLEAN_FALSE => {
                            if let Some(v) = value_type {
                                yield_types.push(v);
                            }
                        }
                        // No done property or unknown done → treat as yield
                        _ => {
                            if let Some(v) = value_type {
                                yield_types.push(v);
                            }
                        }
                    }
                }
            }

            let yield_type = if yield_types.is_empty() {
                TypeId::ANY
            } else {
                yield_types
                    .into_iter()
                    .reduce(|acc, t| db.union2(acc, t))
                    .unwrap_or(TypeId::ANY)
            };

            let return_type = if return_types.is_empty() {
                TypeId::ANY
            } else {
                return_types
                    .into_iter()
                    .reduce(|acc, t| db.union2(acc, t))
                    .unwrap_or(TypeId::ANY)
            };

            (yield_type, return_type)
        }
        Some(TypeData::Object(shape_id)) => {
            let shape = db.object_shape(shape_id);
            let value_type = shape
                .properties
                .iter()
                .find(|p| p.name == value_atom)
                .map_or(TypeId::ANY, |p| p.type_id);
            (value_type, TypeId::ANY)
        }
        _ => (TypeId::ANY, TypeId::ANY),
    }
}

/// Get the element type yielded by an async iterable type.
///
/// This is a convenience wrapper around `get_iterator_info` that extracts
/// just the yield type from async iterators.
pub fn get_async_iterable_element_type(
    db: &dyn crate::db::QueryDatabase,
    type_id: TypeId,
) -> TypeId {
    match get_iterator_info(db, type_id, true) {
        Some(info) => info.yield_type,
        None => match get_iterator_info(db, type_id, false) {
            Some(info) => info.yield_type,
            None => TypeId::ANY,
        },
    }
}
