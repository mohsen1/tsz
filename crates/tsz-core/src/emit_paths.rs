//! Program-owned output path mapping.
//!
//! TypeScript preserves a source file's path relative to the program's emit
//! root when redirecting output. Keeping this decision outside the printer
//! prevents unrelated source files with the same basename from overwriting
//! each other.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::config::{CompilerOptionKey, ProjectProvenance};
use crate::diagnostics::{Diagnostic, RelatedInformation};
use crate::program::{
    CapabilityAnalysis, CapabilityScope, CapabilityTarget, CompilerOptions, ProgramFile,
    is_declaration_source,
};
use crate::source::{
    FileId, SourceText, display_path, normalize_clamped_path_lexically as normalize_lexically,
};

#[derive(Debug, Clone, Default)]
pub(crate) struct EmitFilePlan {
    pub(crate) javascript: Option<PathBuf>,
    pub(crate) declaration: Option<PathBuf>,
}

/// A program-wide, validated set of output products.
///
/// Collision checks happen while this value is built. Printers consume only
/// the paths recorded here, so colliding products are skipped while unrelated
/// products remain eligible for TypeScript-compatible partial emit.
#[derive(Debug, Clone)]
pub(crate) struct EmitPlan {
    files: Vec<EmitFilePlan>,
    diagnostics: Vec<Diagnostic>,
    blocked_products: bool,
}

impl EmitPlan {
    pub(crate) fn empty(file_count: usize) -> Self {
        Self {
            files: vec![EmitFilePlan::default(); file_count],
            diagnostics: Vec::new(),
            blocked_products: false,
        }
    }

    pub(crate) fn for_program(
        files: &[ProgramFile],
        options: &CompilerOptions,
        provenance: &ProjectProvenance,
        capabilities: &CapabilityAnalysis,
    ) -> Self {
        let config_path = provenance.entry_config_path();
        let configured_root = config_path.and_then(Path::parent);
        let paths = EmitPaths::for_program(files, configured_root, options.root_dir.as_deref());
        let file_slots = files
            .iter()
            .map(|file| file.source.id.0 as usize + 1)
            .max()
            .unwrap_or(0);
        let mut plan = Self::empty(file_slots);
        plan.diagnostics
            .extend(paths.explicit_root_diagnostics(files, options, provenance));
        if options.no_emit {
            return plan;
        }
        let mut products = Vec::new();

        for file in files {
            if is_declaration_source(&file.source.path) {
                continue;
            }
            let declarations = options
                .declaration_dir
                .as_deref()
                .or(options.out_dir.as_deref());
            let products_for_file = [
                (
                    CapabilityTarget::JavaScript,
                    options.out_dir.as_deref(),
                    false,
                    options.source_map && !options.inline_source_map,
                ),
                (
                    CapabilityTarget::Declaration,
                    declarations,
                    true,
                    options.declaration_map,
                ),
            ];
            for (capability, directory, declaration, map_enabled) in products_for_file
                .into_iter()
                .take(if options.declaration { 2 } else { 1 })
            {
                if !capabilities
                    .claim(capability, CapabilityScope::File(file.source.id))
                    .is_claimed()
                {
                    continue;
                }
                let output = paths.output_target(&file.source, directory, declaration);
                let map = map_enabled.then(|| output.map());
                let kind = if declaration {
                    plan.files[file.source.id.0 as usize].declaration = Some(output.path.clone());
                    ProductKind::Declaration
                } else {
                    plan.files[file.source.id.0 as usize].javascript = Some(output.path.clone());
                    ProductKind::Javascript
                };
                let product = |kind, target| PlannedProduct {
                    source: file.source.id,
                    kind,
                    target,
                };
                products.push(product(kind, output));
                products.extend(map.map(|target| product(ProductKind::Map, target)));
            }
        }

        if let Some(diagnostic) = paths.inferred_root_diagnostic(options, provenance) {
            plan.diagnostics.push(diagnostic);
        }
        plan.preflight(files, &products);
        plan
    }

    pub(crate) fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub(crate) const fn has_blocked_products(&self) -> bool {
        self.blocked_products
    }

    pub(crate) fn for_file(&self, id: FileId) -> &EmitFilePlan {
        &self.files[id.0 as usize]
    }

