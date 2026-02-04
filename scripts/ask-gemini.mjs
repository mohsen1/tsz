#!/usr/bin/env node

import { execSync } from 'child_process';
import { parseArgs } from 'util';
import { readFileSync, existsSync, statSync } from 'fs';
import 'dotenv/config';
import { GoogleGenerativeAI } from '@google/generative-ai';

/**
 * Extract code skeletons using ast-grep for better Gemini context.
 * Returns function/struct/enum/trait/impl signatures grouped by file.
 * @param targetDir - Directory to scan for skeletons
 * @param excludeBasenames - Set of file basenames to exclude (files already included in full)
 */
function extractSkeletons(targetDir = 'src/', excludeBasenames = new Set()) {
  const patterns = [
    { type: 'fn', pattern: 'fn $NAME' },
    { type: 'struct', pattern: 'struct $NAME' },
    { type: 'enum', pattern: 'enum $NAME' },
    { type: 'trait', pattern: 'trait $NAME' },
    { type: 'impl', pattern: 'impl $TYPE' },
    { type: 'type', pattern: 'type $NAME' },
  ];

  const skeletonsByFile = new Map();

  for (const { type, pattern } of patterns) {
    try {
      const result = execSync(
        `ast-grep -p '${pattern}' ${targetDir} --json 2>/dev/null`,
        { maxBuffer: 50 * 1024 * 1024, encoding: 'utf-8' }
      );

      if (!result.trim()) continue;

      const matches = JSON.parse(result);
      for (const match of matches) {
        const file = match.file;
        const text = match.text || '';
        const line = match.range?.start?.line || 0;

        // Skip files already included in full content (match by basename)
        const basename = file.split('/').pop();
        if (excludeBasenames.has(basename)) {
          continue;
        }

        // Skip test files and test functions
        if (file.includes('/tests/') || file.includes('_test.rs') || file.includes('/benches/')) {
          continue;
        }

        // Skip test functions (fn test_*)
        const fnName = match.metaVariables?.single?.NAME?.text || match.metaVariables?.single?.N?.text || '';
        if (fnName.startsWith('test_') || fnName.startsWith('bench_')) {
          continue;
        }

        // Extract just the signature (up to and including the opening brace or semicolon)
        let signature = extractSignature(text, type);
        if (!signature) continue;

        if (!skeletonsByFile.has(file)) {
          skeletonsByFile.set(file, []);
        }
        skeletonsByFile.get(file).push({ line, type, signature });
      }
    } catch {
      // ast-grep failed for this pattern, skip silently
    }
  }

  // Sort entries by line number within each file
  for (const [file, entries] of skeletonsByFile) {
    entries.sort((a, b) => a.line - b.line);
  }

  // Build the skeleton output
  let output = 'CODE SKELETONS (API Surface - files not included in full):\n';
  output += '==========================================================\n\n';

  const sortedFiles = [...skeletonsByFile.keys()].sort();
  for (const file of sortedFiles) {
    output += `// ${file}\n`;
    for (const { signature } of skeletonsByFile.get(file)) {
      output += `${signature}\n`;
    }
    output += '\n';
  }

  return { output, fileCount: sortedFiles.length, entryCount: [...skeletonsByFile.values()].flat().length };
}

/**
 * Extract just the signature from a full code block.
 * For functions: "fn name(...) -> Type"
 * For structs/enums: "struct Name<...>" or first line
 * For impl: "impl Type for Trait" or "impl Type"
 */
