//! instanceof-based type narrowing methods.
//!
//! Extracted from `mod.rs` to keep individual files under the 2000 LOC threshold.
//! Contains the three core instanceof narrowing entry points:
//! - `narrow_by_instanceof` — dispatches on constructor type shape to extract
//!   the instance type, then filters unions / falls back to exclusion.
//! - `narrow_by_instance_type` — filters unions using instanceof-specific
//!   semantics (type-parameter intersection, primitive exclusion).
//! - `narrow_by_instanceof_false` — false-branch narrowing: keeps primitives,
//!   excludes subtypes of the instance type.

use super::NarrowingContext;
use crate::relations::subtype::SubtypeChecker;
use crate::type_queries::{InstanceTypeKind, classify_for_instance_type};
use crate::types::TypeId;
use crate::utils::{TypeIdExt, intersection_or_single, union_or_single};
use crate::visitor::union_list_id;
use tracing::{Level, span, trace};

impl<'a> NarrowingContext<'a> {
    /// Narrow a type based on an instanceof check.
    ///
    /// Example: `x instanceof MyClass` narrows `A | B` to include only `A` where `A` is an instance of `MyClass`
    pub fn narrow_by_instanceof(
        &self,
        source_type: TypeId,
        constructor_type: TypeId,
        sense: bool,
    ) -> TypeId {
        let _span = span!(
            Level::TRACE,
            "narrow_by_instanceof",
            source_type = source_type.0,
            constructor_type = constructor_type.0,
            sense
        )
        .entered();

        // TODO: Check for static [Symbol.hasInstance] method which overrides standard narrowing
        // TypeScript allows classes to define custom instanceof behavior via:
        //   static [Symbol.hasInstance](value: any): boolean
        // This would require evaluating method calls and type predicates, which is
        // significantly more complex than the standard construct signature approach.

        // CRITICAL: Resolve Lazy types for both source and constructor
        // This ensures type aliases are resolved to their actual types
        let resolved_source = self.resolve_type(source_type);
        let resolved_constructor = self.resolve_type(constructor_type);

        // Extract the instance type from the constructor
        let instance_type = match classify_for_instance_type(self.db, resolved_constructor) {
            InstanceTypeKind::Callable(shape_id) => {
                // For callable types with construct signatures, get the return type of the construct signature
                let shape = self.db.callable_shape(shape_id);
                // Find a construct signature and get its return type (the instance type)
                if let Some(construct_sig) = shape.construct_signatures.first() {
                    construct_sig.return_type
                } else {
                    // No construct signature found, can't narrow
                    trace!("No construct signature found in callable type");
                    return source_type;
                }
            }
            InstanceTypeKind::Function(shape_id) => {
                // For function types, check if it's a constructor
                let shape = self.db.function_shape(shape_id);
                if shape.is_constructor {
                    // The return type is the instance type
                    shape.return_type
                } else {
                    trace!("Function is not a constructor");
                    return source_type;
                }
            }
            InstanceTypeKind::Intersection(members) => {
                // For intersection types, we need to extract instance types from all members
                // For now, create an intersection of the instance types
                let instance_types: Vec<TypeId> = members
                    .iter()
                    .map(|&member| self.narrow_by_instanceof(source_type, member, sense))
                    .collect();

                if sense {
                    intersection_or_single(self.db, instance_types)
                } else {
                    // For negation with intersection, we can't easily exclude
                    // Fall back to returning the source type unchanged
                    source_type
                }
            }
            InstanceTypeKind::Union(members) => {
                // For union types, extract instance types from all members
                let instance_types: Vec<TypeId> = members
                    .iter()
                    .filter_map(|&member| {
                        self.narrow_by_instanceof(source_type, member, sense)
                            .non_never()
                    })
                    .collect();

                if sense {
                    union_or_single(self.db, instance_types)
                } else {
                    // For negation with union, we can't easily exclude
                    // Fall back to returning the source type unchanged
                    source_type
                }
            }
            InstanceTypeKind::Readonly(inner) => {
                // Readonly wrapper - extract from inner type
                return self.narrow_by_instanceof(source_type, inner, sense);
            }
            InstanceTypeKind::TypeParameter { constraint } => {
                // Follow type parameter constraint
                if let Some(constraint) = constraint {
                    return self.narrow_by_instanceof(source_type, constraint, sense);
                }
                trace!("Type parameter has no constraint");
                return source_type;
            }
            InstanceTypeKind::SymbolRef(_) | InstanceTypeKind::NeedsEvaluation => {
                // Complex cases that need further evaluation
                // For now, return the source type unchanged
                trace!("Complex instance type (SymbolRef or NeedsEvaluation), returning unchanged");
                return source_type;
            }
            InstanceTypeKind::NotConstructor => {
                trace!("Constructor type is not a valid constructor");
                return source_type;
            }
        };

        // Now narrow based on the sense (positive or negative)
        if sense {
            // CRITICAL: instanceof DOES narrow any/unknown (unlike equality checks)
            if resolved_source == TypeId::ANY {
                // any narrows to the instance type with instanceof
                trace!("Narrowing any to instance type via instanceof");
                return instance_type;
            }

            if resolved_source == TypeId::UNKNOWN {
                // unknown narrows to the instance type with instanceof
                trace!("Narrowing unknown to instance type via instanceof");
                return instance_type;
            }

            // Handle Union: filter members based on instanceof relationship
            if let Some(members_id) = union_list_id(self.db, resolved_source) {
                let members = self.db.type_list(members_id);
                // PERF: Reuse a single SubtypeChecker across all member checks
                // instead of allocating 4 hash sets per is_subtype_of call.
                let mut checker = SubtypeChecker::new(self.db.as_type_database());
                let mut filtered_members: Vec<TypeId> = Vec::new();
                for &member in &*members {
                    // Check if member is assignable to instance type
                    checker.reset();
                    if checker.is_subtype_of(member, instance_type) {
                        trace!(
                            "Union member {} is assignable to instance type {}, keeping",
                            member.0, instance_type.0
                        );
                        filtered_members.push(member);
                        continue;
                    }

                    // Check if instance type is assignable to member (subclass case)
                    // If we have a Dog and instanceof Animal, Dog is an instance of Animal
                    checker.reset();
                    if checker.is_subtype_of(instance_type, member) {
                        trace!(
                            "Instance type {} is assignable to union member {} (subclass), narrowing to instance type",
                            instance_type.0, member.0
                        );
                        filtered_members.push(instance_type);
                        continue;
                    }

                    // Interface overlap: both are object-like but not assignable
                    // Use intersection to preserve properties from both
                    if self.are_object_like(member) && self.are_object_like(instance_type) {
                        trace!(
                            "Interface overlap between {} and {}, using intersection",
                            member.0, instance_type.0
                        );
                        filtered_members.push(self.db.intersection2(member, instance_type));
                        continue;
                    }

                    trace!("Union member {} excluded by instanceof check", member.0);
                }

                union_or_single(self.db, filtered_members)
            } else {
                // Non-union type: use standard narrowing with intersection fallback
                let narrowed = self.narrow_to_type(resolved_source, instance_type);

                // If that returns NEVER, try intersection approach for interface vs class cases
                // In TypeScript, instanceof on an interface narrows to intersection, not NEVER
                if narrowed == TypeId::NEVER && resolved_source != TypeId::NEVER {
                    // Check for interface overlap before using intersection
                    if self.are_object_like(resolved_source) && self.are_object_like(instance_type)
                    {
                        trace!("Interface vs class detected, using intersection instead of NEVER");
                        self.db.intersection2(resolved_source, instance_type)
                    } else {
                        narrowed
                    }
                } else {
                    narrowed
                }
            }
        } else {
            // Negative: !(x instanceof Constructor) - exclude the instance type
            // For unions, exclude members that are subtypes of the instance type
            if let Some(members_id) = union_list_id(self.db, resolved_source) {
                let members = self.db.type_list(members_id);
                // PERF: Reuse a single SubtypeChecker across all member checks
                let mut checker = SubtypeChecker::new(self.db.as_type_database());
                let mut filtered_members: Vec<TypeId> = Vec::new();
                for &member in &*members {
                    // Exclude members that are definitely subtypes of the instance type
                    checker.reset();
                    if !checker.is_subtype_of(member, instance_type) {
                        filtered_members.push(member);
                    }
                }

                union_or_single(self.db, filtered_members)
            } else {
                // Non-union: use standard exclusion
                self.narrow_excluding_type(resolved_source, instance_type)
            }
        }
    }

