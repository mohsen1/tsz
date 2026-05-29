use super::*;

pub(super) struct CheckerLibFileCheckEnv<'a> {
    pub(super) program: &'a MergedProgram,
    pub(super) options: &'a ResolvedCompilerOptions,
    pub(super) checker_libs: &'a CheckerLibSet,
    pub(super) affected_interfaces: &'a FxHashSet<String>,
    pub(super) extension_interfaces: &'a FxHashSet<String>,
    pub(super) merged_augmentations: &'a MergedAugmentations,
    pub(super) program_context: &'a tsz::checker::context::ProgramContext,
    pub(super) program_has_real_syntax_errors: bool,
    pub(super) program_has_unsupported_js_root: bool,
}

pub(super) fn check_checker_lib_file(
    env: &CheckerLibFileCheckEnv<'_>,
    lib_idx: usize,
    query_cache: &QueryCache,
    shared_lib_cache: Option<Arc<dashmap::DashMap<String, Option<tsz_solver::TypeId>>>>,
) -> (
    Vec<Diagnostic>,
    RequestCacheCounters,
    tsz_solver::StoreStatistics,
) {
    check_checker_lib_file_for_interfaces(
        env,
        lib_idx,
        env.affected_interfaces,
        env.extension_interfaces,
        query_cache,
        shared_lib_cache,
    )
}

pub(super) fn check_checker_lib_file_for_interfaces(
    env: &CheckerLibFileCheckEnv<'_>,
    lib_idx: usize,
    interface_names: &FxHashSet<String>,
    extension_interfaces: &FxHashSet<String>,
    query_cache: &QueryCache,
    shared_lib_cache: Option<Arc<dashmap::DashMap<String, Option<tsz_solver::TypeId>>>>,
) -> (
    Vec<Diagnostic>,
    RequestCacheCounters,
    tsz_solver::StoreStatistics,
) {
    let program = env.program;
    let options = env.options;
    let checker_libs = env.checker_libs;
    let lib_file = &checker_libs.files[lib_idx];
    if should_skip_type_checking_for_file(&lib_file.file_name, options, true) {
        return (
            Vec::new(),
            RequestCacheCounters::default(),
            tsz_solver::StoreStatistics::default(),
        );
    }

    let lib_bound_file =
        build_lib_bound_file_for_interface_checks(program, lib_file, interface_names);
    let binder = create_binder_from_bound_file_with_augmentations(
        &lib_bound_file,
        program,
        program.files.len(),
        env.merged_augmentations,
    );

    let mut checker = CheckerState::with_options(
        &lib_bound_file.arena,
        &binder,
        query_cache,
        lib_bound_file.file_name.clone(),
        &options.checker,
    );
    env.program_context.apply_to(&mut checker.ctx);
    if let Some(shared_lib_cache) = shared_lib_cache {
        checker.ctx.shared_lib_type_cache = Some(shared_lib_cache);
    }

    // The current lib file is already the primary binder for this checker. Exclude it from the
    // fallback lib contexts to avoid duplicate lib declarations during cross-file lookup while
    // still preserving the total count used for capability checks.
    let other_lib_contexts: Vec<LibContext> = checker_libs
        .contexts
        .iter()
        .enumerate()
        .filter(|(idx, _)| *idx != lib_idx)
        .map(|(_, ctx)| ctx.clone())
        .collect();
    checker.ctx.set_lib_contexts(other_lib_contexts);
    checker
        .ctx
        .set_actual_lib_file_count(checker_libs.contexts.len());
    checker.prime_boxed_types();

    // Lib files have no parser diagnostics in the checker pass, but semantic diagnostics from
    // them should still respect tsc's global syntax-error suppression policy.
    checker.ctx.has_parse_errors = env.program_has_real_syntax_errors;
    tsz::checker::reset_stack_overflow_flag();
    checker.check_source_file_interfaces_only_filtered_post_merge(
        lib_bound_file.source_file,
        interface_names,
        extension_interfaces,
    );

    let mut diagnostics = std::mem::take(&mut checker.ctx.diagnostics);
    if env.program_has_real_syntax_errors {
        diagnostics
            .retain(|diag| keep_checker_diagnostic_when_program_has_real_syntax_errors(diag.code));
    }
    if env.program_has_unsupported_js_root && !env.program_has_real_syntax_errors {
        diagnostics
            .retain(|diag| keep_checker_diagnostic_when_program_has_real_syntax_errors(diag.code));
    }
    diagnostics.sort_by(|a, b| a.compare(b));
    diagnostics.dedup_by(|a, b| a.start == b.start && a.code == b.code);

    // PERF: All callers Arc::clone the same shared DefinitionStore; the
    // aggregator computes stats once on the shared store after the loop.
    // See `check_file_for_parallel` for the same rationale.
    (
        diagnostics,
        checker.ctx.request_cache_counters,
        tsz_solver::StoreStatistics::default(),
    )
}

