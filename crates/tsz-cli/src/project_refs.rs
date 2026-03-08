//! Project References Support
//!
//! This module implements TypeScript project references, which enable:
//! - Splitting large codebases into smaller projects
//! - Faster incremental builds through project-level caching
//! - Better editor support with scoped type checking
//! - Cleaner dependency management between project boundaries
//!
//! # Key Concepts
//!
//! - **Composite Project**: A project with `composite: true` that can be referenced
//! - **Project Reference**: A `{ path: string, prepend?: boolean }` entry in tsconfig.json
//! - **Build Order**: Topologically sorted order of projects based on dependencies
//! - **Declaration Output**: .d.ts files that reference consuming projects use

use anyhow::{Context, Result, anyhow, bail};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use std::collections::BinaryHeap;
use std::path::{Path, PathBuf};

use super::config::{CompilerOptions, TsConfig};

/// A project reference as specified in tsconfig.json
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectReference {
    /// Path to the referenced project's tsconfig.json or directory
    pub path: String,
    /// If true, prepend the output of this project to the output of the referencing project
    #[serde(default)]
    pub prepend: bool,
    /// Circular reference allowed (non-standard extension for gradual migration)
    #[serde(default)]
    pub circular: bool,
}

/// Extended `TsConfig` that includes project references
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TsConfigWithReferences {
    #[serde(flatten)]
    pub base: TsConfig,
    /// List of project references
    #[serde(default)]
    pub references: Option<Vec<ProjectReference>>,
}

/// Extended `CompilerOptions` with composite project settings
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CompositeCompilerOptions {
    #[serde(flatten)]
    pub base: CompilerOptions,
    /// Whether this is a composite project that can be referenced
    #[serde(default)]
    pub composite: Option<bool>,
    /// Force consistent casing in file names
    #[serde(default)]
    pub force_consistent_casing_in_file_names: Option<bool>,
    /// Disable solution searching for this project
    #[serde(default)]
    pub disable_solution_searching: Option<bool>,
    /// Disable source project reference redirect
    #[serde(default)]
    pub disable_source_of_project_reference_redirect: Option<bool>,
    /// Disable referenced project load
    #[serde(default)]
    pub disable_referenced_project_load: Option<bool>,
}

/// A resolved project with its configuration and metadata
#[derive(Debug, Clone)]
pub struct ResolvedProject {
    /// Absolute path to the project's tsconfig.json
    pub config_path: PathBuf,
    /// The project's root directory
    pub root_dir: PathBuf,
    /// The parsed configuration
    pub config: TsConfigWithReferences,
    /// Resolved references to other projects
    pub resolved_references: Vec<ResolvedProjectReference>,
    /// Whether this is a composite project
    pub is_composite: bool,
    /// Whether this project has noEmit set
    pub no_emit: bool,
    /// Output directory for declarations
    pub declaration_dir: Option<PathBuf>,
    /// Output directory for JavaScript
    pub out_dir: Option<PathBuf>,
}

/// A resolved reference to another project
#[derive(Debug, Clone)]
pub struct ResolvedProjectReference {
    /// Absolute path to the referenced project's tsconfig.json
    pub config_path: PathBuf,
    /// The original reference from the config
    pub original: ProjectReference,
    /// Whether the reference was successfully resolved
    pub is_valid: bool,
    /// Error message if resolution failed
    pub error: Option<String>,
}

/// Unique identifier for a project in the reference graph
pub type ProjectId = usize;

/// Unique identifier for a strongly connected component in the condensed project graph.
pub type ProjectSccId = usize;

/// SCC condensation view of the project reference graph.
///
/// This keeps cycle-aware dependency information available for invalidation and
/// future scheduler work while the default build path still rejects cycles.
#[derive(Debug, Clone)]
pub struct ProjectReferenceCondensation {
    components: Vec<Vec<ProjectId>>,
    project_to_component: Vec<ProjectSccId>,
    references: FxHashMap<ProjectSccId, Vec<ProjectSccId>>,
    dependents: FxHashMap<ProjectSccId, Vec<ProjectSccId>>,
}

impl ProjectReferenceCondensation {
    /// Get all SCC components in deterministic order.
    pub fn components(&self) -> &[Vec<ProjectId>] {
        &self.components
    }