function extractSignature(text, type) {
  if (!text) return null;

  // Get the first line and clean it up
  const lines = text.split('\n');
  let firstLine = lines[0].trim();

  // For functions, extract up to the opening brace
  if (type === 'fn') {
    // Find the signature - everything before the body
    const braceIndex = text.indexOf('{');
    if (braceIndex > 0) {
      let sig = text.substring(0, braceIndex).trim();
      // Collapse multiline (where clauses) and normalize whitespace
      sig = sig.replace(/\n\s*/g, ' ').replace(/\s+/g, ' ');
      return sig + ' { ... }';
    }
    // Function signature without body (trait method)
    return firstLine.endsWith(';') ? firstLine : firstLine + ';';
  }

  // For structs/enums/traits, extract the declaration line
  if (type === 'struct' || type === 'enum' || type === 'trait') {
    const braceIndex = text.indexOf('{');
    if (braceIndex > 0) {
      let sig = text.substring(0, braceIndex).trim();
      sig = sig.replace(/\n\s*/g, ' ').replace(/\s+/g, ' ');
      return sig + ' { ... }';
    }
    // Tuple struct or unit struct
    const parenIndex = text.indexOf('(');
    const semiIndex = text.indexOf(';');
    if (parenIndex > 0 && (semiIndex < 0 || parenIndex < semiIndex)) {
      const endParen = text.indexOf(')', parenIndex);
      if (endParen > 0) {
        return text.substring(0, endParen + 1).replace(/\n\s*/g, ' ').replace(/\s+/g, ' ') + ';';
      }
    }
    return firstLine.endsWith(';') ? firstLine : firstLine + ' { ... }';
  }

  // For impl blocks, extract the impl line
  if (type === 'impl') {
    const braceIndex = text.indexOf('{');
    if (braceIndex > 0) {
      let sig = text.substring(0, braceIndex).trim();
      sig = sig.replace(/\n\s*/g, ' ').replace(/\s+/g, ' ');
      return sig + ' { ... }';
    }
    return firstLine + ' { ... }';
  }

  // For type aliases
  if (type === 'type') {
    const semiIndex = text.indexOf(';');
    if (semiIndex > 0) {
      return text.substring(0, semiIndex + 1).replace(/\n\s*/g, ' ');
    }
    return firstLine;
  }

  return firstLine;
}

// A CLI tool that uses yek (https://github.com/mohsen1/yek) to give Gemini full context of
// this repo to ask questions. Use focused presets (--solver, --checker, etc.) to pack
// the most relevant context for your question.

// Usage:
// ./scripts/ask-gemini.mjs "How to add feature X?"
// ./scripts/ask-gemini.mjs --solver "How does type inference work?"
// ./scripts/ask-gemini.mjs --checker "How are diagnostics reported?"
// ./scripts/ask-gemini.mjs --include="src/solver" "Custom path question"
// ./scripts/ask-gemini.mjs --no-use-vertex "Use direct Gemini API instead of Vertex"

// Flash model is faster and cheaper, use by default
// Pro model is for complex architectural questions
const DEFAULT_MODEL = 'gemini-3-flash-preview';
const PRO_MODEL = 'gemini-3-pro-preview';
const TARGET_TOKEN_UTILIZATION = 0.90; // Target 90% of 1M context
const MAX_GEMINI_TOKENS = 1_000_000;
const INITIAL_YEK_LIMIT = '4000k'; // Start high, will auto-adjust down

/**
 * Read files directly that might be ignored by yek (e.g., scripts/, docs/).
 * Returns formatted context string for these files.
 */
function readDirectIncludedFiles(paths) {
  const sections = [];
  const includedFiles = [];
  
  if (paths.length > 0) {
    console.log(`  - Reading ${paths.length} directly included file(s)...`);
  }
  
  for (const filePath of paths) {
    // Skip directories, only handle files
    if (existsSync(filePath)) {
      try {
        const stats = statSync(filePath);
        if (stats.isFile()) {
          const content = readFileSync(filePath, 'utf-8');
          sections.push(`>>>> ${filePath}\n${content}`);
          includedFiles.push(filePath);
        }
      } catch (err) {
        console.log(`    Warning: Could not read ${filePath}: ${err.message}`);
      }
    } else {
      console.log(`    Warning: File not found: ${filePath}`);
    }
  }
  
  if (includedFiles.length > 0) {
    console.log(`    Read ${includedFiles.length} file(s) directly`);
  }
  
  return { context: sections.join('\n\n'), files: includedFiles };
}

/**
 * Gather context from yek with given token limit and apply filters.
 * Returns { context, files, estimatedTokens, stats }
 */
