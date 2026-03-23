//! Enum helpers, type overlap checking, readonly properties, and class/function utility methods.

use crate::query_boundaries::dispatch::is_type_parameter_like;
use crate::query_boundaries::type_checking_utilities as query;
use crate::state::{CheckerState, EnumKind, MAX_TREE_WALK_ITERATIONS, MemberAccessLevel};
use rustc_hash::FxHashMap;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

// Thread-local visited set for non-const enum evaluation cycle detection.
//
// Tracks which enum member declarations are currently being evaluated by
// `evaluate_enum_member_access`, preventing infinite recursion from self-references
// (e.g., `B = E.B`) and mutual recursion across enums
// (e.g., `enum E { A = F.B }; enum F { B = E.A }`).
thread_local! {
    static EVAL_VISITED: std::cell::RefCell<rustc_hash::FxHashSet<NodeIndex>>
        = std::cell::RefCell::new(rustc_hash::FxHashSet::default());
}

// RAII guard that removes a `NodeIndex` from `EVAL_VISITED` on drop.
struct VisitedGuard(NodeIndex);
impl Drop for VisitedGuard {
    fn drop(&mut self) {
        EVAL_VISITED.with(|v| {
            v.borrow_mut().remove(&self.0);
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrimitiveOverlapKind {
    String,
    Number,
    BigInt,
    Boolean,
    Symbol,
}

#[derive(Clone, Copy, Debug)]
enum SimpleOverlapType {
    Primitive(PrimitiveOverlapKind),
    StringLiteral(tsz_common::interner::Atom),
    NumberLiteral(f64),
    BigIntLiteral(tsz_common::interner::Atom),
    BooleanLiteral(bool),
}

#[derive(Clone, Debug, PartialEq)]
enum EnumCompatValue {
    Number(f64),
    String(String),
}

impl<'a> CheckerState<'a> {
    pub(crate) fn enum_assignability_override(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> Option<bool> {
        let source_sym = self.enum_symbol_from_full_enum_type(source)?;
        let target_sym = self.enum_symbol_from_full_enum_type(target)?;

        if source_sym == target_sym {
            return None;
        }

        let source_name = self.ctx.binder.get_symbol(source_sym)?.escaped_name.clone();
        let target_name = self.ctx.binder.get_symbol(target_sym)?.escaped_name.clone();
        if source_name != target_name {
            return Some(false);
        }

        if self.is_const_enum_symbol(source_sym) || self.is_const_enum_symbol(target_sym) {
            return Some(false);
        }

        if self.enum_kind(source_sym) != Some(EnumKind::Numeric)
            || self.enum_kind(target_sym) != Some(EnumKind::Numeric)
        {
            return None;
        }

        let source_members = self.enum_member_compat_map(source_sym)?;
        let target_members = self.enum_member_compat_map(target_sym)?;
        let is_subset = source_members
            .iter()
            .all(|(name, value)| target_members.get(name) == Some(value));
        Some(is_subset)
    }

    /// Get the enum symbol from a type reference.
    ///
    /// Returns the symbol ID if the type refers to an enum, None otherwise.
    pub(crate) fn enum_symbol_from_type(&self, type_id: TypeId) -> Option<SymbolId> {
        // Use resolve_type_to_symbol_id instead of get_ref_symbol
        let sym_id = self.ctx.resolve_type_to_symbol_id(type_id)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::ENUM == 0 {
            return None;
        }
        Some(sym_id)
    }

    pub(crate) fn enum_symbol_from_full_enum_type(&self, type_id: TypeId) -> Option<SymbolId> {
        let def_id = tsz_solver::type_queries::get_enum_def_id(self.ctx.types, type_id)?;
        let sym_id = self.ctx.def_to_symbol_id_with_fallback(def_id)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        ((symbol.flags & symbol_flags::ENUM) != 0
            && (symbol.flags & symbol_flags::ENUM_MEMBER) == 0)
            .then_some(sym_id)
    }

    pub(crate) fn enum_symbol_from_enumish_type(&self, type_id: TypeId) -> Option<SymbolId> {
        let def_id = tsz_solver::type_queries::get_enum_def_id(self.ctx.types, type_id)?;
        let sym_id = self.ctx.def_to_symbol_id_with_fallback(def_id)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if (symbol.flags & symbol_flags::ENUM_MEMBER) != 0 {
            return Some(symbol.parent);
        }
        ((symbol.flags & symbol_flags::ENUM) != 0).then_some(sym_id)
    }

    pub(crate) fn apparent_enum_instance_type(&self, type_id: TypeId) -> Option<TypeId> {
        let enum_type =
            tsz_solver::type_queries::get_type_parameter_constraint(self.ctx.types, type_id)
                .filter(|constraint| {
                    tsz_solver::type_queries::get_enum_def_id(self.ctx.types, *constraint).is_some()
                })
                .unwrap_or(type_id);
        let sym_id = self.enum_symbol_from_enumish_type(enum_type)?;
        match self.enum_kind(sym_id)? {
            EnumKind::Numeric => Some(TypeId::NUMBER),
            EnumKind::String => Some(TypeId::STRING),
            EnumKind::Mixed => Some(
                self.ctx
                    .types
                    .factory()
                    .union(vec![TypeId::NUMBER, TypeId::STRING]),
            ),
        }
    }

    pub(crate) fn is_const_enum_symbol(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        symbol.declarations.iter().copied().any(|decl_idx| {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                return false;
            };
            let Some(enum_decl) = self.ctx.arena.get_enum(node) else {
                return false;
            };
            self.ctx
                .arena
                .has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword)
        })
    }

    fn enum_member_compat_map(
        &self,
        sym_id: SymbolId,
    ) -> Option<FxHashMap<String, EnumCompatValue>> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let mut result = FxHashMap::default();
        let mut next_numeric_value = 0.0;
        let mut saw_enum_decl = false;

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(enum_decl) = self.ctx.arena.get_enum_at(decl_idx) else {
                continue;
            };
            saw_enum_decl = true;

            for &member_idx in &enum_decl.members.nodes {
                let member_node = self.ctx.arena.get(member_idx)?;
                let member = self.ctx.arena.get_enum_member(member_node)?;
                let member_name = self.get_property_name(member.name)?;

                let value = if member.initializer.is_some() {
                    let init_node = self.ctx.arena.get(member.initializer)?;
                    match init_node.kind {
                        k if k == SyntaxKind::StringLiteral as u16 => {
                            let lit = self.ctx.arena.get_literal(init_node)?;
                            EnumCompatValue::String(lit.text.clone())
                        }
                        _ => {
                            let value = self.evaluate_constant_expression(member.initializer)?;
                            next_numeric_value = value + 1.0;
                            EnumCompatValue::Number(value)
                        }
                    }
                } else {
                    let value = EnumCompatValue::Number(next_numeric_value);
                    next_numeric_value += 1.0;
                    value
                };

                result.insert(member_name, value);
            }
        }

        saw_enum_decl.then_some(result)
    }

    /// Determine the kind of enum (string, numeric, or mixed).
    ///
    /// Returns None if the symbol is not an enum or has no members.
    pub(crate) fn enum_kind(&self, sym_id: SymbolId) -> Option<EnumKind> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::ENUM == 0 {
            return None;
        }

        let mut saw_string = false;
        let mut saw_numeric = false;

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(enum_decl) = self.ctx.arena.get_enum_at(decl_idx) else {
                continue;
            };
            for &member_idx in &enum_decl.members.nodes {
                let Some(member) = self.ctx.arena.get_enum_member_at(member_idx) else {
                    continue;
                };

                if member.initializer.is_some() {
                    let Some(init_node) = self.ctx.arena.get(member.initializer) else {
                        continue;
                    };
                    match init_node.kind {
                        k if k == SyntaxKind::StringLiteral as u16 => saw_string = true,
                        k if k == SyntaxKind::NumericLiteral as u16 => saw_numeric = true,
                        _ => {}
                    }
                } else {
                    saw_numeric = true;
                }
            }
        }

