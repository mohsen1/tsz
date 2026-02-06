//! Canonicalization for structural type identity (Task #32: Graph Isomorphism)
//!
//! This module implements type canonicalization to achieve O(1) structural equality.
//! It transforms cyclic type definitions into trees using De Bruijn indices:
//!
//! - **Recursive(n)**: Self-reference N levels up the nesting path
//! - **BoundParameter(n)**: Type parameter using positional index for alpha-equivalence
//!
//! ## Key Concepts
//!
//! ### Structural vs Nominal Types
//!
//! - **TypeAlias**: Structural - `type A = { x: A }` and `type B = { x: B }`
//!   should canonicalize to the same type with `Recursive(0)`
//! - **Interface/Class/Enum**: Nominal - Must remain as `Lazy(DefId)` for nominal identity
//!
//! ### De Bruijn Indices
//!
//! - `Recursive(0)`: Immediate self-reference
//! - `Recursive(1)`: One level up (parent in nesting chain)
//! - `BoundParameter(0)`: Innermost type parameter
//! - `BoundParameter(n)`: (n+1)th-most-recently-bound type parameter
//!
//! ## Usage
//!
//! Canonicalization is for **comparison and hashing only**, not for display.
//! Use `canonicalize()` to check if two types are structurally identical:
//!
//! ```rust
//! let canon_a = canonicalizer.canonicalize(type_a);
//! let canon_b = canonicalizer.canonicalize(type_b);
//! assert_eq!(canon_a, canon_b); // Same structure = same TypeId
//! ```

use crate::interner::Atom;
use crate::solver::TypeDatabase;
use crate::solver::def::DefId;
use crate::solver::def::DefKind;
use crate::solver::subtype::TypeResolver;
use crate::solver::types::{
    IndexSignature, ObjectShapeId, TemplateSpan, TupleElement, TypeId, TypeKey,
};
use rustc_hash::FxHashMap;

/// Canonicalizer for structural type identity.
///
/// Transforms type aliases from cyclic graphs to trees using De Bruijn indices.
/// Only processes `DefKind::TypeAlias` (structural types), preserving nominal
/// types (Interface/Class/Enum) as `Lazy(DefId)`.
pub struct Canonicalizer<'a, R: TypeResolver> {
    /// Type interner for creating new TypeIds
    interner: &'a dyn TypeDatabase,
    /// Type resolver for looking up definitions
    resolver: &'a R,
    /// Stack of DefIds currently being expanded (for Recursive(n))
    def_stack: Vec<DefId>,
    /// Stack of type parameter scopes (for BoundParameter(n))
    /// Each scope is a list of parameter names in order
    param_stack: Vec<Vec<Atom>>,
    /// Cache to avoid re-canonicalizing the same type
    cache: FxHashMap<TypeId, TypeId>,
}

impl<'a, R: TypeResolver> Canonicalizer<'a, R> {
    /// Create a new Canonicalizer.
    pub fn new(interner: &'a dyn TypeDatabase, resolver: &'a R) -> Self {
        Canonicalizer {
            interner,
            resolver,
            def_stack: Vec::new(),
            param_stack: Vec::new(),
            cache: FxHashMap::default(),
        }
    }

