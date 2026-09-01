//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/transforms/helpers.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 9dad9c387ef38092e67359f002290ac4ad05e797b8ff753ae24fb7501ba241a9 1063 default_helpers_needed_is_false
    #[test]
    fn default_helpers_needed_is_false() {
        let helpers = HelpersNeeded::default();
        assert!(!helpers.any_needed());
        assert!(helpers.needed_names().is_empty());
        assert!(emit_helpers(&helpers).is_empty());
    }
// TSZ_INLINE_TEST_END 9dad9c387ef38092e67359f002290ac4ad05e797b8ff753ae24fb7501ba241a9

// TSZ_INLINE_TEST_BEGIN 9006ae2cf9abc4fff2c0e3399fcaf1c9359d0d493b963c80509a7976085c357c 1071 any_needed_flips_for_each_individual_flag
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
// TSZ_INLINE_TEST_END 9006ae2cf9abc4fff2c0e3399fcaf1c9359d0d493b963c80509a7976085c357c

// TSZ_INLINE_TEST_BEGIN c6407fb4fd224f730f1d6f12404ff321abbb760a77981cda0d4ec631405e5beb 1129 class_private_field_set_before_get_alone_does_not_trigger_any_needed
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
// TSZ_INLINE_TEST_END c6407fb4fd224f730f1d6f12404ff321abbb760a77981cda0d4ec631405e5beb

// TSZ_INLINE_TEST_BEGIN aff63ef62e6fa36ca63613c8f3025049eeb86ea12f82bda74cec046805b13fe5 1146 needed_names_returns_canonical_helper_strings
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
// TSZ_INLINE_TEST_END aff63ef62e6fa36ca63613c8f3025049eeb86ea12f82bda74cec046805b13fe5

// TSZ_INLINE_TEST_BEGIN 0729130dc42ca73496355b70566ecb3d1a02f64f64a2fbf7c078e2eeced21faf 1159 needed_names_priority_order_for_full_set
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
// TSZ_INLINE_TEST_END 0729130dc42ca73496355b70566ecb3d1a02f64f64a2fbf7c078e2eeced21faf

// TSZ_INLINE_TEST_BEGIN e598c2c73ea951e1b25139dffa1d798c6d6b56e94b939c2c0840c58072c6017f 1238 needed_names_skips_unset_flags
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
// TSZ_INLINE_TEST_END e598c2c73ea951e1b25139dffa1d798c6d6b56e94b939c2c0840c58072c6017f

// TSZ_INLINE_TEST_BEGIN 563c6dc94c10f8c16c24b1ae37d32ea4c7e173f83b26e70548fd54237e5674f3 1256 export_star_before_import_star_flag_alone_does_not_trigger_any_needed
    #[test]
    fn export_star_before_import_star_flag_alone_does_not_trigger_any_needed() {
        // Like `class_private_field_set_before_get`, the request-order flag is
        // emit bookkeeping only. On its own it must not make `any_needed()` true
        // (which would install a spurious tslib import) nor emit a name.
        let helpers = HelpersNeeded {
            export_star_before_import_star: true,
            ..HelpersNeeded::default()
        };
        assert!(!helpers.any_needed());
        assert!(helpers.needed_names().is_empty());
        assert!(emit_helpers(&helpers).is_empty());
    }
// TSZ_INLINE_TEST_END 563c6dc94c10f8c16c24b1ae37d32ea4c7e173f83b26e70548fd54237e5674f3

// TSZ_INLINE_TEST_BEGIN 37cf95414d75bf174dcaa3ba405a5e6e6f08077356cbd1e5a28406fd4c36c7a6 1270 import_star_requested_first_emits_import_star_first
    #[test]
    fn import_star_requested_first_emits_import_star_first() {
        // `export * as ns` (import_star) requested before `export *` (export_star):
        // tsc's stable same-priority sort keeps import_star first.
        let mut helpers = HelpersNeeded::default();
        helpers.mark_import_star();
        helpers.create_binding = true;
        helpers.mark_export_star();

        assert!(!helpers.export_star_before_import_star);
        let names = helpers.needed_names();
        let i_import = names.iter().position(|n| *n == "__importStar").unwrap();
        let i_export = names.iter().position(|n| *n == "__exportStar").unwrap();
        assert!(i_import < i_export, "names: {names:?}");

        let output = emit_helpers(&helpers);
        assert!(
            find_helper(&output, "__importStar") < find_helper(&output, "__exportStar"),
            "emit order wrong:\n{output}",
        );
    }