        if saw_string && saw_numeric {
            Some(EnumKind::Mixed)
        } else if saw_string {
            Some(EnumKind::String)
        } else {
            Some(EnumKind::Numeric)
        }
    }

    /// Get the literal type of an enum member from its initializer.
    ///
    /// Returns the literal type (e.g., Literal(0), Literal("a")) of the enum member.
    /// This is used to create `TypeData::Enum(member_def_id`, `literal_type`) for nominal typing.
    pub(crate) fn enum_member_type_from_decl(&self, member_decl: NodeIndex) -> TypeId {
        let factory = self.ctx.types.factory();
        // Get the member node
        let Some(member_node) = self.ctx.arena.get(member_decl) else {
            return TypeId::ERROR;
        };
        let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
            return TypeId::ERROR;
        };

        // Check if member has an explicit initializer
        if member.initializer.is_some() {
            let Some(init_node) = self.ctx.arena.get(member.initializer) else {
                return TypeId::ERROR;
            };

            match init_node.kind {
                k if k == SyntaxKind::StringLiteral as u16 => {
                    // Get the string literal value
                    if let Some(lit) = self.ctx.arena.get_literal(init_node) {
                        return factory.literal_string(&lit.text);
                    }
                }
                k if k == SyntaxKind::NumericLiteral as u16 => {
                    // Get the numeric literal value
                    if let Some(lit) = self.ctx.arena.get_literal(init_node) {
                        // lit.value is Option<f64>, use it if available
                        if let Some(value) = lit.value {
                            return factory.literal_number(value);
                        }
                        // Fallback: parse from text
                        if let Ok(value) = lit.text.parse::<f64>() {
                            return factory.literal_number(value);
                        }
                    }
                }
                _ => {
                    // Try to evaluate constant expression
                    if let Some(value) = self.evaluate_constant_expression(member.initializer) {
                        return factory.literal_number(value);
                    }
                }
            }
        }

        // No explicit initializer or computed value
        // This could be an auto-incremented numeric member
        // Fall back to NUMBER type (not a specific literal)
        TypeId::NUMBER
    }

    /// Evaluate a constant numeric expression (for enum member initializers).
    ///
    /// Handles: numeric literals, unary +/-/~, binary +/-/*/ // /%/|/&/^/<</>>/>>>,
    /// parenthesized expressions, and references to other enum members via
    /// property access (E.V1) or element access (E["V1"], E[`V1`]).
    /// Returns None if the expression cannot be evaluated at compile time.
    pub(crate) fn evaluate_constant_expression(&self, expr_idx: NodeIndex) -> Option<f64> {
        let node = self.ctx.arena.get(expr_idx)?;
        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.ctx.arena.get_literal(node)?;
                lit.value.or_else(|| lit.text.parse::<f64>().ok())
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.ctx.arena.get_unary_expr(node)?;
                let operand = self.evaluate_constant_expression(unary.operand)?;
                match unary.operator {
                    op if op == SyntaxKind::MinusToken as u16 => Some(-operand),
                    op if op == SyntaxKind::PlusToken as u16 => Some(operand),
                    op if op == SyntaxKind::TildeToken as u16 => Some(!(operand as i32) as f64),
                    _ => None,
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let bin = self.ctx.arena.get_binary_expr(node)?;
                let left = self.evaluate_constant_expression(bin.left)?;
                let right = self.evaluate_constant_expression(bin.right)?;
                match bin.operator_token {
                    op if op == SyntaxKind::PlusToken as u16 => Some(left + right),
                    op if op == SyntaxKind::MinusToken as u16 => Some(left - right),
                    op if op == SyntaxKind::AsteriskToken as u16 => Some(left * right),
                    op if op == SyntaxKind::SlashToken as u16 => Some(left / right),
                    op if op == SyntaxKind::PercentToken as u16 => Some(left % right),
                    op if op == SyntaxKind::BarToken as u16 => {
                        Some((left as i32 | right as i32) as f64)
                    }
                    op if op == SyntaxKind::AmpersandToken as u16 => {
                        Some((left as i32 & right as i32) as f64)
                    }
                    op if op == SyntaxKind::CaretToken as u16 => {
                        Some((left as i32 ^ right as i32) as f64)
                    }
                    op if op == SyntaxKind::LessThanLessThanToken as u16 => {
                        Some(((left as i32) << (right as u32 & 0x1f)) as f64)
                    }
                    op if op == SyntaxKind::GreaterThanGreaterThanToken as u16 => {
                        Some(((left as i32) >> (right as u32 & 0x1f)) as f64)
                    }
                    op if op == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16 => {
                        Some(((left as u32) >> (right as u32 & 0x1f)) as f64)
                    }
                    op if op == SyntaxKind::AsteriskAsteriskToken as u16 => Some(left.powf(right)),
                    _ => None,
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let paren = self.ctx.arena.get_parenthesized(node)?;
                self.evaluate_constant_expression(paren.expression)
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                self.evaluate_enum_member_access(expr_idx)
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                self.evaluate_enum_member_access(expr_idx)
            }
            _ => None,
        }
    }

    /// Resolve a property access or element access expression that references
    /// an enum member, and evaluate its numeric value.
    ///
    /// Handles patterns like:
    /// - `E.V1` (property access on enum)
    /// - `A.B.C.E.V1` (qualified namespace chain)
    /// - `E["V1"]` (element access with string literal)
    /// - `E[`V1`]` (element access with template literal)
    fn evaluate_enum_member_access(&self, expr_idx: NodeIndex) -> Option<f64> {
        // Collect the chain of identifiers and the final member name.
        // For `A.B.C.E.V1`: segments = ["A", "B", "C", "E"], member_name = "V1"
        let (segments, member_name) = self.collect_access_chain(expr_idx)?;
        if segments.is_empty() {
            return None;
        }

        // Walk the binder's symbol table to find the enum symbol.
        let root_name = &segments[0];
        let mut current_sym_id = self.ctx.binder.file_locals.get(root_name)?;

        for segment in &segments[1..] {
            let symbol = self.ctx.binder.get_symbol(current_sym_id)?;
            // Try exports first (for namespaces), then members (for enums/classes)
            current_sym_id = symbol
                .exports
                .as_ref()
                .and_then(|exports| exports.get(segment))
                .or_else(|| {
                    symbol
                        .members
                        .as_ref()
                        .and_then(|members| members.get(segment))
                })?;
        }

        // current_sym_id should now point to the enum symbol.
        // Look up the member in its exports (enum members are stored as exports).
        let enum_symbol = self.ctx.binder.get_symbol(current_sym_id)?;
        let member_sym_id = enum_symbol
            .exports
            .as_ref()
            .and_then(|exports| exports.get(&member_name))
            .or_else(|| {
                enum_symbol
                    .members
                    .as_ref()
                    .and_then(|members| members.get(&member_name))
            })?;

        let member_symbol = self.ctx.binder.get_symbol(member_sym_id)?;
        if member_symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER == 0 {
            return None;
        }

        // Get the member's value declaration and evaluate its initializer.
        let member_decl = member_symbol.value_declaration;
        let member_node = self.ctx.arena.get(member_decl)?;
        let member_data = self.ctx.arena.get_enum_member(member_node)?;

        if member_data.initializer.is_none() {
            // Auto-incremented member — we need to compute its position value.
            // Walk through all declarations of the parent enum to find this member's
            // auto-incremented value.
            //
            // Add to EVAL_VISITED before entering compute_auto_increment_value:
            // that function evaluates prior members' initializers, which can cycle
            // back to this auto-incremented member via cross-enum references.
            // e.g., `enum E { A = F.C }; enum F { B = E.A, C }`
            // E.A -> F.C (auto-inc) -> compute_auto_inc walks F.B -> E.A -> F.C -> ...
            let already_visiting = EVAL_VISITED.with(|v| !v.borrow_mut().insert(member_decl));
            if already_visiting {
                return None; // Circular — treat as non-constant
            }
            let _guard = VisitedGuard(member_decl);
            return self.compute_auto_increment_value(current_sym_id, member_decl);
        }

        // Guard against self-referencing and mutually-recursive enum initializers
        // (e.g., `B = E.B` or `enum E { A = F.B }; enum F { B = E.A }`).
        // Without this, the recursive evaluate_constant_expression call creates
        // infinite recursion → stack overflow.
        //
        // Uses the module-level EVAL_VISITED thread-local with an RAII drop guard
        // to ensure cleanup even if evaluation panics.
        let already_visiting = EVAL_VISITED.with(|v| !v.borrow_mut().insert(member_decl));
        if already_visiting {
            return None; // Circular — treat as non-constant
        }
        let _guard = VisitedGuard(member_decl);
        self.evaluate_constant_expression(member_data.initializer)
    }

    /// Collect the identifier chain from a property/element access expression.
    /// Returns `(object_segments, member_name)`.
    /// For `A.B.C.E.V1`: `(["A", "B", "C", "E"], "V1")`
    /// For `E["V1"]`: (["E"], "V1")
    fn collect_access_chain(&self, expr_idx: NodeIndex) -> Option<(Vec<String>, String)> {
        let node = self.ctx.arena.get(expr_idx)?;

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                let prop_node = self.ctx.arena.get(access.name_or_argument)?;
                let member_name = self
                    .ctx
                    .arena
                    .get_identifier(prop_node)?
                    .escaped_text
                    .clone();

                // Recursively collect the object chain
                let obj_node = self.ctx.arena.get(access.expression)?;
                if obj_node.kind == SyntaxKind::Identifier as u16 {
                    let ident = self.ctx.arena.get_identifier(obj_node)?;
                    Some((vec![ident.escaped_text.clone()], member_name))
                } else if obj_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                    let (mut segments, last_segment) =
                        self.collect_access_chain(access.expression)?;
                    segments.push(last_segment);
                    Some((segments, member_name))
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                // Get the string key from the element access argument
                let arg_node = self.ctx.arena.get(access.name_or_argument)?;
                let member_name = if arg_node.kind == SyntaxKind::StringLiteral as u16
                    || arg_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                {
                    self.ctx.arena.get_literal(arg_node)?.text.clone()
                } else {
                    return None;
                };

                // Collect the object chain
                let obj_node = self.ctx.arena.get(access.expression)?;
                if obj_node.kind == SyntaxKind::Identifier as u16 {
                    let ident = self.ctx.arena.get_identifier(obj_node)?;
                    Some((vec![ident.escaped_text.clone()], member_name))
                } else if obj_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                    let (mut segments, last_segment) =
                        self.collect_access_chain(access.expression)?;
                    segments.push(last_segment);
                    Some((segments, member_name))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Compute the auto-incremented value for an enum member without an initializer.
    /// Walks through all declarations of the parent enum up to the target member,
    /// tracking the auto-increment counter.
    fn compute_auto_increment_value(
        &self,
        enum_sym_id: tsz_binder::SymbolId,
        target_member_decl: NodeIndex,
    ) -> Option<f64> {
        let enum_symbol = self.ctx.binder.get_symbol(enum_sym_id)?;

        for &decl_idx in &enum_symbol.declarations {
            let enum_decl = self.ctx.arena.get_enum_at(decl_idx)?;
            // Reset auto-increment at the start of each declaration block.
            let mut auto_value: f64 = 0.0;
            for &member_idx in &enum_decl.members.nodes {
                if member_idx == target_member_decl {
                    return Some(auto_value);
                }
                let member_node = self.ctx.arena.get(member_idx)?;
                let member_data = self.ctx.arena.get_enum_member(member_node)?;
                if member_data.initializer.is_some() {
                    if let Some(val) = self.evaluate_constant_expression(member_data.initializer) {
                        auto_value = val + 1.0;
                    } else {
                        // Non-numeric initializer breaks auto-increment
                        return None;
                    }
                } else {
                    auto_value += 1.0;
                }
            }
        }
        None
    }

    // evaluate_const_enum_initializer is a free function in the const_enum_eval module.

    // =========================================================================
    // Class Helper Functions
    // =========================================================================

    /// Get the class symbol from an expression node.
    ///
    /// Returns the symbol ID if the expression refers to a class, None otherwise.
    pub(crate) fn class_symbol_from_expression(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            let sym_id = self.resolve_identifier_symbol(expr_idx)?;
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if symbol.flags & symbol_flags::CLASS != 0 {
                return Some(sym_id);
            }
        }
        None
    }

    /// Get the class symbol from a type annotation node.
    ///
    /// Handles type queries like `typeof MyClass`.
    pub(crate) fn class_symbol_from_type_annotation(
        &self,
        type_idx: NodeIndex,
    ) -> Option<SymbolId> {
        let node = self.ctx.arena.get(type_idx)?;
        if node.kind != syntax_kind_ext::TYPE_QUERY {
            return None;
        }
        let query = self.ctx.arena.get_type_query(node)?;
        self.class_symbol_from_expression(query.expr_name)
    }

    /// Get the class symbol from an assignment target.
    ///
    /// Handles cases where the target is a variable with a class type annotation
    /// or initialized with a class expression.
    pub(crate) fn assignment_target_class_symbol(&self, left_idx: NodeIndex) -> Option<SymbolId> {
        let node = self.ctx.arena.get(left_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.resolve_identifier_symbol(left_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::CLASS != 0 {
            return Some(sym_id);
        }
        if symbol.flags
            & (symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE)
            == 0
        {
            return None;
        }
        if symbol.value_declaration.is_none() {
            return None;
        }
        let decl_node = self.ctx.arena.get(symbol.value_declaration)?;
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        if var_decl.type_annotation.is_some()
            && let Some(class_sym) =
                self.class_symbol_from_type_annotation(var_decl.type_annotation)
        {
            return Some(class_sym);
        }
        if var_decl.initializer.is_some()
            && let Some(class_sym) = self.class_symbol_from_expression(var_decl.initializer)
        {
            return Some(class_sym);
        }
        None
    }

    /// Get the access level of a class constructor.
    ///
    /// Returns `Some(MemberAccessLevel::Private)` or `Some(MemberAccessLevel::Protected)` if restricted.
    /// Returns None if public (the default) or if the symbol is not a class.
    ///
    /// Note: If a class has no explicit constructor, it inherits the access level
    /// from its base class's constructor.
    pub(crate) fn class_constructor_access_level(
        &self,
        sym_id: SymbolId,
    ) -> Option<MemberAccessLevel> {
        let mut visited = rustc_hash::FxHashSet::default();
        self.class_constructor_access_level_inner(sym_id, &mut visited)
    }

    fn class_constructor_access_level_inner(
        &self,
        sym_id: SymbolId,
        visited: &mut rustc_hash::FxHashSet<SymbolId>,
    ) -> Option<MemberAccessLevel> {
        // Cycle detection: bail out if we've already visited this symbol
        if !visited.insert(sym_id) {
            return None;
        }

        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::CLASS == 0 {
            return None;
        }
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let class = self.ctx.arena.get_class_at(decl_idx)?;

        // First, check if this class has an explicit constructor
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                continue;
            };
            // Check modifiers for access level
            if self.has_private_modifier(&ctor.modifiers) {
                return Some(MemberAccessLevel::Private);
            }
            if self.has_protected_modifier(&ctor.modifiers) {
                return Some(MemberAccessLevel::Protected);
            }
            // Explicit public constructor - public default
            return None;
        }

        // No explicit constructor found - check base class if extends clause exists
        let Some(ref heritage_clauses) = class.heritage_clauses else {
            // No extends clause - public default
            return None;
        };

        // Find the extends clause and get the base class
        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };

            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            // Only check extends clauses (not implements)
            if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            // Get the first type in the extends clause
            let Some(&first_type_idx) = heritage.types.nodes.first() else {
                continue;
            };

            // Get the expression from ExpressionWithTypeArguments
            let expr_idx = if let Some(type_node) = self.ctx.arena.get(first_type_idx)
                && let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node)
            {
                expr_type_args.expression
            } else {
                first_type_idx
            };

            // Resolve the base class symbol
            let Some(base_sym) = self.resolve_heritage_symbol(expr_idx) else {
                continue;
            };

            // Recursively check the base class's constructor access level
            // This handles inherited private/protected constructors
            return self.class_constructor_access_level_inner(base_sym, visited);
        }

        // No extends clause or couldn't resolve base class - public default
        None
    }

    // =========================================================================
    // =========================================================================
    // Type Query Helper Functions
    // =========================================================================

    /// Get the specific type that makes this type invalid as an index type (TS2538).
    pub(crate) fn type_get_invalid_index_type_member(&self, type_id: TypeId) -> Option<TypeId> {
        query::get_invalid_index_type_member(self.ctx.types, type_id)
    }

    fn classify_simple_overlap_type(&self, type_id: TypeId) -> Option<SimpleOverlapType> {
        use query::LiteralTypeKind;

        let primitive = match type_id {
            TypeId::STRING => Some(PrimitiveOverlapKind::String),
            TypeId::NUMBER => Some(PrimitiveOverlapKind::Number),
            TypeId::BIGINT => Some(PrimitiveOverlapKind::BigInt),
            TypeId::BOOLEAN => Some(PrimitiveOverlapKind::Boolean),
            TypeId::SYMBOL => Some(PrimitiveOverlapKind::Symbol),
            _ => None,
        };
        if let Some(kind) = primitive {
            return Some(SimpleOverlapType::Primitive(kind));
        }

        match query::classify_literal_type(self.ctx.types, type_id) {
            LiteralTypeKind::String(atom) => Some(SimpleOverlapType::StringLiteral(atom)),
            LiteralTypeKind::Number(value) => Some(SimpleOverlapType::NumberLiteral(value)),
            LiteralTypeKind::BigInt(atom) => Some(SimpleOverlapType::BigIntLiteral(atom)),
            LiteralTypeKind::Boolean(value) => Some(SimpleOverlapType::BooleanLiteral(value)),
            LiteralTypeKind::NotLiteral => None,
        }
    }

    fn simple_overlap_types_overlap(
        &self,
        left: SimpleOverlapType,
        right: SimpleOverlapType,
    ) -> bool {
        use PrimitiveOverlapKind as P;
        use SimpleOverlapType as T;

        match (left, right) {
            (T::Primitive(a), T::Primitive(b)) => a == b,
            (T::Primitive(P::String), T::StringLiteral(_))
            | (T::StringLiteral(_), T::Primitive(P::String))
            | (T::Primitive(P::Number), T::NumberLiteral(_))
            | (T::NumberLiteral(_), T::Primitive(P::Number))
            | (T::Primitive(P::BigInt), T::BigIntLiteral(_))
            | (T::BigIntLiteral(_), T::Primitive(P::BigInt))
            | (T::Primitive(P::Boolean), T::BooleanLiteral(_))
            | (T::BooleanLiteral(_), T::Primitive(P::Boolean)) => true,
            (T::StringLiteral(a), T::StringLiteral(b))
            | (T::BigIntLiteral(a), T::BigIntLiteral(b)) => a == b,
            (T::NumberLiteral(a), T::NumberLiteral(b)) => a == b,
            (T::BooleanLiteral(a), T::BooleanLiteral(b)) => a == b,
            _ => false,
        }
    }

    fn union_overlap_fast_path(&self, members: &[TypeId], other: TypeId) -> Option<bool> {
        let other_simple = self.classify_simple_overlap_type(other)?;
        for &member in members {
            let member_simple = self.classify_simple_overlap_type(member)?;
            if self.simple_overlap_types_overlap(member_simple, other_simple) {
                return Some(true);
            }
        }
        Some(false)
    }

    fn simple_overlap_fast_path(&self, left: TypeId, right: TypeId) -> Option<bool> {
        let left_simple = self.classify_simple_overlap_type(left)?;
        let right_simple = self.classify_simple_overlap_type(right)?;
        Some(self.simple_overlap_types_overlap(left_simple, right_simple))
    }

    /// Check if two types have no overlap (for TS2367 validation).
    /// Returns true if the types can never be equal in a comparison.
    pub(crate) fn types_have_no_overlap(&mut self, left: TypeId, right: TypeId) -> bool {
        // Depth guard: `types_have_no_overlap` and `objects_with_independently_overlapping_props`
        // are mutually recursive. For infinitely-expanding recursive types (e.g.,
        // `interface List<T> { owner: List<List<T>> }`), the property-level overlap
        // check re-enters this function with ever-deeper type arguments, causing
        // unbounded stack growth. The depth counter on `ctx.overlap_depth` (limit 20)
        // is generous for real-world types while preventing stack overflow. When the
        // limit is reached we conservatively report "types overlap" (return false) —
        // matching tsc's behavior of assuming comparability for excessively deep
        // recursive types.
        if !self.ctx.overlap_depth.borrow_mut().enter() {
            return false; // Conservatively assume overlap
        }

        let result = self.types_have_no_overlap_inner(left, right);
        self.ctx.overlap_depth.borrow_mut().leave();
        result
    }

    // Inner implementation of overlap checking (after depth guard).

    /// Check if a type is a "weak type" (all properties optional) for overlap purposes.
    /// tsc never emits TS2367 when comparing against weak types.
    fn is_weak_type_for_overlap(&self, type_id: TypeId) -> bool {
        use crate::query_boundaries::dispatch as query;
        // Check direct object types
        if let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)
            && !shape.properties.is_empty()
            && shape.properties.iter().all(|p| p.optional)
        {
            return true;
        }
        // Check union: if ALL members are weak types
        if let Some(members) = query::union_members(self.ctx.types, type_id)
            && !members.is_empty()
            && members.iter().all(|&m| self.is_weak_type_for_overlap(m))
        {
            return true;
        }
        false
    }

    fn types_have_no_overlap_inner(&mut self, left: TypeId, right: TypeId) -> bool {
        tracing::trace!(left = ?left, right = ?right, "types_have_no_overlap called");

        // any, unknown, error types can overlap with anything
        if left == TypeId::ANY || right == TypeId::ANY {
            tracing::trace!("has ANY");
            return false;
        }
        if left == TypeId::UNKNOWN || right == TypeId::UNKNOWN {
            tracing::trace!("has UNKNOWN");
            return false;
        }
        if left == TypeId::ERROR || right == TypeId::ERROR {
            tracing::trace!("has ERROR");
            return false;
        }

        // null/undefined are always comparable with any type (TSC's "comparable relation").
        // Even with strictNullChecks enabled, `null === x` and `undefined === x` should
        // never trigger TS2367.
        if left == TypeId::NULL
            || left == TypeId::UNDEFINED
            || right == TypeId::NULL
            || right == TypeId::UNDEFINED
        {
            return false;
        }

        // Weak types (all-optional properties) overlap with anything.
        // tsc never emits TS2367 when comparing against weak types.
        if self.is_weak_type_for_overlap(left) || self.is_weak_type_for_overlap(right) {
            return false;
        }

        // Same type always overlaps
        if left == right {
            tracing::trace!("same type");
            return false;
        }

        // For type parameters, delegate to the comparability check which correctly handles:
        // - T vs {} → comparable (overlap exists, return false)
        // - T vs U (unrelated) → not comparable (no overlap, return true)
        // - T extends X vs Y → uses constraint resolution
        if is_type_parameter_like(self.ctx.types, left)
            || is_type_parameter_like(self.ctx.types, right)
        {
            return !self.is_type_comparable_to(left, right);
        }

        let effective_left = left;
        let effective_right = right;

        tracing::trace!(
            ?effective_left,
            ?effective_right,
            "effective types for overlap check"
        );

        // Fast path for primitive/literal combinations without recursive relation checks.
        if let Some(has_overlap) = self.simple_overlap_fast_path(effective_left, effective_right) {
            return !has_overlap;
        }

        // Check union types: if any member of one union overlaps with the other, they overlap
        if let query::UnionMembersKind::Union(left_members) =
            query::classify_for_union_members(self.ctx.types, effective_left)
        {
            if let Some(has_overlap) = self.union_overlap_fast_path(&left_members, effective_right)
            {
                return !has_overlap;
            }

            tracing::trace!("effective_left is union");
            for &left_member in &left_members {
                tracing::trace!(?left_member, ?effective_right, "checking union member");
                if !self.types_have_no_overlap(left_member, effective_right) {
                    tracing::trace!("union member overlaps - union overlaps");
                    return false;
                }
            }
            tracing::trace!("no union members overlap - returning true");
            return true;
        }

        if let query::UnionMembersKind::Union(right_members) =
            query::classify_for_union_members(self.ctx.types, effective_right)
        {
            if let Some(has_overlap) = self.union_overlap_fast_path(&right_members, effective_left)
            {
                return !has_overlap;
            }

            tracing::trace!("effective_right is union");
            for &right_member in &right_members {
                if !self.types_have_no_overlap(effective_left, right_member) {
                    return false;
                }
            }
            return true;
        }

        // Intersection type overlap handling (three tiers matching TSC behavior):
        //
        // 1. Type parameters: Resolve to constraints and rebuild intersection.
        //    `T & number` where T extends `string | number` → `number` → no overlap
        //    with `"hello"`.
        //
        // 2. Primitive members (branded types): Per-member overlap check.
        //    `string & { $Brand: any }` overlaps with `"hello"` because the `string`
        //    member overlaps.
        //
        // 3. All-object intersections: Skip per-member check, fall through to the
        //    bidirectional assignability check below. `I1 & I3` vs `I2` should NOT
        //    overlap just because `I1` is assignable from `I2`.
        if let Some(left_members) = query::get_intersection_members(self.ctx.types, effective_left)
        {
            let has_type_param = left_members
                .iter()
                .any(|m| is_type_parameter_like(self.ctx.types, *m));
            if has_type_param {
                // Tier 1: Resolve type parameters to constraints
                let resolved: Vec<TypeId> = left_members
                    .iter()
                    .map(|&m| {
                        if is_type_parameter_like(self.ctx.types, m) {
                            tsz_solver::type_queries::get_type_parameter_constraint(
                                self.ctx.types,
                                m,
                            )
                            .unwrap_or(TypeId::UNKNOWN)
                        } else {
                            m
                        }
                    })
                    .collect();
                let resolved_type = self.ctx.types.intersection(resolved);
                return self.types_have_no_overlap(resolved_type, effective_right);
            }
            // Tier 2: Only do per-member overlap when a primitive member exists
            let has_primitive = left_members
                .iter()
                .any(|m| tsz_solver::is_primitive_type(self.ctx.types, *m) || *m == TypeId::OBJECT);
            if has_primitive {
                for member in &left_members {
                    if !self.types_have_no_overlap(*member, effective_right) {
                        return false;
                    }
                }
            }
            // Tier 3: All-object intersections fall through to assignability below
        }
        if let Some(right_members) =
            query::get_intersection_members(self.ctx.types, effective_right)
        {
            let has_type_param = right_members
                .iter()
                .any(|m| is_type_parameter_like(self.ctx.types, *m));
            if has_type_param {
                let resolved: Vec<TypeId> = right_members
                    .iter()
                    .map(|&m| {
                        if is_type_parameter_like(self.ctx.types, m) {
                            tsz_solver::type_queries::get_type_parameter_constraint(
                                self.ctx.types,
                                m,
                            )
                            .unwrap_or(TypeId::UNKNOWN)
                        } else {
                            m
                        }
                    })
                    .collect();
                let resolved_type = self.ctx.types.intersection(resolved);
                return self.types_have_no_overlap(effective_left, resolved_type);
            }
            let has_primitive = right_members
                .iter()
                .any(|m| tsz_solver::is_primitive_type(self.ctx.types, *m) || *m == TypeId::OBJECT);
            if has_primitive {
                for member in &right_members {
                    if !self.types_have_no_overlap(effective_left, *member) {
                        return false;
                    }
                }
            }
        }

        // If either is assignable to the other, they overlap
        let trace_enabled = tracing::enabled!(tracing::Level::TRACE);
        let left_to_right = self.is_assignable_to(effective_left, effective_right);
        let right_to_left = if left_to_right {
            false
        } else {
            self.is_assignable_to(effective_right, effective_left)
        };

        if trace_enabled {
            let left_type_str = self.format_type(effective_left);
            let right_type_str = self.format_type(effective_right);
            tracing::trace!(
                ?effective_left,
                ?effective_right,
                %left_type_str,
                %right_type_str,
                left_to_right,
                right_to_left,
                "assignability check"
            );
        }
        if left_to_right || right_to_left {
            return false;
        }

        // Additional check: Two object types where ALL common named properties are
        // optional always overlap, because both types include the empty object `{}`.
        // Example: `{ b?: number }` and `{ b?: string }` overlap at `{}`.
        // The assignability check misses this because `number` is not assignable to
        // `string` and vice versa, but the types still share `{}` as a common value.
        if self.objects_with_all_optional_common_props_overlap(effective_left, effective_right) {
            tracing::trace!("objects with all-optional common properties overlap");
            return false;
        }

        // Independent property variance: Two object types overlap if ALL common
        // properties have independently overlapping types, even when neither whole
        // type is assignable to the other.
        // Example: { a: 1, b: string } and { a: number, b: 'a' } overlap because:
        //   - a: 1 overlaps with number (1 is a number literal)
        //   - b: string overlaps with 'a' ('a' is a string literal)
        if self.objects_with_independently_overlapping_props(effective_left, effective_right) {
            tracing::trace!("objects with independently overlapping properties");
            return false;
        }

        tracing::trace!("no overlap detected");
        // No other overlap detected
        true
    }

    /// Check if two types are both object types whose properties are ALL optional.
    /// When this is the case, the empty object `{}` satisfies both types,
    /// so they always overlap (and are comparable).
    ///
    /// Example: `{ b?: number }` and `{ b?: string }` — even though `number` and
    /// `string` are incompatible, both types include `{}` (property absent) as a
    /// valid value, so they overlap. Bidirectional assignability misses this.
    ///
    /// Resolves `Lazy(DefId)` types through the type environment before checking.
    pub(crate) fn objects_with_all_optional_common_props_overlap(
        &mut self,
        left: TypeId,
        right: TypeId,
    ) -> bool {
        use crate::query_boundaries::assignability::object_shape_for_type;

        // Resolve lazy types (interfaces, type aliases, etc.) to their concrete shapes
        let left_resolved = self.evaluate_type_with_resolution(left);
        let right_resolved = self.evaluate_type_with_resolution(right);

        let left_shape = match object_shape_for_type(self.ctx.types, left_resolved) {
            Some(s) => s,
            None => return false,
        };
        let right_shape = match object_shape_for_type(self.ctx.types, right_resolved) {
            Some(s) => s,
            None => return false,
        };

        // ALL properties in BOTH types must be optional (both types admit `{}`)
        for lp in &left_shape.properties {
            if !lp.optional {
                return false;
            }
        }
        for rp in &right_shape.properties {
            if !rp.optional {
                return false;
            }
        }

        // At least one type must have properties (avoid trivial empty-object matching)
        !left_shape.properties.is_empty() || !right_shape.properties.is_empty()
    }

    /// Check if two object types have all common properties with independently
    /// overlapping types. In tsc's comparable relation, each property is checked
    /// independently — if every common property's types overlap, the whole types
    /// are comparable even when neither is assignable to the other.
    ///
    /// Example: `{ a: 1, b: string }` and `{ a: number, b: 'a' }` overlap because:
    /// - `a`: `1` overlaps with `number` (1 is a number literal)
    /// - `b`: `string` overlaps with `'a'` ('a' is a string literal)
    fn objects_with_independently_overlapping_props(
        &mut self,
        left: TypeId,
        right: TypeId,
    ) -> bool {
        use crate::query_boundaries::assignability::object_shape_for_type;

        let left_resolved = self.evaluate_type_with_resolution(left);
        let right_resolved = self.evaluate_type_with_resolution(right);

        let left_shape = match object_shape_for_type(self.ctx.types, left_resolved) {
            Some(s) => s,
            None => return false,
        };
        let right_shape = match object_shape_for_type(self.ctx.types, right_resolved) {
            Some(s) => s,
            None => return false,
        };

        // Skip for types with private/protected members — these use nominal
        // compatibility and can never overlap structurally even if property
        // types match (e.g., two classes with `private a: string` are distinct).
        let has_non_public = |shape: &tsz_solver::ObjectShape| {
            shape
                .properties
                .iter()
                .any(|p| p.visibility != tsz_solver::Visibility::Public)
        };
        if has_non_public(&left_shape) || has_non_public(&right_shape) {
            return false;
        }

        // Need at least one common property to compare
        let mut found_common = false;
        for lp in &left_shape.properties {
            for rp in &right_shape.properties {
                if lp.name == rp.name {
                    found_common = true;
                    // If any common property types DON'T overlap, return false
                    if self.types_have_no_overlap(lp.type_id, rp.type_id) {
                        return false;
                    }
                }
            }
        }
        found_common
    }

    /// Get display string for implicit any return type.
    ///
    /// Returns "any" for null/undefined only types, otherwise formats the type.
    pub(crate) fn implicit_any_return_display(&self, return_type: TypeId) -> String {
        if self.is_null_or_undefined_only(return_type) {
            return "any".to_string();
        }
        self.format_type(return_type)
    }

    /// Check if we should report implicit any return type.
    ///
    /// Only reports when return type is exactly 'any', not when it contains 'any' somewhere.
    /// For example, Promise<void> should not trigger TS7010 even if Promise's definition
    /// contains 'any' in its type structure.
    pub(crate) fn should_report_implicit_any_return(&self, return_type: TypeId) -> bool {
        // void is a valid inferred return type (functions with no return statements),
        // it should NOT trigger TS7010.
        if return_type == TypeId::VOID {
            return false;
        }
        // null and undefined are valid inferred return types, not implicit any.
        // tsc does not emit TS7010 for functions that return null/undefined,
        // regardless of strictNullChecks. Only report when the return type is
        // exactly `any`.
        return_type == TypeId::ANY
    }

    // =========================================================================
    // Type Refinement Helper Functions
    // =========================================================================

    /// Refine variable declaration type based on assignment.
    ///
    /// Returns the more specific type when `prev_type` is ANY and `current_type` is concrete.
    /// This implements type refinement for multiple assignments.
    pub(crate) const fn refine_var_decl_type(
        &self,
        prev_type: TypeId,
        current_type: TypeId,
    ) -> TypeId {
        if matches!(prev_type, TypeId::ANY | TypeId::ERROR)
            && !matches!(current_type, TypeId::ANY | TypeId::ERROR)
        {
            return current_type;
        }
        prev_type
    }

    // =========================================================================
    // Property Readonly Helper Functions
    // =========================================================================

    /// Check if a class property is readonly.
    ///
    /// Looks up the class by name, finds the property member declaration,
    /// and checks if it has a readonly modifier.
    pub(crate) fn is_class_property_readonly(&self, class_name: &str, prop_name: &str) -> bool {
        let Some(class_sym_id) = self.get_symbol_by_name(class_name) else {
            return false;
        };
        let Some(class_sym) = self.ctx.binder.get_symbol(class_sym_id) else {
            return false;
        };
        if class_sym.value_declaration.is_none() {
            return false;
        }
        let Some(class_node) = self.ctx.arena.get(class_sym.value_declaration) else {
            return false;
        };
        let Some(class_data) = self.ctx.arena.get_class(class_node) else {
            return false;
        };
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if let Some(prop_decl) = self.ctx.arena.get_property_decl(member_node) {
                let member_name = self.get_identifier_text_from_idx(prop_decl.name);
                if member_name.as_deref() == Some(prop_name) {
                    return self.has_readonly_modifier(&prop_decl.modifiers)
                        || self.jsdoc_has_readonly_tag(member_idx);
                }
            }
        }
        false
    }

    /// Check if an interface property is readonly by looking up the interface declaration in the AST.
    ///
    /// Given a type name (e.g., "I"), finds the interface declaration and checks
    /// if the named property has a readonly modifier.
    pub(crate) fn is_interface_property_readonly(&self, type_name: &str, prop_name: &str) -> bool {
        use tsz_parser::parser::syntax_kind_ext::PROPERTY_SIGNATURE;

        let Some(sym_id) = self.get_symbol_by_name(type_name) else {
            return false;
        };
        let Some(sym) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        // Check all declarations (interfaces can be merged)
        for &decl_idx in &sym.declarations {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(iface_data) = self.ctx.arena.get_interface(decl_node) else {
                continue;
            };
            for &member_idx in &iface_data.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind != PROPERTY_SIGNATURE {
                    continue;
                }
                let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                    continue;
                };
                let member_name = self.get_identifier_text_from_idx(sig.name);
                if member_name.as_deref() == Some(prop_name) {
                    return self.has_readonly_modifier(&sig.modifiers);
                }
            }
        }
        false
    }

    /// Get the declared type name from a variable expression.
    ///
    /// For `declare const obj: I`, given the expression node for `obj`,
    /// returns "I" (the type reference name from the variable's type annotation).
    pub(crate) fn get_declared_type_name_from_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let node = self.ctx.arena.get(expr_idx)?;

        // Must be an identifier
        self.ctx.arena.get_identifier(node)?;

        // Resolve the variable's symbol
        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        let sym = self.ctx.binder.get_symbol(sym_id)?;

        // Get the variable's declaration
        if sym.value_declaration.is_none() {
            return None;
        }
        let decl_node = self.ctx.arena.get(sym.value_declaration)?;
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;

        // Get the type annotation
        if var_decl.type_annotation.is_none() {
            return None;
        }
        let type_node = self.ctx.arena.get(var_decl.type_annotation)?;

        // If it's a type reference, get the name
        if let Some(type_ref) = self.ctx.arena.get_type_ref(type_node) {
            return self.get_identifier_text_from_idx(type_ref.type_name);
        }

        None
    }

    /// Check if a property of a type is readonly.
    ///
    /// Delegates to the solver's comprehensive implementation which handles:
    /// - `ReadonlyType` wrappers (readonly arrays/tuples)
    /// - Object types with readonly properties
    /// - `ObjectWithIndex` types (readonly index signatures)
    /// - Union types (readonly if ANY member has readonly property)
    /// - Intersection types (readonly ONLY if ALL members have readonly property)
    pub(crate) fn is_property_readonly(&self, type_id: TypeId, prop_name: &str) -> bool {
        self.ctx.types.is_property_readonly(type_id, prop_name)
    }

    /// Get the class name from a variable declaration.
    ///
    /// Returns the class name if the variable is initialized with a class expression.
    pub(crate) fn get_class_name_from_var_decl(&self, decl_idx: NodeIndex) -> Option<String> {
        let var_decl = self.ctx.arena.get_variable_declaration_at(decl_idx)?;

        if var_decl.initializer.is_none() {
            return None;
        }

        let init_node = self.ctx.arena.get(var_decl.initializer)?;
        if init_node.kind != syntax_kind_ext::CLASS_EXPRESSION {
            return None;
        }

        let class = self.ctx.arena.get_class(init_node)?;
        if class.name.is_none() {
            return None;
        }

        let ident = self.ctx.arena.get_identifier_at(class.name)?;
        Some(ident.escaped_text.clone())
    }

    // =========================================================================
    // AST Navigation Helper Functions
    // =========================================================================

    /// Get class expression returned from a function body.
    ///
    /// Searches for return statements that return class expressions.
    pub(crate) fn returned_class_expression(&self, body_idx: NodeIndex) -> Option<NodeIndex> {
        if body_idx.is_none() {
            return None;
        }
        let node = self.ctx.arena.get(body_idx)?;
        if node.kind != syntax_kind_ext::BLOCK {
            return self.class_expression_from_expr(body_idx);
        }
        let block = self.ctx.arena.get_block(node)?;
        for &stmt_idx in &block.statements.nodes {
            let stmt = self.ctx.arena.get(stmt_idx)?;
            if stmt.kind != syntax_kind_ext::RETURN_STATEMENT {
                continue;
            }
            let ret = self.ctx.arena.get_return_statement(stmt)?;
            if ret.expression.is_none() {
                continue;
            }
            if let Some(expr_idx) = self.class_expression_from_expr(ret.expression) {
                return Some(expr_idx);
            }
            let expr_node = self.ctx.arena.get(ret.expression)?;
            if let Some(ident) = self.ctx.arena.get_identifier(expr_node)
                && let Some(class_idx) =
                    self.class_declaration_from_identifier_in_block(block, &ident.escaped_text)
            {
                return Some(class_idx);
            }
        }
        None
    }

    /// Find class declaration by identifier name in a block.
    ///
    /// Searches for class declarations with the given name.
    pub(crate) fn class_declaration_from_identifier_in_block(
        &self,
        block: &tsz_parser::parser::node::BlockData,
        name: &str,
    ) -> Option<NodeIndex> {
        for &stmt_idx in &block.statements.nodes {
            let stmt = self.ctx.arena.get(stmt_idx)?;
            if stmt.kind != syntax_kind_ext::CLASS_DECLARATION {
                continue;
            }
            let class = self.ctx.arena.get_class(stmt)?;
            if class.name.is_none() {
                continue;
            }
            let ident = self.ctx.arena.get_identifier_at(class.name)?;
            if ident.escaped_text == name {
                return Some(stmt_idx);
            }
        }
        None
    }

    /// Get class expression from any expression node.
    ///
    /// Unwraps parenthesized expressions and returns the class expression if found.
    pub(crate) fn class_expression_from_expr(&self, expr_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = expr_idx;
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return None;
            }
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                let paren = self.ctx.arena.get_parenthesized(node)?;
                current = paren.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::CLASS_EXPRESSION {
                return Some(current);
            }
            return None;
        }
    }

    /// Get function declaration from callee expression.
    ///
    /// Returns the function declaration if the callee is a function with a body.
    pub(crate) fn function_decl_from_callee(&self, callee_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(callee_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.resolve_identifier_symbol(callee_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;

        for &decl_idx in &symbol.declarations {
            let func = self.ctx.arena.get_function_at(decl_idx)?;
            if func.body.is_some() {
                return Some(decl_idx);
            }
        }

        if symbol.value_declaration.is_some() {
            let decl_idx = symbol.value_declaration;
            let func = self.ctx.arena.get_function_at(decl_idx)?;
            if func.body.is_some() {
                return Some(decl_idx);
            }
        }

        None
    }

    pub(crate) fn function_like_decl_from_callee(
        &mut self,
        callee_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        if let Some(func_decl_idx) = self.function_decl_from_callee(callee_idx) {
            return Some(func_decl_idx);
        }

        let node = self.ctx.arena.get(callee_idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }

        let access = self.ctx.arena.get_access_expr(node)?;
        let name_node = self.ctx.arena.get(access.name_or_argument)?;
        let method_name = self
            .ctx
            .arena
            .get_identifier(name_node)?
            .escaped_text
            .clone();
        let object_type = self.get_type_of_node(access.expression);
        let class_idx = self
            .get_class_decl_for_display_type(object_type)
            .map(|(class_idx, _)| class_idx)
            .or_else(|| {
                let object_node = self.ctx.arena.get(access.expression)?;
                self.ctx.arena.get_identifier(object_node)?;
                let sym_id = self.resolve_identifier_symbol(access.expression)?;
                let symbol = self.ctx.binder.get_symbol(sym_id)?;
                let value_decl = symbol.value_declaration;
                let decl_iter = value_decl
                    .is_some()
                    .then_some(value_decl)
                    .into_iter()
                    .chain(symbol.declarations.iter().copied());
                for decl_idx in decl_iter {
                    let var_decl_idx = if self
                        .ctx
                        .arena
                        .get_variable_declaration_at(decl_idx)
                        .is_some()
                    {
                        Some(decl_idx)
                    } else {
                        let parent_idx = self.ctx.arena.get_extended(decl_idx)?.parent;
                        self.ctx.arena.get(parent_idx).and_then(|parent| {
                            self.ctx
                                .arena
                                .get_variable_declaration(parent)
                                .map(|_| parent_idx)
                        })
                    }?;
                    let var_decl = self.ctx.arena.get_variable_declaration_at(var_decl_idx)?;
                    let init_idx = var_decl.initializer;
                    let init_node = self.ctx.arena.get(init_idx)?;
                    if init_node.kind == syntax_kind_ext::CLASS_EXPRESSION {
                        return Some(init_idx);
                    }
                }
                None
            })?;
        let class_node = self.ctx.arena.get(class_idx)?;
        let class = self.ctx.arena.get_class(class_node)?;

        for &member_idx in &class.members.nodes {
            let Some(member_name) = self.get_member_name(member_idx) else {
                continue;
            };
            if member_name != method_name {
                continue;
            }
            let member_node = self.ctx.arena.get(member_idx)?;
            if member_node.kind != syntax_kind_ext::METHOD_DECLARATION {
                continue;
            }
            if self.ctx.arena.get_method_decl(member_node)?.body.is_some() {
                return Some(member_idx);
            }
        }

        None
    }

    pub(crate) fn returned_class_name_from_body(&self, body_idx: NodeIndex) -> Option<String> {
        if body_idx.is_none() {
            return None;
        }

        let body_node = self.ctx.arena.get(body_idx)?;
        if body_node.kind != syntax_kind_ext::BLOCK {
            let class_expr_idx = self.class_expression_from_expr(body_idx)?;
            return Some(self.get_class_name_from_decl(class_expr_idx));
        }

        let block = self.ctx.arena.get_block(body_node)?;
        for &stmt_idx in &block.statements.nodes {
            let stmt = self.ctx.arena.get(stmt_idx)?;
            if stmt.kind != syntax_kind_ext::RETURN_STATEMENT {
                continue;
            }
            let ret = self.ctx.arena.get_return_statement(stmt)?;
            if ret.expression.is_none() {
                continue;
            }

            if let Some(class_expr_idx) = self.class_expression_from_expr(ret.expression) {
                return Some(self.get_class_name_from_decl(class_expr_idx));
            }

            let expr_node = self.ctx.arena.get(ret.expression)?;
            if expr_node.kind == SyntaxKind::Identifier as u16
                && let Some(sym_id) = self.resolve_identifier_symbol(ret.expression)
                && let Some(class_idx) = self.get_class_declaration_from_symbol(sym_id)
            {
                return Some(self.get_class_name_from_decl(class_idx));
            }
        }

        None
    }

    // ============================================================================
    // Section 58: Enum Type Utilities
    // ============================================================================

    /// Get enum member type by property name.
    ///
    /// This function resolves the type of an enum member accessed by name.
    /// It searches through all enum declarations for the symbol to find
    /// a matching member name and returns the enum type (not the primitive).
    ///
    /// ## Parameters:
    /// - `sym_id`: The enum symbol ID
    /// - `property_name`: The member property name to search for
    ///
    /// ## Returns:
    /// - `Some(TypeId)`: The enum type (as a Ref to the enum symbol)
    /// - `None`: If the symbol is not an enum or member not found
    ///
    /// ## Examples:
    /// ```typescript
    /// enum Color {
    ///   Red,
    ///   Green,
    ///   Blue
    /// }
    /// type T = Color["Red"];  // Returns the enum type Color
    /// ```
    ///
    /// Note: This returns the enum type itself, not STRING or NUMBER,
    /// which allows proper enum assignability checking.
    pub(crate) fn enum_member_type_for_name(
        &mut self,
        sym_id: SymbolId,
        property_name: &str,
    ) -> Option<TypeId> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::ENUM == 0 {
            return None;
        }

        // Check if the property exists in this enum
        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(enum_decl) = self.ctx.arena.get_enum(node) else {
                continue;
            };
            for &member_idx in &enum_decl.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
                    continue;
                };
                if let Some(name) = self.get_property_name(member.name)
                    && name == property_name
                {
                    // Return the enum type itself by getting the computed type of the symbol
                    // This returns TypeData::Enum(def_id, structural_type) which allows proper
                    // enum assignability checking with nominal identity
                    return Some(self.get_type_of_symbol(sym_id));
                }
            }
        }

        None
    }
}
