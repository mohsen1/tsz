#!/usr/bin/env node

import { execSync } from 'child_process';
import { parseArgs } from 'util';
import 'dotenv/config';

// A CLI tool that uses yek (https://github.com/mohsen1/yek) to give Gemini full context of 
// this repo to ask questions. By default it only sends source files and Rust config files
// (respecting yek.yaml). --include can specify specific files to include instead.

// Usage:
// ./scripts/ask-gemini.mjs "How to add feature X?"
// ./scripts/ask-gemini.mjs --include="src/solver" "How to add feature X in Solver?"
// ./scripts/ask-gemini.mjs --model=gemini-2.5-pro-preview-05-06 "Complex question"

const DEFAULT_CONTEXT_LENGTH = '850k';
const DEFAULT_MODEL = 'gemini-3-flash-preview';

const { values, positionals } = parseArgs({
  options: {
    include: {
      type: 'string',
      short: 'i',
    },
    tokens: {
      type: 'string',
      short: 't',
      default: DEFAULT_CONTEXT_LENGTH,
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
  },
  allowPositionals: true,
});

if (values.help) {
  console.log(`
Usage: ./scripts/ask-gemini.mjs [options] "your prompt"

Options:
  -i, --include=PATH    Include specific path(s) instead of yek's default
  -t, --tokens=SIZE     Max context size (default: ${DEFAULT_CONTEXT_LENGTH})
  -m, --model=NAME      Gemini model to use (default: ${DEFAULT_MODEL})
  -h, --help            Show this help message

Environment:
  GEMINI_API_KEY        Required. Your Google AI API key.

Examples:
  ./scripts/ask-gemini.mjs "How does the solver handle recursive types?"
  ./scripts/ask-gemini.mjs --include=src/solver "Explain the subtype checker"
  ./scripts/ask-gemini.mjs --model=gemini-2.5-pro-preview-05-06 "Complex analysis"
`);
  process.exit(0);
}

const GEMINI_API_KEY = process.env.GEMINI_API_KEY;

if (!GEMINI_API_KEY) {
  console.error('Error: GEMINI_API_KEY environment variable is not set.');
  console.error('Get an API key at: https://aistudio.google.com/apikey');
  process.exit(1);
}

const prompt = positionals[0];

if (!prompt) {
  console.error('Error: No prompt provided.');
  console.error('Usage: ./scripts/ask-gemini.mjs [--include="path"] "your prompt"');
  console.error('Run with --help for more options.');
  process.exit(1);
}

try {
  console.log(`Using model: ${values.model}`);
  console.log('Gathering context with yek...');

  let yekCommand = `yek --config-file yek.yaml --tokens ${values.tokens}`;
  if (values.include) {
    yekCommand += ` ${values.include}`;
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

  // Extract file paths from yek markers (>>>> FILE_PATH)
  const fileMarkerRegex = /^>>>> (.+)$/gm;
  const files = [];
  let match;
  while ((match = fileMarkerRegex.exec(context)) !== null) {
    files.push(match[1]);
  }

  const contextBytes = Buffer.byteLength(context, 'utf-8');
  console.log(`Context gathered (${(contextBytes / 1024).toFixed(0)} KB, ${files.length} files). Sending to Gemini...`);
  
  if (files.length > 0) {
    console.log('\nFiles included:');
    for (const file of files) {
      console.log(`  - ${file}`);
    }
    console.log('');
  }

  const url = `https://generativelanguage.googleapis.com/v1beta/models/${values.model}:generateContent?key=${GEMINI_API_KEY}`;
  
  const payload = {
    contents: [
      {
        parts: [
          { text: `You are an expert on this codebase. Answer questions accurately based on the provided context.\n\nCodebase context:\n${context}\n\nQuestion: ${prompt}` }
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

