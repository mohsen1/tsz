#!/usr/bin/env node
/**
 * generate-lib-assets.mjs
 *
 * Generates src/lib-assets/ directory with TypeScript lib.d.ts files and a manifest.
 * Sources libs from the typescript npm package (version from typescript-versions.json).
 *
 * This script should be run before cargo build to ensure lib files are available.
 * The lib files are NOT committed to the repo - they are fetched from npm at build time.
 *
 * Usage:
 *   node scripts/generate-lib-assets.mjs [--from-submodule] [--npm-version <version>]
 *
 * Options:
 *   --from-submodule   Use TypeScript/src/lib instead of npm package (for development)
 *   --npm-version      Override npm version (default: from typescript-versions.json)
 */

import fs from 'fs';
import path from 'path';
import { execSync } from 'child_process';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');

// Parse command line arguments
const args = process.argv.slice(2);
const fromSubmodule = args.includes('--from-submodule');
const forceRegenerate = args.includes('--force');
const npmVersionIdx = args.indexOf('--npm-version');
const overrideNpmVersion = npmVersionIdx !== -1 ? args[npmVersionIdx + 1] : null;

/**
 * Get TypeScript npm version from typescript-versions.json
 */
function getTypescriptNpmVersion() {
  if (overrideNpmVersion) {
    return overrideNpmVersion;
  }

  const versionsFile = path.join(ROOT_DIR, 'scripts/conformance/typescript-versions.json');
  try {
    const data = JSON.parse(fs.readFileSync(versionsFile, 'utf8'));

    // Try to get submodule SHA
    const submoduleSha = getSubmoduleSha();
    if (submoduleSha && data.mappings[submoduleSha]) {
      return data.mappings[submoduleSha].npm;
    }

    // Check partial SHA match
    if (submoduleSha) {
      const shortSha = submoduleSha.slice(0, 12);
      for (const [sha, mapping] of Object.entries(data.mappings)) {
        if (sha.startsWith(shortSha) || shortSha.startsWith(sha.slice(0, 12))) {
          return mapping.npm;
        }
      }
    }

    return data.default.npm;
  } catch {
    return '5.9.3'; // fallback
  }
}

/**
 * Get TypeScript submodule SHA
 */
function getSubmoduleSha() {
  try {
    const result = execSync('git submodule status TypeScript', {
      cwd: ROOT_DIR,
      encoding: 'utf8',
    });
    const match = result.match(/[+-]?([a-f0-9]{40})/);
    return match ? match[1] : null;
  } catch {
    return null;
  }
}

/**
 * Parse /// <reference lib="..." /> directives from content
 *
 * TypeScript lib files have this structure:
 * 1. Copyright block comment
 * 2. /// <reference no-default-lib="true"/>
 * 3. /// <reference lib="..." /> directives
 * 4. Actual declarations
 *
 * We need to skip the block comment and parse all reference directives.
 */
function parseLibReferences(content) {
  const refs = [];
  const lines = content.split('\n');
  let inBlockComment = false;

  for (const line of lines) {
    const trimmed = line.trim();

    // Handle block comments
    if (trimmed.startsWith('/*')) {
      inBlockComment = true;
    }
    if (inBlockComment) {
      if (trimmed.includes('*/')) {
        inBlockComment = false;
      }
      continue;
    }

    // Parse reference directives
    if (trimmed.startsWith('/// <reference lib=')) {
      const match = trimmed.match(/lib=["']([^"']+)["']/);
      if (match) {
        refs.push(match[1].toLowerCase());
      }
    } else if (trimmed.startsWith('/// <reference')) {
      // Skip other reference types (no-default-lib, path, types)
      continue;
    } else if (trimmed.startsWith('///')) {
      // Skip other triple-slash comments
      continue;
    } else if (trimmed.length > 0) {
      // Stop at first non-comment, non-empty line (actual declarations)
      break;
    }
  }
  return refs;
}

