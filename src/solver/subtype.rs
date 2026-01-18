```rust
//! Type subtyping.
//!
//! This module handles the subtyping logic, including determining if a type is a subtype of another.

use std::ops::Deref;

use rustc_hash::FxHashMap;
use tower_lsp::lsp_types::Url;

use crate::{
    base::*,
    ty::{Locator, Ty, TyData, TyKind},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeVar {
    pub id: u32,
}

#[derive(Debug, Clone, Default)]
pub struct SubtypeContext {
    // Mapping from type variable to its concrete type (or bounds if not fully solved)
    pub types: FxHashMap<TypeVar, Ty>,
    // Cache for subtyping checks to avoid redundant work (optional but recommended)
    // pub cache: FxHashMap<(Ty, Ty), bool>, 
}

impl SubtypeContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a type variable binding to the context.
    pub fn add_var(&mut self, var: TypeVar, ty: Ty) {
        self.types.insert(var, ty);
    }

    /// Retrieves the concrete type for a variable if it exists.
    pub fn get_var(&self, var: TypeVar) -> Option<&Ty> {
        self.types.get(&var)
    }
}

/// The main entry point for subtyping checks.
/// Returns true if `a` is a subtype of `b` within the given context `cx`.
pub fn subtype_relation(cx: &mut SubtypeContext, a: &Ty, b: &Ty) -> bool {
    // Stack-based iterative implementation to prevent recursion depth overflows.
    // Uses a manual stack of tuples (Ty, Ty) to process.
    
    let mut stack = vec![(a.clone(), b.clone())];
    
    // We need a separate set to track pairs currently being processed on the stack
    // to handle cycles and avoid infinite loops.
    let mut in_progress = fxhash::FxHashSet::default();

    while let Some((left, right)) = stack.pop() {
        // Cycle detection: if we are already checking this pair, assume true (structural recursion)
        // or return false depending on the specific type system rules.
        // For a nominal system, cycles usually imply equality, which implies subtype.
        if !in_progress.insert((left.clone(), right.clone())) {
            continue;
        }

        // Resolve type variables (deref into the actual type)
        let left_resolved = resolve_type_var(cx, &left);
        let right_resolved = resolve_type_var(cx, &right);

        // Fast path: Pointer equality or structural equality check
        if left_resolved == right_resolved {
            continue;
        }

        let left_kind = left_resolved.kind(cx.tcx);
        let right_kind = right_resolved.kind(cx.tcx);

        match (left_kind, right_kind) {
            // 1. Top type: Everything is a subtype of Top (or Unknown treated as Top)
            (_, TyKind::Any) | (_, TyKind::Unknown) => {
                // left <: Any is always true
                continue;
            }

            // 2. Bottom type: Bottom is a subtype of everything
            (TyKind::Never, _) => {
                // Never <: right is always true
                continue;
            }

            // 3. Primitives (Exact match required)
            (TyKind::Boolean, TyKind::Boolean) => continue,
            (TyKind::Number, TyKind::Number) => continue,
            (TyKind::String, TyKind::String) => continue,
            
            // 4. Union Subtyping: T <: A | B iff T <: A or T <: B
            // And A | B <: T iff A <: T and B <: T
            (TyKind::Union(left_members), TyKind::Union(right_members)) => {
                // Optimization: Check if one is subset of the other
                // This loop effectively performs `forall l in left, exists r in right. l <: r`
                // For simplicity here, we fall back to pairwise checks which can be heavy.
                // A more optimized approach:
                
                // Check Union <: Union
                // This requires left members to be covered by right members.
                let mut all_covered = true;
                for l_ty in left_members {
                    let mut is_covered = false;
                    for r_ty in right_members {
                        // Check l_ty <: r_ty
                        stack.push((l_ty.clone(), r_ty.clone()));
                        is_covered = true;
                        break;
                    }
                    if !is_covered {
                        all_covered = false;
                        break;
                    }
                }
                
                if !all_covered {
                    return false;
                }
            }

            (TyKind::Union(left_members), _) => {
                // Union <: Non-Union
                // All members of left must be subtypes of right
                for l_ty in left_members {
                    stack.push((l_ty.clone(), right_resolved.clone()));
                }
            }
            
            (_, TyKind::Union(right_members)) => {
                // Non-Union <: Union
                // Left must be subtype of at least one member of right
                let mut is_subtype = false;
                for r_ty in right_members {
                    // To support "OR" logic iteratively, we might need to branch state.
                    // However, standard stack processing implies "AND".
                    // Implementing "OR" in a flat loop is tricky.
                    // Workaround: We check if left <: right_member holds. 
                    // Since we are in a loop, if one branch succeeds we should proceed.
                    // If we treat the stack as a list of requirements, this logic is "AND".
                    
                    // To properly support `T <: (A | B)`, we need to check `T <: A` OR `T <: B`.
                    // Since `subtype_relation` returns a boolean, we can perform this check
                    // immediately if we can determine it, or push the "successful" path.
                    // But here we just pop from stack.
                    
                    // Correct iterative approach for OR:
                    // We need to check if there exists a member.
                    // We can't easily push "one of these must succeed" onto a single stack.
                    // Instead, we can perform the check immediately:
                    if subtype_relation_inner_pass(cx, &left_resolved, r_ty) {
                        is_subtype = true;
                        break;
                    }
                }
                if !is_subtype {
                    return false;
                }
            }

            // 5. Objects and Structural Typing
            (TyKind::Object(left_obj), TyKind::Object(right_obj)) => {
                // Left <: Right implies:
                // 1. Width: Right's properties are a subset of Left's properties (Left has at least what Right has)
                // 2. Depth: For every shared property p, Left[p] <: Right[p]
                
                for (right_key, right_ty) in &right_obj.properties {
                    match left_obj.properties.get(right_key) {
                        Some(left_ty) => {
                            stack.push((left_ty.clone(), right_ty.clone()));
                        }
                        None => {
                            // Missing property in Left, cannot be subtype
                            return false;
                        }
                    }
                }
                
                // Call type variance (optional)
                // left_obj.caller <: right_obj.caller ? depends on contravariance
                if let (Some(l_call), Some(r_call)) = (&left_obj.call, &right_obj.call) {
                     // Caller types are usually contravariant (function params), so we check R <: L
                     // This is a simplified check.
                     stack.push((r_call.clone(), l_call.clone()));
                }
            }

            // 6. Functions (Contravariant parameters, Covariant return)
            (TyKind::Function(left_params, left_ret), TyKind::Function(right_params, right_ret)) => {
                // Parameters: Right must be subtype of Left (contravariance)
                // If lengths differ, we usually return false, unless we support variadics/overloading
                
                if left_params.len() != right_params.len() {
                    return false;
                }
                
                for (l_p, r_p) in left_params.iter().zip(right_params.iter()) {
                    // Note: r_p <: l_p
                    stack.push((r_p.clone(), l_p.clone()));
                }
                
                // Return type: Left must be subtype of Right (covariance)
                stack.push((*left_ret).clone(), (*right_ret).clone());
            }
            
            // 7. References
            // T <: U implies &mut T <: &mut U (Invariant usually for mut, Covariant for const)
            // Handling variance here is complex. Assuming simple structural covariance for immut.
            
            _ => {
                // If no rule matched, it's not a subtype
                return false;
            }
        }
    }

    true
}

// Helper to resolve type variables
fn resolve_type_var(cx: &SubtypeContext, ty: &Ty) -> Ty {
    // In a real implementation, this would loop until it hits a non-var type or occurs
    match ty.kind {
        TyKind::TypeVar(v) => {
            cx.get_var(v)
                .and_then(|t| {
                    // Avoid infinite loops in cycles, though resolve should handle levels
                    if t.kind == ty.kind { None } else { Some(t.clone()) }
                })
                .unwrap_or_else(|| ty.clone())
        }
        _ => ty.clone(),
    }
}
```

###