pub(super) fn check_checker_lib_file_baseline(
    program_context: &tsz::checker::context::ProgramContext,
    options: &ResolvedCompilerOptions,
    checker_libs: &CheckerLibSet,
    lib_idx: usize,
    affected_interfaces: &FxHashSet<String>,
    extension_interfaces: &FxHashSet<String>,
    query_cache: &QueryCache,
) -> (
    Vec<Diagnostic>,
    RequestCacheCounters,
    tsz_solver::StoreStatistics,
) {
    let lib_file = &checker_libs.files[lib_idx];
    if should_skip_type_checking_for_file(&lib_file.file_name, options, true) {
        return (
            Vec::new(),
            RequestCacheCounters::default(),
            tsz_solver::StoreStatistics::default(),
        );
    }

    let mut checker = CheckerState::with_options(
        &lib_file.arena,
        lib_file.binder.as_ref(),
        query_cache,
        lib_file.file_name.clone(),
        &options.checker,
    );
    program_context.apply_to(&mut checker.ctx);
    let other_lib_contexts: Vec<LibContext> = checker_libs
        .contexts
        .iter()
        .enumerate()
        .filter(|(idx, _)| *idx != lib_idx)
        .map(|(_, ctx)| ctx.clone())
        .collect();
    checker.ctx.set_lib_contexts(other_lib_contexts);
    checker
        .ctx
        .set_actual_lib_file_count(checker_libs.contexts.len());
    checker.prime_boxed_types();
    tsz::checker::reset_stack_overflow_flag();
    checker.check_source_file_interfaces_only_filtered_post_merge(
        lib_file.root_index,
        affected_interfaces,
        extension_interfaces,
    );

    let mut diagnostics = std::mem::take(&mut checker.ctx.diagnostics);
    diagnostics.sort_by(|a, b| a.compare(b));
    diagnostics.dedup_by(|a, b| a.start == b.start && a.code == b.code);

    // PERF: Same as `check_checker_lib_file` — callers ignore stats and the
    // aggregator computes them once on the shared store.
    (
        diagnostics,
        checker.ctx.request_cache_counters,
        tsz_solver::StoreStatistics::default(),
    )
}

fn collect_lib_interface_node_symbols(
    arena: &NodeArena,
    statements: &[NodeIndex],
    globals: &SymbolTable,
    fallback_node_symbols: &FxHashMap<u32, tsz::binder::SymbolId>,
    affected_interfaces: &FxHashSet<String>,
    node_symbols: &mut FxHashMap<u32, tsz::binder::SymbolId>,
) {
    for &stmt_idx in statements {
        let Some(stmt_node) = arena.get(stmt_idx) else {
            continue;
        };

        if stmt_node.kind == tsz::parser::syntax_kind_ext::INTERFACE_DECLARATION {
            if let Some(interface) = arena.get_interface(stmt_node)
                && let Some(name) = arena.get_identifier_at(interface.name)
                && affected_interfaces.contains(&name.escaped_text)
                && let Some(sym_id) = globals
                    .get(&name.escaped_text)
                    .or_else(|| fallback_node_symbols.get(&stmt_idx.0).copied())
            {
                node_symbols.insert(stmt_idx.0, sym_id);
                node_symbols.insert(interface.name.0, sym_id);
                if let Some(heritage_clauses) = &interface.heritage_clauses {
                    for &clause_idx in &heritage_clauses.nodes {
                        let Some(clause_node) = arena.get(clause_idx) else {
                            continue;
                        };
                        let Some(heritage) = arena.get_heritage_clause(clause_node) else {
                            continue;
                        };
                        if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                            continue;
                        }
                        for &type_idx in &heritage.types.nodes {
                            let Some(type_node) = arena.get(type_idx) else {
                                continue;
                            };
                            let expr_idx = if let Some(expr_type_args) =
                                arena.get_expr_type_args(type_node)
                            {
                                expr_type_args.expression
                            } else if type_node.kind == tsz::parser::syntax_kind_ext::TYPE_REFERENCE
                            {
                                arena
                                    .get_type_ref(type_node)
                                    .map_or(type_idx, |type_ref| type_ref.type_name)
                            } else {
                                type_idx
                            };
                            if let Some(base_name) = entity_name_text_in_arena(arena, expr_idx)
                                && let Some(base_sym_id) = globals
                                    .get(&base_name)
                                    .or_else(|| fallback_node_symbols.get(&expr_idx.0).copied())
                            {
                                node_symbols.insert(expr_idx.0, base_sym_id);
                            }
                        }
                    }
                }
            }
            continue;
        }

        if stmt_node.kind != tsz::parser::syntax_kind_ext::MODULE_DECLARATION {
            continue;
        }

        let Some(module_decl) = arena.get_module(stmt_node) else {
            continue;
        };
        if module_decl.body.is_none() {
            continue;
        }
        let Some(body_node) = arena.get(module_decl.body) else {
            continue;
        };
        if body_node.kind != tsz::parser::syntax_kind_ext::MODULE_BLOCK {
            continue;
        }
        let Some(block) = arena.get_module_block(body_node) else {
            continue;
        };
        let Some(inner) = &block.statements else {
            continue;
        };
        collect_lib_interface_node_symbols(
            arena,
            &inner.nodes,
            globals,
            fallback_node_symbols,
            affected_interfaces,
            node_symbols,
        );
    }
}

