use anyhow::{Context, Result};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};

use super::OutputFile;
use crate::config::JsxEmit;
use crate::driver::resolution::{is_declaration_file, normalize_path};
use tsz::emitter::NewLineKind;
use tsz::parallel::MergedProgram;
use tsz_common::common::ModuleKind;
use tsz_common::diagnostics::Diagnostic;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;

#[derive(Debug, Clone)]
pub(super) struct DeclarationBundleChunk {
    pub(super) path_key: String,
    pub(super) referenced_path_keys: Vec<String>,
    pub(super) contents: String,
}

#[derive(Debug, Clone)]
pub(super) struct JsBundleChunk {
    pub(super) path_key: String,
    pub(super) referenced_path_keys: Vec<String>,
    pub(super) contents: String,
}

pub(super) fn build_program_file_lookup(program: &MergedProgram) -> FxHashMap<String, String> {
    program
        .files
        .iter()
        .map(|file| {
            let key = normalized_file_key(&file.file_name);
            (key.clone(), key)
        })
        .collect()
}

pub(super) fn normalized_file_key(file_name: &str) -> String {
    normalize_path(Path::new(file_name))
        .to_string_lossy()
        .replace('\\', "/")
}

pub(super) fn normalized_path_key(path: &Path) -> String {
    normalize_path(path).to_string_lossy().replace('\\', "/")
}

pub(super) fn resolve_relative_module_file(
    containing_file: &str,
    module_spec: &str,
    file_lookup: &FxHashMap<String, String>,
) -> Option<String> {
    if !(module_spec.starts_with("./") || module_spec.starts_with("../")) {
        return None;
    }
    let containing = Path::new(containing_file);
    let base = containing
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(module_spec);
    for candidate in module_resolution_candidates(&base) {
        let key = normalize_path(&candidate)
            .to_string_lossy()
            .replace('\\', "/");
        if let Some(path) = file_lookup.get(&key) {
            return Some(path.clone());
        }
    }
    None
}

pub(super) fn module_resolution_candidates(base: &Path) -> Vec<PathBuf> {
    if base.extension().is_some() {
        return vec![base.to_path_buf()];
    }
    vec![
        base.with_extension("ts"),
        base.with_extension("tsx"),
        base.with_extension("d.ts"),
        base.with_extension("js"),
        base.join("index.ts"),
        base.join("index.tsx"),
        base.join("index.d.ts"),
        base.join("index.js"),
    ]
}

pub(super) fn declaration_bundle_reference_path_keys(
    containing_file: &str,
    arena: &NodeArena,
    source_file_idx: NodeIndex,
    file_lookup: &FxHashMap<String, String>,
) -> Vec<String> {
    let Some(source_text) = arena
        .get(source_file_idx)
        .and_then(|node| arena.get_source_file(node))
        .map(|source| source.text.as_ref())
    else {
        return Vec::new();
    };

    tsz::checker::triple_slash_validator::extract_reference_paths(source_text)
        .into_iter()
        .filter_map(|(reference_path, _, _)| {
            resolve_declaration_reference_path_file(containing_file, &reference_path, file_lookup)
        })
        .collect()
}

pub(super) fn js_bundle_reference_path_keys(
    input_path: &Path,
    outfile_bundle_dependencies: Option<&FxHashMap<PathBuf, FxHashSet<PathBuf>>>,
) -> Vec<String> {
    let Some(dependencies) = outfile_bundle_dependencies.and_then(|deps| deps.get(input_path))
    else {
        return Vec::new();
    };

    dependencies
        .iter()
        .map(|path| normalized_path_key(path))
        .collect()
}

pub(super) fn resolve_declaration_reference_path_file(
    containing_file: &str,
    reference_path: &str,
    file_lookup: &FxHashMap<String, String>,
) -> Option<String> {
    if reference_path.is_empty() {
        return None;
    }

    let containing = Path::new(containing_file);
    let base_dir = containing.parent().unwrap_or_else(|| Path::new(""));
    let direct_reference = base_dir.join(reference_path);
    let mut candidates = vec![direct_reference];
    if !reference_path.contains('.') {
        for ext in tsz::checker::triple_slash_validator::reference_path_probe_extensions(true) {
            candidates.push(base_dir.join(format!("{reference_path}{ext}")));
        }
    }

    candidates.into_iter().find_map(|candidate| {
        let key = normalize_path(&candidate)
            .to_string_lossy()
            .replace('\\', "/");
        file_lookup.get(&key).cloned()
    })
}