function gatherContextWithLimit(yekTokenLimit, includePaths, filterTests, directIncludePaths = []) {
  // First, read any directly specified files that yek might ignore
  const directResult = readDirectIncludedFiles(directIncludePaths);
  
  let yekCommand = `yek --config-file yek.yaml --tokens ${yekTokenLimit}`;
  if (includePaths) {
    yekCommand += ` ${includePaths}`;
  }

  let context = execSync(`${yekCommand} 2>/dev/null | cat`, {
    maxBuffer: 100 * 1024 * 1024,
    encoding: 'utf-8',
  });

  // Get set of important files for filtering
  const importantFiles = getAllImportantFiles();

  // Apply filters
  const sections = context.split(/^(?=>>>> )/m);
  let testFilesFiltered = 0;
  let mdFilesFiltered = 0;
  let localeFilesFiltered = 0;

  const filteredSections = sections.filter(section => {
    const match = section.match(/^>>>> (.+)\n/);
    if (!match) return true;
    const filePath = match[1];

    if (filePath.startsWith('TypeScript/')) return false;

    if (filterTests) {
      const isTestFile = (
        filePath.includes('/tests/') || filePath.includes('/test/') ||
        filePath.match(/_tests?\.rs$/) || filePath.match(/_spec\.rs$/) ||
        filePath.includes('/benches/') || filePath.includes('/bench/') ||
        filePath.includes('test_harness') || filePath.includes('test_utils')
      );
      if (isTestFile) { testFilesFiltered++; return false; }
    }

    // Allow important markdown files, filter out others
    if (filePath.endsWith('.md') && !importantFiles.has(filePath)) {
      mdFilesFiltered++;
      return false;
    }

    if ((filePath.includes('/locales/') || filePath.startsWith('locales/')) && filePath.endsWith('.json')) {
      localeFilesFiltered++;
      return false;
    }

    return true;
  });

  context = filteredSections.join('');

  // Merge directly included files at the beginning
  if (directResult.context) {
    context = directResult.context + '\n\n' + context;
  }

  // Extract file list
  const fileMarkerRegex = /^>>>> (.+)$/gm;
  const files = [...directResult.files]; // Start with directly included files
  let match;
  while ((match = fileMarkerRegex.exec(context)) !== null) {
    // Avoid duplicates
    if (!files.includes(match[1])) {
      files.push(match[1]);
    }
  }

  const contextBytes = Buffer.byteLength(context, 'utf-8');
  const estimatedTokens = Math.ceil(contextBytes / 4);

  return {
    context,
    files,
    estimatedTokens,
    contextBytes,
    stats: { testFilesFiltered, mdFilesFiltered, localeFilesFiltered }
  };
}

/**
 * Find optimal yek token limit to target ~90% Gemini utilization.
 * Uses binary search for accuracy.
 */
function findOptimalTokenLimit(includePaths, filterTests, verbose = true, directIncludePaths = []) {
  const targetTokens = Math.floor(MAX_GEMINI_TOKENS * TARGET_TOKEN_UTILIZATION);

  // First pass with high limit to see max content
  if (verbose) process.stdout.write('  - Auto-sizing context...');
  let result = gatherContextWithLimit(INITIAL_YEK_LIMIT, includePaths, filterTests, directIncludePaths);

  if (result.estimatedTokens <= targetTokens) {
    // Already under target, use all content
    if (verbose) console.log(` using all available content (${result.files.length} files)`);
    return { ...result, yekLimit: INITIAL_YEK_LIMIT };
  }

  // Binary search for optimal limit
  let low = 500;  // 500k minimum
  let high = parseInt(INITIAL_YEK_LIMIT);
  let bestResult = result;
  let iterations = 0;

  while (high - low > 100 && iterations < 8) {  // Within 100k precision, max 8 iterations
    iterations++;
    const mid = Math.floor((low + high) / 2);
    const midStr = `${mid}k`;

    result = gatherContextWithLimit(midStr, includePaths, filterTests, directIncludePaths);

    if (result.estimatedTokens <= targetTokens) {
      // Under target, try higher
      low = mid;
      bestResult = result;
      bestResult.yekLimit = midStr;
    } else {
      // Over target, try lower
      high = mid;
    }
  }

  // Final adjustment - use the best result we found
  if (verbose) console.log(` optimal: ${bestResult.yekLimit} (${bestResult.files.length} files)`);
  return bestResult;
}

// Important files that should always be included
const IMPORTANT_FILES = {
  // Core architecture and guidelines (always included)
  core: [
    'AGENTS.md',
    'docs/architecture/NORTH_STAR.md',
    'docs/DEVELOPMENT.md',
  ],
  // Category-specific important files
  solver: [
    'docs/walkthrough/05-solver.md',
    'docs/specs/TS_UNSOUNDNESS_CATALOG.md',
  ],
  checker: [
    'docs/walkthrough/04-checker.md',
    'docs/specs/DIAGNOSTICS.md',
  ],
  binder: [
    'docs/walkthrough/03-binder.md',
  ],
  parser: [
    'docs/walkthrough/02-parser.md',
    'docs/walkthrough/01-scanner.md',
  ],
  emitter: [
    'docs/walkthrough/06-emitter.md',
  ],
  lsp: [
    'docs/walkthrough/08-lsp-gaps.md',
    'docs/todo/lsp/INDEX.md',
  ],
  types: [
    'docs/walkthrough/05-solver.md',
    'docs/walkthrough/04-checker.md',
    'docs/specs/TS_UNSOUNDNESS_CATALOG.md',
  ],
  modules: [
    'docs/architecture/TSC_LIB_LOADING.md',
  ],
  // General (no preset) important files
  general: [
    'docs/walkthrough/README.md',
    'docs/specs/TS_UNSOUNDNESS_CATALOG.md',
  ],
};