fn interface_name_text(arena: &NodeArena, stmt_idx: NodeIndex) -> Option<String> {
    let node = arena.get(stmt_idx)?;
    let interface = arena.get_interface(node)?;
    let ident = arena.get_identifier_at(interface.name)?;
    Some(ident.escaped_text.clone())
}

fn entity_name_text_in_arena(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    let node = arena.get(idx)?;
    if node.kind == tsz::parser::syntax_kind_ext::TYPE_REFERENCE
        && let Some(type_ref) = arena.get_type_ref(node)
    {
        return entity_name_text_in_arena(arena, type_ref.type_name);
    }
    if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
        return arena
            .get_identifier(node)
            .map(|ident| ident.escaped_text.clone());
    }
    if node.kind == tsz::parser::syntax_kind_ext::QUALIFIED_NAME {
        let qn = arena.get_qualified_name(node)?;
        let left = entity_name_text_in_arena(arena, qn.left)?;
        let right = entity_name_text_in_arena(arena, qn.right)?;
        return Some(format!("{left}.{right}"));
    }
    if node.kind == tsz::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        && let Some(access) = arena.get_access_expr(node)
    {
        let left = entity_name_text_in_arena(arena, access.expression)?;
        let right = arena
            .get(access.name_or_argument)
            .and_then(|right_node| arena.get_identifier(right_node))?;
        return Some(format!("{left}.{}", right.escaped_text));
    }
    None
}

fn collect_direct_base_names(
    arena: &NodeArena,
    interface: &tsz_parser::parser::node::InterfaceData,
) -> Vec<String> {
    let Some(heritage_clauses) = &interface.heritage_clauses else {
        return Vec::new();
    };

    let mut names = Vec::new();
    for &clause_idx in &heritage_clauses.nodes {
        let Some(clause_node) = arena.get(clause_idx) else {
            continue;
        };
        let Some(heritage) = arena.get_heritage_clause(clause_node) else {
            continue;
        };
        if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
            continue;
        }
        for &type_idx in &heritage.types.nodes {
            let Some(type_node) = arena.get(type_idx) else {
                continue;
            };
            let expr_idx = if let Some(expr_type_args) = arena.get_expr_type_args(type_node) {
                expr_type_args.expression
            } else if type_node.kind == tsz::parser::syntax_kind_ext::TYPE_REFERENCE {
                arena
                    .get_type_ref(type_node)
                    .map_or(type_idx, |type_ref| type_ref.type_name)
            } else {
                type_idx
            };
            if let Some(name) = entity_name_text_in_arena(arena, expr_idx) {
                names.push(name);
            }
        }
    }
    names
}

fn interface_declaration_has_merge_surface(arena: &NodeArena, stmt_idx: NodeIndex) -> bool {
    let Some(node) = arena.get(stmt_idx) else {
        return false;
    };
    let Some(interface) = arena.get_interface(node) else {
        return false;
    };

    !interface.members.nodes.is_empty()
        || interface
            .type_parameters
            .as_ref()
            .is_some_and(|type_params| !type_params.nodes.is_empty())
        || interface
            .heritage_clauses
            .as_ref()
            .is_some_and(|heritage| !heritage.nodes.is_empty())
}

