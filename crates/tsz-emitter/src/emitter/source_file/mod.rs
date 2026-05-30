#[cfg(test)]
mod block_scoped_hoist_void_zero_tests;
mod const_enums;
mod emit;
mod import_helpers_class_scan;
#[cfg(test)]
mod import_helpers_class_scan_tests;
mod recovery;
#[cfg(test)]
mod tc39_decorator_tests;
mod top_level_using;
mod top_level_using_decorated;

#[cfg(test)]
mod class_es5_field_initializer_tests;
#[cfg(test)]
mod class_expr_computed_static_method_naming_tests;
#[cfg(test)]
mod class_expr_es5_interior_comment_tests;
#[cfg(test)]
mod class_expr_object_property_static_naming_tests;
#[cfg(test)]
mod class_expression_decorator_tests;
#[cfg(test)]
mod class_static_alias_tests;
#[cfg(test)]
mod commonjs_export_class_es5_computed_order_tests;
#[cfg(test)]
mod decorator_metadata_tests;
#[cfg(test)]
mod derived_constructor_tests;
#[cfg(test)]
mod es5_object_rest_tests;
#[cfg(test)]
mod legacy_decorator_computed_tests;
#[cfg(test)]
mod legacy_decorator_static_block_self_alias_tests;
#[cfg(test)]
mod private_field_helper_order_tests;
#[cfg(test)]
mod private_tagged_template_tests;

#[cfg(test)]
mod async_arrow_arguments_capture_tests;
#[cfg(test)]
mod empty_statement_comment_elision_tests;
#[cfg(test)]
mod es5_emit_tests;
#[cfg(test)]
mod es5_for_of_destructure_temp_order_tests;
#[cfg(test)]
mod es5_super_recovery_tests;
#[cfg(test)]
mod labeled_for_await_tests;
