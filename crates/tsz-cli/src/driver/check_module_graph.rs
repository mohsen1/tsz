//! Module dependency graph helpers for CLI checking.

use super::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;
use std::sync::Arc;
use tsz_binder::BinderState;

pub(super) fn compute_module_dependency_stats(
    file_count: usize,
    resolved_module_paths: &FxHashMap<(usize, String), usize>,
) -> super::ModuleDependencyStats {
    // Build a deduplicated adjacency list from resolved_module_paths.
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); file_count];
    let mut edge_count: usize = 0;
    for ((src, _specifier), &tgt) in resolved_module_paths {
        if *src < file_count && tgt < file_count && !adj[*src].contains(&tgt) {
            adj[*src].push(tgt);
            edge_count += 1;
        }
    }
    let sccs = tarjan_scc(file_count, &adj);
    let cycles: Vec<&Vec<usize>> = sccs.iter().filter(|scc| scc.len() > 1).collect();
    let import_cycles = cycles.len();
    let largest_cycle_size = cycles.iter().map(|c| c.len()).max().unwrap_or(0);
    super::ModuleDependencyStats {
        file_count,
        dependency_edges: edge_count,
        import_cycles,
        largest_cycle_size,
    }
}

/// Tarjan's algorithm for finding strongly connected components.
///
/// Returns SCCs in reverse topological order. Each SCC is a `Vec<usize>` of node indices.
/// Import cycles correspond to SCCs with more than one node.
pub(super) fn tarjan_scc(n: usize, adj: &[Vec<usize>]) -> Vec<Vec<usize>> {
    struct State<'a> {
        adj: &'a [Vec<usize>],
        index_counter: usize,
        stack: Vec<usize>,
        on_stack: Vec<bool>,
        indices: Vec<Option<usize>>,
        lowlinks: Vec<usize>,
        result: Vec<Vec<usize>>,
    }

    fn strongconnect(v: usize, state: &mut State<'_>) {
        state.indices[v] = Some(state.index_counter);
        state.lowlinks[v] = state.index_counter;
        state.index_counter += 1;
        state.stack.push(v);
        state.on_stack[v] = true;

        for &w in &state.adj[v] {
            if state.indices[w].is_none() {
                strongconnect(w, state);
                state.lowlinks[v] = state.lowlinks[v].min(state.lowlinks[w]);
            } else if state.on_stack[w] {
                state.lowlinks[v] = state.lowlinks[v].min(state.indices[w].unwrap());
            }
        }

        if state.lowlinks[v] == state.indices[v].unwrap() {
            let mut scc = Vec::new();
            loop {
                let w = state.stack.pop().unwrap();
                state.on_stack[w] = false;
                scc.push(w);
                if w == v {
                    break;
                }
            }
            state.result.push(scc);
        }
    }

    let mut state = State {
        adj,
        index_counter: 0,
        stack: Vec::new(),
        on_stack: vec![false; n],
        indices: vec![None; n],
        lowlinks: vec![0; n],
        result: Vec::new(),
    };

    for v in 0..n {
        if state.indices[v].is_none() {
            strongconnect(v, &mut state);
        }
    }

    state.result
}

pub(super) fn propagate_module_export_maps(
    binder: &mut BinderState,
    specifier: &str,
    target_idx: usize,
    program: &MergedProgram,
    resolved_module_paths: &FxHashMap<(usize, String), usize>,
) {
    let mut worklist: Vec<(String, usize)> = vec![(specifier.to_owned(), target_idx)];
    let mut seen: rustc_hash::FxHashSet<(String, usize)> = rustc_hash::FxHashSet::default();

    while let Some((current_specifier, current_target_idx)) = worklist.pop() {
        if !seen.insert((current_specifier.clone(), current_target_idx)) {
            continue;
        }

        let target_file_name = &program.files[current_target_idx].file_name;

        if let Some(exports) = program.module_exports.get(target_file_name).cloned() {
            Arc::make_mut(&mut binder.module_exports).insert(current_specifier.clone(), exports);
        }
        if let Some(wildcards) = program.wildcard_reexports.get(target_file_name).cloned() {
            Arc::make_mut(&mut binder.wildcard_reexports)
                .insert(current_specifier.clone(), wildcards.clone());
        }
        if let Some(type_only_flags) = program
            .wildcard_reexports_type_only
            .get(target_file_name)
            .cloned()
        {
            Arc::make_mut(&mut binder.wildcard_reexports_type_only)
                .insert(current_specifier.clone(), type_only_flags);
        }
        if let Some(reexports) = program.reexports.get(target_file_name).cloned() {
            Arc::make_mut(&mut binder.reexports).insert(current_specifier.clone(), reexports);
        }

        if let Some(source_modules) = program.wildcard_reexports.get(target_file_name).cloned() {
            for source_module in source_modules {
                if let Some(&source_target_idx) =
                    resolved_module_paths.get(&(current_target_idx, source_module.clone()))
                {
                    worklist.push((source_module, source_target_idx));
                }
            }
        }

        // Also follow named re-exports: `export { X } from './other'`
        // Extract unique source modules from the re-export map so the
        // importing file's binder receives transitive exports.
        if let Some(file_reexports) = program.reexports.get(target_file_name).cloned() {
            let mut reexport_sources: rustc_hash::FxHashSet<String> =
                rustc_hash::FxHashSet::default();
            for (source_module, _) in file_reexports.values() {
                reexport_sources.insert(source_module.clone());
            }
            for source_module in reexport_sources {
                if let Some(&source_target_idx) =
                    resolved_module_paths.get(&(current_target_idx, source_module.clone()))
                {
                    worklist.push((source_module, source_target_idx));
                }
            }
        }
    }
}