pub(super) fn normalize_ts2883_diagnostics(diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut exact_seen: FxHashMap<(u32, String, u32, u32, String), usize> = FxHashMap::default();
    let mut unique: Vec<(Diagnostic, bool)> = Vec::new();

    for diagnostic in diagnostics {
        let mut diagnostic = diagnostic;
        let mut was_canonicalized = false;
        if diagnostic.code == 2883
            && let Some(message) =
                canonical_ts2883_named_reference_message(&diagnostic.message_text)
        {
            diagnostic.message_text = message;
            was_canonicalized = true;
        }
        let exact_key = (
            diagnostic.code,
            diagnostic.file.clone(),
            diagnostic.start,
            diagnostic.length,
            diagnostic.message_text.clone(),
        );
        if let Some(&existing_idx) = exact_seen.get(&exact_key) {
            if !was_canonicalized && unique[existing_idx].1 {
                unique[existing_idx] = (diagnostic, was_canonicalized);
            }
            continue;
        }

        exact_seen.insert(exact_key, unique.len());
        unique.push((diagnostic, was_canonicalized));
    }

    let surviving_canonical_sites: FxHashSet<_> = unique
        .iter()
        .filter_map(|(diagnostic, was_canonicalized)| {
            if diagnostic.code != 2883 || *was_canonicalized {
                return None;
            }
            let (first, second) = parse_ts2883_named_reference_message(&diagnostic.message_text)?;
            (!looks_like_module_path(&first) && looks_like_module_path(&second))
                .then(|| (diagnostic.file.clone(), diagnostic.start, diagnostic.length))
        })
        .collect();

    unique
        .into_iter()
        .filter_map(|(diagnostic, was_canonicalized)| {
            if diagnostic.code != 2883 {
                return Some(diagnostic);
            }

            let Some((first, second)) =
                parse_ts2883_named_reference_message(&diagnostic.message_text)
            else {
                return Some(diagnostic);
            };

            if !was_canonicalized
                || looks_like_module_path(&first)
                || !looks_like_module_path(&second)
            {
                return Some(diagnostic);
            }

            (!surviving_canonical_sites.contains(&(
                diagnostic.file.clone(),
                diagnostic.start,
                diagnostic.length,
            )))
            .then_some(diagnostic)
        })
        .collect()
}

pub(super) fn parse_ts2883_named_reference_message(message: &str) -> Option<(String, String)> {
    let prefix = "cannot be named without a reference to '";
    let start = message.find(prefix)? + prefix.len();
    let rest = &message[start..];
    let (first, tail) = rest.split_once("' from '")?;
    let (second, _) = tail.split_once('\'')?;
    Some((first.to_string(), second.to_string()))
}

pub(super) fn canonical_ts2883_named_reference_message(message: &str) -> Option<String> {
    let (first, second) = parse_ts2883_named_reference_message(message)?;
    if !looks_like_module_path(&first) || looks_like_module_path(&second) {
        return None;
    }

    Some(message.replace(
        &format!("reference to '{first}' from '{second}'"),
        &format!("reference to '{second}' from '{first}'"),
    ))
}

pub(super) fn looks_like_module_path(text: &str) -> bool {
    text.starts_with('.')
        || text.starts_with('/')
        || text.contains('/')
        || text.contains('\\')
        || text.contains("node_modules")
}

pub(super) fn map_output_info(output_path: &Path) -> Option<(PathBuf, String, String)> {
    let output_name = output_path.file_name()?.to_string_lossy().into_owned();
    let map_name = format!("{output_name}.map");
    let map_path = output_path.with_file_name(&map_name);
    Some((map_path, map_name, output_name))
}