/**
 * Convert file name to lib name
 * e.g., "lib.es2015.promise.d.ts" -> "es2015.promise"
 *       "es5.d.ts" -> "es5"
 *       "dom.generated.d.ts" -> "dom"
 */
function fileNameToLibName(fileName) {
  let name = fileName;

  // Remove lib. prefix
  if (name.startsWith('lib.')) {
    name = name.slice(4);
  }

  // Remove .d.ts suffix
  if (name.endsWith('.d.ts')) {
    name = name.slice(0, -5);
  }

  // Remove .generated suffix
  if (name.endsWith('.generated')) {
    name = name.slice(0, -10);
  }

  return name.toLowerCase();
}

/**
 * Get lib source directory
 */
function getLibSourceDir() {
  if (fromSubmodule) {
    return path.join(ROOT_DIR, 'TypeScript/src/lib');
  }

  // Use conformance node_modules typescript
  const conformanceTs = path.join(ROOT_DIR, 'scripts/conformance/node_modules/typescript/lib');
  if (fs.existsSync(conformanceTs)) {
    return conformanceTs;
  }

  // Fall back to submodule
  console.warn('Warning: typescript npm package not found, using submodule');
  return path.join(ROOT_DIR, 'TypeScript/src/lib');
}

/**
 * Check if lib-assets are already up to date
 */
function isUpToDate(targetDir, expectedVersion) {
  const versionPath = path.join(targetDir, 'lib_version.json');
  if (!fs.existsSync(versionPath)) {
    return false;
  }
  try {
    const version = JSON.parse(fs.readFileSync(versionPath, 'utf8'));
    return version.npm_version === expectedVersion;
  } catch {
    return false;
  }
}

/**
 * Ensure typescript is installed in conformance/
 */
function ensureTypescriptInstalled(version) {
  const pkgPath = path.join(ROOT_DIR, 'scripts/conformance/node_modules/typescript/package.json');

  if (fs.existsSync(pkgPath)) {
    const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
    if (pkg.version === version) {
      console.log(`TypeScript ${version} already installed`);
      return true;
    }
  }

  console.log(`Installing typescript@${version}...`);
  try {
    execSync(`npm install --no-save typescript@${version}`, {
      cwd: path.join(ROOT_DIR, 'scripts/conformance'),
      stdio: 'inherit',
    });
    return true;
  } catch (e) {
    console.error(`Failed to install typescript@${version}:`, e.message);
    return false;
  }
}

/**
 * Main: generate lib-assets
 */