fn collect_user_global_interface_seeds(program: &MergedProgram) -> FxHashSet<String> {
    let mut seeds = FxHashSet::default();

    for file in &program.files {
        if !file.is_external_module
            && let Some(source_file) = file.arena.get_source_file_at(file.source_file)
        {
            for &stmt_idx in &source_file.statements.nodes {
                if interface_declaration_has_merge_surface(file.arena.as_ref(), stmt_idx)
                    && let Some(name) = interface_name_text(file.arena.as_ref(), stmt_idx)
                {
                    seeds.insert(name);
                }
            }
        }

        for (name, augmentations) in file.global_augmentations.iter() {
            let affects_interface = augmentations.iter().any(|augmentation| {
                if (augmentation.flags & tsz::binder::symbol_flags::INTERFACE) == 0 {
                    return true;
                }
                let arena = augmentation
                    .arena
                    .as_deref()
                    .unwrap_or_else(|| file.arena.as_ref());
                interface_declaration_has_merge_surface(arena, augmentation.node)
            });
            if affects_interface {
                seeds.insert(name.clone());
            }
        }
    }

    seeds
}

fn member_name_text(arena: &NodeArena, member_idx: NodeIndex) -> Option<String> {
    let member_node = arena.get(member_idx)?;
    if let Some(sig) = arena.get_signature(member_node) {
        return arena
            .get(sig.name)
            .and_then(|name_node| arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.clone());
    }
    if let Some(accessor) = arena.get_accessor(member_node) {
        return arena
            .get(accessor.name)
            .and_then(|name_node| arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.clone());
    }
    None
}

fn collect_user_global_interface_member_names(program: &MergedProgram) -> FxHashSet<String> {
    let mut member_names = FxHashSet::default();

    for file in &program.files {
        if file.is_external_module {
            continue;
        }
        let Some(source_file) = file.arena.get_source_file_at(file.source_file) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = file.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = file.arena.get_interface(stmt_node) else {
                continue;
            };
            for &member_idx in &interface.members.nodes {
                if let Some(name) = member_name_text(file.arena.as_ref(), member_idx) {
                    member_names.insert(name);
                }
            }
        }
    }

    member_names
}

fn add_user_global_interface_declaration_arenas(
    program: &MergedProgram,
    declaration_arenas: &mut tsz::binder::state::DeclarationArenaMap,
) {
    for file in &program.files {
        if file.is_external_module {
            continue;
        }
        let Some(source_file) = file.arena.get_source_file_at(file.source_file) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(name) = interface_name_text(file.arena.as_ref(), stmt_idx) else {
                continue;
            };
            let Some(sym_id) = program.globals.get(&name) else {
                continue;
            };
            let target = declaration_arenas.entry((sym_id, stmt_idx)).or_default();
            if !target.iter().any(|arena| Arc::ptr_eq(arena, &file.arena)) {
                target.push(Arc::clone(&file.arena));
            }
        }
    }
}

fn type_node_contains_tag_name_map_indexed_access(
    arena: &NodeArena,
    type_idx: NodeIndex,
    fuel: &mut u32,
) -> bool {
    if type_idx == NodeIndex::NONE || *fuel == 0 {
        return false;
    }
    *fuel -= 1;

    let Some(node) = arena.get(type_idx) else {
        return false;
    };
    if node.kind == tsz::parser::syntax_kind_ext::INDEXED_ACCESS_TYPE {
        return arena
            .get_indexed_access_type(node)
            .and_then(|indexed| entity_name_text_in_arena(arena, indexed.object_type))
            .is_some_and(|name| name.contains("TagNameMap"));
    }

    if let Some(type_ref) = arena.get_type_ref(node) {
        return type_ref.type_arguments.as_ref().is_some_and(|args| {
            args.nodes
                .iter()
                .any(|&arg| type_node_contains_tag_name_map_indexed_access(arena, arg, fuel))
        });
    }
    if let Some(composite) = arena.get_composite_type(node) {
        return composite
            .types
            .nodes
            .iter()
            .any(|&ty| type_node_contains_tag_name_map_indexed_access(arena, ty, fuel));
    }
    if let Some(array) = arena.get_array_type(node) {
        return type_node_contains_tag_name_map_indexed_access(arena, array.element_type, fuel);
    }
    if let Some(wrapped) = arena.get_wrapped_type(node) {
        return type_node_contains_tag_name_map_indexed_access(arena, wrapped.type_node, fuel);
    }
    if let Some(type_operator) = arena.get_type_operator(node) {
        return type_node_contains_tag_name_map_indexed_access(
            arena,
            type_operator.type_node,
            fuel,
        );
    }
    if let Some(function_type) = arena.get_function_type(node) {
        if type_node_contains_tag_name_map_indexed_access(
            arena,
            function_type.type_annotation,
            fuel,
        ) {
            return true;
        }
        for &param_idx in &function_type.parameters.nodes {
            let Some(param_node) = arena.get(param_idx) else {
                continue;
            };
            let Some(param) = arena.get_parameter(param_node) else {
                continue;
            };
            if type_node_contains_tag_name_map_indexed_access(arena, param.type_annotation, fuel) {
                return true;
            }
        }
    }
    if let Some(conditional) = arena.get_conditional_type(node) {
        return [
            conditional.check_type,
            conditional.extends_type,
            conditional.true_type,
            conditional.false_type,
        ]
        .into_iter()
        .any(|ty| type_node_contains_tag_name_map_indexed_access(arena, ty, fuel));
    }

    false
}