    /// Get the number of SCC components.
    pub const fn component_count(&self) -> usize {
        self.components.len()
    }

    /// Get the projects that belong to a component.
    pub fn component_members(&self, id: ProjectSccId) -> &[ProjectId] {
        self.components
            .get(id)
            .map_or(&[], |members| members.as_slice())
    }

    /// Get the SCC component that contains a project.
    pub fn component_for_project(&self, project_id: ProjectId) -> Option<ProjectSccId> {
        self.project_to_component.get(project_id).copied()
    }

    /// Get outgoing SCC edges for a component.
    pub fn get_references(&self, id: ProjectSccId) -> &[ProjectSccId] {
        self.references
            .get(&id)
            .map_or(&[], |edges| edges.as_slice())
    }

    /// Get incoming SCC edges for a component.
    pub fn get_dependents(&self, id: ProjectSccId) -> &[ProjectSccId] {
        self.dependents
            .get(&id)
            .map_or(&[], |edges| edges.as_slice())
    }

    /// Get all transitive dependencies of a project through the SCC condensation graph.
    pub fn transitive_dependencies(&self, id: ProjectId) -> FxHashSet<ProjectId> {
        let Some(start_component) = self.component_for_project(id) else {
            return FxHashSet::default();
        };

        let mut dependencies = FxHashSet::default();
        for &peer in self.component_members(start_component) {
            if peer != id {
                dependencies.insert(peer);
            }
        }

        let mut visited_components = FxHashSet::default();
        let mut stack = self.get_references(start_component).to_vec();
        while let Some(component_id) = stack.pop() {
            if !visited_components.insert(component_id) {
                continue;
            }
            for &project_id in self.component_members(component_id) {
                dependencies.insert(project_id);
            }
            for &next in self.get_references(component_id) {
                stack.push(next);
            }
        }

        dependencies
    }

    /// Get all projects affected by a change to a project through the SCC condensation graph.
    pub fn affected_projects(&self, id: ProjectId) -> FxHashSet<ProjectId> {
        let Some(start_component) = self.component_for_project(id) else {
            return FxHashSet::default();
        };

        let mut affected = FxHashSet::default();
        for &peer in self.component_members(start_component) {
            if peer != id {
                affected.insert(peer);
            }
        }

        let mut visited_components = FxHashSet::default();
        let mut stack = self.get_dependents(start_component).to_vec();
        while let Some(component_id) = stack.pop() {
            if !visited_components.insert(component_id) {
                continue;
            }
            for &project_id in self.component_members(component_id) {
                affected.insert(project_id);
            }
            for &next in self.get_dependents(component_id) {
                stack.push(next);
            }
        }

        affected
    }
}

/// Graph of project references for build ordering
#[derive(Debug, Default)]
pub struct ProjectReferenceGraph {
    /// All projects indexed by their ID
    projects: Vec<ResolvedProject>,
    /// Map from config path to project ID
    path_to_id: FxHashMap<PathBuf, ProjectId>,
    /// Adjacency list: project ID -> IDs of projects it references
    references: FxHashMap<ProjectId, Vec<ProjectId>>,
    /// Reverse adjacency: project ID -> IDs of projects that reference it
    dependents: FxHashMap<ProjectId, Vec<ProjectId>>,
}

impl ProjectReferenceGraph {
    /// Create a new empty graph
    pub fn new() -> Self {
        Self::default()
    }

    /// Load a project reference graph starting from a root tsconfig
    pub fn load(root_config_path: &Path) -> Result<Self> {
        let mut graph = Self::new();
        let mut visited = FxHashSet::default();
        let mut stack = Vec::new();

        // Start with the root project
        let canonical_root = std::fs::canonicalize(root_config_path).with_context(|| {
            format!(
                "failed to canonicalize root config: {}",
                root_config_path.display()
            )
        })?;

        stack.push(canonical_root);

        // BFS to load all referenced projects
        while let Some(config_path) = stack.pop() {
            if visited.contains(&config_path) {
                continue;
            }
            visited.insert(config_path.clone());

            let project = load_project(&config_path)?;
            graph.add_project(project.clone());

            // Queue referenced projects for loading
            for ref_info in &project.resolved_references {
                if ref_info.is_valid && !visited.contains(&ref_info.config_path) {
                    stack.push(ref_info.config_path.clone());
                }
            }
        }

        // Build the reference edges
        graph.build_edges()?;

        Ok(graph)
    }

