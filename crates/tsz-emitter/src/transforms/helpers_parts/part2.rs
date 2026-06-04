#[cfg(test)]
mod tests {
    use super::*;

    /// Setter callback used by the per-flag `any_needed` test.
    type FlagSetter = fn(&mut HelpersNeeded);

    // -----------------------------------------------------------------
    // HelpersNeeded::any_needed
    // -----------------------------------------------------------------

    #[test]
    fn default_helpers_needed_is_false() {
        let helpers = HelpersNeeded::default();
        assert!(!helpers.any_needed());
        assert!(helpers.needed_names().is_empty());
        assert!(emit_helpers(&helpers).is_empty());
    }

    #[test]
    fn any_needed_flips_for_each_individual_flag() {
        // Each individual flag, when set in isolation, must flip `any_needed`
        // to true. This guards against `any_needed` forgetting to OR a new
        // field after someone adds a new helper to `HelpersNeeded`.
        let setters: &[(&str, FlagSetter)] = &[
            ("extends", |h| h.extends = true),
            ("assign", |h| h.assign = true),
            ("rest", |h| h.rest = true),
            ("decorate", |h| h.decorate = true),
            ("param", |h| h.param = true),
            ("metadata", |h| h.metadata = true),
            ("awaiter", |h| h.awaiter = true),
            ("generator", |h| h.generator = true),
            ("values", |h| h.values = true),
            ("read", |h| h.read = true),
            ("spread_array", |h| h.spread_array = true),
            ("async_values", |h| h.async_values = true),
            ("async_generator", |h| h.async_generator = true),
            ("async_delegator", |h| h.async_delegator = true),
            ("await_helper", |h| h.await_helper = true),
            ("export_star", |h| h.export_star = true),
            ("import_default", |h| h.import_default = true),
            ("import_star", |h| h.import_star = true),
            ("make_template_object", |h| h.make_template_object = true),
            ("class_private_field_get", |h| {
                h.class_private_field_get = true
            }),
            ("class_private_field_set", |h| {
                h.class_private_field_set = true
            }),
            ("class_private_field_in", |h| {
                h.class_private_field_in = true
            }),
            ("create_binding", |h| h.create_binding = true),
            ("add_disposable_resource", |h| {
                h.add_disposable_resource = true
            }),
            ("dispose_resources", |h| h.dispose_resources = true),
            ("es_decorate", |h| h.es_decorate = true),
            ("run_initializers", |h| h.run_initializers = true),
            ("prop_key", |h| h.prop_key = true),
            ("set_function_name", |h| h.set_function_name = true),
            ("rewrite_relative_import_extension", |h| {
                h.rewrite_relative_import_extension = true;
            }),
        ];

        for (name, setter) in setters {
            let mut helpers = HelpersNeeded::default();
            setter(&mut helpers);
            assert!(
                helpers.any_needed(),
                "any_needed() should be true when only `{name}` is set",
            );
        }
    }

    #[test]
    fn class_private_field_set_before_get_alone_does_not_trigger_any_needed() {
        // The ordering flag is bookkeeping for emit ordering only — by itself
        // it should NOT make any_needed() return true, otherwise the emitter
        // would erroneously install a tslib import for a no-op state.
        let helpers = HelpersNeeded {
            class_private_field_set_before_get: true,
            ..HelpersNeeded::default()
        };
        assert!(!helpers.any_needed());
        assert!(helpers.needed_names().is_empty());
    }

    // -----------------------------------------------------------------
    // HelpersNeeded::needed_names
    // -----------------------------------------------------------------

    #[test]
    fn needed_names_returns_canonical_helper_strings() {
        let helpers = HelpersNeeded {
            extends: true,
            assign: true,
            awaiter: true,
            ..HelpersNeeded::default()
        };

        let names = helpers.needed_names();
        assert_eq!(names, vec!["__extends", "__assign", "__awaiter"]);
    }