pub(super) fn declaration_map_source_name(map_path: &Path, source_path: &Path) -> String {
    let map_dir = map_path.parent().unwrap_or_else(|| Path::new(""));
    relative_path_from_dir(map_dir, source_path)
        .unwrap_or_else(|| source_path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
}

pub(super) fn relative_path_from_dir(from_dir: &Path, to_path: &Path) -> Option<PathBuf> {
    if from_dir.is_absolute() != to_path.is_absolute() {
        return None;
    }

    let from_components = normalized_path_components(from_dir);
    let to_components = normalized_path_components(to_path);
    let mut common_len = 0;
    while common_len < from_components.len()
        && common_len < to_components.len()
        && from_components[common_len] == to_components[common_len]
    {
        common_len += 1;
    }

    let mut relative = PathBuf::new();
    for _ in common_len..from_components.len() {
        relative.push("..");
    }
    for component in &to_components[common_len..] {
        relative.push(component);
    }

    Some(relative)
}

pub(super) fn normalized_path_components(path: &Path) -> Vec<std::ffi::OsString> {
    path.components()
        .filter_map(|component| match component {
            std::path::Component::CurDir | std::path::Component::RootDir => None,
            std::path::Component::ParentDir => Some(std::ffi::OsString::from("..")),
            std::path::Component::Normal(part) => Some(part.to_os_string()),
            std::path::Component::Prefix(prefix) => Some(prefix.as_os_str().to_os_string()),
        })
        .collect()
}

pub(super) fn append_source_mapping_url(contents: &mut String, map_name: &str, new_line: &str) {
    if !contents.is_empty() && !contents.ends_with(new_line) {
        contents.push_str(new_line);
    }
    contents.push_str("//# sourceMappingURL=");
    contents.push_str(map_name);
}

pub(super) fn append_inline_source_mapping_url(
    contents: &mut String,
    map_json: &str,
    new_line: &str,
) {
    if !contents.is_empty() && !contents.ends_with(new_line) {
        contents.push_str(new_line);
    }
    contents.push_str("//# sourceMappingURL=data:application/json;base64,");
    contents.push_str(&base64_encode(map_json.as_bytes()));
}

pub(super) fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);

        encoded.push(ALPHABET[(b0 >> 2) as usize] as char);
        encoded.push(ALPHABET[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            encoded.push(ALPHABET[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            encoded.push('=');
        }
        if chunk.len() > 2 {
            encoded.push(ALPHABET[(b2 & 0b0011_1111) as usize] as char);
        } else {
            encoded.push('=');
        }
    }

    encoded
}

pub(super) const fn new_line_str(kind: NewLineKind) -> &'static str {
    match kind {
        NewLineKind::LineFeed => "\n",
        NewLineKind::CarriageReturnLineFeed => "\r\n",
    }
}