    fn preflight(&mut self, files: &[ProgramFile], products: &[PlannedProduct]) {
        let input_paths: BTreeSet<String> = files
            .iter()
            .map(|file| path_key(&file.source.host_path))
            .collect();
        let mut by_target: BTreeMap<String, Vec<&PlannedProduct>> = BTreeMap::new();
        for product in products {
            by_target
                .entry(path_key(&product.target.host_path))
                .or_default()
                .push(product);
        }

        for (key, products) in by_target {
            let target = &products[0].target.host_path;
            let diagnostic = if input_paths.contains(&key) {
                Diagnostic::global(
                    format!(
                        "Cannot write file '{}' because it would overwrite input file.",
                        display_path(target)
                    ),
                    5055,
                )
            } else if products
                .iter()
                .any(|product| product.source != products[0].source)
            {
                Diagnostic::global(
                    format!(
                        "Cannot write file '{}' because it would be overwritten by multiple input files.",
                        display_path(target)
                    ),
                    5056,
                )
            } else {
                continue;
            };
            self.diagnostics.push(diagnostic);
            for product in products {
                self.block_product(product);
            }
        }
    }

    fn block_product(&mut self, product: &PlannedProduct) {
        self.blocked_products = true;
        let file = &mut self.files[product.source.0 as usize];
        match product.kind {
            ProductKind::Javascript => file.javascript = None,
            ProductKind::Declaration => file.declaration = None,
            ProductKind::Map => {}
        }
    }
}