/**
 * Get all important files across all categories (for filtering).
 * @returns {Set<string>} - Set of all important file paths
 */
function getAllImportantFiles() {
  const allFiles = new Set();
  for (const category of Object.values(IMPORTANT_FILES)) {
    if (Array.isArray(category)) {
      for (const file of category) {
        allFiles.add(file);
      }
    }
  }
  return allFiles;
}

/**
 * Read and include important files directly in context (bypassing yek's ignore patterns).
 * @param {string} category - Category name (solver, checker, etc.) or 'general' for no preset
 * @returns {string} - Formatted context string with important files
 */
function includeImportantFiles(category = 'general') {
  const important = [
    ...IMPORTANT_FILES.core,
    ...(IMPORTANT_FILES[category] || []),
  ];
  
  const sections = [];
  let filesIncluded = 0;
  
  for (const filePath of important) {
    if (existsSync(filePath)) {
      try {
        const content = readFileSync(filePath, 'utf-8');
        sections.push(`>>>> ${filePath}\n${content}`);
        filesIncluded++;
      } catch (err) {
        // Silently skip files that can't be read
      }
    }
  }
  
  if (sections.length > 0) {
    return `\n${'='.repeat(50)}\nIMPORTANT DOCUMENTATION FILES (${filesIncluded} files):\n${'='.repeat(50)}\n\n${sections.join('\n\n')}\n\n`;
  }
  
  return '';
}

/**
 * Merge important files into a paths array, avoiding duplicates.
 * @param {string[]} paths - Existing paths
 * @param {string} category - Category name (solver, checker, etc.) or 'general' for no preset
 * @returns {string[]} - Paths with important files added
 */
function addImportantFiles(paths, category = 'general') {
  const important = [
    ...IMPORTANT_FILES.core,
    ...(IMPORTANT_FILES[category] || []),
  ];
  
  // Add important files that aren't already in paths
  const pathSet = new Set(paths);
  for (const file of important) {
    if (!pathSet.has(file)) {
      paths.push(file);
    }
  }
  
  return paths;
}

// Focused presets for different areas of the codebase
// Each preset uses directory wildcards to include all related files automatically
// Focused presets - paths to include for each area (auto-sized to fit context)
const PRESETS = {
  solver: {
    description: 'Type solver, inference, compatibility, and type operations',
    paths: ['src/solver/', 'src/checker/state.rs', 'src/checker/context.rs'],
  },
  checker: {
    description: 'Type checker, AST traversal, diagnostics, and error reporting',
    paths: ['src/checker/', 'src/binder.rs'],
  },
  binder: {
    description: 'Symbol binding, scopes, and control flow graph construction',
    paths: ['src/binder/', 'src/binder.rs', 'src/checker/flow_graph_builder.rs'],
  },
  parser: {
    description: 'Parser, scanner, and AST nodes',
    paths: ['src/parser/', 'src/scanner.rs', 'src/scanner_impl.rs', 'src/span.rs'],
  },
  emitter: {
    description: 'Code emission, transforms, source maps, and declaration files',
    paths: ['src/emitter/', 'src/transforms/', 'src/declaration_emitter.rs', 'src/source_map.rs', 'src/source_writer.rs', 'src/printer.rs'],
  },
  lsp: {
    description: 'Language server protocol implementation',
    paths: ['src/lsp/', 'src/bin/tsz_lsp.rs', 'src/bin/tsz_server/'],
  },
  types: {
    description: 'Type system overview (solver + checker type-related)',
    paths: ['src/solver/', 'src/checker/', 'src/lowering_pass.rs'],
  },
  modules: {
    description: 'Module resolution, imports, exports, and module graph',
    paths: ['src/module_resolver.rs', 'src/module_graph.rs', 'src/imports.rs', 'src/exports.rs', 'src/transforms/module_*.rs', 'src/emitter/module_*.rs'],
  },
};