pub(super) fn write_outputs_impl(outputs: &[OutputFile], emit_bom: bool) -> Result<Vec<PathBuf>> {
    outputs.par_iter().try_for_each(|output| -> Result<()> {
        if let Some(parent) = output.path.parent() {
            std::fs::create_dir_all::<&Path>(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }
        let contents = if emit_bom && !output.contents.starts_with('\u{feff}') {
            format!("\u{feff}{}", output.contents)
        } else {
            output.contents.clone()
        };
        std::fs::write(&output.path, contents)
            .with_context(|| format!("failed to write {}", output.path.display()))?;
        Ok(())
    })?;

    Ok(outputs.iter().map(|output| output.path.clone()).collect())
}

pub(super) fn js_output_path(
    base_dir: &Path,
    root_dir: Option<&Path>,
    out_dir: Option<&Path>,
    jsx: Option<JsxEmit>,
    input_path: &Path,
) -> Option<PathBuf> {
    if is_declaration_file(input_path) {
        return None;
    }

    let extension = js_extension_for(input_path, jsx)?;
    let mut output = if should_emit_next_to_source(root_dir, out_dir, input_path) {
        input_path.to_path_buf()
    } else {
        let relative = output_relative_path(base_dir, root_dir, input_path);
        match out_dir {
            Some(out_dir) => out_dir.join(relative),
            None => input_path.to_path_buf(),
        }
    };
    output.set_extension(extension);
    Some(output)
}

pub(super) fn declaration_output_path(
    base_dir: &Path,
    root_dir: Option<&Path>,
    out_dir: Option<&Path>,
    input_path: &Path,
) -> Option<PathBuf> {
    if is_declaration_file(input_path) {
        return None;
    }

    let relative = output_relative_path(base_dir, root_dir, input_path);
    let file_name = relative.file_name()?.to_str()?;
    let new_name = declaration_file_name(file_name)?;

    let mut output = if should_emit_next_to_source(root_dir, out_dir, input_path) {
        input_path.to_path_buf()
    } else {
        match out_dir {
            Some(out_dir) => out_dir.join(relative),
            None => input_path.to_path_buf(),
        }
    };
    output.set_file_name(new_name);
    Some(output)
}

pub(super) fn declaration_bundle_output_path(
    base_dir: &Path,
    out_dir: Option<&Path>,
    out_file: &Path,
) -> Option<PathBuf> {
    let relative = if out_file.is_absolute() {
        PathBuf::from(out_file.file_name()?)
    } else {
        out_file.to_path_buf()
    };

    let mut output = match out_dir {
        Some(out_dir) => out_dir.join(&relative),
        None if out_file.is_absolute() => out_file.to_path_buf(),
        None => base_dir.join(&relative),
    };
    let file_name = output.file_name()?.to_str()?;
    let new_name = declaration_file_name(file_name)?;
    output.set_file_name(new_name);
    Some(output)
}

pub(super) fn join_declaration_bundle_chunks(
    chunks: &[DeclarationBundleChunk],
    new_line: &str,
) -> String {
    let ordered_indices = declaration_bundle_chunk_order(chunks);
    let mut bundled = String::new();
    for chunk in ordered_indices
        .into_iter()
        .filter_map(|idx| chunks.get(idx))
        .map(|chunk| chunk.contents.as_str())
    {
        if !bundled.is_empty() && !bundled.ends_with(new_line) {
            bundled.push_str(new_line);
        }
        bundled.push_str(chunk.trim_end_matches(['\r', '\n']));
        bundled.push_str(new_line);
    }
    if bundled.ends_with(new_line) {
        bundled.truncate(bundled.len() - new_line.len());
    }
    bundled
}

pub(super) fn declaration_bundle_chunk_order(chunks: &[DeclarationBundleChunk]) -> Vec<usize> {
    bundle_chunk_order(
        chunks.len(),
        chunks.iter().enumerate().map(|(idx, chunk)| {
            (
                idx,
                chunk.path_key.as_str(),
                chunk.referenced_path_keys.as_slice(),
            )
        }),
    )
}

pub(super) fn js_bundle_chunk_order(chunks: &[JsBundleChunk]) -> Vec<usize> {
    bundle_chunk_order(
        chunks.len(),
        chunks.iter().enumerate().map(|(idx, chunk)| {
            (
                idx,
                chunk.path_key.as_str(),
                chunk.referenced_path_keys.as_slice(),
            )
        }),
    )
}

pub(super) fn bundle_chunk_order<'a, I>(len: usize, chunk_refs: I) -> Vec<usize>
where
    I: IntoIterator<Item = (usize, &'a str, &'a [String])>,
{
    let mut references_by_idx: Vec<&'a [String]> = vec![&[]; len];
    let mut by_path: FxHashMap<&'a str, usize> = FxHashMap::default();
    for (idx, path_key, referenced_path_keys) in chunk_refs {
        if idx < len {
            references_by_idx[idx] = referenced_path_keys;
            by_path.insert(path_key, idx);
        }
    }
    let mut ordered = Vec::with_capacity(len);
    let mut emitted = FxHashSet::default();
    let mut visiting = FxHashSet::default();

    fn visit(
        idx: usize,
        references_by_idx: &[&[String]],
        by_path: &FxHashMap<&str, usize>,
        emitted: &mut FxHashSet<usize>,
        visiting: &mut FxHashSet<usize>,
        ordered: &mut Vec<usize>,
    ) {
        if emitted.contains(&idx) || !visiting.insert(idx) {
            return;
        }

        let Some(references) = references_by_idx.get(idx) else {
            return;
        };
        for referenced_path in *references {
            if let Some(&referenced_idx) = by_path.get(referenced_path.as_str()) {
                visit(
                    referenced_idx,
                    references_by_idx,
                    by_path,
                    emitted,
                    visiting,
                    ordered,
                );
            }
        }

        visiting.remove(&idx);
        if emitted.insert(idx) {
            ordered.push(idx);
        }
    }

    for idx in 0..len {
        visit(
            idx,
            &references_by_idx,
            &by_path,
            &mut emitted,
            &mut visiting,
            &mut ordered,
        );
    }

    ordered
}

pub(super) fn bundle_declaration_output(
    contents: &str,
    module_kind: ModuleKind,
    fallback_module_name: Option<&str>,
) -> String {
    if !matches!(module_kind, ModuleKind::AMD) {
        return contents.to_string();
    }

    wrap_amd_declaration_output(contents, fallback_module_name)
        .unwrap_or_else(|| contents.to_string())
}

pub(super) fn wrap_amd_declaration_output(
    contents: &str,
    fallback_module_name: Option<&str>,
) -> Option<String> {
    let mut directive_lines = Vec::new();
    let mut body_lines = Vec::new();
    let mut in_header = true;
    let mut amd_module_name = None;

    for line in contents.lines() {
        if in_header && line.trim_start().starts_with("///") {
            directive_lines.push(line.to_string());
            if amd_module_name.is_none() && is_amd_module_directive(line) {
                amd_module_name = extract_amd_module_name(line);
            }
            continue;
        }
        in_header = false;
        body_lines.push(line.to_string());
    }

    let amd_module_name = amd_module_name.or_else(|| fallback_module_name.map(str::to_string))?;
    let mut wrapped = String::new();
    for directive in directive_lines {
        wrapped.push_str(&directive);
        wrapped.push('\n');
    }
    wrapped.push_str("declare module \"");
    wrapped.push_str(&amd_module_name);
    wrapped.push_str("\" {\n");

    for line in body_lines {
        let Some(rewritten) = rewrite_ambient_module_member_line(&line, &amd_module_name) else {
            continue;
        };
        for rewritten_line in rewritten.lines() {
            wrapped.push_str("    ");
            wrapped.push_str(rewritten_line);
            wrapped.push('\n');
        }
    }

    wrapped.push('}');
    Some(wrapped)
}

pub(super) fn extract_amd_module_name(line: &str) -> Option<String> {
    let needle = "name=";
    let pos = line.find(needle)?;
    let after = &line[pos + needle.len()..];
    let quote = after.as_bytes().first().copied()?;
    if !matches!(quote, b'\'' | b'"') {
        return None;
    }
    let quote = quote as char;
    let end = after[1..].find(quote)?;
    Some(after[1..1 + end].to_string())
}

pub(super) fn is_amd_module_directive(line: &str) -> bool {
    let Some(rest) = line.trim_start().strip_prefix("///") else {
        return false;
    };
    let Some(after_tag) = rest.trim_start().strip_prefix("<amd-module") else {
        return false;
    };
    match after_tag.chars().next() {
        None => true,
        Some(ch) => ch.is_ascii_whitespace() || matches!(ch, '/' | '>'),
    }
}

pub(super) fn rewrite_ambient_module_member_line(line: &str, module_name: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];

    if trimmed == "export {};" {
        return None;
    }

    if let Some((comment, declaration)) = split_leading_single_line_jsdoc_declaration(trimmed) {
        let rewritten_declaration = rewrite_ambient_module_member_declaration_line(
            &format!("{indent}{declaration}"),
            module_name,
        )?;
        return Some(format!("{indent}{comment}\n{rewritten_declaration}"));
    }

    rewrite_ambient_module_member_declaration_line(line, module_name)
}