fn interface_declares_member_named(
    arena: &NodeArena,
    interface: &tsz_parser::parser::node::InterfaceData,
    member_names: &FxHashSet<String>,
) -> bool {
    !member_names.is_empty()
        && interface.members.nodes.iter().any(|&member_idx| {
            member_name_text(arena, member_idx).is_some_and(|name| member_names.contains(&name))
        })
}

fn interface_declares_index_signature(
    arena: &NodeArena,
    interface: &tsz_parser::parser::node::InterfaceData,
) -> bool {
    interface.members.nodes.iter().any(|&member_idx| {
        arena
            .get(member_idx)
            .is_some_and(|member| member.kind == tsz::parser::syntax_kind_ext::INDEX_SIGNATURE)
    })
}

fn collect_user_global_interfaces_with_index_signatures(
    program: &MergedProgram,
) -> FxHashSet<String> {
    let mut names = FxHashSet::default();

    for file in &program.files {
        if !file.is_external_module
            && let Some(source_file) = file.arena.get_source_file_at(file.source_file)
        {
            for &stmt_idx in &source_file.statements.nodes {
                let Some(stmt_node) = file.arena.get(stmt_idx) else {
                    continue;
                };
                let Some(interface) = file.arena.get_interface(stmt_node) else {
                    continue;
                };
                if interface_declares_index_signature(file.arena.as_ref(), interface)
                    && let Some(name) = interface_name_text(file.arena.as_ref(), stmt_idx)
                {
                    names.insert(name);
                }
            }
        }

        for (name, augmentations) in file.global_augmentations.iter() {
            if augmentations.iter().any(|augmentation| {
                let arena = augmentation
                    .arena
                    .as_deref()
                    .unwrap_or_else(|| file.arena.as_ref());
                arena
                    .get(augmentation.node)
                    .and_then(|node| arena.get_interface(node))
                    .is_some_and(|interface| interface_declares_index_signature(arena, interface))
            }) {
                names.insert(name.clone());
            }
        }
    }

    names
}

fn interface_has_indexed_access_member_type(
    arena: &NodeArena,
    interface: &tsz_parser::parser::node::InterfaceData,
) -> bool {
    for &member_idx in &interface.members.nodes {
        let Some(member_node) = arena.get(member_idx) else {
            continue;
        };
        if let Some(sig) = arena.get_signature(member_node) {
            let mut fuel = 256;
            if type_node_contains_tag_name_map_indexed_access(arena, sig.type_annotation, &mut fuel)
            {
                return true;
            }
            for &param_idx in sig.parameters.as_ref().map_or(&[][..], |p| &p.nodes) {
                let Some(param_node) = arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = arena.get_parameter(param_node) else {
                    continue;
                };
                let mut fuel = 256;
                if type_node_contains_tag_name_map_indexed_access(
                    arena,
                    param.type_annotation,
                    &mut fuel,
                ) {
                    return true;
                }
            }
        }
        if let Some(accessor) = arena.get_accessor(member_node) {
            let mut fuel = 256;
            if type_node_contains_tag_name_map_indexed_access(
                arena,
                accessor.type_annotation,
                &mut fuel,
            ) {
                return true;
            }
        }
    }

    false
}