    #[test]
    fn needed_names_priority_order_for_full_set() {
        // When every flag is set, `needed_names` must produce the names in
        // the documented `compareEmitHelpers` priority order.
        let helpers = HelpersNeeded {
            extends: true,
            make_template_object: true,
            assign: true,
            create_binding: true,
            decorate: true,
            es_decorate: true,
            run_initializers: true,
            import_star: true,
            export_star: true,
            metadata: true,
            param: true,
            awaiter: true,
            generator: true,
            await_helper: true,
            async_generator: true,
            async_delegator: true,
            rest: true,
            values: true,
            read: true,
            spread_array: true,
            async_values: true,
            import_default: true,
            class_private_field_get: true,
            class_private_field_set: true,
            class_private_field_set_before_get: false,
            class_private_field_in: true,
            add_disposable_resource: true,
            dispose_resources: true,
            prop_key: true,
            set_function_name: true,
            rewrite_relative_import_extension: true,
            ..HelpersNeeded::default()
        };

        let names = helpers.needed_names();
        // Lock the full canonical order. This regression-catches a missing
        // entry, an out-of-order entry, or a duplicate.
        assert_eq!(
            names,
            vec![
                "__extends",
                "__makeTemplateObject",
                "__assign",
                "__createBinding",
                "__decorate",
                "__esDecorate",
                "__runInitializers",
                "__importStar",
                "__exportStar",
                "__metadata",
                "__param",
                "__awaiter",
                "__generator",
                "__addDisposableResource",
                "__disposeResources",
                "__propKey",
                "__setFunctionName",
                "__await",
                "__asyncGenerator",
                "__asyncDelegator",
                "__rest",
                "__values",
                "__read",
                "__spreadArray",
                "__asyncValues",
                "__importDefault",
                "__classPrivateFieldGet",
                "__classPrivateFieldSet",
                "__classPrivateFieldIn",
                "__rewriteRelativeImportExtension",
            ],
        );
    }

    #[test]
    fn needed_names_skips_unset_flags() {
        let helpers = HelpersNeeded {
            assign: true,
            spread_array: true,
            ..HelpersNeeded::default()
        };

        let names = helpers.needed_names();
        // Only the two set helpers, in priority order (`__assign` is priority
        // 1, `__spreadArray` is unprioritized so it comes later).
        assert_eq!(names, vec!["__assign", "__spreadArray"]);
    }

    // -----------------------------------------------------------------
    // emit_helpers priority ordering
    // -----------------------------------------------------------------

    /// Find the byte offset of a helper's `var __name` declaration in the
    /// emitted source. Asserts the helper is present.
    fn find_helper(output: &str, name: &str) -> usize {
        let needle = format!("var {name} ");
        output
            .find(&needle)
            .unwrap_or_else(|| panic!("expected `{needle}` in emit_helpers output:\n{output}"))
    }

    #[test]
    fn emit_helpers_priority_order_extends_assign_decorate_metadata_param_awaiter_generator() {
        // tsc priority order (from helpers.rs doc-comment):
        //   0: extends, makeTemplateObject
        //   1: assign, createBinding
        //   2: decorate, esDecorate, runInitializers, importStar, exportStar
        //   3: metadata
        //   4: param
        //   5: awaiter
        //   6: generator
        let helpers = HelpersNeeded {
            extends: true,
            assign: true,
            decorate: true,
            metadata: true,
            param: true,
            awaiter: true,
            generator: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);

        let i_extends = find_helper(&output, "__extends");
        let i_assign = find_helper(&output, "__assign");
        let i_decorate = find_helper(&output, "__decorate");
        let i_metadata = find_helper(&output, "__metadata");
        let i_param = find_helper(&output, "__param");
        let i_awaiter = find_helper(&output, "__awaiter");
        let i_generator = find_helper(&output, "__generator");

        assert!(
            i_extends < i_assign
                && i_assign < i_decorate
                && i_decorate < i_metadata
                && i_metadata < i_param
                && i_param < i_awaiter
                && i_awaiter < i_generator,
            "priority order broken: extends={i_extends} assign={i_assign} \
             decorate={i_decorate} metadata={i_metadata} param={i_param} \
             awaiter={i_awaiter} generator={i_generator}\noutput:\n{output}",
        );
    }