pub(super) fn split_leading_single_line_jsdoc_declaration(trimmed: &str) -> Option<(&str, &str)> {
    if !trimmed.starts_with("/**") {
        return None;
    }

    let comment_end = trimmed.find("*/")? + "*/".len();
    let declaration = trimmed[comment_end..].trim_start();
    if declaration.starts_with("declare ") || declaration.starts_with("export declare ") {
        Some((&trimmed[..comment_end], declaration))
    } else {
        None
    }
}

pub(super) fn rewrite_ambient_module_member_declaration_line(
    line: &str,
    module_name: &str,
) -> Option<String> {
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];

    if let Some(rest) = trimmed.strip_prefix("export declare ") {
        return Some(rewrite_amd_relative_module_specifier_line(
            format!("{indent}export {rest}"),
            module_name,
        ));
    }
    if let Some(rest) = trimmed.strip_prefix("declare ") {
        return Some(rewrite_amd_relative_module_specifier_line(
            format!("{indent}{rest}"),
            module_name,
        ));
    }

    Some(rewrite_amd_relative_module_specifier_line(
        line.to_string(),
        module_name,
    ))
}

pub(super) fn rewrite_amd_relative_module_specifier_line(
    line: String,
    module_name: &str,
) -> String {
    let trimmed = line.trim_start();
    if trimmed.starts_with("/*") || trimmed.starts_with('*') || trimmed.starts_with("//") {
        return line;
    }

    let line = rewrite_amd_relative_import_type_specifiers(line, module_name);
    let trimmed = line.trim_start();
    let Some(after_keyword) = trimmed
        .strip_prefix("module \"")
        .or_else(|| trimmed.strip_prefix("import \""))
        .or_else(|| {
            trimmed
                .strip_prefix("import ")
                .and_then(|rest| rest.rsplit_once(" from \"").map(|(_, spec)| spec))
        })
    else {
        return line;
    };
    let Some(end_quote) = after_keyword.find('"') else {
        return line;
    };
    let specifier = &after_keyword[..end_quote];
    if !specifier.starts_with('.') {
        return line;
    }
    let Some(resolved) = resolve_amd_relative_module_specifier(module_name, specifier) else {
        return line;
    };

    let spec_start = line.len() - trimmed.len()
        + trimmed
            .find(specifier)
            .expect("specifier came from trimmed line");
    let spec_end = spec_start + specifier.len();
    let mut rewritten = String::with_capacity(line.len() + resolved.len());
    rewritten.push_str(&line[..spec_start]);
    rewritten.push_str(&resolved);
    rewritten.push_str(&line[spec_end..]);
    rewritten
}