    /// Canonicalize a type to its structural form.
    ///
    /// Returns a TypeId that represents the canonical structural form.
    /// Two types with the same structure will return the same TypeId.
    pub fn canonicalize(&mut self, type_id: TypeId) -> TypeId {
        // 1. Check cache
        if let Some(&cached) = self.cache.get(&type_id) {
            return cached;
        }

        // 2. Look up TypeKey
        let key = match self.interner.lookup(type_id) {
            Some(k) => k,
            None => return type_id, // Error/None - preserve as-is
        };

        let result = match key {
            // Handle Type Alias Expansion (structural only)
            TypeKey::Lazy(def_id) => {
                match self.resolver.get_def_kind(def_id) {
                    Some(DefKind::TypeAlias) => {
                        // Structural type: canonicalize recursively
                        self.canonicalize_type_alias(def_id)
                    }
                    _ => {
                        // Nominal type (Interface/Class/Enum): preserve identity
                        // But canonicalize generic arguments if it's an Application
                        // For now, just return the Lazy as-is (nominal types keep their identity)
                        type_id
                    }
                }
            }

            // Handle Type Parameters -> De Bruijn indices
            TypeKey::TypeParameter(info) => {
                if let Some(index) = self.find_param_index(info.name) {
                    self.interner.intern(TypeKey::BoundParameter(index))
                } else {
                    // Free variable (shouldn't happen in valid code)
                    type_id
                }
            }

            // Handle Recursive references (pass through - already canonical)
            TypeKey::Recursive(_) => type_id,

            // Handle BoundParameter references (pass through - already canonical)
            TypeKey::BoundParameter(_) => type_id,

            // Recurse into composite types
            TypeKey::Array(elem) => {
                let c_elem = self.canonicalize(elem);
                self.interner.array(c_elem)
            }

            TypeKey::Tuple(list_id) => {
                let elements = self.interner.tuple_list(list_id);
                let c_elements: Vec<TupleElement> = elements
                    .iter()
                    .map(|e| TupleElement {
                        type_id: self.canonicalize(e.type_id),
                        name: e.name,
                        optional: e.optional,
                        rest: e.rest,
                    })
                    .collect();
                self.interner.tuple(c_elements)
            }

            TypeKey::Union(members_id) => {
                let members = self.interner.type_list(members_id);
                let c_members: Vec<TypeId> =
                    members.iter().map(|&m| self.canonicalize(m)).collect();
                // Sort and deduplicate (union is commutative)
                // Sort by raw u32 value since TypeId doesn't implement Ord
                let mut sorted = c_members;
                sorted.sort_by_key(|t| t.0);
                sorted.dedup();
                self.interner.union(sorted)
            }

            TypeKey::Intersection(members_id) => {
                let members = self.interner.type_list(members_id);
                // 1. Canonicalize all members
                let c_members: Vec<TypeId> =
                    members.iter().map(|&m| self.canonicalize(m)).collect();

                // 2. Separate callables (preserve order) from structural types (sort)
                let mut structural = Vec::new();
                let mut callables = Vec::new();
                for m in c_members {
                    if self.is_callable_type(m) {
                        callables.push(m);
                    } else {
                        structural.push(m);
                    }
                }

                // 3. Sort structural members by canonical TypeId (commutative)
                structural.sort_by_key(|t| t.0);
                structural.dedup();

                // 4. Combine: structural first (sorted), then callables (preserved order)
                let mut final_members = structural;
                final_members.extend(callables);
                self.interner.intersection(final_members)
            }

            // Generic type application (e.g., Box<string>)
            TypeKey::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                // Canonicalize base type
                let c_base = self.canonicalize(app.base);
                // Canonicalize all generic arguments
                let c_args: Vec<TypeId> =
                    app.args.iter().map(|&arg| self.canonicalize(arg)).collect();
                self.interner.application(c_base, c_args)
            }

            TypeKey::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);

                // Enter new scope if this function has type parameters (alpha-equivalence)
                let pushed_scope = if !shape.type_params.is_empty() {
                    let param_names: Vec<Atom> = shape.type_params.iter().map(|p| p.name).collect();
                    self.param_stack.push(param_names);
                    true
                } else {
                    false
                };

                // Canonicalize this_type if present
                let c_this_type = shape.this_type.map(|t| self.canonicalize(t));
                // Canonicalize return type
                let c_return_type = self.canonicalize(shape.return_type);
                // Canonicalize parameter types
                let c_params: Vec<crate::solver::types::ParamInfo> = shape
                    .params
                    .iter()
                    .map(|p| crate::solver::types::ParamInfo {
                        name: p.name,
                        type_id: self.canonicalize(p.type_id),
                        optional: p.optional,
                        rest: p.rest,
                    })
                    .collect();

                // Canonicalize type parameter constraints and defaults
                let c_type_params: Vec<crate::solver::types::TypeParamInfo> = shape
                    .type_params
                    .iter()
                    .map(|tp| crate::solver::types::TypeParamInfo {
                        name: tp.name,
                        constraint: tp.constraint.map(|c| self.canonicalize(c)),
                        default: tp.default.map(|d| self.canonicalize(d)),
                        is_const: tp.is_const,
                    })
                    .collect();

                // Canonicalize type predicate (if it has a type_id)
                let c_type_predicate =
                    shape
                        .type_predicate
                        .as_ref()
                        .map(|pred| crate::solver::types::TypePredicate {
                            asserts: pred.asserts,
                            target: pred.target.clone(),
                            type_id: pred.type_id.map(|t| self.canonicalize(t)),
                        });

                // Pop scope
                if pushed_scope {
                    self.param_stack.pop();
                }

                let new_shape = crate::solver::types::FunctionShape {
                    type_params: c_type_params,
                    params: c_params,
                    this_type: c_this_type,
                    return_type: c_return_type,
                    type_predicate: c_type_predicate,
                    is_constructor: shape.is_constructor,
                    is_method: shape.is_method,
                };

