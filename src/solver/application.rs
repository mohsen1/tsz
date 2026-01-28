//! Application Type Evaluation
//!
//! This module handles evaluation of generic type applications like `Store<ExtractState<R>>`.
//! The key operation is:
//! 1. Resolve the base type reference (e.g., `Store`) to get its body
//! 2. Get the type parameters from the symbol
//! 3. Instantiate the body with the provided type arguments
//! 4. Recursively evaluate any nested applications
//!
//! This module implements the solver-first architecture principle: pure type logic
//! belongs in the solver, while the checker handles AST traversal and symbol resolution.

use crate::solver::subtype::TypeResolver;
use crate::solver::type_queries;
use crate::solver::types::*;
use crate::solver::{TypeDatabase, TypeSubstitution, instantiate_type};
use rustc_hash::FxHashSet;
use std::cell::RefCell;

/// Maximum depth for recursive application evaluation.
/// Prevents stack overflow on deeply recursive generic types.
pub const MAX_APPLICATION_DEPTH: u32 = 50;

/// Result of application type evaluation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplicationResult {
    /// Successfully evaluated to a concrete type
    Resolved(TypeId),
    /// The type is not an application type (pass through)
    NotApplication(TypeId),
    /// Recursion limit reached
    DepthExceeded(TypeId),
    /// Symbol resolution failed
    ResolutionFailed(TypeId),
}

/// Evaluator for generic type applications.
///
/// This evaluator takes a type application like `Box<string>` and:
/// 1. Looks up the definition of `Box` (via the resolver)
/// 2. Gets its type parameters
/// 3. Substitutes the type arguments
/// 4. Returns the resulting type
///
/// # Type Resolver
///
/// The evaluator uses a `TypeResolver` to handle symbol resolution.
/// This abstraction allows the solver to remain independent of the binder/checker:
/// - `resolve_ref(symbol)` - get the body type of a type alias/interface
/// - `get_type_params(symbol)` - get the type parameters for a symbol
pub struct ApplicationEvaluator<'a, R: TypeResolver> {
    interner: &'a dyn TypeDatabase,
    resolver: &'a R,
    /// Recursion depth counter
    depth: RefCell<u32>,
    /// Set of types currently being evaluated (cycle detection)
    visiting: RefCell<FxHashSet<TypeId>>,
    /// Cache for evaluated applications
    cache: RefCell<rustc_hash::FxHashMap<TypeId, TypeId>>,
}

impl<'a, R: TypeResolver> ApplicationEvaluator<'a, R> {
    /// Create a new application evaluator.
    pub fn new(interner: &'a dyn TypeDatabase, resolver: &'a R) -> Self {
        Self {
            interner,
            resolver,
            depth: RefCell::new(0),
            visiting: RefCell::new(FxHashSet::default()),
            cache: RefCell::new(rustc_hash::FxHashMap::default()),
        }
    }

    /// Clear the evaluation cache.
    /// Call this when contextual type changes to ensure fresh evaluation.
    pub fn clear_cache(&self) {
        self.cache.borrow_mut().clear();
    }

    /// Evaluate an Application type by resolving the base symbol and instantiating.
    ///
    /// This handles types like `Store<ExtractState<R>>` by:
    /// 1. Resolving the base type reference to get its body
    /// 2. Getting the type parameters
    /// 3. Instantiating the body with the provided type arguments
    /// 4. Recursively evaluating the result
    ///
    /// # Returns
    /// - `ApplicationResult::Resolved(type_id)` - successfully evaluated
    /// - `ApplicationResult::NotApplication(type_id)` - input was not an application type
    /// - `ApplicationResult::DepthExceeded(type_id)` - recursion limit reached
    /// - `ApplicationResult::ResolutionFailed(type_id)` - symbol resolution failed
    pub fn evaluate(&self, type_id: TypeId) -> ApplicationResult {
        // Check if it's a generic application type
        if !type_queries::is_generic_type(self.interner, type_id) {
            return ApplicationResult::NotApplication(type_id);
        }

        // Check cache
        if let Some(&cached) = self.cache.borrow().get(&type_id) {
            return ApplicationResult::Resolved(cached);
        }

        // Cycle detection
        if !self.visiting.borrow_mut().insert(type_id) {
            return ApplicationResult::Resolved(type_id);
        }

        // Depth check
        if *self.depth.borrow() >= MAX_APPLICATION_DEPTH {
            self.visiting.borrow_mut().remove(&type_id);
            return ApplicationResult::DepthExceeded(type_id);
        }

        *self.depth.borrow_mut() += 1;
        let result = self.evaluate_inner(type_id);
        *self.depth.borrow_mut() -= 1;

        self.visiting.borrow_mut().remove(&type_id);

        if let ApplicationResult::Resolved(result_type) = result {
            self.cache.borrow_mut().insert(type_id, result_type);
        }

        result
    }