pub(super) fn rewrite_amd_relative_import_type_specifiers(
    line: String,
    module_name: &str,
) -> String {
    let mut rewritten = String::with_capacity(line.len());
    let mut rest = line.as_str();

    while let Some(start) = rest.find("import(") {
        rewritten.push_str(&rest[..start]);
        let after_import = &rest[start + "import(".len()..];
        let Some(quote) = after_import.as_bytes().first().copied() else {
            rewritten.push_str(&rest[start..]);
            return rewritten;
        };
        if !matches!(quote, b'\'' | b'"') {
            rewritten.push_str("import(");
            rest = after_import;
            continue;
        }

        let quote_ch = quote as char;
        let specifier_start = 1;
        let Some(specifier_end) = after_import[specifier_start..].find(quote_ch) else {
            rewritten.push_str(&rest[start..]);
            return rewritten;
        };
        let specifier_end = specifier_start + specifier_end;
        let specifier = &after_import[specifier_start..specifier_end];
        let after_specifier = &after_import[specifier_end + quote_ch.len_utf8()..];

        rewritten.push_str("import(");
        rewritten.push(quote_ch);
        if specifier.starts_with('.') {
            if let Some(resolved) = resolve_amd_relative_module_specifier(module_name, specifier) {
                rewritten.push_str(&resolved);
            } else {
                rewritten.push_str(specifier);
            }
        } else {
            rewritten.push_str(specifier);
        }
        rewritten.push(quote_ch);
        rest = after_specifier;
    }

    rewritten.push_str(rest);
    rewritten
}