    /// Add a project to the graph
    fn add_project(&mut self, project: ResolvedProject) -> ProjectId {
        let id = self.projects.len();
        self.path_to_id.insert(project.config_path.clone(), id);
        self.projects.push(project);
        self.references.insert(id, Vec::new());
        self.dependents.insert(id, Vec::new());
        id
    }

    /// Build reference edges between projects
    fn build_edges(&mut self) -> Result<()> {
        for (id, project) in self.projects.iter().enumerate() {
            for ref_info in &project.resolved_references {
                if !ref_info.is_valid {
                    continue;
                }
                if let Some(&ref_id) = self.path_to_id.get(&ref_info.config_path) {
                    self.references
                        .get_mut(&id)
                        .expect("project id exists in references map (inserted in build_graph)")
                        .push(ref_id);
                    self.dependents
                        .get_mut(&ref_id)
                        .expect("reference id exists in dependents map (inserted in build_graph)")
                        .push(id);
                }
            }
        }
        Ok(())
    }

    /// Get project by ID
    pub fn get_project(&self, id: ProjectId) -> Option<&ResolvedProject> {
        self.projects.get(id)
    }

    /// Get project ID by config path
    pub fn get_project_id(&self, config_path: &Path) -> Option<ProjectId> {
        self.path_to_id.get(config_path).copied()
    }

    /// Get all projects
    pub fn projects(&self) -> &[ResolvedProject] {
        &self.projects
    }

    /// Get the number of projects
    pub const fn project_count(&self) -> usize {
        self.projects.len()
    }

    /// Get direct references of a project
    pub fn get_references(&self, id: ProjectId) -> &[ProjectId] {
        self.references.get(&id).map_or(&[], |v| v.as_slice())
    }

    /// Get direct dependents of a project (projects that reference it)
    pub fn get_dependents(&self, id: ProjectId) -> &[ProjectId] {
        self.dependents.get(&id).map_or(&[], |v| v.as_slice())
    }

    fn compare_project_ids(&self, left: ProjectId, right: ProjectId) -> std::cmp::Ordering {
        self.projects[left]
            .config_path
            .cmp(&self.projects[right].config_path)
    }

    fn sorted_project_ids(&self) -> Vec<ProjectId> {
        let mut ids: Vec<ProjectId> = (0..self.projects.len()).collect();
        ids.sort_by(|left, right| self.compare_project_ids(*left, *right));
        ids
    }

    fn has_self_reference(&self, id: ProjectId) -> bool {
        self.get_references(id).contains(&id)
    }