/// Compute a topological ordering of file indices based on resolved module dependencies.
///
/// Given `resolved_module_paths` mapping `(source_file_idx, specifier) -> target_file_idx`,
/// this produces a dependency-first ordering: files with no dependencies come first,
/// followed by files that depend only on already-listed files.
///
/// If cycles exist, the cycle participants are appended at the end in their original
/// order (matching tsc behavior which gracefully handles circular imports).
///
/// Only file indices present in `file_indices` are included in the output.
pub(super) fn topological_file_order(
    file_indices: &[usize],
    resolved_module_paths: &FxHashMap<(usize, String), usize>,
) -> Vec<usize> {
    if file_indices.len() <= 1 {
        return file_indices.to_vec();
    }

    // Build adjacency list: src -> [targets it imports].
    // Edge A -> B means "A depends on B" (A imports B).
    let file_set: FxHashSet<usize> = file_indices.iter().copied().collect();
    let mut deps: FxHashMap<usize, Vec<usize>> = FxHashMap::default();
    for &idx in file_indices {
        deps.insert(idx, Vec::new());
    }
    for (&(src, _), &target) in resolved_module_paths.iter() {
        if file_set.contains(&src) && file_set.contains(&target) && src != target {
            deps.entry(src).or_default().push(target);
        }
    }

    // Kahn's algorithm on the dependency graph.
    // We want dependencies first: if A imports B, B should appear before A.
    // in_degree[x] = number of imports x has (edges leaving x in the dep graph).
    // Nodes with in_degree 0 have no imports and can be processed first.
    let mut in_degree: FxHashMap<usize, usize> = FxHashMap::default();
    // reverse_deps[B] = [A, ...] means "A depends on B"
    let mut reverse_deps: FxHashMap<usize, Vec<usize>> = FxHashMap::default();
    for &idx in file_indices {
        in_degree.insert(idx, 0);
        reverse_deps.insert(idx, Vec::new());
    }
    for (&src, dep_list) in &deps {
        for &dep in dep_list {
            if dep != src {
                reverse_deps.entry(dep).or_default().push(src);
                *in_degree.entry(src).or_default() += 1;
            }
        }
    }

    // Seed queue with nodes that have no dependencies, in sorted order for determinism.
    let mut queue: VecDeque<usize> = VecDeque::new();
    let mut sorted_indices: Vec<usize> = file_indices.to_vec();
    sorted_indices.sort_unstable();
    for &idx in &sorted_indices {
        if in_degree[&idx] == 0 {
            queue.push_back(idx);
        }
    }

    let mut result = Vec::with_capacity(file_indices.len());
    while let Some(node) = queue.pop_front() {
        result.push(node);
        if let Some(dependents) = reverse_deps.get(&node) {
            let mut sorted_dependents = dependents.clone();
            sorted_dependents.sort_unstable();
            for &dependent in &sorted_dependents {
                let deg = in_degree.get_mut(&dependent).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    queue.push_back(dependent);
                }
            }
        }
    }

    // If cycles exist, append remaining nodes in their original order.
    if result.len() < file_indices.len() {
        let in_result: FxHashSet<usize> = result.iter().copied().collect();
        for &idx in file_indices {
            if !in_result.contains(&idx) {
                result.push(idx);
            }
        }
    }

    result
}
