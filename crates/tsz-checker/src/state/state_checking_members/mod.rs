//! Declaration and statement checking (member submodules).

mod accessor_accessibility;
mod ambient_constructor_checks;
mod ambient_signature_checks;
mod class_expression_initializers;
mod class_member_checks;
mod class_member_context;
mod class_member_modifiers;
mod class_type_param_checks;
mod decorator_signature_checks;
mod default_export_overload_group;
mod function_declaration_checks;
mod implicit_any_checks;
mod implicit_any_destructuring_nullish_initializer;
mod implicit_any_param_context;
mod index_signature_checks;
#[cfg(test)]
mod index_signature_checks_tests;
mod index_signature_inherited_display;
mod index_signature_key_helpers;
mod index_signature_type_helpers;
mod index_signature_validity;
mod interface_checks;
mod mapped_type_param_provisional;
mod member_access;
mod member_declaration_checks;
mod member_type_constraint_checks;
mod mixin_member_access;
mod overload_compatibility;
mod statement_callback_bridge;
mod statement_checks;
mod statement_helpers;