pub(super) fn affected_lib_interface_names(
    program: &MergedProgram,
    checker_libs: &CheckerLibSet,
) -> FxHashSet<String> {
    let seed_interfaces = collect_user_global_interface_seeds(program);
    let index_signature_seed_interfaces =
        collect_user_global_interfaces_with_index_signatures(program);
    let mut affected = seed_interfaces.clone();
    let user_member_names = collect_user_global_interface_member_names(program);
    let mut inheritance_graph: FxHashMap<String, FxHashSet<String>> = FxHashMap::default();

    for lib in &checker_libs.files {
        let Some(source_file) = lib.arena.get_source_file_at(lib.root_index) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = lib.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = lib.arena.get_interface(stmt_node) else {
                continue;
            };
            let Some(name) = interface_name_text(lib.arena.as_ref(), stmt_idx) else {
                continue;
            };
            let bases = collect_direct_base_names(lib.arena.as_ref(), interface);
            inheritance_graph.entry(name).or_default().extend(bases);
        }
    }

    let mut index_signature_affected = index_signature_seed_interfaces;
    let mut changed = true;
    while changed {
        changed = false;
        for (name, bases) in &inheritance_graph {
            if index_signature_affected.contains(name) {
                continue;
            }
            if bases
                .iter()
                .any(|base| index_signature_affected.contains(base))
            {
                changed = index_signature_affected.insert(name.clone());
            }
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        for (name, bases) in &inheritance_graph {
            if affected.contains(name) {
                continue;
            }
            if bases.iter().any(|base| affected.contains(base)) {
                changed = affected.insert(name.clone());
            }
        }
    }

    let mut relevant = FxHashSet::default();
    for lib in &checker_libs.files {
        let Some(source_file) = lib.arena.get_source_file_at(lib.root_index) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = lib.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = lib.arena.get_interface(stmt_node) else {
                continue;
            };
            let Some(name) = interface_name_text(lib.arena.as_ref(), stmt_idx) else {
                continue;
            };
            if !affected.contains(&name) {
                continue;
            }
            if (index_signature_affected.contains(&name)
                && interface_declares_index_signature(lib.arena.as_ref(), interface))
                || interface_declares_member_named(
                    lib.arena.as_ref(),
                    interface,
                    &user_member_names,
                )
                || interface_has_indexed_access_member_type(lib.arena.as_ref(), interface)
            {
                relevant.insert(name);
            }
        }
    }

    relevant.extend(seed_interfaces);
    let mut ancestor_queue: Vec<String> = relevant.iter().cloned().collect();
    while let Some(name) = ancestor_queue.pop() {
        let Some(bases) = inheritance_graph.get(&name) else {
            continue;
        };
        for base in bases {
            if relevant.insert(base.clone()) {
                ancestor_queue.push(base.clone());
            }
        }
    }

    let mut result = if relevant.is_empty() {
        affected
    } else {
        relevant
    };
    result.retain(|name| inheritance_graph.contains_key(name));
    result
}

pub(super) fn affected_lib_extension_interface_names(
    program: &MergedProgram,
    checker_libs: &CheckerLibSet,
    affected_interfaces: &FxHashSet<String>,
) -> FxHashSet<String> {
    let user_member_names = collect_user_global_interface_member_names(program);
    let index_signature_seed_interfaces =
        collect_user_global_interfaces_with_index_signatures(program);
    let mut extension_interfaces = FxHashSet::default();
    let mut inheritance_graph: FxHashMap<String, FxHashSet<String>> = FxHashMap::default();

    for lib in &checker_libs.files {
        let Some(source_file) = lib.arena.get_source_file_at(lib.root_index) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = lib.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = lib.arena.get_interface(stmt_node) else {
                continue;
            };
            let Some(name) = interface_name_text(lib.arena.as_ref(), stmt_idx) else {
                continue;
            };
            let bases = collect_direct_base_names(lib.arena.as_ref(), interface);
            inheritance_graph
                .entry(name.clone())
                .or_default()
                .extend(bases);
            if affected_interfaces.contains(&name)
                && interface_declares_member_named(
                    lib.arena.as_ref(),
                    interface,
                    &user_member_names,
                )
            {
                extension_interfaces.insert(name);
            }
        }
    }

    let mut index_signature_affected = index_signature_seed_interfaces;
    let mut changed = true;
    while changed {
        changed = false;
        for (name, bases) in &inheritance_graph {
            if index_signature_affected.contains(name) {
                continue;
            }
            if bases
                .iter()
                .any(|base| index_signature_affected.contains(base))
            {
                changed = index_signature_affected.insert(name.clone());
            }
        }
    }
    for lib in &checker_libs.files {
        let Some(source_file) = lib.arena.get_source_file_at(lib.root_index) else {
            continue;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = lib.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = lib.arena.get_interface(stmt_node) else {
                continue;
            };
            let Some(name) = interface_name_text(lib.arena.as_ref(), stmt_idx) else {
                continue;
            };
            if affected_interfaces.contains(&name)
                && index_signature_affected.contains(&name)
                && interface_declares_index_signature(lib.arena.as_ref(), interface)
            {
                extension_interfaces.insert(name);
            }
        }
    }

    extension_interfaces
}