    /// Narrow a type by instanceof check using the instance type.
    ///
    /// Unlike `narrow_to_type` which uses structural assignability to filter union members,
    /// this method uses instanceof-specific semantics:
    /// - Type parameters with constraints assignable to the target are kept (intersected)
    /// - When a type parameter absorbs the target, anonymous object types are excluded
    ///   since they cannot be class instances at runtime
    ///
    /// This prevents anonymous object types like `{ x: string }` from surviving instanceof
    /// narrowing when they happen to be structurally compatible with the class type.
    pub fn narrow_by_instance_type(&self, source_type: TypeId, instance_type: TypeId) -> TypeId {
        let resolved_source = self.resolve_type(source_type);

        if resolved_source == TypeId::ERROR && source_type != TypeId::ERROR {
            return source_type;
        }

        let resolved_target = self.resolve_type(instance_type);
        if resolved_target == TypeId::ERROR && instance_type != TypeId::ERROR {
            return source_type;
        }

        if resolved_source == resolved_target {
            return source_type;
        }

        // any/unknown narrow to instance type with instanceof
        if resolved_source == TypeId::ANY || resolved_source == TypeId::UNKNOWN {
            return instance_type;
        }

        // If source is a union, filter members using instanceof semantics
        if let Some(members) = union_list_id(self.db, resolved_source) {
            let members = self.db.type_list(members);
            trace!(
                "instanceof: narrowing union with {} members {:?} to instance type {}",
                members.len(),
                members.iter().map(|m| m.0).collect::<Vec<_>>(),
                instance_type.0
            );

            // First pass: check if any type parameter matches the instance type.
            let mut type_param_results: Vec<(usize, TypeId)> = Vec::new();
            for (i, &member) in members.iter().enumerate() {
                if let Some(narrowed) = self.narrow_type_param(member, instance_type) {
                    type_param_results.push((i, narrowed));
                }
            }

            let matching: Vec<TypeId> = if !type_param_results.is_empty() {
                // Type parameter(s) matched: keep type params and exclude anonymous
                // object types that can't be class instances at runtime.
                let mut result = Vec::new();
                let tp_indices: Vec<usize> = type_param_results.iter().map(|(i, _)| *i).collect();
                for &(_, narrowed) in &type_param_results {
                    result.push(narrowed);
                }
                for (i, &member) in members.iter().enumerate() {
                    if tp_indices.contains(&i) {
                        continue;
                    }
                    if crate::type_queries::is_object_type(self.db, member) {
                        trace!(
                            "instanceof: excluding anonymous object {} (type param absorbs)",
                            member.0
                        );
                        continue;
                    }
                    if crate::relations::subtype::is_subtype_of_with_db(
                        self.db,
                        member,
                        instance_type,
                    ) {
                        result.push(member);
                    } else if crate::relations::subtype::is_subtype_of_with_db(
                        self.db,
                        instance_type,
                        member,
                    ) {
                        result.push(instance_type);
                    }
                }
                result
            } else {
                // No type parameter match: filter by instanceof semantics.
                // Primitives can never pass instanceof; non-primitives are
                // checked for assignability with the instance type.
                members
                    .iter()
                    .filter_map(|&member| {
                        // Primitive types can never pass `instanceof` at runtime.
                        if self.is_js_primitive(member) {
                            return None;
                        }
                        if let Some(narrowed) = self.narrow_type_param(member, instance_type) {
                            return Some(narrowed);
                        }
                        // Member assignable to instance type → keep member
                        if self.is_assignable_to(member, instance_type) {
                            return Some(member);
                        }
                        // Instance type assignable to member → narrow to instance
                        // (e.g., member=Animal, instance=Dog → Dog)
                        if self.is_assignable_to(instance_type, member) {
                            return Some(instance_type);
                        }
                        // Neither direction holds — create intersection per tsc
                        // semantics. This handles cases like Date instanceof Object
                        // where assignability checks may miss the relationship.
                        // The intersection preserves the member's shape while
                        // constraining it to the instance type.
                        Some(self.db.intersection2(member, instance_type))
                    })
                    .collect()
            };

            if matching.is_empty() {
                return self.narrow_to_type(source_type, instance_type);
            } else if matching.len() == 1 {
                return matching[0];
            }
            return self.db.union(matching);
        }

        // Non-union: use instanceof-specific semantics
        trace!(
            "instanceof: non-union path for source_type={}",
            source_type.0
        );

        // Try type parameter narrowing first (produces T & InstanceType)
        if let Some(narrowed) = self.narrow_type_param(resolved_source, instance_type) {
            return narrowed;
        }

        // For non-primitive, non-type-param source types, instanceof narrowing
        // should keep them when there's a potential runtime relationship.
        // This handles cases like `readonly number[]` narrowed by `instanceof Array`:
        // - readonly number[] is NOT a subtype of Array<T> (missing mutating methods)
        // - Array<T> is NOT a subtype of readonly number[] (unbound T)
        // - But at runtime, a readonly array IS an Array instance
        if !self.is_js_primitive(resolved_source) {
            if self.is_assignable_to(resolved_source, instance_type) {
                return source_type;
            }
            if self.is_assignable_to(instance_type, resolved_source) {
                return instance_type;
            }
            // Non-primitive types may still be instances at runtime.
            // Neither direction holds — create intersection per tsc semantics.
            // This handles cases like `interface I {}` narrowed by `instanceof RegExp`.
            return self.db.intersection2(source_type, instance_type);
        }
        // Primitives can never pass instanceof
        TypeId::NEVER
    }

