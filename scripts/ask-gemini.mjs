#!/usr/bin/env node

import { execSync } from 'child_process';
import { parseArgs } from 'util';
import 'dotenv/config';

/**
 * Extract code skeletons using ast-grep for better Gemini context.
 * Returns function/struct/enum/trait/impl signatures grouped by file.
 */
function extractSkeletons(targetDir = 'src/') {
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
  let output = 'CODE SKELETONS (API Surface):\n';
  output += '=============================\n\n';

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

const DEFAULT_CONTEXT_LENGTH = '850k';
const DEFAULT_MODEL = 'gemini-3-pro-preview';

// Focused presets for different areas of the codebase
// Each preset uses directory wildcards to include all related files automatically
const PRESETS = {
  solver: {
    description: 'Type solver, inference, compatibility, and type operations',
    paths: ['src/solver/', 'src/checker/state.rs', 'src/checker/context.rs', 'AGENTS.md'],
    tokens: '600k',
  },
  checker: {
    description: 'Type checker, AST traversal, diagnostics, and error reporting',
    paths: ['src/checker/', 'src/binder.rs', 'AGENTS.md'],
    tokens: '700k',
  },
  binder: {
    description: 'Symbol binding, scopes, and control flow graph construction',
    paths: ['src/binder/', 'src/binder.rs', 'src/checker/flow_graph_builder.rs', 'AGENTS.md'],
    tokens: '400k',
  },
  parser: {
    description: 'Parser, scanner, and AST nodes',
    paths: ['src/parser/', 'src/scanner.rs', 'src/scanner_impl.rs', 'src/span.rs'],
    tokens: '700k',
  },
  emitter: {
    description: 'Code emission, transforms, source maps, and declaration files',
    paths: ['src/emitter/', 'src/transforms/', 'src/declaration_emitter.rs', 'src/source_map.rs', 'src/source_writer.rs', 'src/printer.rs'],
    tokens: '600k',
  },
  lsp: {
    description: 'Language server protocol implementation',
    paths: ['src/lsp/', 'src/cli/'],
    tokens: '600k',
  },
  types: {
    description: 'Type system overview (solver + checker type-related)',
    paths: ['src/solver/', 'src/checker/', 'src/lowering_pass.rs', 'AGENTS.md'],
    tokens: '800k',
  },
  modules: {
    description: 'Module resolution, imports, exports, and module graph',
    paths: ['src/module_resolver.rs', 'src/module_graph.rs', 'src/imports.rs', 'src/exports.rs', 'src/transforms/module_*.rs', 'src/emitter/module_*.rs'],
    tokens: '400k',
  },
};

const { values, positionals } = parseArgs({
  options: {
    include: {
      type: 'string',
      short: 'i',
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
    // Show what files would be included without sending to API
    dry: { type: 'boolean', default: false },
    // Print the full query payload without sending to API
    'print-query-only': { type: 'boolean', default: false },
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

General Options:
  -i, --include=PATH  Include specific path(s) (overrides preset)
  -t, --tokens=SIZE   Max context size (default: ${DEFAULT_CONTEXT_LENGTH}, or preset default)
  -m, --model=NAME    Gemini model (default: ${DEFAULT_MODEL})
  --dry               Show files that would be included without calling API
  --print-query-only  Print the full query payload (system prompt + context + prompt)
                      without sending to API. Useful for inspecting what would be sent.
  --no-skeleton       Disable code skeleton extraction (signatures for all Rust files).
                      Skeletons are included by default to show the full API surface.
  --list              List all available presets with descriptions
  --no-use-vertex     Use direct Gemini API instead of Vertex AI (fallback for
                      rate limits or when Vertex credentials aren't available)
  -h, --help          Show this help message

Note: Test files (*_test.rs, tests/, benches/) are excluded by default.
      To include tests, use --include with a path containing "test" or "bench".

Environment:
  GCP_VERTEX_EXPRESS_API_KEY  Required for Vertex AI Express (default).
  GEMINI_API_KEY              Required for direct Gemini API (--no-use-vertex).

Examples:
  ./scripts/ask-gemini.mjs --solver "How does type inference work?"
  ./scripts/ask-gemini.mjs --checker "How are diagnostics reported?"
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

// Determine token limit: explicit flag > preset default > global default
const tokenLimit = values.tokens || (activePreset?.tokens) || DEFAULT_CONTEXT_LENGTH;

// Determine paths to include
let includePaths = null;
if (values.include) {
  includePaths = values.include;
} else if (activePreset) {
  includePaths = activePreset.paths.join(' ');
}

const useVertex = values['use-vertex'];
const GEMINI_API_KEY = process.env.GEMINI_API_KEY;
const GCP_VERTEX_EXPRESS_API_KEY = process.env.GCP_VERTEX_EXPRESS_API_KEY;

if (!values.dry && !values['print-query-only']) {
  if (useVertex) {
    if (!GCP_VERTEX_EXPRESS_API_KEY) {
      console.error('Error: GCP_VERTEX_EXPRESS_API_KEY environment variable is not set.');
      console.error('Get an API key from Vertex AI Express Mode, or use --no-use-vertex for direct Gemini API.');
      console.error('(Use --dry or --print-query-only to see files/query without calling API)');
      process.exit(1);
    }
  } else {
    if (!GEMINI_API_KEY) {
      console.error('Error: GEMINI_API_KEY environment variable is not set.');
      console.error('Get an API key at: https://aistudio.google.com/apikey');
      console.error('(Use --dry or --print-query-only to see files/query without calling API)');
      process.exit(1);
    }
  }
}

const prompt = positionals[0];

if (!prompt && !values.dry) {
  console.error('Error: No prompt provided.');
  console.error('Usage: ./scripts/ask-gemini.mjs [--preset] "your prompt"');
  console.error('Run with --help for more options, --list for presets.');
  process.exit(1);
}

try {
  if (presetName) {
    console.log(`Using preset: --${presetName} (${activePreset.description})`);
  }
  console.log(`Using model: ${values.model}`);
  console.log(`Using API: ${useVertex ? 'Vertex AI Express' : 'Direct Gemini API'}`);
  console.log(`Token limit: ${tokenLimit}`);
  console.log('Gathering context...');

  // Extract code skeletons for API surface overview
  let skeletonOutput = '';
  if (values.skeleton) {
    console.log('  - Extracting code skeletons with ast-grep...');
    try {
      const skeletonDir = includePaths ? includePaths.split(' ')[0] : 'src/';
      const { output, fileCount, entryCount } = extractSkeletons(skeletonDir);
      skeletonOutput = output;
      console.log(`  - Extracted ${entryCount} signatures from ${fileCount} files`);
    } catch (err) {
      console.log(`  - Skeleton extraction failed: ${err.message}`);
    }
  }

  // Now get the actual file contents
  console.log('  - Gathering file contents...');
  let yekCommand = `yek --config-file yek.yaml --tokens ${tokenLimit}`;
  if (includePaths) {
    yekCommand += ` ${includePaths}`;
  }

  // Running yek and capturing stdout. Pipe to cat to force non-TTY output mode.
  // Suppress stderr to avoid yek progress messages.
  let context = execSync(`${yekCommand} 2>/dev/null | cat`, {
    maxBuffer: 100 * 1024 * 1024,
    encoding: 'utf-8',
  });

  // Filter out TypeScript submodule files and test files
  // Test files are only included if user explicitly specifies them via --include
  const userExplicitlyIncludedTests = values.include && (
    values.include.includes('test') ||
    values.include.includes('spec') ||
    values.include.includes('bench')
  );

  if (!userExplicitlyIncludedTests) {
    console.log('  - Filtering out test files (use --include with test path to include them)');
  }

  const sections = context.split(/^(?=>>>> )/m);
  let testFilesFiltered = 0;
  const filteredSections = sections.filter(section => {
    const match = section.match(/^>>>> (.+)\n/);
    if (!match) return true;
    const filePath = match[1];

    // Skip TypeScript submodule files
    if (filePath.startsWith('TypeScript/')) {
      return false;
    }

    // Skip test files unless user explicitly requested them via --include
    if (!userExplicitlyIncludedTests) {
      const isTestFile = (
        // Test directory patterns
        filePath.includes('/tests/') || filePath.includes('/test/') ||
        // Test file naming patterns
        filePath.match(/_tests?\.rs$/) || filePath.match(/_spec\.rs$/) ||
        // Bench directory patterns
        filePath.includes('/benches/') || filePath.includes('/bench/') ||
        // Common test harness files
        filePath.includes('test_harness') || filePath.includes('test_utils')
      );
      if (isTestFile) {
        testFilesFiltered++;
        return false;
      }
    }

    return true;
  });

  if (testFilesFiltered > 0) {
    console.log(`  - Filtered out ${testFilesFiltered} test file(s)`);
  }
  context = filteredSections.join('');

  // Prepend skeletons to context
  let contextParts = [];
  if (skeletonOutput) {
    contextParts.push(skeletonOutput);
  }
  contextParts.push(`${'='.repeat(50)}\nFILE CONTENTS:\n${'='.repeat(50)}\n\n${context}`);
  context = contextParts.join('\n');

  // Extract file paths from yek markers (>>>> FILE_PATH)
  const fileMarkerRegex = /^>>>> (.+)$/gm;
  const files = [];
  let match;
  while ((match = fileMarkerRegex.exec(context)) !== null) {
    files.push(match[1]);
  }

  const contextBytes = Buffer.byteLength(context, 'utf-8');
  console.log(`Context gathered (${(contextBytes / 1024).toFixed(0)} KB, ${files.length} files).`);

  // Show files included
  if (files.length > 0) {
    console.log('\nFiles included:');
    for (const file of files) {
      console.log(`  - ${file}`);
    }
    console.log('');
  }

  // Dry run mode - just show what would be included
  if (values.dry) {
    console.log('Dry run complete. No API call made.');
    process.exit(0);
  }

  console.log(`Sending to Gemini via ${useVertex ? 'Vertex AI Express' : 'direct API'}...`);

  let url;
  const headers = { 'Content-Type': 'application/json' };

  if (useVertex) {
    // Vertex AI Express Mode endpoint - uses API key
    url = `https://aiplatform.googleapis.com/v1/publishers/google/models/${values.model}:generateContent?key=${GCP_VERTEX_EXPRESS_API_KEY}`;
  } else {
    // Direct Gemini API endpoint
    url = `https://generativelanguage.googleapis.com/v1beta/models/${values.model}:generateContent?key=${GEMINI_API_KEY}`;
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

  // Print query only mode - show full payload without sending
  if (values['print-query-only']) {
    console.log('\n=== SYSTEM INSTRUCTION ===\n');
    console.log(systemPrompt);
    console.log('\n=== USER MESSAGE ===\n');
    console.log(`Codebase context:\n${context}\n\nQuestion: ${prompt}`);
    console.log('\n=== GENERATION CONFIG ===\n');
    console.log(JSON.stringify(payload.generationConfig, null, 2));
    console.log('\n--- Print query only mode. No API call made. ---');
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