pub(super) fn baseline_lib_datetimeformatpart_spelling_interface_names(
    checker_libs: &CheckerLibSet,
) -> FxHashSet<String> {
    let mut interfaces = FxHashSet::default();

    for lib in &checker_libs.files {
        let Some(file_name) = Path::new(&lib.file_name)
            .file_name()
            .and_then(|name| name.to_str())
        else {
            continue;
        };
        if !is_datetimeformatpart_spelling_baseline_lib(file_name) {
            continue;
        }
        let Some(source_file) = lib.arena.get_source_file_at(lib.root_index) else {
            continue;
        };
        let text = source_file.text.as_ref();
        if !text.contains("DateTimeFormatPart") || !text.contains("DateTimeFormat") {
            continue;
        }

        if matches!(file_name, "lib.es2021.intl.d.ts" | "es2021.intl.d.ts") {
            interfaces.insert("DateTimeRangeFormatPart".to_string());
        }
        if matches!(file_name, "lib.esnext.intl.d.ts" | "esnext.intl.d.ts") {
            interfaces.insert("DateTimeFormat".to_string());
        }
    }

    interfaces
}

pub(super) fn should_preserve_datetimeformatpart_spelling_baseline(
    checker_libs: &CheckerLibSet,
) -> bool {
    checker_libs.files.iter().any(|lib| {
        Path::new(&lib.file_name)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_datetimeformatpart_spelling_baseline_trigger_lib)
    })
}

pub(super) fn has_esnext_umbrella_lib(checker_libs: &CheckerLibSet) -> bool {
    checker_libs.files.iter().any(|lib| {
        Path::new(&lib.file_name)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| matches!(name, "lib.esnext.d.ts" | "esnext.d.ts"))
    })
}

pub(super) fn has_parallel_order_sensitive_global_lib(checker_libs: &CheckerLibSet) -> bool {
    checker_libs.files.iter().any(|lib| {
        Path::new(&lib.file_name)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_parallel_order_sensitive_global_lib)
    })
}

fn is_parallel_order_sensitive_global_lib(file_name: &str) -> bool {
    matches!(
        file_name,
        "lib.dom.d.ts" | "dom.d.ts" | "lib.webworker.d.ts" | "webworker.d.ts"
    )
}

fn is_datetimeformatpart_spelling_baseline_trigger_lib(file_name: &str) -> bool {
    matches!(
        file_name,
        "lib.esnext.date.d.ts"
            | "esnext.date.d.ts"
            | "lib.esnext.temporal.d.ts"
            | "esnext.temporal.d.ts"
    )
}

fn is_datetimeformatpart_spelling_baseline_lib(file_name: &str) -> bool {
    matches!(
        file_name,
        "lib.es2021.intl.d.ts" | "es2021.intl.d.ts" | "lib.esnext.intl.d.ts" | "esnext.intl.d.ts"
    )
}

pub(super) fn is_datetimeformatpart_spelling_baseline_diagnostic(diag: &Diagnostic) -> bool {
    if diag.code != 2552
        || diag.message_text
            != "Cannot find name 'DateTimeFormatPart'. Did you mean 'DateTimeFormat'?"
    {
        return false;
    }

    Path::new(&diag.file)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(is_datetimeformatpart_spelling_baseline_lib)
}