const { values, positionals } = parseArgs({
  options: {
    include: {
      type: 'string',
      short: 'i',
      multiple: true,  // Allow multiple --include flags
    },
    tokens: {
      type: 'string',
      short: 't',
    },
    model: {
      type: 'string',
      short: 'm',
      default: DEFAULT_MODEL,
    },
    // Use Pro model for complex questions
    pro: { type: 'boolean', default: false },
    help: {
      type: 'boolean',
      short: 'h',
      default: false,
    },
    // Focused presets
    solver: { type: 'boolean', default: false },
    checker: { type: 'boolean', default: false },
    binder: { type: 'boolean', default: false },
    parser: { type: 'boolean', default: false },
    emitter: { type: 'boolean', default: false },
    lsp: { type: 'boolean', default: false },
    types: { type: 'boolean', default: false },
    modules: { type: 'boolean', default: false },
    // Dry run - show what would be sent without calling API
    dry: { type: 'boolean', default: false },
    // Also print the full query payload (use with --dry)
    query: { type: 'boolean', default: false },
    // List available presets
    list: { type: 'boolean', default: false },
    // Use Vertex AI (default) or direct Gemini API (--no-use-vertex)
    'use-vertex': { type: 'boolean', default: true },
    // Include code skeletons (function/struct/enum/trait signatures) - on by default
    skeleton: { type: 'boolean', default: true },
  },
  allowPositionals: true,
  allowNegative: true,
});

if (values.list) {
  console.log('\nAvailable presets:\n');
  for (const [name, preset] of Object.entries(PRESETS)) {
    console.log(`  --${name.padEnd(10)} ${preset.description}`);
    console.log(`               Paths: ${preset.paths.slice(0, 3).join(', ')}${preset.paths.length > 3 ? '...' : ''}`);
    console.log('');
  }
  process.exit(0);
}

if (values.help) {
  console.log(`
Usage: ./scripts/ask-gemini.mjs [options] "your prompt"

Focused Presets (pick one for best context):
  --solver            Type solver, inference, compatibility
  --checker           Type checker, diagnostics, AST traversal
  --binder            Symbol binding, scopes, CFG
  --parser            Parser, scanner, AST nodes
  --emitter           Code emission, transforms, source maps
  --lsp               Language server protocol
  --types             Type system overview (solver + checker)
  --modules           Module resolution, imports, exports

Model Selection:
  --pro               Use Gemini Pro for complex architectural questions
                      (default: Flash for faster responses)
  --flash             Use Gemini Flash model (explicit, same as default)

General Options:
  -i, --include=PATH  Include specific path(s) (overrides preset)
  -t, --tokens=SIZE   Override yek token limit (default: auto-sized to ~90% of Gemini's 1M context)
  -m, --model=NAME    Specific Gemini model (overrides --pro/--flash)
  --dry               Show files that would be included without calling API
  --query             Print the full query payload (system prompt + context + prompt)
  --no-skeleton       Disable code skeleton extraction. Skeletons show fn/struct/enum/
                      trait/impl signatures for files NOT fully included in context.
  --list              List all available presets with descriptions
  --no-use-vertex     Use direct Gemini API instead of Vertex AI (fallback for
                      rate limits or when Vertex credentials aren't available)
  -h, --help          Show this help message

When to use Flash vs Pro:
  Flash (default):     Most questions - code lookup, simple fixes, "how does X work"
  Pro (--pro flag):    Complex architectural decisions, multi-file changes, "how should I redesign X"

Note: Test files, markdown docs, and locale JSONs are excluded from full content
      by default. Skeletons still include all code signatures.

Environment:
  GCP_VERTEX_EXPRESS_API_KEY  Required for Vertex AI Express (default).
  GEMINI_API_KEY              Required for direct Gemini API (--no-use-vertex).

Examples:
  ./scripts/ask-gemini.mjs --solver "How does type inference work?"
  ./scripts/ask-gemini.mjs --checker "How are diagnostics reported?"
  ./scripts/ask-gemini.mjs --pro "Review this implementation for correctness"
  ./scripts/ask-gemini.mjs --types "How does the type system handle generics?"
  ./scripts/ask-gemini.mjs --parser "How does ASI work?"
  ./scripts/ask-gemini.mjs --emitter "How are source maps generated?"
  ./scripts/ask-gemini.mjs --lsp "How does go-to-definition work?"
  ./scripts/ask-gemini.mjs --modules "How does module resolution work?"
  ./scripts/ask-gemini.mjs --include=src/solver "Custom path question"
  ./scripts/ask-gemini.mjs --no-use-vertex "Use direct Gemini API (fallback)"
  ./scripts/ask-gemini.mjs --list  # Show all presets
`);
  process.exit(0);
}