#[derive(Debug, Clone)]
struct PlannedProduct {
    source: FileId,
    kind: ProductKind,
    target: OutputTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProductKind {
    Javascript,
    Declaration,
    Map,
}

#[derive(Debug, Clone)]
struct OutputTarget {
    path: PathBuf,
    host_path: PathBuf,
}

impl OutputTarget {
    fn map(&self) -> Self {
        let [path, host_path] = [&self.path, &self.host_path].map(|path| {
            let mut path = path.clone();
            path.as_mut_os_string().push(".map");
            path
        });
        Self { path, host_path }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct EmitPaths {
    source_root: Option<PathBuf>,
    common_source_root: Option<PathBuf>,
    configured_root: Option<PathBuf>,
}

impl EmitPaths {
    /// Build the path mapping once for the whole program.
    ///
    /// A configuration project supplies its entry configuration directory as
    /// the source root. Command-line programs instead use the common directory
    /// of their emit-capable sources, matching TypeScript 7's two layouts.
    fn for_program(
        files: &[ProgramFile],
        configured_root: Option<&Path>,
        explicit_root: Option<&Path>,
    ) -> Self {
        let common_source_root = common_source_directory(files);
        let configured_root = configured_root.map(normalize_lexically);
        let source_root = explicit_root
            .map(normalize_lexically)
            .or_else(|| configured_root.clone())
            .or_else(|| common_source_root.clone());
        Self {
            source_root,
            common_source_root,
            configured_root,
        }
    }

    fn output_target(
        &self,
        source: &SourceText,
        directory: Option<&Path>,
        declaration: bool,
    ) -> OutputTarget {
        let name = output_file_name(&source.path, declaration);
        let beside_source = || OutputTarget {
            path: source.path.with_file_name(&name),
            host_path: source.host_path.with_file_name(&name),
        };
        let Some(directory) = directory else {
            return beside_source();
        };

        let source_path = normalize_lexically(&source.host_path);
        let Some(relative) = self
            .source_root
            .as_deref()
            .and_then(|root| source_path.strip_prefix(root).ok())
            .filter(|path| !path.as_os_str().is_empty())
        else {
            // TypeScript reports TS6059 for a configured source outside the
            // emit root and leaves that source's product beside the source.
            // The diagnostics owner is separate; path mapping must still not
            // collapse multiple outside roots to a shared basename.
            return beside_source();
        };
        let path = directory.join(relative.with_file_name(name));
        OutputTarget {
            host_path: host_path_for_logical_output(source, &path),
            path,
        }
    }

    fn inferred_root_diagnostic(
        &self,
        options: &CompilerOptions,
        provenance: &ProjectProvenance,
    ) -> Option<Diagnostic> {
        if options.root_dir.is_some()
            || (options.out_dir.is_none() && options.declaration_dir.is_none())
        {
            return None;
        }
        let config_path = provenance.entry_config_path()?;
        let configured_root = self.configured_root.as_deref()?;
        let common_source_root = self.common_source_root.as_deref()?;
        if common_source_root == configured_root {
            return None;
        }
        let config_name = config_path
            .file_name()
            .unwrap_or(config_path.as_os_str())
            .to_string_lossy();
        let inferred = relative_path(configured_root, common_source_root);
        let message = format!(
            "The common source directory of '{config_name}' is '{inferred}'. The 'rootDir' setting must be explicitly set to this or another path to adjust your output's file layout.\n  Visit https://aka.ms/ts6 for migration information."
        );
        let option = if options.out_dir.is_some() {
            CompilerOptionKey::OutDir
        } else {
            CompilerOptionKey::DeclarationDir
        };
        Some(
            if let Some(origin) = provenance.entry_option_origin(option) {
                origin.diagnostic_at_key(message, 5011)
            } else {
                Diagnostic::global(message, 5011)
            },
        )
    }

    fn explicit_root_diagnostics(
        &self,
        files: &[ProgramFile],
        options: &CompilerOptions,
        provenance: &ProjectProvenance,
    ) -> Vec<Diagnostic> {
        let Some(root) = options.root_dir.as_deref().map(normalize_lexically) else {
            return Vec::new();
        };
        files
            .iter()
            .filter(|file| !is_declaration_source(&file.source.host_path))
            .filter_map(|file| {
                let path = normalize_lexically(&file.source.host_path);
                if path.starts_with(&root) {
                    return None;
                }
                let related = provenance.root_reason(&path).map_or_else(Vec::new, |reason| {
                    let (message, code) = reason.diagnostic();
                    vec![
                        RelatedInformation::unlocated(
                            "The file is in the program because:",
                            1411,
                            1,
                        ),
                        RelatedInformation::unlocated(message, code, 2),
                    ]
                });
                Some(
                    Diagnostic::global(
                        format!(
                            "File '{}' is not under 'rootDir' '{}'. 'rootDir' is expected to contain all source files.",
                            display_path(&path),
                            display_path(&root)
                        ),
                        6059,
                    )
                    .with_related_information(related),
                )
            })
            .collect()
    }
}

fn common_source_directory(files: &[ProgramFile]) -> Option<PathBuf> {
    let mut directories = files
        .iter()
        .filter(|file| !is_declaration_source(&file.source.host_path))
        .filter_map(|file| {
            normalize_lexically(&file.source.host_path)
                .parent()
                .map(Path::to_path_buf)
        });
    let mut common = directories.next()?;
    for directory in directories {
        while !directory.starts_with(&common) && common.pop() {}
    }
    Some(common)
}

fn output_file_name(source: &Path, declaration: bool) -> String {
    let stem = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("output");
    let extension = source
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let extension = match (declaration, extension.as_str()) {
        (true, "mts") => "d.mts",
        (true, "cts") => "d.cts",
        (true, _) => "d.ts",
        (false, "mts") => "mjs",
        (false, "cts") => "cjs",
        (false, _) => "js",
    };
    format!("{stem}.{extension}")
}

fn relative_path(base: &Path, target: &Path) -> String {
    if let Ok(relative) = target.strip_prefix(base) {
        let relative = display_path(relative);
        return if relative.is_empty() {
            ".".to_string()
        } else {
            format!("./{relative}")
        };
    }
    display_path(target)
}

fn path_key(path: &Path) -> String {
    display_path(&normalize_lexically(path))
}

fn host_path_for_logical_output(source: &SourceText, output: &Path) -> PathBuf {
    if output.is_absolute() {
        return output.to_path_buf();
    }
    let logical_source = normalize_lexically(&source.path);
    let host_source = normalize_lexically(&source.host_path);
    if !host_source.is_absolute()
        || logical_source.is_absolute()
        || !host_source.ends_with(&logical_source)
    {
        return output.to_path_buf();
    }
    let mut working_directory = host_source;
    for _ in logical_source.components() {
        working_directory.pop();
    }
    working_directory.join(output)
}

#[cfg(test)]
#[path = "../rewrite-tests/emit_paths_unit.rs"]
mod tests;
