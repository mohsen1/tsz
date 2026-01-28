#!/usr/bin/env node

import { execSync } from 'child_process';
import { parseArgs } from 'util';
import 'dotenv/config';

// A CLI tool that uses yek (https://github.com/mohsen1/yek) to give Gemini full context of
// this repo to ask questions. Use focused presets (--solver, --checker, etc.) to pack
// the most relevant context for your question.

// Usage:
// ./scripts/ask-gemini.mjs "How to add feature X?"
// ./scripts/ask-gemini.mjs --solver "How does type inference work?"
// ./scripts/ask-gemini.mjs --checker "How are diagnostics reported?"
// ./scripts/ask-gemini.mjs --include="src/solver" "Custom path question"

const DEFAULT_CONTEXT_LENGTH = '850k';
const DEFAULT_MODEL = 'gemini-3-pro-preview';

// Focused presets for different areas of the codebase
// Each preset defines paths to include and recommended context for questions in that area
const PRESETS = {
  solver: {
    description: 'Type solver, inference, compatibility, and type operations',
    paths: ['src/solver', 'src/checker/state.rs', 'src/checker/context.rs', 'AGENTS.md'],
    coreFiles: [
      'src/solver/state.rs',      // Main solver state and type operations
      'src/solver/infer.rs',      // Type inference
      'src/solver/compat.rs',     // Type compatibility/assignability
      'src/solver/contextual.rs', // Contextual typing
      'src/solver/evaluate.rs',   // Type evaluation
      'src/solver/instantiate.rs',// Generic instantiation
      'src/solver/visitor.rs',    // Type visitor pattern
      'src/solver/db.rs',         // Type database
    ],
    tokens: '600k',
  },
  checker: {
    description: 'Type checker, AST traversal, diagnostics, and error reporting',
    paths: ['src/checker', 'src/diagnostics.rs', 'src/binder.rs', 'AGENTS.md'],
    coreFiles: [
      'src/checker/state.rs',         // Main checker state
      'src/checker/context.rs',       // Checker context
      'src/checker/error_reporter.rs',// Error reporting
      'src/checker/control_flow.rs',  // Control flow analysis
      'src/checker/declarations.rs',  // Declaration checking
      'src/checker/flow_analysis.rs', // Flow analysis
      'src/diagnostics.rs',           // Diagnostic definitions
    ],
    tokens: '700k',
  },
  binder: {
    description: 'Symbol binding, scopes, and control flow graph construction',
    paths: ['src/binder', 'src/binder.rs', 'src/checker/flow_graph_builder.rs', 'AGENTS.md'],
    coreFiles: [
      'src/binder/state.rs',              // Binder state
      'src/binder.rs',                    // Main binder
      'src/checker/flow_graph_builder.rs',// CFG builder
    ],
    tokens: '400k',
  },
  parser: {
    description: 'Parser, scanner, and AST nodes',
    paths: ['src/parser', 'src/scanner.rs', 'src/scanner_impl.rs', 'src/span.rs'],
    coreFiles: [
      'src/parser/state.rs',    // Parser state machine
      'src/parser/node.rs',     // AST node definitions
      'src/scanner.rs',         // Scanner interface
      'src/scanner_impl.rs',    // Scanner implementation
    ],
    tokens: '700k',
  },
  emitter: {
    description: 'Code emission, transforms, source maps, and declaration files',
    paths: ['src/emitter', 'src/transforms', 'src/declaration_emitter.rs', 'src/source_map.rs', 'src/source_writer.rs', 'src/printer.rs'],
    coreFiles: [
      'src/emitter/mod.rs',             // Main emitter
      'src/transforms/es5.rs',          // ES5 transforms
      'src/declaration_emitter.rs',     // .d.ts generation
      'src/source_map.rs',              // Source map generation
    ],
    tokens: '600k',
  },
  lsp: {
    description: 'Language server protocol implementation',
    paths: ['src/lsp', 'src/cli'],
    coreFiles: [
      'src/lsp/project.rs',       // LSP project management
      'src/lsp/completions.rs',   // Autocompletion
      'src/lsp/hover.rs',         // Hover information
      'src/lsp/definition.rs',    // Go to definition
      'src/lsp/references.rs',    // Find references
      'src/lsp/code_actions.rs',  // Code actions
    ],
    tokens: '600k',
  },
  types: {
    description: 'Type system overview (solver + checker type-related)',
    paths: ['src/solver', 'src/checker/state.rs', 'src/checker/context.rs', 'src/checker/flow_analysis.rs', 'src/lowering_pass.rs', 'AGENTS.md'],
    coreFiles: [
      'src/solver/state.rs',
      'src/solver/infer.rs',
      'src/solver/compat.rs',
      'src/checker/state.rs',
      'src/lowering_pass.rs',
    ],
    tokens: '800k',
  },
  modules: {
    description: 'Module resolution, imports, exports, and module graph',
    paths: ['src/module_resolver.rs', 'src/module_graph.rs', 'src/imports.rs', 'src/exports.rs', 'src/transforms/module_commonjs.rs', 'src/emitter/module_emission.rs'],
    coreFiles: [
      'src/module_resolver.rs',
      'src/module_graph.rs',
      'src/imports.rs',
      'src/exports.rs',
    ],
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
    // List available presets
    list: { type: 'boolean', default: false },
  },
  allowPositionals: true,
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
  --list              List all available presets with descriptions
  -h, --help          Show this help message

Environment:
  GEMINI_API_KEY      Required. Your Google AI API key.

Examples:
  ./scripts/ask-gemini.mjs --solver "How does type inference work?"
  ./scripts/ask-gemini.mjs --checker "How are diagnostics reported?"
  ./scripts/ask-gemini.mjs --types "How does the type system handle generics?"
  ./scripts/ask-gemini.mjs --parser "How does ASI work?"
  ./scripts/ask-gemini.mjs --emitter "How are source maps generated?"
  ./scripts/ask-gemini.mjs --lsp "How does go-to-definition work?"
  ./scripts/ask-gemini.mjs --modules "How does module resolution work?"
  ./scripts/ask-gemini.mjs --include=src/solver "Custom path question"
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

const GEMINI_API_KEY = process.env.GEMINI_API_KEY;

if (!GEMINI_API_KEY && !values.dry) {
  console.error('Error: GEMINI_API_KEY environment variable is not set.');
  console.error('Get an API key at: https://aistudio.google.com/apikey');
  console.error('(Use --dry to see files without calling API)');
  process.exit(1);
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
  console.log(`Token limit: ${tokenLimit}`);
  console.log('Gathering context with yek...');

  // First, get the full file tree so Gemini knows what files exist
  console.log('  - Building file tree...');
  let fileTree = execSync('yek --config-file yek.yaml --tree-only src/ docs/ 2>/dev/null | cat', {
    maxBuffer: 10 * 1024 * 1024,
    encoding: 'utf-8',
  });

  // Also get root-level important files
  let rootFiles = execSync('ls -1 *.rs *.toml *.md *.yaml 2>/dev/null || true', {
    encoding: 'utf-8',
  }).trim();

  const fullTree = `Repository File Structure:
=========================

Root files:
${rootFiles.split('\n').map(f => `  ${f}`).join('\n')}

Source code (src/):
${fileTree}
`;

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

  // Filter out TypeScript submodule files (yek bug: doesn't respect ignore patterns for submodules)
  const sections = context.split(/^(?=>>>> )/m);
  const filteredSections = sections.filter(section => {
    const match = section.match(/^>>>> (.+)\n/);
    if (!match) return true;
    const filePath = match[1];
    // Skip TypeScript submodule files
    return !filePath.startsWith('TypeScript/');
  });
  context = filteredSections.join('');

  // Prepend the file tree to context
  context = `${fullTree}\n${'='.repeat(50)}\nFILE CONTENTS:\n${'='.repeat(50)}\n\n${context}`;

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
    // Highlight core files from the preset if applicable
    // Match by filename or by full path
    const coreFilePaths = activePreset?.coreFiles || [];
    const coreFileNames = new Set(coreFilePaths.map(f => f.split('/').pop()));
    const coreFileSet = new Set(coreFilePaths);

    let hasCore = false;
    for (const file of files) {
      const fileName = file.split('/').pop();
      const isCore = coreFileSet.has(file) || coreFileSet.has(`src/${file}`) || coreFileNames.has(fileName);
      if (isCore) hasCore = true;
      console.log(`  ${isCore ? '★' : '-'} ${file}`);
    }
    if (hasCore) {
      console.log('\n  ★ = Core file for this preset');
    }
    console.log('');
  }

  // Dry run mode - just show what would be included
  if (values.dry) {
    console.log('Dry run complete. No API call made.');
    process.exit(0);
  }

  console.log('Sending to Gemini...');

  const url = `https://generativelanguage.googleapis.com/v1beta/models/${values.model}:generateContent?key=${GEMINI_API_KEY}`;

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
1. A FULL FILE TREE showing all files in the repository (even those not included in detail)
2. DETAILED CONTENTS of the most relevant files for your question

If you need additional files to answer the question accurately, list them at the end of your response like this:
---
NEED MORE FILES:
- src/path/to/file1.rs (reason: needed to understand X)
- src/path/to/file2.rs (reason: needed to see Y)
---

Answer questions accurately based on the provided context. Reference specific files and line numbers when relevant.`;

  const payload = {
    contents: [
      {
        parts: [
          { text: `${systemPrompt}\n\nCodebase context:\n${context}\n\nQuestion: ${prompt}` }
        ]
      }
    ],
    generationConfig: {
      temperature: 0.2,
      maxOutputTokens: 8192,
    }
  };

  const response = await fetch(url, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(payload),
  });

  if (!response.ok) {
    const errorText = await response.text();
    console.error(`HTTP Error ${response.status}: ${errorText}`);
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