// TSZ_INLINE_TEST_END 37cf95414d75bf174dcaa3ba405a5e6e6f08077356cbd1e5a28406fd4c36c7a6

// TSZ_INLINE_TEST_BEGIN f1dfd6b70b75c0872066450f07ca32f3c6ff15d08ad00b5f559199edc8d6aaf3 1292 export_star_requested_first_emits_export_star_first
    #[test]
    fn export_star_requested_first_emits_export_star_first() {
        // `export *` (export_star) requested before `export * as ns` (import_star):
        // tsc's stable same-priority sort keeps export_star first. This is the
        // case the fixed-order emitter got wrong.
        let mut helpers = HelpersNeeded::default();
        helpers.mark_export_star();
        helpers.create_binding = true;
        helpers.mark_import_star();

        assert!(helpers.export_star_before_import_star);
        let names = helpers.needed_names();
        let i_import = names.iter().position(|n| *n == "__importStar").unwrap();
        let i_export = names.iter().position(|n| *n == "__exportStar").unwrap();
        assert!(i_export < i_import, "names: {names:?}");

        let output = emit_helpers(&helpers);
        assert!(
            find_helper(&output, "__exportStar") < find_helper(&output, "__importStar"),
            "emit order wrong:\n{output}",
        );
    }
// TSZ_INLINE_TEST_END f1dfd6b70b75c0872066450f07ca32f3c6ff15d08ad00b5f559199edc8d6aaf3

// TSZ_INLINE_TEST_BEGIN 5bbc4d1646ceefb0636cc009c73d2a3c7ec511c51fc4130d074090d04655f91e 1315 star_request_order_is_recorded_only_on_first_request
    #[test]
    fn star_request_order_is_recorded_only_on_first_request() {
        // Marking import_star, then export_star, then import_star again must not
        // flip the recorded order (dedup-safe): import_star stays first.
        let mut helpers = HelpersNeeded::default();
        helpers.mark_import_star();
        helpers.mark_export_star();
        helpers.mark_import_star();
        assert!(!helpers.export_star_before_import_star);

        // And the mirror: export first stays export-first despite re-marks.
        let mut h2 = HelpersNeeded::default();
        h2.mark_export_star();
        h2.mark_import_star();
        h2.mark_export_star();
        assert!(h2.export_star_before_import_star);
    }
// TSZ_INLINE_TEST_END 5bbc4d1646ceefb0636cc009c73d2a3c7ec511c51fc4130d074090d04655f91e

// TSZ_INLINE_TEST_BEGIN 1a18154f848f20622ab0787c85abeda92abe3f0c265fa1d7f61929f859bd74de 1346 emit_helpers_priority_order_extends_assign_decorate_metadata_param_awaiter_generator
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
// TSZ_INLINE_TEST_END 1a18154f848f20622ab0787c85abeda92abe3f0c265fa1d7f61929f859bd74de

// TSZ_INLINE_TEST_BEGIN f016df6bd9d1c57e074bc5f2ce4af97b3466095821f4dd294fdbf1c31ed069d6 1390 emit_helpers_priority_zero_extends_before_make_template_object
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
// TSZ_INLINE_TEST_END f016df6bd9d1c57e074bc5f2ce4af97b3466095821f4dd294fdbf1c31ed069d6

// TSZ_INLINE_TEST_BEGIN cf71de303857b9ecec7d013c9842f8f5abf2474a5f9d7d1ce2c65ffef3be69f2 1406 emit_helpers_priority_one_assign_before_create_binding
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
// TSZ_INLINE_TEST_END cf71de303857b9ecec7d013c9842f8f5abf2474a5f9d7d1ce2c65ffef3be69f2