fn build_lib_bound_file_for_interface_checks(
    program: &MergedProgram,
    lib_file: &Arc<LibFile>,
    affected_interfaces: &FxHashSet<String>,
) -> tsz::parallel::BoundFile {
    let mut node_symbols = FxHashMap::default();
    if let Some(source_file) = lib_file.arena.get_source_file_at(lib_file.root_index) {
        collect_lib_interface_node_symbols(
            lib_file.arena.as_ref(),
            &source_file.statements.nodes,
            &program.globals,
            lib_file.binder.node_symbols.as_ref(),
            affected_interfaces,
            &mut node_symbols,
        );
    }

    // Deep-clone the program-wide `declaration_arenas` into the per-call map
    // so we can mutate it below. `program.declaration_arenas` is an `Arc`-shared
    // map; `Arc::clone().as_ref().clone()` gets us an owned copy of the inner
    // `DeclarationArenaMap` without disturbing the shared data.
    let mut declaration_arenas: tsz::binder::state::DeclarationArenaMap =
        (*program.declaration_arenas).clone();
    add_user_global_interface_declaration_arenas(program, &mut declaration_arenas);
    let sym_to_decl_indices = std::sync::Arc::new({
        let mut index = tsz::binder::state::SymToDeclIndicesMap::default();
        for &(sym_id, decl_idx) in declaration_arenas.keys() {
            index.entry(sym_id).or_default().push(decl_idx);
        }
        index
    });

    tsz::parallel::BoundFile {
        file_name: lib_file.file_name.clone(),
        source_file: lib_file.root_index,
        arena: Arc::clone(&lib_file.arena),
        node_symbols: std::sync::Arc::new(node_symbols),
        symbol_arenas: std::sync::Arc::clone(&program.symbol_arenas),
        declaration_arenas: std::sync::Arc::new(declaration_arenas),
        sym_to_decl_indices,
        module_declaration_exports_publicly: std::sync::Arc::new(FxHashMap::default()),
        scopes: std::sync::Arc::new(Vec::new()),
        node_scope_ids: std::sync::Arc::new(FxHashMap::default()),
        parse_diagnostics: Vec::new(),
        global_augmentations: std::sync::Arc::new(FxHashMap::default()),
        module_augmentations: std::sync::Arc::new(FxHashMap::default()),
        augmentation_target_modules: std::sync::Arc::new(FxHashMap::default()),
        flow_nodes: std::sync::Arc::new(tsz::binder::FlowNodeArena::default()),
        node_flow: std::sync::Arc::new(FxHashMap::default()),
        switch_clause_to_switch: std::sync::Arc::new(FxHashMap::default()),
        is_external_module: lib_file.binder.is_external_module,
        expando_properties: std::sync::Arc::new(FxHashMap::default()),
        file_features: tsz::binder::FileFeatures::NONE,
        lib_symbol_reverse_remap: std::sync::Arc::new(FxHashMap::default()),
        semantic_defs: std::sync::Arc::new(FxHashMap::default()),
    }
}

type LibDiagnosticFingerprint = (String, u32, u32, String);

fn lib_diagnostic_fingerprint(diag: &Diagnostic) -> LibDiagnosticFingerprint {
    (
        diag.file.clone(),
        diag.start,
        diag.code,
        diag.message_text.clone(),
    )
}

pub(super) fn collect_checker_lib_baseline_fingerprints(
    program: &MergedProgram,
    options: &ResolvedCompilerOptions,
    checker_libs: &CheckerLibSet,
    affected_interfaces: &FxHashSet<String>,
    extension_interfaces: &FxHashSet<String>,
    program_context: &tsz::checker::context::ProgramContext,
) -> FxHashSet<LibDiagnosticFingerprint> {
    let mut fingerprints = FxHashSet::default();

    for lib_idx in 0..checker_libs.files.len() {
        let query_cache = QueryCache::new(&program.type_interner);
        let (diagnostics, _, _) = check_checker_lib_file_baseline(
            program_context,
            options,
            checker_libs,
            lib_idx,
            affected_interfaces,
            extension_interfaces,
            &query_cache,
        );
        fingerprints.extend(diagnostics.iter().map(lib_diagnostic_fingerprint));
    }

    fingerprints
}

pub(super) fn collect_checker_lib_baseline_diagnostics_for_codes(
    program: &MergedProgram,
    options: &ResolvedCompilerOptions,
    checker_libs: &CheckerLibSet,
    affected_interfaces: &FxHashSet<String>,
    extension_interfaces: &FxHashSet<String>,
    program_context: &tsz::checker::context::ProgramContext,
    codes: &[u32],
) -> Vec<Diagnostic> {
    let code_filter = codes.iter().copied().collect::<FxHashSet<_>>();
    let mut diagnostics = Vec::new();

    for lib_idx in 0..checker_libs.files.len() {
        let query_cache = QueryCache::new(&program.type_interner);
        let (lib_diagnostics, _, _) = check_checker_lib_file_baseline(
            program_context,
            options,
            checker_libs,
            lib_idx,
            affected_interfaces,
            extension_interfaces,
            &query_cache,
        );
        diagnostics.extend(
            lib_diagnostics
                .into_iter()
                .filter(|diag| code_filter.contains(&diag.code)),
        );
    }

    diagnostics.sort_by(|a, b| a.compare(b));
    diagnostics.dedup_by(|a, b| lib_diagnostic_fingerprint(a) == lib_diagnostic_fingerprint(b));
    diagnostics
}

pub(super) fn retain_program_induced_lib_diagnostics(
    diagnostics: &mut Vec<Diagnostic>,
    baseline_fingerprints: &FxHashSet<LibDiagnosticFingerprint>,
) {
    diagnostics.retain(|diag| !baseline_fingerprints.contains(&lib_diagnostic_fingerprint(diag)));
}