// Determine which preset is active (if any)
const presetNames = Object.keys(PRESETS);
const activePresets = presetNames.filter(name => values[name]);

if (activePresets.length > 1) {
  console.error(`Error: Multiple presets specified (${activePresets.join(', ')}). Please choose one.`);
  console.error('Run with --list to see available presets.');
  process.exit(1);
}

const activePreset = activePresets[0] ? PRESETS[activePresets[0]] : null;
const presetName = activePresets[0] || null;

// Use Pro model if --pro flag is set
const effectiveModel = values.pro ? PRO_MODEL : values.model;

// Token limit: explicit flag overrides auto-sizing
const explicitTokenLimit = values.tokens || null;

// Determine paths to include
let includePaths = null;
let directIncludePaths = []; // Paths to read directly (bypass yek ignore patterns)

if (values.include && values.include.length > 0) {
  // User-specified paths: flatten multiple --include flags and split by whitespace
  const paths = values.include.flatMap(p => p.split(/\s+/)).filter(p => p);
  // All user-specified paths should be read directly (bypass yek ignore patterns)
  directIncludePaths = [...paths];
  const pathsWithImportant = addImportantFiles(paths, 'general');
  includePaths = pathsWithImportant.length > 0 ? pathsWithImportant.join(' ') : null;
} else if (activePreset) {
  // Preset active: add category-specific important files
  const paths = [...activePreset.paths];
  const pathsWithImportant = addImportantFiles(paths, presetName);
  includePaths = pathsWithImportant.length > 0 ? pathsWithImportant.join(' ') : null;
} else {
  // No preset: add general important files
  const paths = [];
  const pathsWithImportant = addImportantFiles(paths, 'general');
  includePaths = pathsWithImportant.length > 0 ? pathsWithImportant.join(' ') : null;
}

const useVertex = values['use-vertex'];
const GEMINI_API_KEY = process.env.GEMINI_API_KEY;
const GCP_VERTEX_EXPRESS_API_KEY = process.env.GCP_VERTEX_EXPRESS_API_KEY;

const isDryRun = values.dry || values.query;

if (!isDryRun) {
  if (useVertex) {
    if (!GCP_VERTEX_EXPRESS_API_KEY) {
      console.error('Error: GCP_VERTEX_EXPRESS_API_KEY environment variable is not set.');
      console.error('Get an API key from Vertex AI Express Mode, or use --no-use-vertex for direct Gemini API.');
      console.error('(Use --dry to see files, --query to see full query without calling API)');
      process.exit(1);
    }
  } else {
    if (!GEMINI_API_KEY) {
      console.error('Error: GEMINI_API_KEY environment variable is not set.');
      console.error('Get an API key at: https://aistudio.google.com/apikey');
      console.error('(Use --dry to see files, --query to see full query without calling API)');
      process.exit(1);
    }
  }
}

const prompt = positionals[0];

if (!prompt && !isDryRun) {
  console.error('Error: No prompt provided.');
  console.error('Usage: ./scripts/ask-gemini.mjs [--preset] "your prompt"');
  console.error('Run with --help for more options, --list for presets.');
  process.exit(1);
}