pub(super) fn resolve_amd_relative_module_specifier(
    module_name: &str,
    specifier: &str,
) -> Option<String> {
    let base_dir = module_name
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or("");
    let mut parts: Vec<&str> = if base_dir.is_empty() {
        Vec::new()
    } else {
        base_dir.split('/').collect()
    };
    for part in specifier.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    if let Some(last) = parts.last_mut() {
        *last = last
            .strip_suffix(".ts")
            .or_else(|| last.strip_suffix(".tsx"))
            .or_else(|| last.strip_suffix(".js"))
            .or_else(|| last.strip_suffix(".jsx"))
            .unwrap_or(last);
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

pub(super) fn output_relative_path(
    base_dir: &Path,
    root_dir: Option<&Path>,
    input_path: &Path,
) -> PathBuf {
    if let Some(root_dir) = root_dir
        && let Ok(relative) = input_path.strip_prefix(root_dir)
    {
        return relative.to_path_buf();
    }

    input_path
        .strip_prefix(base_dir)
        .unwrap_or(input_path)
        .to_path_buf()
}

pub(super) fn should_emit_next_to_source(
    root_dir: Option<&Path>,
    out_dir: Option<&Path>,
    input_path: &Path,
) -> bool {
    root_dir.is_some_and(|root_dir| input_path.strip_prefix(root_dir).is_err()) && out_dir.is_some()
}

pub(super) fn bundled_module_name(
    base_dir: &Path,
    root_dir: Option<&Path>,
    input_path: &Path,
) -> Option<String> {
    let mut relative = output_relative_path(base_dir, root_dir, input_path);
    relative.set_extension("");
    let module_name = relative.to_string_lossy().replace('\\', "/");
    (!module_name.is_empty()).then_some(module_name)
}

pub(super) fn declaration_file_name(file_name: &str) -> Option<String> {
    if file_name.ends_with(".mts") {
        return Some(file_name.trim_end_matches(".mts").to_string() + ".d.mts");
    }
    if file_name.ends_with(".mjs") {
        return Some(file_name.trim_end_matches(".mjs").to_string() + ".d.mts");
    }
    if file_name.ends_with(".cts") {
        return Some(file_name.trim_end_matches(".cts").to_string() + ".d.cts");
    }
    if file_name.ends_with(".cjs") {
        return Some(file_name.trim_end_matches(".cjs").to_string() + ".d.cts");
    }
    if file_name.ends_with(".tsx") {
        return Some(file_name.trim_end_matches(".tsx").to_string() + ".d.ts");
    }
    if file_name.ends_with(".ts") || file_name.ends_with(".jsx") || file_name.ends_with(".js") {
        let suffix = if file_name.ends_with(".ts") {
            ".ts"
        } else if file_name.ends_with(".jsx") {
            ".jsx"
        } else {
            ".js"
        };
        return Some(file_name.trim_end_matches(suffix).to_string() + ".d.ts");
    }

    None
}

pub(super) fn js_extension_for(path: &Path, jsx: Option<JsxEmit>) -> Option<&'static str> {
    let name = path.file_name().and_then(|name| name.to_str())?;
    if name.ends_with(".mts") {
        return Some("mjs");
    }
    if name.ends_with(".cts") {
        return Some("cjs");
    }

    match path.extension().and_then(|ext| ext.to_str()) {
        Some("tsx") => match jsx {
            Some(JsxEmit::Preserve) => Some("jsx"),
            Some(JsxEmit::React)
            | Some(JsxEmit::ReactJsx)
            | Some(JsxEmit::ReactJsxDev)
            | Some(JsxEmit::ReactNative)
            | None => Some("js"),
        },
        // .ts files emit as .js. JS input files (.js, .jsx, .mjs, .cjs) are valid
        // inputs that go through the emit pipeline (adding "use strict" for
        // alwaysStrict, module transforms, etc.) and produce output with the same
        // extension. This matches tsc behavior where `allowJs` files are emitted
        // alongside .ts files.
        Some("ts") | Some("js") => Some("js"),
        Some("jsx") => Some("jsx"),
        Some("mjs") => Some("mjs"),
        Some("cjs") => Some("cjs"),
        _ => None,
    }
}

pub(super) fn js_input_skipped_by_node_modules_depth(path: &Path, max_depth: u32) -> bool {
    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };
    if !matches!(ext, "js" | "jsx" | "mjs" | "cjs") {
        return false;
    }
    let depth = path
        .components()
        .filter(|component| component.as_os_str() == "node_modules")
        .count() as u32;
    depth > max_depth
}

pub(super) fn path_has_node_modules_segment(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == "node_modules")
}