// TSZ_INLINE_TEST_BEGIN 5ea7747a285cadce05500b3df0da53357acc1ae13a30fd895fa27d6b0049e0dd 1421 emit_helpers_order_decorators_and_async_helpers
    #[test]
    fn emit_helpers_order_decorators_and_async_helpers() {
        // emit_helpers source orders priority-2 helpers as:
        //   decorate, esDecorate, runInitializers,
        //   importStar, rewriteRelativeImportExtension, exportStar
        // (`__setModuleDefault` is priority 1 and emitted earlier, before any
        // priority-2 helper). `__propKey` has no priority in tsc, so it sorts
        // after every prioritized helper (including importStar/exportStar and the
        // priority-3..6 helpers) and immediately before `__setFunctionName`.
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
        // priority-2 importStar/exportStar precede the no-priority `__propKey`.
        assert!(i_run < i_import_star);
        assert!(i_import_star < i_rewrite);
        assert!(i_rewrite < i_export_star);
        assert!(i_export_star < i_awaiter);
        assert!(i_awaiter < i_generator);
        // `__propKey` (no priority) sorts after every prioritized helper and
        // just before `__setFunctionName`.
        assert!(i_generator < i_prop_key);
        assert!(i_prop_key < i_set_name);
        assert!(i_set_name < i_await);
        assert!(i_await < i_async_generator);
    }
// TSZ_INLINE_TEST_END 5ea7747a285cadce05500b3df0da53357acc1ae13a30fd895fa27d6b0049e0dd