async function main() {
  // Output directly to src/lib-assets for Rust compilation
  const targetDir = path.join(ROOT_DIR, 'src/lib-assets');
  const npmVersion = getTypescriptNpmVersion();

  // Check if already up to date (skip if --force)
  if (!forceRegenerate && !fromSubmodule && isUpToDate(targetDir, npmVersion)) {
    console.log(`Lib assets already up to date (TypeScript ${npmVersion})`);
    console.log('Use --force to regenerate.');
    return;
  }

  console.log(`TypeScript npm version: ${npmVersion}`);
  console.log(`Source: ${fromSubmodule ? 'TypeScript submodule' : 'npm package'}`);
  console.log(`Output directory: ${targetDir}`);

  // Ensure typescript is installed if using npm
  if (!fromSubmodule) {
    if (!ensureTypescriptInstalled(npmVersion)) {
      console.log('Falling back to submodule...');
    }
  }

  const libSourceDir = getLibSourceDir();
  console.log(`Lib source directory: ${libSourceDir}`);

  if (!fs.existsSync(libSourceDir)) {
    console.error(`Error: Lib source directory not found: ${libSourceDir}`);
    process.exit(1);
  }

  // Create target directory
  if (fs.existsSync(targetDir)) {
    fs.rmSync(targetDir, { recursive: true });
  }
  fs.mkdirSync(targetDir, { recursive: true });

  // Find all lib files
  const files = fs.readdirSync(libSourceDir).filter(f => f.endsWith('.d.ts'));
  console.log(`Found ${files.length} lib files`);

  // Build manifest
  const manifest = {
    version: npmVersion,
    source: fromSubmodule ? 'submodule' : 'npm',
    generatedAt: new Date().toISOString(),
    libs: {},
  };

  // Copy files and build manifest entries
  for (const file of files) {
    const srcPath = path.join(libSourceDir, file);
    const content = fs.readFileSync(srcPath, 'utf8');
    const references = parseLibReferences(content);

    // Determine the destination file name
    // Special case: lib.d.ts -> es5.full.d.ts (the ES5+DOM meta-file)
    let destFileName = file;
    if (file === 'lib.d.ts') {
      destFileName = 'es5.full.d.ts';
    } else if (destFileName.startsWith('lib.')) {
      destFileName = destFileName.slice(4); // Remove 'lib.' prefix
    }

    // Determine lib name from destination file
    const libName = fileNameToLibName(destFileName);

    // Determine canonical file name (what TypeScript uses - with lib. prefix)
    let canonicalFileName = file;
    if (!canonicalFileName.startsWith('lib.')) {
      canonicalFileName = `lib.${canonicalFileName}`;
    }

    // Copy to target
    const destPath = path.join(targetDir, destFileName);
    fs.writeFileSync(destPath, content);

    // Add to manifest
    manifest.libs[libName] = {
      fileName: destFileName,  // Actual file name on disk (without lib. prefix)
      canonicalFileName,       // TypeScript's canonical name (with lib. prefix)
      references,
      size: content.length,
    };
  }

  // Write manifest
  const manifestPath = path.join(targetDir, 'lib_manifest.json');
  fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2));
  console.log(`Wrote manifest: ${manifestPath}`);

  // Write version file for Rust includes
  const versionPath = path.join(targetDir, 'lib_version.json');
  fs.writeFileSync(versionPath, JSON.stringify({
    npm_version: npmVersion,
    source: fromSubmodule ? 'submodule' : 'npm',
    generated_at: manifest.generatedAt,
    lib_count: files.length,
  }, null, 2));
  console.log(`Wrote version: ${versionPath}`);

  // Generate Rust include file directly to src/ for compilation
  const rustIncludePath = path.join(ROOT_DIR, 'src/embedded_libs.rs');
  generateRustIncludes(manifest, rustIncludePath);
  console.log(`Wrote Rust embedded_libs: ${rustIncludePath}`);

  console.log(`\nGenerated ${files.length} lib files in ${targetDir}`);
  console.log('Ready for cargo build - lib files are in place.');
}

/**
 * Generate Rust include statements for embedded libs
 */