    #[test]
    fn emit_helpers_priority_zero_extends_before_make_template_object() {
        // Both share priority 0; doc-comment + emit_helpers source state that
        // extends comes first within priority 0 (matches tsc factory order).
        let helpers = HelpersNeeded {
            extends: true,
            make_template_object: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        let i_extends = find_helper(&output, "__extends");
        let i_make = find_helper(&output, "__makeTemplateObject");
        assert!(i_extends < i_make);
    }

    #[test]
    fn emit_helpers_priority_one_assign_before_create_binding() {
        // Both share priority 1; assign first.
        let helpers = HelpersNeeded {
            assign: true,
            create_binding: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        let i_assign = find_helper(&output, "__assign");
        let i_create = find_helper(&output, "__createBinding");
        assert!(i_assign < i_create);
    }

    #[test]
    fn emit_helpers_order_decorators_and_async_helpers() {
        // emit_helpers source orders priority-2 helpers as:
        //   decorate, esDecorate, runInitializers,
        //   propKey, importStar (with setModuleDefault),
        //   rewriteRelativeImportExtension, exportStar
        // and places setFunctionName after awaiter/generator but before
        // async-generator helpers.
        let helpers = HelpersNeeded {
            decorate: true,
            run_initializers: true,
            es_decorate: true,
            set_function_name: true,
            prop_key: true,
            awaiter: true,
            generator: true,
            await_helper: true,
            async_generator: true,
            import_star: true,
            rewrite_relative_import_extension: true,
            export_star: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        let i_decorate = find_helper(&output, "__decorate");
        let i_es_decorate = find_helper(&output, "__esDecorate");
        let i_run = find_helper(&output, "__runInitializers");
        let i_set_name = find_helper(&output, "__setFunctionName");
        let i_prop_key = find_helper(&output, "__propKey");
        let i_awaiter = find_helper(&output, "__awaiter");
        let i_generator = find_helper(&output, "__generator");
        let i_await = find_helper(&output, "__await");
        let i_async_generator = find_helper(&output, "__asyncGenerator");
        let i_import_star = find_helper(&output, "__importStar");
        let i_rewrite = find_helper(&output, "__rewriteRelativeImportExtension");
        let i_export_star = find_helper(&output, "__exportStar");

        assert!(i_decorate < i_es_decorate);
        assert!(i_es_decorate < i_run);
        assert!(i_run < i_prop_key);
        assert!(i_prop_key < i_import_star);
        assert!(i_import_star < i_rewrite);
        assert!(i_rewrite < i_export_star);
        assert!(i_export_star < i_awaiter);
        assert!(i_awaiter < i_generator);
        assert!(i_generator < i_set_name);
        assert!(i_set_name < i_await);
        assert!(i_await < i_async_generator);
    }

    #[test]
    fn emit_helpers_order_class_private_before_set_function_name() {
        let mut helpers = HelpersNeeded {
            set_function_name: true,
            await_helper: true,
            ..HelpersNeeded::default()
        };
        helpers.mark_class_private_field_get();

        let output = emit_helpers(&helpers);
        let i_get = find_helper(&output, "__classPrivateFieldGet");
        let i_set_name = find_helper(&output, "__setFunctionName");
        let i_await = find_helper(&output, "__await");

        assert!(i_get < i_set_name);
        assert!(i_set_name < i_await);
    }

    #[test]
    fn emit_helpers_order_tc39_set_function_name_before_private_helpers() {
        let mut helpers = HelpersNeeded {
            es_decorate: true,
            set_function_name: true,
            ..HelpersNeeded::default()
        };
        helpers.mark_class_private_field_in();
        helpers.mark_class_private_field_get();
        helpers.mark_class_private_field_set();

        let output = emit_helpers(&helpers);
        let i_es_decorate = find_helper(&output, "__esDecorate");
        let i_set_name = find_helper(&output, "__setFunctionName");
        let i_in = find_helper(&output, "__classPrivateFieldIn");
        let i_get = find_helper(&output, "__classPrivateFieldGet");
        let i_set = find_helper(&output, "__classPrivateFieldSet");

        assert!(i_es_decorate < i_set_name);
        assert!(i_set_name < i_in);
        assert!(i_in < i_get);
        assert!(i_get < i_set);
    }

    #[test]
    fn emit_helpers_orders_member_decorator_initializers_before_es_decorate() {
        let helpers = HelpersNeeded {
            run_initializers: true,
            run_initializers_before_es_decorate: true,
            es_decorate: true,
            set_function_name: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        let i_run = find_helper(&output, "__runInitializers");
        let i_es_decorate = find_helper(&output, "__esDecorate");
        let i_set_name = find_helper(&output, "__setFunctionName");

        assert!(i_run < i_es_decorate);
        assert!(i_es_decorate < i_set_name);
    }

    #[test]
    fn emit_helpers_no_priority_block_emits_last() {
        // The unprioritized block should emit AFTER any prioritized helper.
        let helpers = HelpersNeeded {
            // Priority 6 (last prioritized).
            generator: true,
            // Unprioritized block:
            rest: true,
            values: true,
            read: true,
            spread_array: true,
            import_default: true,
            async_values: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        let i_generator = find_helper(&output, "__generator");
        let i_rest = find_helper(&output, "__rest");
        let i_values = find_helper(&output, "__values");
        let i_read = find_helper(&output, "__read");
        let i_spread = find_helper(&output, "__spreadArray");
        let i_import_default = find_helper(&output, "__importDefault");
        let i_async_values = find_helper(&output, "__asyncValues");

        assert!(i_generator < i_rest, "generator must precede rest");
        assert!(i_rest < i_values);
        assert!(i_values < i_read);
        assert!(i_read < i_spread);
        assert!(i_spread < i_import_default);
        // async_values is emitted after disposable helpers, which are also
        // unprioritized; in this configuration without disposable helpers,
        // async_values still comes after import_default.
        assert!(i_import_default < i_async_values);
    }

    #[test]
    fn emit_helpers_unprioritized_helpers_follow_first_request_order() {
        // tsc keeps same-priority helpers in request order. Object rest can be
        // seen before async-generator helpers in the source walk.
        let mut helpers = HelpersNeeded::default();
        helpers.mark_rest();
        helpers.mark_await_helper();
        helpers.mark_async_generator();

        let output = emit_helpers(&helpers);
        let i_rest = find_helper(&output, "__rest");
        let i_await = find_helper(&output, "__await");
        let i_async_generator = find_helper(&output, "__asyncGenerator");
        assert!(i_rest < i_await);
        assert!(i_await < i_async_generator);
    }

    #[test]
    fn emit_helpers_private_field_before_rest_when_requested_first() {
        let mut helpers = HelpersNeeded::default();
        helpers.mark_class_private_field_get();
        helpers.mark_rest();

        let output = emit_helpers(&helpers);
        let i_get = find_helper(&output, "__classPrivateFieldGet");
        let i_rest = find_helper(&output, "__rest");
        assert!(i_get < i_rest);
    }

    #[test]
    fn emit_helpers_read_precedes_spread_array_when_requested_after_spread() {
        let mut helpers = HelpersNeeded::default();
        helpers.mark_spread_array();
        helpers.mark_read();

        let output = emit_helpers(&helpers);
        let i_read = find_helper(&output, "__read");
        let i_spread = find_helper(&output, "__spreadArray");
        assert!(i_read < i_spread);
        assert_eq!(helpers.needed_names(), vec!["__read", "__spreadArray"]);
    }

    #[test]
    fn emit_helpers_async_values_precedes_spread_array_when_requested_after_spread() {
        let mut helpers = HelpersNeeded::default();
        helpers.mark_spread_array();
        helpers.mark_async_values();

        let output = emit_helpers(&helpers);
        let i_async_values = find_helper(&output, "__asyncValues");
        let i_spread = find_helper(&output, "__spreadArray");
        assert!(i_async_values < i_spread);
        assert_eq!(
            helpers.needed_names(),
            vec!["__asyncValues", "__spreadArray"]
        );
    }

    #[test]
    fn needed_names_tracks_unprioritized_first_request_order() {
        let mut helpers = HelpersNeeded {
            assign: true,
            ..HelpersNeeded::default()
        };
        helpers.mark_rest();
        helpers.mark_import_default();
        helpers.mark_read();

        assert_eq!(
            helpers.needed_names(),
            vec!["__assign", "__rest", "__importDefault", "__read"],
        );
    }

    #[test]
    fn emit_helpers_class_private_field_default_get_before_set() {
        // Default ordering: Get before Set.
        let helpers = HelpersNeeded {
            class_private_field_get: true,
            class_private_field_set: true,
            class_private_field_set_before_get: false,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        let i_get = find_helper(&output, "__classPrivateFieldGet");
        let i_set = find_helper(&output, "__classPrivateFieldSet");
        assert!(
            i_get < i_set,
            "default order should put Get before Set, got Get={i_get} Set={i_set}",
        );
    }

    #[test]
    fn emit_helpers_class_private_field_set_before_get_flips_order() {
        // When set_before_get is true (set was registered first), Set emits
        // before Get.
        let helpers = HelpersNeeded {
            class_private_field_get: true,
            class_private_field_set: true,
            class_private_field_set_before_get: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        let i_get = find_helper(&output, "__classPrivateFieldGet");
        let i_set = find_helper(&output, "__classPrivateFieldSet");
        assert!(
            i_set < i_get,
            "set_before_get=true should put Set before Get, got Get={i_get} Set={i_set}",
        );
    }

    #[test]
    fn emit_helpers_class_private_field_set_before_get_only_set_emits_only_set() {
        // Even with set_before_get=true, if only Set is needed, only Set is emitted.
        let helpers = HelpersNeeded {
            class_private_field_set: true,
            class_private_field_set_before_get: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        assert!(output.contains("var __classPrivateFieldSet"));
        assert!(!output.contains("var __classPrivateFieldGet"));
    }

    #[test]
    fn emit_helpers_import_star_emits_set_module_default_before_import_star() {
        // import_star=true should emit BOTH __setModuleDefault and __importStar,
        // with __setModuleDefault first (since __importStar references it).
        let helpers = HelpersNeeded {
            import_star: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        let i_set_default = find_helper(&output, "__setModuleDefault");
        let i_import_star = find_helper(&output, "__importStar");
        assert!(
            i_set_default < i_import_star,
            "__setModuleDefault must precede __importStar (referenced by it)",
        );
    }

    #[test]
    fn emit_helpers_each_helper_terminated_by_newline() {
        // Each emitted helper string is followed by a newline so consecutive
        // helpers don't run together on the same line.
        let helpers = HelpersNeeded {
            extends: true,
            assign: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        // The output should end with a newline.
        assert!(output.ends_with('\n'));
        // It should contain both helper bodies.
        assert!(output.contains("var __extends"));
        assert!(output.contains("var __assign"));
        // And each `var __` line should be at the start of a line (preceded
        // by a newline) except the very first one.
        let first = output.find("var __extends").expect("__extends present");
        let assign_pos = output.find("var __assign").expect("__assign present");
        // The byte before `var __assign` must be a newline.
        assert_eq!(&output[assign_pos - 1..assign_pos], "\n");
        // First helper starts at offset 0.
        assert_eq!(first, 0);
    }

    #[test]
    fn emit_helpers_disposable_resource_pair_ordered_add_before_dispose() {
        // add_disposable_resource emits before dispose_resources.
        let helpers = HelpersNeeded {
            add_disposable_resource: true,
            dispose_resources: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        let i_add = find_helper(&output, "__addDisposableResource");
        let i_dispose = find_helper(&output, "__disposeResources");
        assert!(i_add < i_dispose);
    }

    // -----------------------------------------------------------------
    // Helper constant invariants
    // -----------------------------------------------------------------

    /// Every public helper constant should be a non-empty `var __<name>` JS
    /// declaration so that `emit_helpers` produces valid JavaScript when the
    /// constant is concatenated.
    #[test]
    fn helper_constants_are_var_declarations() {
        let cases: &[(&str, &str)] = &[
            ("EXTENDS_HELPER", EXTENDS_HELPER),
            ("ASSIGN_HELPER", ASSIGN_HELPER),
            ("REST_HELPER", REST_HELPER),
            ("DECORATE_HELPER", DECORATE_HELPER),
            ("PARAM_HELPER", PARAM_HELPER),
            ("METADATA_HELPER", METADATA_HELPER),
            ("AWAITER_HELPER", AWAITER_HELPER),
            ("GENERATOR_HELPER", GENERATOR_HELPER),
            ("VALUES_HELPER", VALUES_HELPER),
            ("AWAIT_HELPER", AWAIT_HELPER),
            ("ASYNC_GENERATOR_HELPER", ASYNC_GENERATOR_HELPER),
            ("ASYNC_DELEGATOR_HELPER", ASYNC_DELEGATOR_HELPER),
            ("ASYNC_VALUES_HELPER", ASYNC_VALUES_HELPER),
            ("READ_HELPER", READ_HELPER),
            ("SPREAD_ARRAY_HELPER", SPREAD_ARRAY_HELPER),
            ("IMPORT_DEFAULT_HELPER", IMPORT_DEFAULT_HELPER),
            ("IMPORT_STAR_HELPER", IMPORT_STAR_HELPER),
            ("EXPORT_STAR_HELPER", EXPORT_STAR_HELPER),
            ("MAKE_TEMPLATE_OBJECT_HELPER", MAKE_TEMPLATE_OBJECT_HELPER),
            (
                "CLASS_PRIVATE_FIELD_GET_HELPER",
                CLASS_PRIVATE_FIELD_GET_HELPER,
            ),
            (
                "CLASS_PRIVATE_FIELD_SET_HELPER",
                CLASS_PRIVATE_FIELD_SET_HELPER,
            ),
            (
                "CLASS_PRIVATE_FIELD_IN_HELPER",
                CLASS_PRIVATE_FIELD_IN_HELPER,
            ),
            ("CREATE_BINDING_HELPER", CREATE_BINDING_HELPER),
            ("SET_MODULE_DEFAULT_HELPER", SET_MODULE_DEFAULT_HELPER),
            (
                "ADD_DISPOSABLE_RESOURCE_HELPER",
                ADD_DISPOSABLE_RESOURCE_HELPER,
            ),
            ("DISPOSE_RESOURCES_HELPER", DISPOSE_RESOURCES_HELPER),
            ("ES_DECORATE_HELPER", ES_DECORATE_HELPER),
            ("RUN_INITIALIZERS_HELPER", RUN_INITIALIZERS_HELPER),
            ("PROP_KEY_HELPER", PROP_KEY_HELPER),
            ("SET_FUNCTION_NAME_HELPER", SET_FUNCTION_NAME_HELPER),
            (
                "REWRITE_RELATIVE_IMPORT_EXTENSION_HELPER",
                REWRITE_RELATIVE_IMPORT_EXTENSION_HELPER,
            ),
        ];

        for (name, body) in cases {
            assert!(!body.is_empty(), "{name} should not be empty");
            assert!(
                body.starts_with("var __"),
                "{name} should start with `var __`, got: {head:?}",
                head = &body[..body.len().min(20)],
            );
        }
    }

    #[test]
    fn helper_constants_match_needed_names_basenames() {
        // The helper constant body should declare a function whose name
        // matches `__<base>` for the `needed_names` entry that triggers it.
        // Spot-check the priority-0/1 helpers + the async block.
        let pairs: &[(&str, &str)] = &[
            ("__extends", EXTENDS_HELPER),
            ("__makeTemplateObject", MAKE_TEMPLATE_OBJECT_HELPER),
            ("__assign", ASSIGN_HELPER),
            ("__createBinding", CREATE_BINDING_HELPER),
            ("__decorate", DECORATE_HELPER),
            ("__esDecorate", ES_DECORATE_HELPER),
            ("__runInitializers", RUN_INITIALIZERS_HELPER),
            ("__metadata", METADATA_HELPER),
            ("__param", PARAM_HELPER),
            ("__awaiter", AWAITER_HELPER),
            ("__generator", GENERATOR_HELPER),
            ("__await", AWAIT_HELPER),
            ("__asyncGenerator", ASYNC_GENERATOR_HELPER),
            ("__asyncDelegator", ASYNC_DELEGATOR_HELPER),
            ("__asyncValues", ASYNC_VALUES_HELPER),
            ("__rest", REST_HELPER),
            ("__values", VALUES_HELPER),
            ("__read", READ_HELPER),
            ("__spreadArray", SPREAD_ARRAY_HELPER),
            ("__importDefault", IMPORT_DEFAULT_HELPER),
            ("__importStar", IMPORT_STAR_HELPER),
            ("__exportStar", EXPORT_STAR_HELPER),
            ("__classPrivateFieldGet", CLASS_PRIVATE_FIELD_GET_HELPER),
            ("__classPrivateFieldSet", CLASS_PRIVATE_FIELD_SET_HELPER),
            ("__classPrivateFieldIn", CLASS_PRIVATE_FIELD_IN_HELPER),
            ("__addDisposableResource", ADD_DISPOSABLE_RESOURCE_HELPER),
            ("__disposeResources", DISPOSE_RESOURCES_HELPER),
            ("__propKey", PROP_KEY_HELPER),
            ("__setFunctionName", SET_FUNCTION_NAME_HELPER),
            (
                "__rewriteRelativeImportExtension",
                REWRITE_RELATIVE_IMPORT_EXTENSION_HELPER,
            ),
            ("__setModuleDefault", SET_MODULE_DEFAULT_HELPER),
        ];

        for (name, body) in pairs {
            let expected_prefix = format!("var {name} ");
            assert!(
                body.starts_with(&expected_prefix),
                "{name} body should start with `{expected_prefix}`, got: {head:?}",
                head = &body[..body.len().min(50)],
            );
        }
    }

    #[test]
    fn emit_helpers_round_trips_through_string_concat() {
        // emit_helpers output must contain each requested helper exactly
        // once (no accidental duplicate emission).
        let helpers = HelpersNeeded {
            extends: true,
            assign: true,
            awaiter: true,
            generator: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        for marker in [
            "var __extends",
            "var __assign",
            "var __awaiter",
            "var __generator",
        ] {
            assert_eq!(
                output.matches(marker).count(),
                1,
                "marker `{marker}` should appear exactly once in output:\n{output}",
            );
        }
    }

    #[test]
    fn emit_helpers_only_emits_requested_helpers() {
        let helpers = HelpersNeeded {
            extends: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        assert!(output.contains("var __extends"));
        // Spot-check a few helpers that should NOT be present.
        assert!(!output.contains("var __assign"));
        assert!(!output.contains("var __awaiter"));
        assert!(!output.contains("var __decorate"));
        assert!(!output.contains("var __setModuleDefault"));
    }

    // -----------------------------------------------------------------
    // any_needed × needed_names × emit_helpers triangle
    // -----------------------------------------------------------------

    #[test]
    fn any_needed_implies_non_empty_needed_names_and_emit() {
        // For every individual flag that flips any_needed, both
        // needed_names() and emit_helpers() must produce non-empty output.
        let setters: &[fn(&mut HelpersNeeded)] = &[
            |h| h.extends = true,
            |h| h.assign = true,
            |h| h.awaiter = true,
            |h| h.generator = true,
            |h| h.import_star = true,
            |h| h.class_private_field_get = true,
            |h| h.dispose_resources = true,
            |h| h.rewrite_relative_import_extension = true,
        ];

        for setter in setters {
            let mut helpers = HelpersNeeded::default();
            setter(&mut helpers);
            assert!(helpers.any_needed());
            assert!(!helpers.needed_names().is_empty());
            assert!(!emit_helpers(&helpers).is_empty());
        }
    }

    #[test]
    fn helpers_needed_clone_round_trips() {
        // HelpersNeeded derives Clone; ensure cloning preserves all flags.
        let original = HelpersNeeded {
            extends: true,
            class_private_field_set_before_get: true,
            generator: true,
            ..HelpersNeeded::default()
        };
        let cloned = original.clone();
        assert_eq!(cloned.extends, original.extends);
        assert_eq!(
            cloned.class_private_field_set_before_get,
            original.class_private_field_set_before_get,
        );
        assert_eq!(cloned.generator, original.generator);
        assert_eq!(cloned.needed_names(), original.needed_names());
        assert_eq!(emit_helpers(&cloned), emit_helpers(&original));
    }
}