    /// Inner evaluation logic without recursion guards.
    fn evaluate_inner(&self, type_id: TypeId) -> ApplicationResult {
        // Get application info (base type and type arguments)
        let Some((base, args)) = type_queries::get_application_info(self.interner, type_id) else {
            return ApplicationResult::NotApplication(type_id);
        };

        // Check if the base is a Ref (symbol reference)
        let Some(sym_ref) = type_queries::get_symbol_ref(self.interner, base) else {
            return ApplicationResult::NotApplication(type_id);
        };

        // Resolve the symbol to get its body type
        let Some(body_type) = self.resolver.resolve_ref(sym_ref, self.interner) else {
            return ApplicationResult::ResolutionFailed(type_id);
        };

        if body_type == TypeId::ANY || body_type == TypeId::ERROR {
            return ApplicationResult::Resolved(type_id);
        }

        // Get type parameters for this symbol
        let type_params = self.resolver.get_type_params(sym_ref).unwrap_or_default();

        if type_params.is_empty() {
            return ApplicationResult::Resolved(body_type);
        }

        // Evaluate type arguments recursively
        let evaluated_args: Vec<TypeId> = args
            .iter()
            .map(|&arg| match self.evaluate(arg) {
                ApplicationResult::Resolved(t) => t,
                _ => arg,
            })
            .collect();

        // Create substitution and instantiate
        let substitution =
            TypeSubstitution::from_args(self.interner, &type_params, &evaluated_args);
        let instantiated = instantiate_type(self.interner, body_type, &substitution);

        // Recursively evaluate for nested applications
        match self.evaluate(instantiated) {
            ApplicationResult::Resolved(result) => ApplicationResult::Resolved(result),
            _ => ApplicationResult::Resolved(instantiated),
        }
    }

    /// Evaluate a type and return the result, falling back to the original type.
    ///
    /// This is a convenience method that unwraps the ApplicationResult.
    pub fn evaluate_or_original(&self, type_id: TypeId) -> TypeId {
        match self.evaluate(type_id) {
            ApplicationResult::Resolved(t) => t,
            ApplicationResult::NotApplication(t) => t,
            ApplicationResult::DepthExceeded(t) => t,
            ApplicationResult::ResolutionFailed(t) => t,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::TypeInterner;
    use crate::solver::subtype::NoopResolver;

    #[test]
    fn test_non_application_passthrough() {
        let interner = TypeInterner::new();
        let string_type = interner.intern(TypeKey::Intrinsic(IntrinsicKind::String));

        let evaluator = ApplicationEvaluator::new(&interner, &NoopResolver);
        let result = evaluator.evaluate(string_type);

        assert!(matches!(result, ApplicationResult::NotApplication(_)));
    }

    #[test]
    fn test_primitives_are_not_applications() {
        let interner = TypeInterner::new();
        let evaluator = ApplicationEvaluator::new(&interner, &NoopResolver);

        // Primitives should pass through as NotApplication
        assert!(matches!(
            evaluator.evaluate(TypeId::ANY),
            ApplicationResult::NotApplication(_)
        ));
        assert!(matches!(
            evaluator.evaluate(TypeId::NEVER),
            ApplicationResult::NotApplication(_)
        ));
        assert!(matches!(
            evaluator.evaluate(TypeId::STRING),
            ApplicationResult::NotApplication(_)
        ));
    }
}