function generateRustIncludes(manifest, outputPath) {
  const lines = [
    '//! Embedded TypeScript Library Files',
    '//!',
    '//! This module embeds the official TypeScript library definition files directly into',
    '//! the binary using `include_str!`. This allows tsz to work without requiring',
    '//! separate lib file installation.',
    '//!',
    '//! The lib files are sourced from the TypeScript npm package, versioned via',
    '//! `scripts/conformance/typescript-versions.json`.',
    '//!',
    '//! # Build Requirements',
    '//!',
    '//! Before building, ensure lib assets are generated:',
    '//! ```bash',
    '//! node scripts/generate-lib-assets.mjs',
    '//! ```',
    '//!',
    '//! # Auto-Generated',
    '//!',
    `//! Generated from TypeScript npm version: ${manifest.version}`,
    `//! Generated at: ${manifest.generatedAt}`,
    '//!',
    '//! DO NOT EDIT - regenerate with: node scripts/generate-lib-assets.mjs',
    '',
    'use crate::common::ScriptTarget;',
    '',
    '/// An embedded TypeScript library file.',
    '#[derive(Debug, Clone, Copy)]',
    'pub struct EmbeddedLib {',
    '    /// The lib name (e.g., "es5", "es2015.promise", "dom")',
    '    pub name: &\'static str,',
    '    /// The file name (e.g., "lib.es5.d.ts")',
    '    pub file_name: &\'static str,',
    '    /// The file content',
    '    pub content: &\'static str,',
    '    /// Referenced libs (from /// <reference lib="..." />)',
    '    pub references: &\'static [&\'static str],',
    '}',
    '',
  ];

  // Sort libs for consistent output
  const libNames = Object.keys(manifest.libs).sort();

  // Generate constants for each lib
  for (const libName of libNames) {
    const lib = manifest.libs[libName];
    const constName = `LIB_${libName.toUpperCase().replace(/\./g, '_').replace(/-/g, '_')}`;
    const refsArray = lib.references.length > 0
      ? `&[${lib.references.map(r => `"${r}"`).join(', ')}]`
      : '&[]';

    lines.push(`/// ${libName} library`);
    lines.push(`pub const ${constName}: EmbeddedLib = EmbeddedLib {`);
    lines.push(`    name: "${libName}",`);
    lines.push(`    file_name: "${lib.canonicalFileName}",`);
    lines.push(`    content: include_str!("lib-assets/${lib.fileName}"),`);
    lines.push(`    references: ${refsArray},`);
    lines.push('};');
    lines.push('');
  }

  // Generate ALL_LIBS array
  lines.push('/// All embedded libraries');
  lines.push('pub static ALL_LIBS: &[EmbeddedLib] = &[');
  for (const libName of libNames) {
    const constName = `LIB_${libName.toUpperCase().replace(/\./g, '_').replace(/-/g, '_')}`;
    lines.push(`    ${constName},`);
  }
  lines.push('];');
  lines.push('');

  // Generate lookup function
  lines.push('/// Get an embedded lib by name.');
  lines.push('pub fn get_lib(name: &str) -> Option<&\'static EmbeddedLib> {');
  lines.push('    ALL_LIBS.iter().find(|lib| lib.name == name)');
  lines.push('}');
  lines.push('');

  // Generate get_lib_by_file_name function
  lines.push('/// Get an embedded lib by file name.');
  lines.push('///');
  lines.push('/// The file name should match the lib file name (e.g., "lib.es5.d.ts", "lib.dom.d.ts").');
  lines.push('pub fn get_lib_by_file_name(file_name: &str) -> Option<&\'static EmbeddedLib> {');
  lines.push('    ALL_LIBS.iter().find(|lib| lib.file_name == file_name)');
  lines.push('}');
  lines.push('');

  // Generate get_all_libs function
  lines.push('/// Get all embedded libs.');
  lines.push('pub fn get_all_libs() -> &\'static [EmbeddedLib] {');
  lines.push('    ALL_LIBS');
  lines.push('}');
  lines.push('');

  // Generate resolve function that follows references
  lines.push('/// Resolve a lib and all its dependencies in dependency order.');
  lines.push('pub fn resolve_lib_with_dependencies(name: &str) -> Vec<&\'static EmbeddedLib> {');
  lines.push('    let mut resolved = Vec::new();');
  lines.push('    let mut seen = std::collections::HashSet::new();');
  lines.push('    resolve_lib_recursive(name, &mut resolved, &mut seen);');
  lines.push('    resolved');
  lines.push('}');
  lines.push('');
  lines.push('fn resolve_lib_recursive(');
  lines.push('    name: &str,');
  lines.push('    resolved: &mut Vec<&\'static EmbeddedLib>,');
  lines.push('    seen: &mut std::collections::HashSet<String>,');
  lines.push(') {');
  lines.push('    if seen.contains(name) {');
  lines.push('        return;');
  lines.push('    }');
  lines.push('    seen.insert(name.to_string());');
  lines.push('');
  lines.push('    if let Some(lib) = get_lib(name) {');
  lines.push('        // Resolve dependencies first');
  lines.push('        for dep in lib.references {');
  lines.push('            resolve_lib_recursive(dep, resolved, seen);');
  lines.push('        }');
  lines.push('        resolved.push(lib);');
  lines.push('    }');
  lines.push('}');
  lines.push('');

  // Generate default libs for target function
  lines.push('/// Get default libs for a script target (without DOM).');
  lines.push('pub fn get_libs_for_target(target: ScriptTarget) -> Vec<&\'static EmbeddedLib> {');
  lines.push('    let base_lib = match target {');
  lines.push('        ScriptTarget::ES3 | ScriptTarget::ES5 => "es5",');
  lines.push('        ScriptTarget::ES2015 => "es2015",');
  lines.push('        ScriptTarget::ES2016 => "es2016",');
  lines.push('        ScriptTarget::ES2017 => "es2017",');
  lines.push('        ScriptTarget::ES2018 => "es2018",');
  lines.push('        ScriptTarget::ES2019 => "es2019",');
  lines.push('        ScriptTarget::ES2020 => "es2020",');
  lines.push('        ScriptTarget::ES2021 => "es2021",');
  lines.push('        ScriptTarget::ES2022 => "es2022",');
  lines.push('        ScriptTarget::ESNext => "esnext",');
  lines.push('    };');
  lines.push('    resolve_lib_with_dependencies(base_lib)');
  lines.push('}');
  lines.push('');

  // Generate default libs with DOM
  lines.push('/// Get the default libs for a given script target (with DOM).');
  lines.push('///');
  lines.push('/// Returns the libs needed for the specified ECMAScript version plus DOM and ScriptHost.');
  lines.push('/// This matches tsc\'s default behavior when no explicit `lib` option is specified.');
  lines.push('pub fn get_default_libs_for_target(target: ScriptTarget) -> Vec<&\'static EmbeddedLib> {');
  lines.push('    let mut libs = get_libs_for_target(target);');
  lines.push('');
  lines.push('    // Add DOM libs (same as tsc default)');
  lines.push('    if let Some(dom) = get_lib("dom") {');
  lines.push('        libs.push(dom);');
  lines.push('    }');
  lines.push('    if let Some(dom_iterable) = get_lib("dom.iterable") {');
  lines.push('        libs.push(dom_iterable);');
  lines.push('    }');
  lines.push('    if let Some(webworker_importscripts) = get_lib("webworker.importscripts") {');
  lines.push('        libs.push(webworker_importscripts);');
  lines.push('    }');
  lines.push('    if let Some(scripthost) = get_lib("scripthost") {');
  lines.push('        libs.push(scripthost);');
  lines.push('    }');
  lines.push('');
  lines.push('    libs');
  lines.push('}');
  lines.push('');

  // Generate parse_lib_references function
  lines.push('/// Parse `/// <reference lib="..." />` directives from lib content.');
  lines.push('///');
  lines.push('/// Returns a vector of referenced lib names.');
  lines.push('pub fn parse_lib_references(content: &str) -> Vec<&str> {');
  lines.push('    let mut refs = Vec::new();');
  lines.push('    for line in content.lines() {');
  lines.push('        let trimmed = line.trim();');
  lines.push('        if trimmed.starts_with("/// <reference lib=") {');
  lines.push('            // Parse: /// <reference lib="es5" />');
  lines.push("            if let Some(start) = trimmed.find('\"') {");
  lines.push("                if let Some(end) = trimmed[start + 1..].find('\"') {");
  lines.push('                    refs.push(&trimmed[start + 1..start + 1 + end]);');
  lines.push('                }');
  lines.push('            }');
  lines.push('        } else if !trimmed.starts_with("///") && !trimmed.is_empty() {');
  lines.push('            // Stop at first non-reference line');
  lines.push('            break;');
  lines.push('        }');
  lines.push('    }');
  lines.push('    refs');
  lines.push('}');
  lines.push('');

  // Generate tests
  lines.push('#[cfg(test)]');
  lines.push('mod tests {');
  lines.push('    use super::*;');
  lines.push('');
  lines.push('    #[test]');
  lines.push('    fn test_get_lib() {');
  lines.push('        let es5 = get_lib("es5").expect("es5 lib should exist");');
  lines.push('        assert_eq!(es5.name, "es5");');
  lines.push('        assert!(es5.content.contains("interface Object"));');
  lines.push('    }');
  lines.push('');
  lines.push('    #[test]');
  lines.push('    fn test_get_lib_by_file_name() {');
  lines.push('        let es5 = get_lib_by_file_name("lib.es5.d.ts").expect("lib.es5.d.ts should exist");');
  lines.push('        assert_eq!(es5.name, "es5");');
  lines.push('    }');
  lines.push('');
  lines.push('    #[test]');
  lines.push('    fn test_all_libs_count() {');
  lines.push('        // We should have all the expected libs');
  lines.push('        assert!(ALL_LIBS.len() >= 80, "Should have at least 80 libs");');
  lines.push('    }');
  lines.push('');
  lines.push('    #[test]');
  lines.push('    fn test_parse_lib_references() {');
  lines.push('        let content = r#"/// <reference lib="es5" />');
  lines.push('/// <reference lib="es2015.promise" />');
  lines.push('/// <reference lib="dom" />');
  lines.push('');
  lines.push('interface Foo {}');
  lines.push('"#;');
  lines.push('        let refs = parse_lib_references(content);');
  lines.push('        assert_eq!(refs, vec!["es5", "es2015.promise", "dom"]);');
  lines.push('    }');
  lines.push('');
  lines.push('    #[test]');
  lines.push('    fn test_get_libs_for_target() {');
  lines.push('        let es5_libs = get_libs_for_target(ScriptTarget::ES5);');
  lines.push('        assert!(es5_libs.iter().any(|lib| lib.name == "es5"));');
  lines.push('');
  lines.push('        let es2015_libs = get_libs_for_target(ScriptTarget::ES2015);');
  lines.push('        assert!(es2015_libs.iter().any(|lib| lib.name == "es5"));');
  lines.push('        assert!(es2015_libs.iter().any(|lib| lib.name == "es2015.promise"));');
  lines.push('    }');
  lines.push('');
  lines.push('    #[test]');
  lines.push('    fn test_resolve_lib_with_dependencies() {');
  lines.push('        let libs = resolve_lib_with_dependencies("es2015");');
  lines.push('        // Should include es5 and all es2015 components');
  lines.push('        let names: Vec<_> = libs.iter().map(|lib| lib.name).collect();');
  lines.push('        assert!(names.contains(&"es5"));');
  lines.push('        assert!(names.contains(&"es2015.promise"));');
  lines.push('        assert!(names.contains(&"es2015.collection"));');
  lines.push('    }');
  lines.push('');
  lines.push('    #[test]');
  lines.push('    fn test_dom_lib_has_window() {');
  lines.push('        let dom = get_lib("dom").expect("dom lib should exist");');
  lines.push('        assert!(dom.content.contains("interface Window"));');
  lines.push('        assert!(dom.content.contains("declare var window"));');
  lines.push('    }');
  lines.push('');
  lines.push('    #[test]');
  lines.push('    fn test_es5_has_core_types() {');
  lines.push('        let es5 = get_lib("es5").expect("es5 lib should exist");');
  lines.push('        assert!(es5.content.contains("interface Object"));');
  lines.push('        assert!(es5.content.contains("interface Array<T>"));');
  lines.push('        assert!(es5.content.contains("interface Function"));');
  lines.push('        assert!(es5.content.contains("interface String"));');
  lines.push('        assert!(es5.content.contains("interface Number"));');
  lines.push('        assert!(es5.content.contains("interface Boolean"));');
  lines.push('    }');
  lines.push('');
  lines.push('    #[test]');
  lines.push('    fn test_references_field() {');
  lines.push('        // ES2015 should reference its component libs');
  lines.push('        let es2015 = get_lib("es2015").expect("es2015 lib should exist");');
  lines.push('        assert!(es2015.references.contains(&"es5"));');
  lines.push('        assert!(es2015.references.contains(&"es2015.promise"));');
  lines.push('        assert!(es2015.references.contains(&"es2015.collection"));');
  lines.push('    }');
  lines.push('}');

  fs.writeFileSync(outputPath, lines.join('\n'));
}

main().catch(e => {
  console.error(e);
  process.exit(1);
});