    /// Build the SCC condensation graph in deterministic path order.
    pub fn condensation_graph(&self) -> ProjectReferenceCondensation {
        #[derive(Default)]
        struct TarjanState {
            index: usize,
            indices: Vec<Option<usize>>,
            lowlinks: Vec<usize>,
            stack: Vec<ProjectId>,
            on_stack: FxHashSet<ProjectId>,
            components: Vec<Vec<ProjectId>>,
        }

        fn strong_connect(
            graph: &ProjectReferenceGraph,
            adjacency: &[Vec<ProjectId>],
            node: ProjectId,
            state: &mut TarjanState,
        ) {
            let node_index = state.index;
            state.indices[node] = Some(node_index);
            state.lowlinks[node] = node_index;
            state.index += 1;
            state.stack.push(node);
            state.on_stack.insert(node);

            for &neighbor in &adjacency[node] {
                if state.indices[neighbor].is_none() {
                    strong_connect(graph, adjacency, neighbor, state);
                    state.lowlinks[node] = state.lowlinks[node].min(state.lowlinks[neighbor]);
                } else if state.on_stack.contains(&neighbor)
                    && let Some(neighbor_index) = state.indices[neighbor]
                {
                    state.lowlinks[node] = state.lowlinks[node].min(neighbor_index);
                }
            }

            if state.lowlinks[node] == node_index {
                let mut component = Vec::new();
                while let Some(member) = state.stack.pop() {
                    state.on_stack.remove(&member);
                    component.push(member);
                    if member == node {
                        break;
                    }
                }
                component.sort_by(|left, right| graph.compare_project_ids(*left, *right));
                state.components.push(component);
            }
        }

        let project_count = self.projects.len();
        let mut adjacency = vec![Vec::new(); project_count];
        for node in 0..project_count {
            let mut refs = self.get_references(node).to_vec();
            refs.sort_by(|left, right| self.compare_project_ids(*left, *right));
            adjacency[node] = refs;
        }

        let mut state = TarjanState {
            index: 0,
            indices: vec![None; project_count],
            lowlinks: vec![0; project_count],
            stack: Vec::new(),
            on_stack: FxHashSet::default(),
            components: Vec::new(),
        };

        for node in self.sorted_project_ids() {
            if state.indices[node].is_none() {
                strong_connect(self, &adjacency, node, &mut state);
            }
        }

        state
            .components
            .sort_by(|left, right| self.compare_project_ids(left[0], right[0]));

        let mut project_to_component = vec![0; project_count];
        for (component_id, component) in state.components.iter().enumerate() {
            for &project_id in component {
                project_to_component[project_id] = component_id;
            }
        }

        let mut references: FxHashMap<ProjectSccId, Vec<ProjectSccId>> = FxHashMap::default();
        let mut dependents: FxHashMap<ProjectSccId, Vec<ProjectSccId>> = FxHashMap::default();
        for component_id in 0..state.components.len() {
            references.insert(component_id, Vec::new());
            dependents.insert(component_id, Vec::new());
        }

        let mut seen_edges = FxHashSet::default();
        for (&from_project, refs) in &self.references {
            let from_component = project_to_component[from_project];
            for &to_project in refs {
                let to_component = project_to_component[to_project];
                if from_component == to_component
                    || !seen_edges.insert((from_component, to_component))
                {
                    continue;
                }

                references
                    .get_mut(&from_component)
                    .expect("all SCC ids initialized in references map")
                    .push(to_component);
                dependents
                    .get_mut(&to_component)
                    .expect("all SCC ids initialized in dependents map")
                    .push(from_component);
            }
        }

        for edges in references.values_mut() {
            edges.sort_by(|left, right| {
                self.compare_project_ids(state.components[*left][0], state.components[*right][0])
            });
        }
        for edges in dependents.values_mut() {
            edges.sort_by(|left, right| {
                self.compare_project_ids(state.components[*left][0], state.components[*right][0])
            });
        }

        ProjectReferenceCondensation {
            components: state.components,
            project_to_component,
            references,
            dependents,
        }
    }

    /// Get all strongly connected components in deterministic path order.
    pub fn strongly_connected_components(&self) -> Vec<Vec<ProjectId>> {
        self.condensation_graph().components
    }

    /// Check for circular references
    pub fn detect_cycles(&self) -> Vec<Vec<ProjectId>> {
        self.strongly_connected_components()
            .into_iter()
            .filter(|component| {
                component.len() > 1
                    || component
                        .first()
                        .copied()
                        .is_some_and(|project_id| self.has_self_reference(project_id))
            })
            .collect()
    }

