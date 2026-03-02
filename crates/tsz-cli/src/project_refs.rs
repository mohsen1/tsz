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

    /// Check for circular references
    pub fn detect_cycles(&self) -> Vec<Vec<ProjectId>> {
        let mut cycles = Vec::new();
        let mut visited = FxHashSet::default();
        let mut rec_stack = FxHashSet::default();
        let mut path = Vec::new();

        for id in 0..self.projects.len() {
            if !visited.contains(&id) {
                self.detect_cycles_dfs(id, &mut visited, &mut rec_stack, &mut path, &mut cycles);
            }
        }

        cycles
    }

    fn detect_cycles_dfs(
        &self,
        node: ProjectId,
        visited: &mut FxHashSet<ProjectId>,
        rec_stack: &mut FxHashSet<ProjectId>,
        path: &mut Vec<ProjectId>,
        cycles: &mut Vec<Vec<ProjectId>>,
    ) {
        visited.insert(node);
        rec_stack.insert(node);
        path.push(node);

        for &neighbor in self.get_references(node) {
            if !visited.contains(&neighbor) {
                self.detect_cycles_dfs(neighbor, visited, rec_stack, path, cycles);
            } else if rec_stack.contains(&neighbor) {
                // Found a cycle - extract it from path
                if let Some(start_idx) = path.iter().position(|&x| x == neighbor) {
                    cycles.push(path[start_idx..].to_vec());
                }
            }
        }

        path.pop();
        rec_stack.remove(&node);
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

        let mut queue: Vec<ProjectId> = in_degree
            .iter()
            .filter(|&(_, &deg)| deg == 0)
            .map(|(&id, _)| id)
            .collect();
        queue.sort(); // Deterministic order

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
            queue.sort(); // Keep deterministic
        }

        // Reverse because we want dependencies first
        order.reverse();
        Ok(order)
    }

    /// Get all transitive dependencies of a project
    pub fn transitive_dependencies(&self, id: ProjectId) -> FxHashSet<ProjectId> {
        let mut deps = FxHashSet::default();
        let mut stack = vec![id];

        while let Some(current) = stack.pop() {
            for &dep_id in self.get_references(current) {
                if deps.insert(dep_id) {
                    stack.push(dep_id);
                }
            }
        }

        deps
    }

    /// Get all projects that would be affected by changes in a project
    pub fn affected_projects(&self, id: ProjectId) -> FxHashSet<ProjectId> {
        let mut affected = FxHashSet::default();
        let mut stack = vec![id];

        while let Some(current) = stack.pop() {
            for &dep_id in self.get_dependents(current) {
                if affected.insert(dep_id) {
                    stack.push(dep_id);
                }
            }
        }

        affected
    }

    /// Validate project reference constraints.
    /// Returns a list of (error_code, message) pairs for any violations.
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