try {
  if (presetName) {
    console.log(`Using preset: --${presetName} (${activePreset.description})`);
  }
  if (values.pro) {
    console.log(`Using model: ${effectiveModel} (Pro - for complex questions)`);
  } else {
    console.log(`Using model: ${effectiveModel} (Flash - fast, for most questions)`);
  }
  console.log(`Using API: ${useVertex ? 'Vertex AI Express' : 'Direct Gemini API'}`);
  console.log('Gathering context...');

  // Check if tests should be filtered
  const includeStr = values.include ? values.include.join(' ') : '';
  const filterTests = !(includeStr && (
    includeStr.includes('test') ||
    includeStr.includes('spec') ||
    includeStr.includes('bench')
  ));

  // Gather context - either with explicit limit or auto-sized
  let contextResult;
  if (explicitTokenLimit) {
    console.log(`  - Using explicit token limit: ${explicitTokenLimit}`);
    contextResult = gatherContextWithLimit(explicitTokenLimit, includePaths, filterTests, directIncludePaths);
    contextResult.yekLimit = explicitTokenLimit;
  } else {
    contextResult = findOptimalTokenLimit(includePaths, filterTests, true, directIncludePaths);
  }

  let { context, files, estimatedTokens, contextBytes, stats, yekLimit } = contextResult;

  // Log filter stats
  if (filterTests) {
    console.log('  - Filtering out test files (use --include with test path to include them)');
  }
  if (stats.testFilesFiltered > 0) {
    console.log(`  - Filtered out ${stats.testFilesFiltered} test file(s)`);
  }
  if (stats.mdFilesFiltered > 0) {
    console.log(`  - Filtered out ${stats.mdFilesFiltered} markdown file(s) (keeping important docs)`);
  }
  if (stats.localeFilesFiltered > 0) {
    console.log(`  - Filtered out ${stats.localeFilesFiltered} locale file(s)`);
  }

  // Build a set of fully included files for exclusion from skeletons
  // yek outputs short names like "apparent.rs", ast-grep outputs "src/solver/apparent.rs"
  // We'll store basenames and match by basename
  const includedBasenames = new Set(files.map(f => {
    // Extract basename (last part of path)
    const parts = f.split('/');
    return parts[parts.length - 1];
  }));

  // Extract code skeletons for API surface overview, excluding files already fully included
  let skeletonOutput = '';
  if (values.skeleton) {
    console.log('  - Extracting code skeletons with ast-grep...');
    try {
      const skeletonDir = includePaths ? includePaths.split(' ')[0] : 'src/';
      const { output, fileCount, entryCount } = extractSkeletons(skeletonDir, includedBasenames);
      skeletonOutput = output;
      console.log(`  - Extracted ${entryCount} signatures from ${fileCount} files (excluding ${files.length} fully-included files)`);
    } catch (err) {
      console.log(`  - Skeleton extraction failed: ${err.message}`);
    }
  }

  // Include important files directly (bypassing yek's ignore patterns)
  const importantFilesContext = includeImportantFiles(presetName || 'general');
  if (importantFilesContext) {
    console.log('  - Including important documentation files directly');
  }

  // Assemble context: important files -> skeletons -> file contents
  let contextParts = [];
  if (importantFilesContext) {
    contextParts.push(importantFilesContext);
  }
  if (skeletonOutput) {
    contextParts.push(skeletonOutput);
  }
  contextParts.push(`${'='.repeat(50)}\nFILE CONTENTS (${files.length} files):\n${'='.repeat(50)}\n\n${context}`);
  context = contextParts.join('\n');

  // Recalculate bytes after adding skeletons
  const finalContextBytes = Buffer.byteLength(context, 'utf-8');
  const finalEstimatedTokens = Math.ceil(finalContextBytes / 4);
  const utilization = ((finalEstimatedTokens / MAX_GEMINI_TOKENS) * 100).toFixed(1);

  console.log(`Context: ${(finalContextBytes / 1024).toFixed(0)} KB, ${files.length} files + skeletons`);
  console.log(`Tokens: ~${finalEstimatedTokens.toLocaleString()} / ${MAX_GEMINI_TOKENS.toLocaleString()} (${utilization}% utilization)`);

  if (finalEstimatedTokens > MAX_GEMINI_TOKENS) {
    console.log(`⚠️  Warning: Estimated tokens exceed Gemini's context window!`);
  }

  // Show files included
  if (files.length > 0) {
    console.log('\nFiles included:');
    for (const file of files) {
      console.log(`  - ${file}`);
    }
    console.log('');
  }

  // Dry run mode - show files only (unless --query is also set)
  if (values.dry && !values.query) {
    console.log('Dry run complete. Use --query to also see full payload.');
    process.exit(0);
  }

  console.log(`Sending to Gemini via ${useVertex ? 'Vertex AI Express' : 'direct API'}...`);

  let url;
  const headers = { 'Content-Type': 'application/json' };

  if (useVertex) {
    // Vertex AI Express Mode endpoint - uses API key
    url = `https://aiplatform.googleapis.com/v1/publishers/google/models/${effectiveModel}:generateContent?key=${GCP_VERTEX_EXPRESS_API_KEY}`;
  } else {
    // Direct Gemini API endpoint
    url = `https://generativelanguage.googleapis.com/v1beta/models/${effectiveModel}:generateContent?key=${GEMINI_API_KEY}`;
  }

  // Build system prompt based on preset
  let systemPrompt = 'You are an expert on the tsz TypeScript compiler codebase (TypeScript compiler written in Rust).';

  if (presetName) {
    const presetContexts = {
      solver: `You are focused on the TYPE SOLVER component. Key concepts:
- Solver handles WHAT (pure type operations and relations)
- Checker handles WHERE (AST traversal, diagnostics)
- Use visitor pattern from src/solver/visitor.rs for type operations
- Key files: state.rs (main state), infer.rs (inference), compat.rs (assignability)`,

      checker: `You are focused on the TYPE CHECKER component. Key concepts:
- Checker is a thin wrapper that delegates type logic to solver
- Checker extracts AST data, calls solver, reports errors with source locations
- Control flow analysis lives in checker
- Key files: state.rs (main state), control_flow.rs, error_reporter.rs`,

      binder: `You are focused on the BINDER component. Key concepts:
- Binder handles SYMBOLS (symbol table, scopes, control flow graph)
- Binder never computes types - that's checker/solver's job
- Binder creates symbols, manages scopes, builds CFG
- Key file: binder/state.rs`,

      parser: `You are focused on the PARSER component. Key concepts:
- Parser state machine in parser/state.rs
- AST node definitions in parser/node.rs
- Scanner handles lexical analysis
- Focus on TypeScript-specific syntax (types, decorators, etc.)`,

      emitter: `You are focused on the EMITTER component. Key concepts:
- Transforms convert modern syntax to target (ES5, CommonJS, etc.)
- Declaration emitter generates .d.ts files
- Source maps track original positions
- Key files: emitter/mod.rs, transforms/es5.rs`,

      lsp: `You are focused on the LSP (Language Server Protocol) component. Key concepts:
- project.rs manages file state and incremental updates
- Each LSP feature (completions, hover, etc.) has its own module
- Leverages checker for type information`,

      types: `You are analyzing the TYPE SYSTEM as a whole. Key concepts:
- Solver-first architecture: pure type logic in solver
- Checker delegates to solver, handles AST and diagnostics
- Visitor pattern for type operations (never manual TypeKey matches)
- Type inference, compatibility, instantiation all in solver`,

      modules: `You are focused on MODULE RESOLUTION. Key concepts:
- module_resolver.rs handles finding modules
- module_graph.rs tracks dependencies
- imports.rs/exports.rs handle bindings
- Transforms handle CommonJS/ESM conversion`,
    };

    systemPrompt += `\n\n${presetContexts[presetName] || ''}`;
  }

  systemPrompt += `

IMPORTANT: The context includes:
1. CODE SKELETONS showing function/struct/enum/trait/impl signatures across the codebase (API surface overview)
2. DETAILED CONTENTS of the most relevant files for your question

Answer questions accurately based on the provided context. Reference specific files and line numbers when relevant.`;

  const payload = {
    contents: [
      {
        role: 'user',
        parts: [
          { text: `Codebase context:\n${context}\n\nQuestion: ${prompt}` }
        ]
      }
    ],
    systemInstruction: {
      parts: [{ text: systemPrompt }]
    },
    generationConfig: {
      temperature: 0.2,
      maxOutputTokens: 8192,
    }
  };

  // Query mode - show full payload without sending
  if (values.query) {
    console.log('\n=== SYSTEM INSTRUCTION ===\n');
    console.log(systemPrompt);
    console.log('\n=== USER MESSAGE ===\n');
    console.log(`Codebase context:\n${context}\n\nQuestion: ${prompt}`);
    console.log('\n=== GENERATION CONFIG ===\n');
    console.log(JSON.stringify(payload.generationConfig, null, 2));
    console.log('\n--- Query mode. No API call made. ---');
    process.exit(0);
  }

  const response = await fetch(url, {
    method: 'POST',
    headers,
    body: JSON.stringify(payload),
  });

  if (!response.ok) {
    const errorText = await response.text();
    console.error(`HTTP Error ${response.status}: ${errorText}`);
    if (useVertex && (response.status === 429 || response.status === 503)) {
      console.error('\nHint: If Vertex AI is rate-limited, try --no-use-vertex to use direct Gemini API.');
    }
    process.exit(1);
  }

  const data = await response.json();

  if (data.error) {
    console.error('Gemini API Error:', JSON.stringify(data.error, null, 2));
    process.exit(1);
  }

  const text = data.candidates?.[0]?.content?.parts?.[0]?.text;
  if (text) {
    console.log('\n--- Gemini Response ---\n');
    console.log(text);
  } else {
    const finishReason = data.candidates?.[0]?.finishReason;
    if (finishReason === 'SAFETY') {
      console.error('Response blocked due to safety filters.');
    } else {
      console.log('No response from Gemini.');
      console.log('Full response:', JSON.stringify(data, null, 2));
    }
    process.exit(1);
  }
} catch (error) {
  if (error.code === 'ENOENT') {
    console.error('Error: yek is not installed. Install it with: cargo install yek');
  } else {
    console.error('Error:', error.message);
  }
  process.exit(1);
}