                self.interner.function(new_shape)
            }

            TypeKey::Callable(shape_id) => self.canonicalize_callable(shape_id),

            // Task #39: Mapped type canonicalization for alpha-equivalence
            // When comparing mapped types over type parameters (deferred), we need
            // to canonicalize the constraint, template, and name_type to achieve
            // structural identity. The type_param name is handled via param_stack.
            TypeKey::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(mapped_id);

                // 1. Canonicalize the constraint FIRST (Outside scope)
                // The iteration variable K is NOT visible in its own constraint
                let c_constraint = self.canonicalize(mapped.constraint);

                // 2. Enter new scope for the iteration variable (alpha-equivalence)
                self.param_stack.push(vec![mapped.type_param.name]);

                // 3. Canonicalize the template type (Inside scope - K is visible here)
                let c_template = self.canonicalize(mapped.template);

                // 4. Canonicalize name_type if present (Inside scope - as clause sees K)
                let c_name_type = mapped.name_type.map(|t| self.canonicalize(t));

                // 5. Pop scope
                self.param_stack.pop();

                // 6. Normalize the TypeParamInfo name for alpha-equivalence
                // We must erase the original name ("K", "P", etc.) so that
                // { [K in T]: K } and { [P in T]: P } hash to the same value.
                // Since we use De Bruijn indices (BoundParameter) in the body,
                // this name is never looked up, only used for hashing identity.
                let mut c_type_param = mapped.type_param.clone();
                c_type_param.name = self.interner.intern_string("");
                // Also canonicalize constraint/default inside TypeParamInfo if present
                c_type_param.constraint = c_type_param.constraint.map(|c| self.canonicalize(c));
                c_type_param.default = c_type_param.default.map(|d| self.canonicalize(d));

                let c_mapped = crate::solver::types::MappedType {
                    type_param: c_type_param,
                    constraint: c_constraint,
                    template: c_template,
                    name_type: c_name_type,
                    readonly_modifier: mapped.readonly_modifier,
                    optional_modifier: mapped.optional_modifier,
                };

                self.interner.mapped(c_mapped)
            }

            // Primitives and literals are already canonical
            TypeKey::Intrinsic(_) | TypeKey::Literal(_) | TypeKey::Error => type_id,

            // Object types: canonicalize property types while preserving metadata
            TypeKey::Object(shape_id) => self.canonicalize_object(shape_id, false),

            TypeKey::ObjectWithIndex(shape_id) => self.canonicalize_object(shape_id, true),

            // Task #47: Template Literal canonicalization for alpha-equivalence
            // Uppercase<T> and Uppercase<U> should be identical when T and U are identical
            TypeKey::TemplateLiteral(id) => {
                let spans = self.interner.template_list(id);
                let c_spans: Vec<TemplateSpan> = spans
                    .iter()
                    .map(|span| match span {
                        TemplateSpan::Text(atom) => TemplateSpan::Text(*atom),
                        TemplateSpan::Type(t) => TemplateSpan::Type(self.canonicalize(*t)),
                    })
                    .collect();
                self.interner.template_literal(c_spans)
            }

            // Task #47: String Intrinsic canonicalization for alpha-equivalence
            // Uppercase<T>, Lowercase<T>, etc. should canonicalize nested type parameters
            TypeKey::StringIntrinsic { kind, type_arg } => {
                let c_arg = self.canonicalize(type_arg);
                self.interner.intern(TypeKey::StringIntrinsic {
                    kind,
                    type_arg: c_arg,
                })
            }

            // Other types: preserve as-is (will be handled as needed)
            _ => type_id,
        };

        self.cache.insert(type_id, result);
        result
    }

    /// Canonicalize a type alias definition.
    ///
    /// This handles:
    /// - Cycle detection via def_stack
    /// - Generic parameter scope management
    /// - Recursive self-references -> Recursive(n)
    fn canonicalize_type_alias(&mut self, def_id: DefId) -> TypeId {
        // Check for cycles (mutual recursion or self-reference)
        if let Some(depth) = self.get_recursion_depth(def_id) {
            return self.interner.intern(TypeKey::Recursive(depth));
        }

        // Push to stack for cycle detection
        self.def_stack.push(def_id);

        // Enter new scope if generic
        let params = self.resolver.get_lazy_type_params(def_id);
        let pushed_scope = if let Some(ps) = params {
            let param_names: Vec<Atom> = ps.iter().map(|p| p.name).collect();
            self.param_stack.push(param_names);
            true
        } else {
            false
        };

        // Resolve the alias body and canonicalize recursively
        let body = self
            .resolver
            .resolve_lazy(def_id, self.interner)
            .unwrap_or(TypeId::ERROR);
        let canonical_body = self.canonicalize(body);

        // Pop scope and def_stack
        if pushed_scope {
            self.param_stack.pop();
        }
        self.def_stack.pop();

        canonical_body
    }

    /// Get the recursion depth for a DefId if it's in the def_stack.
    ///
    /// Returns Some(depth) if the DefId is being expanded, where:
    /// - 0 = immediate self-reference (current DefId)
    /// - n = n levels up the nesting chain
    fn get_recursion_depth(&self, def_id: DefId) -> Option<u32> {
        self.def_stack
            .iter()
            .rev()
            .position(|&d| d == def_id)
            .map(|pos| pos as u32)
    }

    /// Find the De Bruijn index for a type parameter by name.
    ///
    /// Searches from the top of the stack (innermost scope) downward.
    /// Returns Some(index) if found, where:
    /// - 0 = innermost parameter
    /// - n = (n+1)th-most-recently-bound parameter
    fn find_param_index(&self, name: Atom) -> Option<u32> {
        let mut flattened_index = 0u32;

        // Search from top of stack (innermost scope) to bottom
        for scope in self.param_stack.iter().rev() {
            for (idx, &param_name) in scope.iter().enumerate() {
                if param_name == name {
                    // Calculate flattened index from innermost
                    let innermost_offset = scope.len() - idx - 1;
                    return Some(flattened_index + innermost_offset as u32);
                }
            }
            flattened_index += scope.len() as u32;
        }

        None
    }

    /// Canonicalize an object type by recursively canonicalizing property types.
    ///
    /// Preserves all metadata (names, optional, readonly, visibility, parent_id)
    /// and nominal symbols. Only transforms the TypeIds within properties.
    fn canonicalize_object(&mut self, shape_id: ObjectShapeId, _with_index: bool) -> TypeId {
        let shape = self.interner.object_shape(shape_id);

        // Canonicalize all properties
        let mut new_props = Vec::with_capacity(shape.properties.len());
        for prop in &shape.properties {
            let mut new_prop = prop.clone();
            // Canonicalize read type (getter/lookup)
            new_prop.type_id = self.canonicalize(prop.type_id);
            // Canonicalize write type (setter/assignment)
            new_prop.write_type = self.canonicalize(prop.write_type);
            // Preserve all other metadata as-is
            // - name (Atom): Property names are NOT remapped
            // - optional (bool): Part of type identity
            // - readonly (bool): Part of type identity
            // - is_method (bool): Part of type identity
            // - visibility (Visibility): Part of type identity (nominal subtyping)
            // - parent_id (Option<SymbolId>): Brand for private/protected members
            new_props.push(new_prop);
        }

        // Canonicalize index signatures if present
        let new_string_index = shape.string_index.as_ref().map(|idx| IndexSignature {
            key_type: self.canonicalize(idx.key_type),
            value_type: self.canonicalize(idx.value_type),
            readonly: idx.readonly,
        });

        let new_number_index = shape.number_index.as_ref().map(|idx| IndexSignature {
            key_type: self.canonicalize(idx.key_type),
            value_type: self.canonicalize(idx.value_type),
            readonly: idx.readonly,
        });

        // Preserve the symbol field for nominal types (class instances)
        // This ensures that class A and class B with same properties remain distinct
        let symbol = shape.symbol;

        // Create new object shape with canonicalized types but preserved metadata
        let new_shape = crate::solver::types::ObjectShape {
            flags: shape.flags,
            properties: new_props,
            string_index: new_string_index,
            number_index: new_number_index,
            symbol,
        };

        // Intern using the appropriate method
        // Note: object_with_index takes ObjectShape by value and sorts properties
        self.interner.object_with_index(new_shape)
    }

    /// Check if a type is a callable (Function or Callable).
    fn is_callable_type(&self, type_id: TypeId) -> bool {
        match self.interner.lookup(type_id) {
            Some(TypeKey::Function(_)) | Some(TypeKey::Callable(_)) => true,
            _ => false,
        }
    }

    /// Canonicalize a single call signature with type parameter scope management.
    fn canonicalize_signature(
        &mut self,
        sig: &crate::solver::types::CallSignature,
    ) -> crate::solver::types::CallSignature {
        // Enter new scope if this signature has type parameters (alpha-equivalence)
        let pushed_scope = if !sig.type_params.is_empty() {
            let param_names: Vec<Atom> = sig.type_params.iter().map(|p| p.name).collect();
            self.param_stack.push(param_names);
            true
        } else {
            false
        };

        // Canonicalize this_type if present
        let c_this_type = sig.this_type.map(|t| self.canonicalize(t));

        // Canonicalize return type
        let c_return_type = self.canonicalize(sig.return_type);

        // Canonicalize parameter types
        let c_params: Vec<crate::solver::types::ParamInfo> = sig
            .params
            .iter()
            .map(|p| crate::solver::types::ParamInfo {
                name: p.name,
                type_id: self.canonicalize(p.type_id),
                optional: p.optional,
                rest: p.rest,
            })
            .collect();

        // Canonicalize type parameter constraints and defaults
        let c_type_params: Vec<crate::solver::types::TypeParamInfo> = sig
            .type_params
            .iter()
            .map(|tp| crate::solver::types::TypeParamInfo {
                name: tp.name,
                constraint: tp.constraint.map(|c| self.canonicalize(c)),
                default: tp.default.map(|d| self.canonicalize(d)),
                // Preserve other fields as-is
                is_const: tp.is_const,
            })
            .collect();

        // Canonicalize type predicate (if it has a type_id)
        let c_type_predicate =
            sig.type_predicate
                .as_ref()
                .map(|pred| crate::solver::types::TypePredicate {
                    asserts: pred.asserts,
                    target: pred.target.clone(),
                    type_id: pred.type_id.map(|t| self.canonicalize(t)),
                });

        // Pop scope
        if pushed_scope {
            self.param_stack.pop();
        }

        crate::solver::types::CallSignature {
            type_params: c_type_params,
            params: c_params,
            this_type: c_this_type,
            return_type: c_return_type,
            type_predicate: c_type_predicate,
            is_method: sig.is_method,
        }
    }

    /// Canonicalize a callable type (overloaded functions).
    fn canonicalize_callable(&mut self, shape_id: crate::solver::types::CallableShapeId) -> TypeId {
        let shape = self.interner.callable_shape(shape_id);

        // Canonicalize all call signatures (order matters for overload resolution)
        let c_call_signatures: Vec<crate::solver::types::CallSignature> = shape
            .call_signatures
            .iter()
            .map(|sig| self.canonicalize_signature(sig))
            .collect();

        // Canonicalize all construct signatures
        let c_construct_signatures: Vec<crate::solver::types::CallSignature> = shape
            .construct_signatures
            .iter()
            .map(|sig| self.canonicalize_signature(sig))
            .collect();

        // Canonicalize properties
        let mut new_props = Vec::with_capacity(shape.properties.len());
        for prop in &shape.properties {
            let mut new_prop = prop.clone();
            new_prop.type_id = self.canonicalize(prop.type_id);
            new_prop.write_type = self.canonicalize(prop.write_type);
            new_props.push(new_prop);
        }

        // Canonicalize index signatures
        let new_string_index =
            shape
                .string_index
                .as_ref()
                .map(|idx| crate::solver::types::IndexSignature {
                    key_type: self.canonicalize(idx.key_type),
                    value_type: self.canonicalize(idx.value_type),
                    readonly: idx.readonly,
                });

        let new_number_index =
            shape
                .number_index
                .as_ref()
                .map(|idx| crate::solver::types::IndexSignature {
                    key_type: self.canonicalize(idx.key_type),
                    value_type: self.canonicalize(idx.value_type),
                    readonly: idx.readonly,
                });

        let new_shape = crate::solver::types::CallableShape {
            call_signatures: c_call_signatures,
            construct_signatures: c_construct_signatures,
            properties: new_props,
            string_index: new_string_index,
            number_index: new_number_index,
            symbol: shape.symbol,
        };

        self.interner.callable(new_shape)
    }

    /// Clear the cache (useful for testing or bulk operations).
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::intern::TypeInterner;
    use crate::solver::subtype::TypeEnvironment;

    #[test]
    fn test_canonicalizer_creation() {
        let interner = TypeInterner::new();
        let env = TypeEnvironment::new();
        let _canonicalizer = Canonicalizer::new(&interner, &env);
    }

    #[test]
    fn test_canonicalize_primitive() {
        let interner = TypeInterner::new();
        let env = TypeEnvironment::new();
        let mut canon = Canonicalizer::new(&interner, &env);

        let number = TypeId::NUMBER;
        let canon_number = canon.canonicalize(number);

        // Primitives should canonicalize to themselves
        assert_eq!(canon_number, number);
    }
}
