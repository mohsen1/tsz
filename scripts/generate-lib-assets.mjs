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

  const versionsFile = path.join(ROOT_DIR, 'conformance/typescript-versions.json');
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
 */
function parseLibReferences(content) {
  const refs = [];
  const lines = content.split('\n');
  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed.startsWith('/// <reference lib=')) {
      const match = trimmed.match(/lib=["']([^"']+)["']/);
      if (match) {
        refs.push(match[1].toLowerCase());
      }
    } else if (!trimmed.startsWith('///') && trimmed.length > 0) {
      // Stop at first non-reference, non-empty line
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
  const conformanceTs = path.join(ROOT_DIR, 'conformance/node_modules/typescript/lib');
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
  const pkgPath = path.join(ROOT_DIR, 'conformance/node_modules/typescript/package.json');

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
      cwd: path.join(ROOT_DIR, 'conformance'),
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

  // Generate Rust include file (for reference, not used directly)
  const rustIncludePath = path.join(targetDir, 'embedded_libs_generated.rs');
  generateRustIncludes(manifest, rustIncludePath);
  console.log(`Wrote Rust includes: ${rustIncludePath}`);

  console.log(`\nGenerated ${files.length} lib files in ${targetDir}`);
  console.log('Ready for cargo build - lib files are in place.');
}

/**
 * Generate Rust include statements for embedded libs
 */
function generateRustIncludes(manifest, outputPath) {
  const lines = [
    '//! Auto-generated embedded TypeScript library files.',
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
  lines.push('/// Get default libs for a script target (with DOM).');
  lines.push('pub fn get_default_libs_for_target(target: ScriptTarget) -> Vec<&\'static EmbeddedLib> {');
  lines.push('    let mut libs = get_libs_for_target(target);');
  lines.push('    // Add DOM libs');
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
  lines.push('    libs');
  lines.push('}');

  fs.writeFileSync(outputPath, lines.join('\n'));
}

main().catch(e => {
  console.error(e);
  process.exit(1);
});
