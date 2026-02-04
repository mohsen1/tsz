//! Type Printer - Convert TypeId to TypeScript syntax
//!
//! This module handles type reification: converting the Solver's internal TypeId
//! representation into printable TypeScript syntax for declaration emit (.d.ts files).

use crate::interner::Atom;
use crate::solver::TypeInterner;
use crate::solver::types::{TypeId, TypeKey};

/// Prints types as TypeScript syntax for declaration emit.
///
/// # Examples
///
/// ```
/// let printer = TypePrinter::new(&interner);
/// assert_eq!(printer.print_type(TypeId::STRING), "string");
/// assert_eq!(printer.print_type(TypeId::NUMBER), "number");
/// ```
pub struct TypePrinter<'a> {
    interner: &'a TypeInterner,
    string_interner_cache: std::sync::Arc<dyn Fn(Atom) -> String + Sync + Send>,
}

impl<'a> TypePrinter<'a> {
    pub fn new(interner: &'a TypeInterner) -> Self {
        Self {
            interner,
            string_interner_cache: std::sync::Arc::new(|atom| {
                // For now, return a placeholder. In production, this would
                // look up the string from the interner
                format!("<atom:{}>", atom.0)
            }),
        }
    }

    /// Convert a TypeId to TypeScript syntax string.
    pub fn print_type(&self, type_id: TypeId) -> String {
        // Fast path: check built-in intrinsics (TypeId < 100)
        if type_id.is_intrinsic() {
            return self.print_intrinsic_type(type_id);
        }

        // Look up the type structure from the interner
        let type_key = match self.interner.lookup(type_id) {
            Some(key) => key,
            None => return "any".to_string(), // Fallback for missing types
        };

        // Match on the type structure
        match type_key {
            TypeKey::Intrinsic(_) => {
                // Should have been caught by is_intrinsic() check above
                "any".to_string()
            }

            TypeKey::Literal(literal) => self.print_literal(&literal),

            TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                self.print_object_type(shape_id)
            }

            TypeKey::Union(type_list_id) => self.print_union(type_list_id),

            TypeKey::Intersection(type_list_id) => self.print_intersection(type_list_id),

            TypeKey::Array(elem_id) => format!("{}[]", self.print_type(elem_id)),

            TypeKey::Tuple(tuple_id) => self.print_tuple(tuple_id),

            TypeKey::Function(func_id) => self.print_function_type(func_id),

            TypeKey::Callable(callable_id) => self.print_callable(callable_id),

            TypeKey::TypeParameter(param_info) => self.print_type_parameter(&param_info),

            TypeKey::Lazy(def_id) => self.print_lazy_type(def_id),

            TypeKey::Enum(def_id, members_id) => self.print_enum(def_id, members_id),

            TypeKey::Application(app_id) => self.print_type_application(app_id),

            TypeKey::Conditional(cond_id) => self.print_conditional(cond_id),

            TypeKey::TemplateLiteral(template_id) => self.print_template_literal(template_id),

            TypeKey::Mapped(mapped_id) => self.print_mapped_type(mapped_id),

            TypeKey::IndexAccess(container, index) => self.print_index_access(container, index),

            TypeKey::TypeQuery(_) => "any".to_string(),

            TypeKey::KeyOf(type_id) => format!("keyof {}", self.print_type(type_id)),

            TypeKey::ReadonlyType(type_id) => format!("readonly {}", self.print_type(type_id)),

            TypeKey::UniqueSymbol(_) => "unique symbol".to_string(),

            TypeKey::Infer(param_info) => self.print_type_parameter(&param_info),

            TypeKey::ThisType => "this".to_string(),

            TypeKey::StringIntrinsic { kind, type_arg } => {
                self.print_string_intrinsic(kind, type_arg)
            }

            TypeKey::ModuleNamespace(_) => "any".to_string(),

            TypeKey::Error => "any".to_string(),
        }
    }

    fn print_intrinsic_type(&self, type_id: TypeId) -> String {
        match type_id {
            TypeId::ERROR => "any".to_string(), // Errors emit as `any` in declarations
            TypeId::NEVER => "never".to_string(),
            TypeId::UNKNOWN => "unknown".to_string(),
            TypeId::ANY => "any".to_string(),
            TypeId::VOID => "void".to_string(),
            TypeId::UNDEFINED => "undefined".to_string(),
            TypeId::NULL => "null".to_string(),
            TypeId::BOOLEAN => "boolean".to_string(),
            TypeId::NUMBER => "number".to_string(),
            TypeId::STRING => "string".to_string(),
            TypeId::BIGINT => "bigint".to_string(),
            TypeId::SYMBOL => "symbol".to_string(),
            TypeId::OBJECT => "object".to_string(),
            TypeId::FUNCTION => "Function".to_string(),
            TypeId::BOOLEAN_TRUE => "true".to_string(),
            TypeId::BOOLEAN_FALSE => "false".to_string(),
            _ => "any".to_string(),
        }
    }

    fn print_literal(&self, literal: &crate::solver::types::LiteralValue) -> String {
        match literal {
            crate::solver::types::LiteralValue::String(atom) => {
                // TODO: Look up actual string from interner
                format!("\"{}\"", (self.string_interner_cache)(*atom))
            }
            crate::solver::types::LiteralValue::Number(n) => n.0.to_string(),
            crate::solver::types::LiteralValue::Boolean(b) => b.to_string(),
            crate::solver::types::LiteralValue::BigInt(atom) => {
                // TODO: Look up actual string from interner
                format!("{}n", (self.string_interner_cache)(*atom))
            }
        }
    }

    fn print_object_type(&self, shape_id: crate::solver::types::ObjectShapeId) -> String {
        let shape = self.interner.object_shape(shape_id);

        if shape.properties.is_empty() {
            return "{}".to_string();
        }

        let mut members = Vec::new();
        for property in shape.properties.iter() {
            let mut member = String::new();

            // Property name
            member.push_str(&format!("<atom:{}>", property.name.0));

            // Optional marker
            if property.optional {
                member.push('?');
            }

            // Property type
            member.push_str(": ");
            member.push_str(&self.print_type(property.type_id));

            members.push(member);
        }

        format!("{{ {} }}", members.join("; "))
    }

    fn print_union(&self, type_list_id: crate::solver::types::TypeListId) -> String {
        let types = self.interner.type_list(type_list_id);
        if types.is_empty() {
            return "never".to_string();
        }

        let mut parts = Vec::with_capacity(types.len());
        for &type_id in types.iter() {
            parts.push(self.print_type(type_id));
        }

        // Join with " | "
        parts.join(" | ")
    }

    fn print_intersection(&self, type_list_id: crate::solver::types::TypeListId) -> String {
        let types = self.interner.type_list(type_list_id);
        if types.is_empty() {
            return "unknown".to_string(); // Intersection of 0 types is unknown
        }

        let mut parts = Vec::with_capacity(types.len());
        for &type_id in types.iter() {
            parts.push(self.print_type(type_id));
        }

        // Join with " & "
        parts.join(" & ")
    }

    fn print_tuple(&self, tuple_id: crate::solver::types::TupleListId) -> String {
        let elements = self.interner.tuple_list(tuple_id);

        if elements.is_empty() {
            return "[]".to_string();
        }

        let mut parts = Vec::with_capacity(elements.len());
        for elem in elements.iter() {
            let type_str = self.print_type(elem.type_id);

            // Handle optional properties
            if elem.optional {
                parts.push(format!("{}?", type_str));
            } else {
                parts.push(type_str);
            }

            // Handle rest elements
            if elem.rest {
                // Remove last element and add ... prefix
                if let Some(last) = parts.pop() {
                    parts.push(format!("...{}", last));
                }
            }
        }

        format!("[{}]", parts.join(", "))
    }

    fn print_function_type(&self, func_id: crate::solver::types::FunctionShapeId) -> String {
        let func_shape = self.interner.function_shape(func_id);

        // Type parameters
        let type_params_str = if !func_shape.type_params.is_empty() {
            let params: Vec<String> = func_shape
                .type_params
                .iter()
                .map(|tp| self.print_type_parameter(tp))
                .collect();
            format!("<{}>", params.join(", "))
        } else {
            String::new()
        };

        // Parameters
        let mut params = Vec::new();
        for param in &func_shape.params {
            let mut param_str = String::new();

            // Rest parameter
            if param.rest {
                param_str.push_str("...");
            }

            // Parameter name (optional in function types)
            if let Some(name) = param.name {
                param_str.push_str(&format!("<atom:{}>", name.0));
                param_str.push_str(": ");
            }

            // Parameter type
            param_str.push_str(&self.print_type(param.type_id));

            // Optional parameter
            if param.optional {
                param_str.push('?');
            }

            params.push(param_str);
        }

        // Return type
        let return_type = self.print_type(func_shape.return_type);

        format!(
            "{}({}) => {}",
            type_params_str,
            params.join(", "),
            return_type
        )
    }

    fn print_callable(&self, _callable_id: crate::solver::types::CallableShapeId) -> String {
        // TODO: Implement callable type printing (for overloaded call signatures)
        // For now, treat as a simple function type
        "Function".to_string()
    }

    fn print_type_parameter(&self, param_info: &crate::solver::types::TypeParamInfo) -> String {
        // Type parameter names are Atoms
        format!("<atom:{}>", param_info.name.0)
    }

    fn print_lazy_type(&self, _def_id: crate::solver::def::DefId) -> String {
        // TODO: Implement lazy type resolution
        "any".to_string()
    }

    fn print_enum(&self, _def_id: crate::solver::def::DefId, _members_id: TypeId) -> String {
        // TODO: Implement enum type printing
        "any".to_string()
    }

    fn print_type_application(&self, app_id: crate::solver::types::TypeApplicationId) -> String {
        let app = self.interner.type_application(app_id);

        if app.args.is_empty() {
            self.print_type(app.base)
        } else {
            let args: Vec<String> = app.args.iter().map(|&id| self.print_type(id)).collect();
            format!("{}<{}>", self.print_type(app.base), args.join(", "))
        }
    }

    fn print_conditional(&self, _cond_id: crate::solver::types::ConditionalTypeId) -> String {
        // TODO: Implement conditional type printing
        "any".to_string()
    }

    fn print_template_literal(
        &self,
        _template_id: crate::solver::types::TemplateLiteralId,
    ) -> String {
        // TODO: Implement template literal type printing
        "string".to_string()
    }

    fn print_mapped_type(&self, _mapped_id: crate::solver::types::MappedTypeId) -> String {
        // TODO: Implement mapped type printing
        "any".to_string()
    }

    fn print_index_access(&self, _container: TypeId, _index: TypeId) -> String {
        // TODO: Implement index access type printing
        "any".to_string()
    }

    fn print_string_intrinsic(
        &self,
        _kind: crate::solver::types::StringIntrinsicKind,
        _type_arg: TypeId,
    ) -> String {
        // TODO: Implement string intrinsic type printing
        "string".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitive_types() {
        // For now we can't easily test without a real TypeInterner
        // In the future we'll need to set up a mock or test fixture
        assert_eq!(TypeId::STRING.is_intrinsic(), true);
        assert_eq!(TypeId::NUMBER.is_intrinsic(), true);
        assert_eq!(TypeId::BOOLEAN.is_intrinsic(), true);
    }
}