    /// Narrow a type for the false branch of `instanceof`.
    ///
    /// Keeps primitive types (which can never pass instanceof) and excludes
    /// non-primitive members that are subtypes of the instance type.
    /// For example, `string | number | Date` with `instanceof Object` false
    /// branch gives `string | number` (Date is excluded as it's an Object instance).
    pub fn narrow_by_instanceof_false(&self, source_type: TypeId, instance_type: TypeId) -> TypeId {
        let resolved_source = self.resolve_type(source_type);

        if let Some(members) = union_list_id(self.db, resolved_source) {
            let members = self.db.type_list(members);
            let remaining: Vec<TypeId> = members
                .iter()
                .filter(|&&member| {
                    // Primitives always survive the false branch of instanceof
                    if self.is_js_primitive(member) {
                        return true;
                    }
                    // A member only fails to reach the false branch if it is GUARANTEED
                    // to pass the true branch. In TypeScript, this means the member
                    // is assignable to the instance type.
                    // If it is NOT assignable, it MIGHT fail at runtime, so we MUST keep it.
                    !self.is_assignable_to(member, instance_type)
                })
                .copied()
                .collect();

            if remaining.is_empty() {
                return TypeId::NEVER;
            } else if remaining.len() == 1 {
                return remaining[0];
            }
            return self.db.union(remaining);
        }

        // Non-union: if it's guaranteed to be an instance, it will never reach the false branch.
        if self.is_assignable_to(resolved_source, instance_type) {
            return TypeId::NEVER;
        }

        // Otherwise, it might reach the false branch, so we keep the original type.
        source_type
    }
}