// TSZ_INLINE_TEST_BEGIN a598b650a9ae97bc14010fb9a4f43b618d595d8ff3fd6ad10241bda6a24cb6d3 1476 emit_helpers_prop_key_is_no_priority_after_every_prioritized_helper
    #[test]
    fn emit_helpers_prop_key_is_no_priority_after_every_prioritized_helper() {
        // `__propKey` (`typescript:propKey`) has no priority field in tsc, so
        // it sorts after every priority 0-7 helper. Previously tsz emitted it
        // at priority 2, ahead of importStar/metadata/param/awaiter/generator.
        //
        // tsc (`--target ES2022 --module CommonJS --esModuleInterop`) on a
        // namespace-import + decorated computed-key member emits __propKey last:
        //   __createBinding, __setModuleDefault, __runInitializers,
        //   __esDecorate, __importStar, __propKey
        let helpers = HelpersNeeded {
            prop_key: true,
            import_star: true,
            create_binding: true,
            es_decorate: true,
            metadata: true,
            param: true,
            awaiter: true,
            generator: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        let i_prop_key = find_helper(&output, "__propKey");
        for prioritized in [
            "__createBinding",
            "__setModuleDefault",
            "__esDecorate",
            "__importStar",
            "__metadata",
            "__param",
            "__awaiter",
            "__generator",
        ] {
            assert!(
                find_helper(&output, prioritized) < i_prop_key,
                "{prioritized} (prioritized) must precede no-priority __propKey\n{output}",
            );
        }
    }
// TSZ_INLINE_TEST_END a598b650a9ae97bc14010fb9a4f43b618d595d8ff3fd6ad10241bda6a24cb6d3

// TSZ_INLINE_TEST_BEGIN 6d579409308f2478a580bc3bcec0f118441a604ba19fcd4ef3029918a59f9fec 1517 emit_helpers_order_class_private_before_set_function_name
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
// TSZ_INLINE_TEST_END 6d579409308f2478a580bc3bcec0f118441a604ba19fcd4ef3029918a59f9fec

// TSZ_INLINE_TEST_BEGIN e4d0adef74007191436a46ec42969ec23daea8857ea6787a6efed50d5948eab5 1535 emit_helpers_order_tc39_set_function_name_before_private_helpers
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
// TSZ_INLINE_TEST_END e4d0adef74007191436a46ec42969ec23daea8857ea6787a6efed50d5948eab5

// TSZ_INLINE_TEST_BEGIN e7528a112d2a318106fa45a7b1e46a2cc40d8211c4fe5110f6ffd4d084b5a4e4 1559 emit_helpers_orders_member_decorator_initializers_before_es_decorate
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
// TSZ_INLINE_TEST_END e7528a112d2a318106fa45a7b1e46a2cc40d8211c4fe5110f6ffd4d084b5a4e4

// TSZ_INLINE_TEST_BEGIN 1222345e02aa9213986234f67dc5440d581b3ed26a2ebc67bd7839d2f91d0ff5 1578 emit_helpers_no_priority_block_emits_last
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
// TSZ_INLINE_TEST_END 1222345e02aa9213986234f67dc5440d581b3ed26a2ebc67bd7839d2f91d0ff5

// TSZ_INLINE_TEST_BEGIN 50af5ddf0b83c7a6a1b7465fed5f3471180992620d6484deb9c05a4a8af5284e 1614 emit_helpers_unprioritized_helpers_follow_first_request_order
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
// TSZ_INLINE_TEST_END 50af5ddf0b83c7a6a1b7465fed5f3471180992620d6484deb9c05a4a8af5284e

// TSZ_INLINE_TEST_BEGIN db9c50db07f8252c6e87e821cf968dd6156522aafe9ffc73753ebafa6c0d63ca 1631 emit_helpers_private_field_before_rest_when_requested_first
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
// TSZ_INLINE_TEST_END db9c50db07f8252c6e87e821cf968dd6156522aafe9ffc73753ebafa6c0d63ca

// TSZ_INLINE_TEST_BEGIN 325f3d0009653d223db8428eb29b02f01eac05c3c4b1b1e2f596da618afecaa4 1643 emit_helpers_read_precedes_spread_array_when_requested_after_spread
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
// TSZ_INLINE_TEST_END 325f3d0009653d223db8428eb29b02f01eac05c3c4b1b1e2f596da618afecaa4

// TSZ_INLINE_TEST_BEGIN ff78cc5f10e8939311c784be3fef7c0ca1b1e540ee1c38d91130df4e7089c534 1656 emit_helpers_async_values_precedes_spread_array_when_requested_after_spread
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
// TSZ_INLINE_TEST_END ff78cc5f10e8939311c784be3fef7c0ca1b1e540ee1c38d91130df4e7089c534

// TSZ_INLINE_TEST_BEGIN 449cbe0f6edd97135de6f36eef54a21a1d3253edf6fc73fa20656ab6b36fd344 1672 emit_helpers_rest_precedes_values_when_values_requested_first
    #[test]
    fn emit_helpers_rest_precedes_values_when_values_requested_first() {
        // `__values` (ES2015 iteration) requested first — e.g. `yield*`/`for..of`
        // earlier in the source than an object-rest destructuring. `__rest`
        // (ES2018 object-rest pass) must still emit ahead of `__values`, matching
        // tsc's transform-pass ordering.
        let mut helpers = HelpersNeeded::default();
        helpers.mark_values();
        helpers.mark_rest();

        let output = emit_helpers(&helpers);
        let i_rest = find_helper(&output, "__rest");
        let i_values = find_helper(&output, "__values");
        assert!(
            i_rest < i_values,
            "rest must precede values, got rest={i_rest} values={i_values}",
        );
        assert_eq!(helpers.needed_names(), vec!["__rest", "__values"]);
    }
// TSZ_INLINE_TEST_END 449cbe0f6edd97135de6f36eef54a21a1d3253edf6fc73fa20656ab6b36fd344

// TSZ_INLINE_TEST_BEGIN 4f02849ee703c8a7bff6b3182d6024b4b1addc0c2ed02bb7e8460bdca95372ed 1692 emit_helpers_es2018_tier_precedes_whole_es2015_tier_when_requested_last
    #[test]
    fn emit_helpers_es2018_tier_precedes_whole_es2015_tier_when_requested_last() {
        // for-await-of (`__asyncValues`, ES2018) requested after the ES2015
        // iteration/spread constructs must still emit ahead of the entire ES2015
        // tier (`__values`/`__read`/`__spreadArray`), not merely before
        // `__spreadArray`.
        let mut helpers = HelpersNeeded::default();
        helpers.mark_values();
        helpers.mark_read();
        helpers.mark_spread_array();
        helpers.mark_async_values();

        let output = emit_helpers(&helpers);
        let i_async_values = find_helper(&output, "__asyncValues");
        let i_values = find_helper(&output, "__values");
        let i_read = find_helper(&output, "__read");
        let i_spread = find_helper(&output, "__spreadArray");
        assert!(i_async_values < i_values, "asyncValues must precede values");
        assert!(i_values < i_read, "values must precede read");
        assert!(i_read < i_spread, "read must precede spreadArray");
        assert_eq!(
            helpers.needed_names(),
            vec!["__asyncValues", "__values", "__read", "__spreadArray"],
        );
    }
// TSZ_INLINE_TEST_END 4f02849ee703c8a7bff6b3182d6024b4b1addc0c2ed02bb7e8460bdca95372ed

// TSZ_INLINE_TEST_BEGIN 03d67f0a60311b75f86a93b297d019c9d614112dd7f4c762312c92a5252b7d7f 1718 needed_names_tracks_unprioritized_first_request_order
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
// TSZ_INLINE_TEST_END 03d67f0a60311b75f86a93b297d019c9d614112dd7f4c762312c92a5252b7d7f

// TSZ_INLINE_TEST_BEGIN 9133cb5426572de7cb3286c8b7fc241d048fb346dfc5bde0b9aa11dbf395fad0 1734 emit_helpers_class_private_field_default_get_before_set
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
// TSZ_INLINE_TEST_END 9133cb5426572de7cb3286c8b7fc241d048fb346dfc5bde0b9aa11dbf395fad0

// TSZ_INLINE_TEST_BEGIN fa37c0590a222ad2041cac6fdbd6b038e4d3b89950ed0dc01fcd270e5339af5b 1753 emit_helpers_class_private_field_set_before_get_flips_order
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
// TSZ_INLINE_TEST_END fa37c0590a222ad2041cac6fdbd6b038e4d3b89950ed0dc01fcd270e5339af5b

// TSZ_INLINE_TEST_BEGIN 4294960a2188335a29158daca8e2bc2884d9cae60f3f1f2dcccfbca739161248 1773 emit_helpers_class_private_field_set_before_get_only_set_emits_only_set
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
// TSZ_INLINE_TEST_END 4294960a2188335a29158daca8e2bc2884d9cae60f3f1f2dcccfbca739161248

// TSZ_INLINE_TEST_BEGIN 527ee108eb2695a38586e02b5293ca85655b7ec7a8b2aba2274d1888d16b156d 1787 emit_helpers_import_star_emits_set_module_default_before_import_star
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
// TSZ_INLINE_TEST_END 527ee108eb2695a38586e02b5293ca85655b7ec7a8b2aba2274d1888d16b156d

// TSZ_INLINE_TEST_BEGIN 61221738a80f23385e066c39c48027ebd325638d97e322a08c2bb22a31f54708 1805 emit_helpers_set_module_default_is_priority_one_before_decorators
    #[test]
    fn emit_helpers_set_module_default_is_priority_one_before_decorators() {
        // tsc's `compareEmitHelpers` puts `__setModuleDefault`
        // (`typescript:commonjscreatevalue`) at priority 1, tied with
        // `__createBinding`, so it is emitted before every priority-2 helper
        // (decorators, `__importStar`). Witness: `import * as ns` combined with
        // a decorated class — tsc 6.x emits
        //   __createBinding, __setModuleDefault, __runInitializers,
        //   __esDecorate, __importStar
        // Regression: tsz previously bundled `__setModuleDefault` with
        // `__importStar` at priority 2, emitting it AFTER the decorator helpers.
        let helpers = HelpersNeeded {
            create_binding: true,
            import_star: true,
            es_decorate: true,
            run_initializers: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        let i_create = find_helper(&output, "__createBinding");
        let i_set_default = find_helper(&output, "__setModuleDefault");
        let i_run = find_helper(&output, "__runInitializers");
        let i_es_decorate = find_helper(&output, "__esDecorate");
        let i_import_star = find_helper(&output, "__importStar");
        assert!(
            i_create < i_set_default,
            "__createBinding then __setModuleDefault within priority 1",
        );
        assert!(
            i_set_default < i_run,
            "priority-1 __setModuleDefault before priority-2 __runInitializers",
        );
        assert!(
            i_set_default < i_es_decorate,
            "priority-1 __setModuleDefault before priority-2 __esDecorate",
        );
        assert!(
            i_set_default < i_import_star,
            "__setModuleDefault still precedes __importStar",
        );
    }
// TSZ_INLINE_TEST_END 61221738a80f23385e066c39c48027ebd325638d97e322a08c2bb22a31f54708

// TSZ_INLINE_TEST_BEGIN b5257792f7b04e7ff34068bc0afaee0503bf4da3d0e2f5037fc55d86fa0140b3 1848 emit_helpers_each_helper_terminated_by_newline
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
// TSZ_INLINE_TEST_END b5257792f7b04e7ff34068bc0afaee0503bf4da3d0e2f5037fc55d86fa0140b3

// TSZ_INLINE_TEST_BEGIN d41b143d4ab62659e71656d18c58b3fcee3937724ab3d0d723d38c674aecbb53 1874 emit_helpers_disposable_resource_pair_ordered_add_before_dispose
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
// TSZ_INLINE_TEST_END d41b143d4ab62659e71656d18c58b3fcee3937724ab3d0d723d38c674aecbb53

// TSZ_INLINE_TEST_BEGIN 128a15a993078c78e3aa1832fb074549f7e91d71d58c88fbd1ad17137b83abfc 1896 helper_constants_are_var_declarations
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
// TSZ_INLINE_TEST_END 128a15a993078c78e3aa1832fb074549f7e91d71d58c88fbd1ad17137b83abfc

// TSZ_INLINE_TEST_BEGIN 40b71265ea1fc61cb996311a072f25fc20ca365c7f61489488663c16a42137eb 1957 helper_constants_match_needed_names_basenames
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
// TSZ_INLINE_TEST_END 40b71265ea1fc61cb996311a072f25fc20ca365c7f61489488663c16a42137eb

// TSZ_INLINE_TEST_BEGIN 372bf8eaf5bad0704e6a813562ad0a89e0fbd24828a7060ef94b86b807594ac7 2009 emit_helpers_round_trips_through_string_concat
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
// TSZ_INLINE_TEST_END 372bf8eaf5bad0704e6a813562ad0a89e0fbd24828a7060ef94b86b807594ac7

// TSZ_INLINE_TEST_BEGIN ffacc92cde4883303c9ae35734f56bdd89cf9821cbce05cf6fce87bf15756119 2036 emit_helpers_only_emits_requested_helpers
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
// TSZ_INLINE_TEST_END ffacc92cde4883303c9ae35734f56bdd89cf9821cbce05cf6fce87bf15756119

// TSZ_INLINE_TEST_BEGIN f10ae19ba8de69570470e058e1685ae1e6cf67e4ce7de43293b3ba1f63ab84b3 2056 any_needed_implies_non_empty_needed_names_and_emit
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
// TSZ_INLINE_TEST_END f10ae19ba8de69570470e058e1685ae1e6cf67e4ce7de43293b3ba1f63ab84b3

// TSZ_INLINE_TEST_BEGIN c73c42da36546b139966126713aeab457ab0f8590cfccd877494c3d49ee23dd7 2080 helpers_needed_clone_round_trips
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
// TSZ_INLINE_TEST_END c73c42da36546b139966126713aeab457ab0f8590cfccd877494c3d49ee23dd7