    /// Get a topologically sorted build order
    /// Returns Err if there are cycles that prevent ordering
    pub fn build_order(&self) -> Result<Vec<ProjectId>> {
        let cycles = self.detect_cycles();
        if !cycles.is_empty() {
            let cycle_desc: Vec<String> = cycles
                .iter()
                .map(|cycle| {
                    let names: Vec<String> = cycle
                        .iter()
                        .filter_map(|&id| self.projects.get(id))
                        .map(|p| p.config_path.display().to_string())
                        .collect();
                    names.join(" -> ")
                })
                .collect();
            bail!(
                "Circular project references detected:\n{}",
                cycle_desc.join("\n")
            );
        }

        // Kahn's algorithm for topological sort
        let mut in_degree: FxHashMap<ProjectId, usize> = FxHashMap::default();
        for id in 0..self.projects.len() {
            in_degree.insert(id, 0);
        }
        for refs in self.references.values() {
            for &ref_id in refs {
                *in_degree.entry(ref_id).or_insert(0) += 1;
            }
        }

        let mut queue: BinaryHeap<ProjectId> = in_degree
            .iter()
            .filter(|&(_, &deg)| deg == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut order = Vec::new();
        while let Some(node) = queue.pop() {
            order.push(node);
            for &neighbor in self.get_references(node) {
                let deg = in_degree
                    .get_mut(&neighbor)
                    .expect("all graph nodes initialized in in_degree map");
                *deg -= 1;
                if *deg == 0 {
                    queue.push(neighbor);
                }
            }
        }

        // Reverse because we want dependencies first
        order.reverse();
        Ok(order)
    }

    /// Get all transitive dependencies of a project
    pub fn transitive_dependencies(&self, id: ProjectId) -> FxHashSet<ProjectId> {
        self.condensation_graph().transitive_dependencies(id)
    }

    /// Get all projects that would be affected by changes in a project
    pub fn affected_projects(&self, id: ProjectId) -> FxHashSet<ProjectId> {
        self.condensation_graph().affected_projects(id)
    }

    /// Validate project reference constraints.
    /// Returns a list of (`error_code`, message) pairs for any violations.
    pub fn validate(&self) -> Vec<ProjectReferenceDiagnostic> {
        let mut diagnostics = Vec::new();

        for (id, project) in self.projects.iter().enumerate() {
            for ref_info in &project.resolved_references {
                if !ref_info.is_valid {
                    continue;
                }
                if let Some(&ref_id) = self.path_to_id.get(&ref_info.config_path) {
                    let ref_project = &self.projects[ref_id];

                    // TS6306: Referenced project must have composite: true
                    if !ref_project.is_composite {
                        diagnostics.push(ProjectReferenceDiagnostic {
                            code: 6306,
                            message: format!(
                                "Referenced project '{}' must have setting \"composite\": true.",
                                ref_project.config_path.display()
                            ),
                            project_id: id,
                            referenced_project_id: Some(ref_id),
                        });
                    }

                    // TS6310: Referenced project may not disable emit
                    if ref_project.no_emit {
                        diagnostics.push(ProjectReferenceDiagnostic {
                            code: 6310,
                            message: format!(
                                "Referenced project '{}' may not disable emit.",
                                ref_project.config_path.display()
                            ),
                            project_id: id,
                            referenced_project_id: Some(ref_id),
                        });
                    }
                }
            }
        }

        // TS6202: Circular references
        let cycles = self.detect_cycles();
        for cycle in &cycles {
            let names: Vec<String> = cycle
                .iter()
                .filter_map(|&id| self.projects.get(id))
                .map(|p| p.config_path.display().to_string())
                .collect();
            diagnostics.push(ProjectReferenceDiagnostic {
                code: 6202,
                message: format!(
                    "Project references may not form a circular graph. Cycle detected: {}",
                    names.join(" -> ")
                ),
                project_id: cycle.first().copied().unwrap_or(0),
                referenced_project_id: None,
            });
        }

        diagnostics
    }
}

/// A diagnostic from project reference validation
#[derive(Debug, Clone)]
pub struct ProjectReferenceDiagnostic {
    /// The diagnostic error code (e.g., 6306, 6310, 6202)
    pub code: u32,
    /// The diagnostic message
    pub message: String,
    /// The project that triggered this diagnostic
    pub project_id: ProjectId,
    /// The referenced project that has the issue (if applicable)
    pub referenced_project_id: Option<ProjectId>,
}

/// Load a project from its tsconfig.json path
pub fn load_project(config_path: &Path) -> Result<ResolvedProject> {
    let source = std::fs::read_to_string(config_path)
        .with_context(|| format!("failed to read tsconfig: {}", config_path.display()))?;

    let config = parse_tsconfig_with_references(&source)
        .with_context(|| format!("failed to parse tsconfig: {}", config_path.display()))?;

    let root_dir = config_path
        .parent()
        .ok_or_else(|| anyhow!("tsconfig has no parent directory"))?
        .to_path_buf();

    let root_dir = std::fs::canonicalize(&root_dir).unwrap_or(root_dir);

    // Resolve project references
    let resolved_references = resolve_project_references(&root_dir, &config.references)?;

    // Check composite from the deserialized CompilerOptions field
    let is_composite = config
        .base
        .compiler_options
        .as_ref()
        .and_then(|opts| opts.composite)
        .unwrap_or(false);

    // Check noEmit
    let no_emit = config
        .base
        .compiler_options
        .as_ref()
        .and_then(|opts| opts.no_emit)
        .unwrap_or(false);

    // Get output directories
    let declaration_dir = config
        .base
        .compiler_options
        .as_ref()
        .and_then(|opts| opts.declaration_dir.as_ref())
        .map(|d| root_dir.join(d));

    let out_dir = config
        .base
        .compiler_options
        .as_ref()
        .and_then(|opts| opts.out_dir.as_ref())
        .map(|d| root_dir.join(d));

    Ok(ResolvedProject {
        config_path: std::fs::canonicalize(config_path)
            .unwrap_or_else(|_| config_path.to_path_buf()),
        root_dir,
        config,
        resolved_references,
        is_composite,
        no_emit,
        declaration_dir,
        out_dir,
    })
}

/// Parse tsconfig with references support
pub fn parse_tsconfig_with_references(source: &str) -> Result<TsConfigWithReferences> {
    let stripped = strip_jsonc(source);
    let normalized = remove_trailing_commas(&stripped);
    let config = serde_json::from_str(&normalized)
        .context("failed to parse tsconfig JSON with references")?;
    Ok(config)
}

/// Resolve project references to absolute paths
fn resolve_project_references(
    root_dir: &Path,
    references: &Option<Vec<ProjectReference>>,
) -> Result<Vec<ResolvedProjectReference>> {
    let Some(refs) = references else {
        return Ok(Vec::new());
    };

    let mut resolved = Vec::with_capacity(refs.len());

    for ref_entry in refs {
        let resolved_ref = resolve_single_reference(root_dir, ref_entry);
        resolved.push(resolved_ref);
    }

    Ok(resolved)
}

/// Resolve a single project reference
fn resolve_single_reference(
    root_dir: &Path,
    reference: &ProjectReference,
) -> ResolvedProjectReference {
    let ref_path = PathBuf::from(&reference.path);

    // Make path absolute
    let abs_path = if ref_path.is_absolute() {
        ref_path
    } else {
        root_dir.join(&ref_path)
    };

    // Check if it's a directory or a file
    let config_path = if abs_path.is_dir() {
        abs_path.join("tsconfig.json")
    } else if abs_path.extension().is_some_and(|ext| ext == "json") {
        abs_path
    } else {
        // Assume directory and append tsconfig.json
        abs_path.join("tsconfig.json")
    };

    // Canonicalize if possible
    let canonical_path =
        std::fs::canonicalize(&config_path).unwrap_or_else(|_| config_path.clone());

    // Validate the reference exists
    let (is_valid, error) = if canonical_path.exists() {
        (true, None)
    } else {
        (
            false,
            Some(format!(
                "Referenced project not found: {}",
                config_path.display()
            )),
        )
    };

    ResolvedProjectReference {
        config_path: canonical_path,
        original: reference.clone(),
        is_valid,
        error,
    }
}

// Helper functions copied from config.rs (ideally these would be shared)
fn strip_jsonc(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escape = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while let Some(ch) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
                out.push(ch);
            }
            continue;
        }

        if in_block_comment {
            if ch == '*' {
                if let Some('/') = chars.peek().copied() {
                    chars.next();
                    in_block_comment = false;
                }
            } else if ch == '\n' {
                out.push(ch);
            }
            continue;
        }

        if in_string {
            out.push(ch);
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            out.push(ch);
            continue;
        }

        if ch == '/'
            && let Some(&next) = chars.peek()
        {
            if next == '/' {
                chars.next();
                in_line_comment = true;
                continue;
            }
            if next == '*' {
                chars.next();
                in_block_comment = true;
                continue;
            }
        }

        out.push(ch);
    }

    out
}

fn remove_trailing_commas(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escape = false;

    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            out.push(ch);
            continue;
        }

        if ch == ',' {
            let mut lookahead = chars.clone();
            while let Some(next) = lookahead.peek().copied() {
                if next.is_whitespace() {
                    lookahead.next();
                    continue;
                }
                break;
            }

            if let Some(next) = lookahead.peek().copied()
                && (next == '}' || next == ']')
            {
                continue;
            }
        }

        out.push(ch);
    }

    out
}

#[cfg(test)]
#[path = "project_refs_tests.rs"]
mod tests;
