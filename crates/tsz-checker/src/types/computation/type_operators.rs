//! Union, intersection, type operator, and keyof type computation.
//! Also includes class-type helpers for brand property resolution.

use crate::query_boundaries::type_computation::complex as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Get type from a union type node (A | B).
    ///
    /// ## Behavior:
    /// - Zero members → NEVER
    /// - One member → that member's type
    /// - Multiple members → Union type with all members
    ///
    /// ## Member Resolution:
    /// - Each member is resolved via `get_type_from_type_node`
    /// - Handles nested typeof expressions and type references
    ///
    /// ## TypeScript Semantics:
    /// Union types represent values that can be any of the members:
    /// - Primitives: `string | number` accepts either
    /// - Objects: Combines properties from all members
    /// - Functions: Union of function signatures
    pub(crate) fn get_type_from_union_type(&mut self, idx: NodeIndex) -> TypeId {
        let factory = self.ctx.types.factory();
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        // UnionType uses CompositeTypeData which has a types list
        if let Some(composite) = self.ctx.arena.get_composite_type(node) {
            let mut member_types = Vec::new();
            for &type_idx in &composite.types.nodes {
                // Use get_type_from_type_node to properly resolve typeof expressions via binder
                member_types.push(self.get_type_from_type_node(type_idx));
            }

            if member_types.is_empty() {
                return TypeId::NEVER;
            }
            if member_types.len() == 1 {
                return member_types[0];
            }

            return factory.union(member_types);
        }

        TypeId::ERROR // Missing composite type data - propagate error
    }

    /// Get type from an intersection type node (A & B).
    ///
    /// Uses `CheckerState`'s `get_type_from_type_node` for each member to ensure
    /// typeof expressions are resolved via binder (same reason as union types).
    pub(crate) fn get_type_from_intersection_type(&mut self, idx: NodeIndex) -> TypeId {
        let factory = self.ctx.types.factory();
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        if let Some(composite) = self.ctx.arena.get_composite_type(node) {
            let mut member_types = Vec::new();
            for &type_idx in &composite.types.nodes {
                member_types.push(self.get_type_from_type_node(type_idx));
            }

            if member_types.is_empty() {
                return TypeId::UNKNOWN;
            }
            if member_types.len() == 1 {
                return member_types[0];
            }

            return factory.intersection(member_types);
        }

        TypeId::ERROR
    }

    /// Get type from a type operator node (readonly T[], readonly [T, U], unique symbol).
    ///
    /// Handles type modifiers like:
    /// - `readonly T[]` - Creates `ReadonlyType` wrapper
    /// - `unique symbol` - Special marker for unique symbols
    pub(crate) fn get_type_from_type_operator(&mut self, idx: NodeIndex) -> TypeId {
        let factory = self.ctx.types.factory();
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        if let Some(type_op) = self.ctx.arena.get_type_operator(node) {
            let operator = type_op.operator;
            let inner_type = self.get_type_from_type_node(type_op.type_node);

            // Handle readonly operator
            if operator == SyntaxKind::ReadonlyKeyword as u16 {
                // Wrap the inner type in ReadonlyType
                return factory.readonly_type(inner_type);
            }

            // Handle unique operator
            if operator == SyntaxKind::UniqueKeyword as u16 {
                // unique is handled differently - it's a type modifier for symbols
                // For now, just return the inner type
                return inner_type;
            }

            // Unknown operator - return inner type
            inner_type
        } else {
            TypeId::ERROR // Missing type operator data - propagate error
        }
    }

    /// Get the `keyof` type for a given type.
    ///
    /// Computes the type of all property keys for a given object type.
    /// For example: `keyof { x: number; y: string }` = `"x" | "y"`.
    pub(crate) fn get_keyof_type(&mut self, operand: TypeId) -> TypeId {
        use tsz_solver::type_queries::{TypeResolutionKind, classify_for_type_resolution};

        let deferred_keyof = self.ctx.types.keyof(operand);

        // Prefer the shared solver `keyof` evaluator so intersections, unions, and
        // instantiated discriminated shapes follow the same semantics everywhere.
        // Fall back to the legacy shallow property collection only when evaluation
        // cannot reduce the operator.
        let evaluated = self.evaluate_type_with_env(deferred_keyof);
        if evaluated != deferred_keyof {
            return evaluated;
        }

        // Handle Lazy types by attempting to resolve them first
        // This allows keyof Lazy(DefId) to work correctly for circular dependencies
        match classify_for_type_resolution(self.ctx.types, operand) {
            TypeResolutionKind::Lazy(def_id) => {
                if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) {
                    let resolved = self.get_type_of_symbol(sym_id);
                    // Guard: if resolution returned the same Lazy type (symbol is
                    // currently being resolved — circular placeholder), bail out.
                    // Without this, the recursive get_keyof_type call loops forever
                    // because the same Lazy placeholder is returned from the cache
                    // each time. In debug mode this was masked by stack exhaustion,
                    // but in release mode LLVM tail-call optimizes the recursion
                    // into an infinite loop with no stack growth.
                    if resolved == operand {
                        return deferred_keyof;
                    }
                    // Also check if the resolved type is still a Lazy pointing
                    // to the same DefId — factory.lazy(def_id) may intern to a
                    // different TypeId than the original operand.
                    if let Some(resolved_def) = tsz_solver::lazy_def_id(self.ctx.types, resolved) {
                        if resolved_def == def_id {
                            return deferred_keyof;
                        }
                    }
                    // Recursively get keyof of the resolved type
                    return self.get_keyof_type(resolved);
                }
            }
            TypeResolutionKind::Application => {
                // Evaluate application types first
                let evaluated = self.evaluate_type_for_assignability(operand);
                // Guard: if evaluation couldn't reduce the type, bail out to
                // prevent infinite recursion.
                if evaluated == operand {
                    return deferred_keyof;
                }
                return self.get_keyof_type(evaluated);
            }
            TypeResolutionKind::Resolved => {}
        }

        tsz_solver::type_queries::keyof_object_properties(self.ctx.types, operand)
            .unwrap_or(TypeId::NEVER)
    }

    /// Get the class declaration node from a `TypeId`.
    ///
    /// This function attempts to find the class declaration for a given type
    /// by looking for "private brand" properties that TypeScript adds to class
    /// instances for brand checking.
    pub(crate) fn get_class_decl_from_type(&self, type_id: TypeId) -> Option<NodeIndex> {
        // Fast path: check the direct instance-type-to-class-declaration map first.
        // This correctly handles derived classes that have no brand properties.
        if let Some(&class_idx) = self.ctx.class_instance_type_to_decl.get(&type_id) {
            return Some(class_idx);
        }
        if self.ctx.class_decl_miss_cache.borrow().contains(&type_id) {
            return None;
        }

        use tsz_binder::SymbolId;

        fn parse_brand_name(name: &str) -> Option<Result<SymbolId, NodeIndex>> {
            const NODE_PREFIX: &str = "__private_brand_node_";
            const PREFIX: &str = "__private_brand_";

            if let Some(rest) = name.strip_prefix(NODE_PREFIX) {
                let node_id: u32 = rest.parse().ok()?;
                return Some(Err(NodeIndex(node_id)));
            }
            if let Some(rest) = name.strip_prefix(PREFIX) {
                let sym_id: u32 = rest.parse().ok()?;
                return Some(Ok(SymbolId(sym_id)));
            }

            None
        }

        fn collect_candidates<'a>(
            checker: &CheckerState<'a>,
            type_id: TypeId,
            out: &mut Vec<NodeIndex>,
        ) {
            match query::classify_for_class_decl(checker.ctx.types, type_id) {
                query::ClassDeclTypeKind::Object(shape_id) => {
                    let shape = checker.ctx.types.object_shape(shape_id);
                    for prop in &shape.properties {
                        let name = checker.ctx.types.resolve_atom_ref(prop.name);
                        if let Some(parsed) = parse_brand_name(&name) {
                            let class_idx = match parsed {
                                Ok(sym_id) => checker.get_class_declaration_from_symbol(sym_id),
                                Err(node_idx) => Some(node_idx),
                            };
                            if let Some(class_idx) = class_idx {
                                out.push(class_idx);
                            }
                        }
                    }
                }
                query::ClassDeclTypeKind::Members(members) => {
                    for member in members {
                        collect_candidates(checker, member, out);
                    }
                }
                query::ClassDeclTypeKind::NotObject => {}
            }
        }

        let mut candidates = Vec::new();
        collect_candidates(self, type_id, &mut candidates);
        if candidates.is_empty() {
            self.ctx.class_decl_miss_cache.borrow_mut().insert(type_id);
            return None;
        }
        if candidates.len() == 1 {
            let class_idx = candidates[0];
            self.ctx.class_decl_miss_cache.borrow_mut().remove(&type_id);
            return Some(class_idx);
        }

        let resolved = candidates
            .iter()
            .find(|&&candidate| {
                candidates.iter().all(|&other| {
                    candidate == other || self.is_class_derived_from(candidate, other)
                })
            })
            .copied();
        if resolved.is_none() {
            self.ctx.class_decl_miss_cache.borrow_mut().insert(type_id);
        } else {
            self.ctx.class_decl_miss_cache.borrow_mut().remove(&type_id);
        }
        resolved
    }

    /// Get the class name from a `TypeId` if it represents a class instance.
    ///
    /// Returns the class name as a string if the type represents a class,
    /// or None if the type doesn't represent a class or the class has no name.
    pub(crate) fn get_class_name_from_type(&self, type_id: TypeId) -> Option<String> {
        self.get_class_decl_for_display_type(type_id)
            .map(|(class_idx, _)| self.get_class_name_from_decl(class_idx))
    }
}
