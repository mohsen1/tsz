import React, { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  getDefaultExampleKey,
  getExampleByKey,
  isValidExampleKey,
  playgroundExamples,
} from "./examples.js";

const LIB_FILES = [
  "lib.es5.d.ts",
  "lib.es2015.d.ts",
  "lib.es2015.core.d.ts",
  "lib.es2015.collection.d.ts",
  "lib.es2015.promise.d.ts",
  "lib.es2015.symbol.d.ts",
  "lib.es2015.iterable.d.ts",
  "lib.es2015.generator.d.ts",
  "lib.dom.d.ts",
  "lib.decorators.d.ts",
  "lib.decorators.legacy.d.ts",
];

const TSCONFIG_MODEL_URI = "file:///tsconfig.json";
const WASM_CACHE_KEY =
  document.querySelector('meta[name="tsz-build-sha"]')?.getAttribute("content") || `dev-${Date.now()}`;

const TSCONFIG_SCHEMA = {
  type: "object",
  allowTrailingCommas: true,
  properties: {
    compilerOptions: {
      type: "object",
      description: "Options passed to the TypeScript compiler.",
      properties: {
        strict: {
          type: "boolean",
          description: "Enable all strict type-checking options.",
        },
        target: {
          type: "string",
          enum: ["ES3", "ES5", "ES6", "ES2015", "ES2016", "ES2017", "ES2018", "ES2019", "ES2020", "ES2021", "ES2022", "ES2023", "ES2024", "ES2025", "ESNext"],
          description: "Set the JavaScript language version for emitted JavaScript.",
        },
        module: {
          type: "string",
          enum: ["None", "CommonJS", "AMD", "UMD", "System", "ES6", "ES2015", "ES2020", "ES2022", "ESNext", "Node16", "Node18", "Node20", "NodeNext", "Preserve"],
          description: "Specify what module code is generated.",
        },
        moduleResolution: {
          type: "string",
          enum: ["Classic", "Node", "Node10", "Node16", "NodeNext", "Bundler"],
          description: "Specify how modules are resolved from a given module specifier.",
        },
        jsx: {
          type: "string",
          enum: ["Preserve", "React", "ReactNative", "ReactJSX", "ReactJSXDev"],
          description: "Specify what JSX code is generated.",
        },
        lib: {
          type: "array",
          items: { type: "string" },
          description: "Specify bundled library declaration files.",
        },
        declaration: { type: "boolean" },
        noEmit: { type: "boolean" },
        sourceMap: { type: "boolean" },
        allowJs: { type: "boolean" },
        checkJs: { type: "boolean" },
        isolatedModules: { type: "boolean" },
        esModuleInterop: { type: "boolean" },
        skipLibCheck: { type: "boolean" },
        noLib: { type: "boolean" },
        noResolve: { type: "boolean" },
        noImplicitAny: { type: "boolean" },
        noImplicitReturns: { type: "boolean" },
        noImplicitThis: { type: "boolean" },
        strictNullChecks: { type: "boolean" },
        strictFunctionTypes: { type: "boolean" },
        strictBindCallApply: { type: "boolean" },
        strictPropertyInitialization: { type: "boolean" },
        strictBuiltinIteratorReturn: { type: "boolean" },
        noUncheckedIndexedAccess: { type: "boolean" },
        exactOptionalPropertyTypes: { type: "boolean" },
        useUnknownInCatchVariables: { type: "boolean" },
        rootDir: { type: "string" },
        outDir: { type: "string" },
        baseUrl: { type: "string" },
        paths: {
          type: "object",
          additionalProperties: {
            type: "array",
            items: { type: "string" },
          },
        },
      },
      additionalProperties: true,
    },
    include: {
      type: "array",
      items: { type: "string" },
    },
    exclude: {
      type: "array",
      items: { type: "string" },
    },
    files: {
      type: "array",
      items: { type: "string" },
    },
    extends: {
      type: "string",
    },
    references: {
      type: "array",
      items: {
        type: "object",
        properties: {
          path: { type: "string" },
        },
      },
    },
  },
  additionalProperties: true,
};

function createTsconfigText(strict) {
  return JSON.stringify({
    compilerOptions: {
      strict,
      module: "ESNext",
    },
  }, null, 2);
}

const SUPPORTED_BOOLEAN_COMPILER_OPTIONS = [
  "strict",
  "noImplicitAny",
  "strictNullChecks",
  "strictFunctionTypes",
  "strictBindCallApply",
  "strictPropertyInitialization",
  "noImplicitReturns",
  "noImplicitThis",
  "useUnknownInCatchVariables",
  "strictBuiltinIteratorReturn",
  "noUncheckedIndexedAccess",
  "exactOptionalPropertyTypes",
  "noLib",
  "allowJs",
  "checkJs",
  "declaration",
  "sourceMap",
  "noResolve",
];

const TS_TARGET_NUMERIC_VALUES = {
  es3: 0,
  es5: 1,
  es6: 2,
  es2015: 2,
  es2016: 3,
  es2017: 4,
  es2018: 5,
  es2019: 6,
  es2020: 7,
  es2021: 8,
  es2022: 9,
  es2023: 10,
  es2024: 11,
  es2025: 12,
  esnext: 99,
};

const TS_MODULE_NUMERIC_VALUES = {
  none: 0,
  commonjs: 1,
  amd: 2,
  umd: 3,
  system: 4,
  es6: 5,
  es2015: 5,
  es2020: 6,
  es2022: 7,
  esnext: 99,
  node16: 100,
  node18: 101,
  node20: 102,
  nodenext: 199,
  preserve: 200,
};

const TS_JSX_NUMERIC_VALUES = {
  preserve: 1,
  react: 2,
  reactnative: 3,
  "react-jsx": 4,
  reactjsx: 4,
  "react-jsxdev": 5,
  reactjsxdev: 5,
};

function normalizeTsconfigEnumValue(value) {
  return String(value).trim().replace(/[\s_]/g, "").toLowerCase();
}

function coerceNumericCompilerOption(value, valueMap) {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value !== "string") {
    return null;
  }
  return valueMap[normalizeTsconfigEnumValue(value)] ?? null;
}

function stripJsonComments(text) {
  let output = "";
  let inString = false;
  let stringQuote = "";
  let escaped = false;

  for (let i = 0; i < text.length; i += 1) {
    const char = text[i];
    const next = text[i + 1];

    if (inString) {
      output += char;
      if (escaped) {
        escaped = false;
      } else if (char === "\\") {
        escaped = true;
      } else if (char === stringQuote) {
        inString = false;
      }
      continue;
    }

    if (char === "\"" || char === "'") {
      inString = true;
      stringQuote = char;
      output += char;
      continue;
    }

    if (char === "/" && next === "/") {
      while (i < text.length && text[i] !== "\n") {
        i += 1;
      }
      output += "\n";
      continue;
    }

    if (char === "/" && next === "*") {
      i += 2;
      while (i < text.length && !(text[i] === "*" && text[i + 1] === "/")) {
        i += 1;
      }
      i += 1;
      continue;
    }

    output += char;
  }

  return output;
}

function parseTsconfigText(text) {
  const withoutComments = stripJsonComments(text);
  const withoutTrailingCommas = withoutComments.replace(/,\s*([}\]])/g, "$1");
  return JSON.parse(withoutTrailingCommas);
}

function readCompilerOptionsFromTsconfig(text) {
  try {
    const config = parseTsconfigText(text);
    const compilerOptions = config?.compilerOptions;
    if (!compilerOptions || typeof compilerOptions !== "object" || Array.isArray(compilerOptions)) {
      return {};
    }

    const options = {};
    for (const key of SUPPORTED_BOOLEAN_COMPILER_OPTIONS) {
      if (typeof compilerOptions[key] === "boolean") {
        options[key] = compilerOptions[key];
      }
    }

    const target = coerceNumericCompilerOption(compilerOptions.target, TS_TARGET_NUMERIC_VALUES);
    if (target !== null) {
      options.target = target;
    }

    const module = coerceNumericCompilerOption(compilerOptions.module, TS_MODULE_NUMERIC_VALUES);
    if (module !== null) {
      options.module = module;
    }

    const jsx = coerceNumericCompilerOption(compilerOptions.jsx, TS_JSX_NUMERIC_VALUES);
    if (jsx !== null) {
      options.jsx = jsx;
    }

    return options;
  } catch {
    return {};
  }
}

function readStrictFromTsconfig(text) {
  try {
    const config = parseTsconfigText(text);
    const strict = config?.compilerOptions?.strict;
    return typeof strict === "boolean" ? strict : null;
  } catch {
    return null;
  }
}

function updateTsconfigStrict(text, strict) {
  try {
    const config = parseTsconfigText(text);
    const nextConfig = config && typeof config === "object" && !Array.isArray(config)
      ? config
      : {};
    const compilerOptions = nextConfig.compilerOptions;
    nextConfig.compilerOptions = compilerOptions && typeof compilerOptions === "object" && !Array.isArray(compilerOptions)
      ? compilerOptions
      : {};
    nextConfig.compilerOptions.strict = strict;
    return JSON.stringify(nextConfig, null, 2);
  } catch {
    return createTsconfigText(strict);
  }
}

function getExampleFromUrl() {
  const params = new URLSearchParams(window.location.search);
  const key = params.get("example");
  return isValidExampleKey(key) ? key : null;
}

function setExampleInUrl(key) {
  const url = new URL(window.location.href);
  if (isValidExampleKey(key)) {
    url.searchParams.set("example", key);
  } else {
    url.searchParams.delete("example");
  }
  window.history.replaceState({}, "", `${url.pathname}${url.search}${url.hash}`);
}

function getExampleUrl(key) {
  const url = new URL(window.location.href);
  if (isValidExampleKey(key)) {
    url.searchParams.set("example", key);
  } else {
    url.searchParams.delete("example");
  }
  return `${url.pathname}${url.search}${url.hash}`;
}

function isDiagnosticsDebugEnabled() {
  const params = new URLSearchParams(window.location.search);
  return params.get("debugDiagnostics") === "1";
}

function debugDiagnosticsLog(label, payload) {
  if (!isDiagnosticsDebugEnabled()) {
    return;
  }
  console.log(`[playground diagnostics] ${label}`, payload);
}

function completionKindToMonaco(monaco, kind) {
  if (typeof kind === "number") return kind;
  switch (kind) {
    case "Function": return monaco.languages.CompletionItemKind.Function;
    case "Class": return monaco.languages.CompletionItemKind.Class;
    case "Method": return monaco.languages.CompletionItemKind.Method;
    case "Parameter": return monaco.languages.CompletionItemKind.Variable;
    case "Property": return monaco.languages.CompletionItemKind.Property;
    case "Keyword": return monaco.languages.CompletionItemKind.Keyword;
    case "Interface": return monaco.languages.CompletionItemKind.Interface;
    case "Enum": return monaco.languages.CompletionItemKind.Enum;
    case "TypeAlias": return monaco.languages.CompletionItemKind.Struct;
    case "Module": return monaco.languages.CompletionItemKind.Module;
    case "TypeParameter": return monaco.languages.CompletionItemKind.TypeParameter;
    case "Constructor": return monaco.languages.CompletionItemKind.Constructor;
    default:
      return monaco.languages.CompletionItemKind.Variable;
  }
}

function PlaygroundApp() {
  const initialExampleKey = getExampleFromUrl() || getDefaultExampleKey();
  const initialExample = getExampleByKey(initialExampleKey);

  const editorContainerRef = useRef(null);
  const jsContainerRef = useRef(null);
  const dtsContainerRef = useRef(null);
  const tsconfigContainerRef = useRef(null);
  const editorRef = useRef(null);
  const jsEditorRef = useRef(null);
  const dtsEditorRef = useRef(null);
  const tsconfigEditorRef = useRef(null);
  const tsconfigModelRef = useRef(null);
  const monacoRef = useRef(null);
  const wasmRef = useRef(null);
  const libFilesRef = useRef({});
  const lspParserRef = useRef(null);
  const lspParserStateRef = useRef(null);
  const checkTimeoutRef = useRef(null);
  const hasRunInitialCheckRef = useRef(false);
  const outputCacheRef = useRef({ key: null, js: null, dts: null });
  const codeRef = useRef(initialExample.source);
  const strictModeRef = useRef(true);
  const tsconfigRef = useRef(createTsconfigText(true));
  const tsconfigSyncingRef = useRef(false);
  const initialSoundMode = initialExampleKey.startsWith("sound_mode");
  const soundModeRef = useRef(initialSoundMode);

  const [selectedExampleKey, setSelectedExampleKey] = useState(initialExampleKey);
  const [code, setCode] = useState(initialExample.source);
  const [strictMode, setStrictMode] = useState(true);
  const [tsconfigText, setTsconfigText] = useState(() => createTsconfigText(true));
  const [soundMode, setSoundMode] = useState(initialSoundMode);
  const [activePanel, setActivePanel] = useState("diagnostics");
  const [diagnostics, setDiagnostics] = useState([]);
  const [status, setStatus] = useState({ text: "loading editor...", className: "status-loading" });
  const [loadError, setLoadError] = useState("");
  const [editorsReady, setEditorsReady] = useState(false);
  const [wasmReady, setWasmReady] = useState(false);
  const [jsOutput, setJsOutput] = useState("");
  const [dtsOutput, setDtsOutput] = useState("");

  codeRef.current = code;
  strictModeRef.current = strictMode;
  tsconfigRef.current = tsconfigText;
  soundModeRef.current = soundMode;

  function getCurrentCompilerOptions() {
    const tsconfigOptions = readCompilerOptionsFromTsconfig(tsconfigRef.current);
    return {
      ...tsconfigOptions,
      strict: strictModeRef.current,
      soundMode: soundModeRef.current,
    };
  }

  function resetOutputCache() {
    outputCacheRef.current = { key: null, js: null, dts: null };
    setJsOutput("");
    setDtsOutput("");
  }

  function syncTsconfigEditorText(nextText) {
    tsconfigSyncingRef.current = true;
    tsconfigRef.current = nextText;
    setTsconfigText(nextText);
    if (tsconfigEditorRef.current && tsconfigEditorRef.current.getValue() !== nextText) {
      tsconfigEditorRef.current.setValue(nextText);
    }
    window.setTimeout(() => {
      tsconfigSyncingRef.current = false;
    }, 0);
  }

  function setStrictEverywhere(nextStrict) {
    setStrictMode(nextStrict);
    syncTsconfigEditorText(updateTsconfigStrict(tsconfigRef.current, nextStrict));
    resetOutputCache();
  }

  function getOutputStateKey(nextCode, options) {
    return JSON.stringify({
      code: nextCode,
      options,
    });
  }

  function disposeLspParser() {
    if (lspParserRef.current && typeof lspParserRef.current.dispose === "function") {
      lspParserRef.current.dispose();
    } else if (lspParserRef.current && typeof lspParserRef.current.free === "function") {
      lspParserRef.current.free();
    }
    lspParserRef.current = null;
    lspParserStateRef.current = null;
  }

  function createEmitProgram(nextCode, options) {
    const program = new wasmRef.current.TsProgram();
    program.setCompilerOptions(JSON.stringify(options));
    for (const [name, content] of Object.entries(libFilesRef.current)) {
      program.addLibFile(name, content);
    }
    program.addSourceFile("input.ts", nextCode);
    return program;
  }

  function createCheckProgram(nextCode, options) {
    if (wasmRef.current.Parser) {
      const parser = new wasmRef.current.Parser("input.ts", nextCode);
      parser.setCompilerOptions(JSON.stringify(options));
      for (const [name, content] of Object.entries(libFilesRef.current)) {
        parser.addLibFile(name, content);
      }
      parser.parseSourceFile();
      if (typeof parser.bindSourceFile === "function") {
        parser.bindSourceFile();
      }
      return parser;
    }

    if (!wasmRef.current.WasmProgram) {
      throw new Error("WasmProgram is required for playground diagnostics");
    }

    const program = new wasmRef.current.WasmProgram();
    program.setCompilerOptions(JSON.stringify(options));
    for (const [name, content] of Object.entries(libFilesRef.current)) {
      program.addLibFile(name, content);
    }
    if (typeof program.addFile === "function") {
      program.addFile("input.ts", nextCode);
    } else {
      program.addSourceFile("input.ts", nextCode);
    }
    return program;
  }

  function normalizeDiagnostics(program, nextCode) {
    if (typeof program.checkSourceFile === "function") {
      const result = JSON.parse(program.checkSourceFile() || "{}");
      const diagnostics = Array.isArray(result.diagnostics) ? result.diagnostics : [];
      return diagnostics.map(diagnostic => ({
        start: diagnostic.start ?? 0,
        length: diagnostic.length ?? 1,
        messageText: diagnostic.messageText || diagnostic.message_text || diagnostic.message || "",
        category: diagnostic.category === "Warning"
          ? 0
          : diagnostic.category === "Suggestion"
            ? 2
            : diagnostic.category === "Message"
              ? 3
              : 1,
        code: diagnostic.code,
      }));
    }

    if (typeof program.getPreEmitDiagnosticsJson === "function") {
      return JSON.parse(program.getPreEmitDiagnosticsJson() || "[]");
    }

    if (typeof program.checkAll === "function") {
      const result = JSON.parse(program.checkAll() || "{}");
      const files = Array.isArray(result.files) ? result.files : [];
      const file = files.find(entry => (entry.fileName || entry.file_name) === "input.ts");
      if (!file) {
        return [];
      }

      const parseDiagnostics = Array.isArray(file.parseDiagnostics)
        ? file.parseDiagnostics
        : Array.isArray(file.parse_diagnostics)
          ? file.parse_diagnostics
          : [];
      const checkDiagnostics = Array.isArray(file.checkDiagnostics)
        ? file.checkDiagnostics
        : Array.isArray(file.check_diagnostics)
          ? file.check_diagnostics
          : [];

      const normalizedParseDiagnostics = parseDiagnostics.map(diagnostic => ({
        start: diagnostic.start ?? 0,
        length: diagnostic.length ?? 0,
        messageText: diagnostic.messageText || diagnostic.message || "",
        category: 1,
        code: diagnostic.code,
      }));
      const normalizedCheckDiagnostics = checkDiagnostics.map(diagnostic => ({
        start: diagnostic.start ?? 0,
        length: diagnostic.length ?? 0,
        messageText: diagnostic.messageText || diagnostic.message_text || "",
        category: diagnostic.category === "Warning"
          ? 0
          : diagnostic.category === "Suggestion"
            ? 2
            : diagnostic.category === "Message"
              ? 3
              : 1,
        code: diagnostic.code,
      }));

      return [...normalizedParseDiagnostics, ...normalizedCheckDiagnostics];
    }

    console.warn("No supported diagnostics API found on wasm program", {
      keys: Object.keys(program || {}),
      codePreview: nextCode.slice(0, 80),
    });
    return [];
  }

  function getDiagnosticIdentity(diagnostic) {
    return JSON.stringify({
      start: diagnostic.start ?? 0,
      length: diagnostic.length ?? 0,
      code: diagnostic.code,
      messageText: diagnostic.messageText || "",
      category: diagnostic.category,
    });
  }

  function withSoundDiagnosticDisplayCodes(soundDiagnostics, baselineDiagnostics, forcedDisplayCode = null) {
    const baselineIdentities = new Set(baselineDiagnostics.map(getDiagnosticIdentity));

    return soundDiagnostics.map(diagnostic => {
      if (!forcedDisplayCode && baselineIdentities.has(getDiagnosticIdentity(diagnostic))) {
        return diagnostic;
      }

      return {
        ...diagnostic,
        displayCode: forcedDisplayCode || "TSZ3006",
        originalCode: `TS${diagnostic.code}`,
        domain: "sound",
      };
    });
  }

  function formatDiagnosticCode(diagnostic) {
    return diagnostic.displayCode || `TS${diagnostic.code}`;
  }

  function toLspPosition(position) {
    return {
      line: Math.max(0, position.lineNumber - 1),
      character: Math.max(0, position.column - 1),
    };
  }

  function toMonacoRange(range) {
    if (!range || !range.start || !range.end || !monacoRef.current) return undefined;
    return new monacoRef.current.Range(
      range.start.line + 1,
      range.start.character + 1,
      range.end.line + 1,
      range.end.character + 1
    );
  }

  function ensureLspParser() {
    if (!wasmRef.current || !editorRef.current) return null;

    const nextCode = codeRef.current;
    const options = getCurrentCompilerOptions();
    const state = JSON.stringify({
      code: nextCode,
      options,
      libCount: Object.keys(libFilesRef.current).length,
    });

    if (lspParserRef.current && lspParserStateRef.current === state) {
      return lspParserRef.current;
    }

    disposeLspParser();

    try {
      const parser = new wasmRef.current.Parser("input.ts", nextCode);
      parser.setCompilerOptions(JSON.stringify(options));
      for (const [name, content] of Object.entries(libFilesRef.current)) {
        parser.addLibFile(name, content);
      }
      parser.parseSourceFile();
      if (typeof parser.bindSourceFile === "function") {
        parser.bindSourceFile();
      }
      lspParserRef.current = parser;
      lspParserStateRef.current = state;
      return parser;
    } catch (error) {
      console.warn("Failed to build TSZ parser for LSP features:", error);
      disposeLspParser();
      return null;
    }
  }

  function configureTsconfigJsonLanguage(monaco) {
    if (!monaco.languages?.json?.jsonDefaults) return;

    monaco.languages.json.jsonDefaults.setDiagnosticsOptions({
      validate: true,
      allowComments: true,
      enableSchemaRequest: true,
      schemas: [
        {
          uri: "https://json.schemastore.org/tsconfig",
          fileMatch: [TSCONFIG_MODEL_URI, "tsconfig.json"],
          schema: TSCONFIG_SCHEMA,
        },
      ],
    });
  }

  async function loadMonaco() {
    if (window.monaco) {
      monacoRef.current = window.monaco;
      return window.monaco;
    }

    return new Promise((resolve, reject) => {
      const script = document.createElement("script");
      script.src = "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/min/vs/loader.js";
      script.onload = () => {
        window.require.config({
          paths: { vs: "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/min/vs" },
        });

        window.require(["vs/editor/editor.main"], () => {
          monacoRef.current = window.monaco;
          resolve(window.monaco);
        });
      };
      script.onerror = reject;
      document.head.appendChild(script);
    });
  }

  async function loadWasm() {
    const module = await import(`/wasm/tsz_wasm.js?v=${encodeURIComponent(WASM_CACHE_KEY)}`);
    await module.default();
    wasmRef.current = module;
    return module;
  }

  async function loadLibFiles() {
    const libs = {};
    await Promise.all(
      LIB_FILES.map(async name => {
        const response = await fetch(`/lib/${name}`);
        if (response.ok) {
          libs[name] = await response.text();
        }
      })
    );
    libFilesRef.current = libs;
  }

  async function updateActiveOutputPanel() {
    if (!wasmRef.current || !editorRef.current) return;
    if (activePanel !== "js" && activePanel !== "dts") return;

    const options = getCurrentCompilerOptions();
    const cacheKey = getOutputStateKey(codeRef.current, options);
    if (outputCacheRef.current.key !== cacheKey) {
      outputCacheRef.current = { key: cacheKey, js: null, dts: null };
    }

    if (activePanel === "js" && jsEditorRef.current) {
      if (outputCacheRef.current.js === null) {
        try {
          const program = createEmitProgram(codeRef.current, options);
          outputCacheRef.current.js = program.emitFile("input.ts") || "// (empty output)";
          program.dispose();
        } catch (error) {
          outputCacheRef.current.js = `// Emit error: ${error.message}`;
        }
      }
      setJsOutput(outputCacheRef.current.js);
      jsEditorRef.current.setValue(outputCacheRef.current.js);
      return;
    }

    if (activePanel === "dts" && dtsEditorRef.current) {
      if (outputCacheRef.current.dts === null) {
        try {
          const resultJson = wasmRef.current.transpileModule(
            codeRef.current,
            JSON.stringify({ ...options, declaration: true })
          );
          const result = JSON.parse(resultJson || "{}");
          outputCacheRef.current.dts = typeof result.declarationText === "string"
            ? result.declarationText
            : typeof result.declaration_text === "string"
              ? result.declaration_text
              : "// (no declaration output)";
          if (!outputCacheRef.current.dts) {
            outputCacheRef.current.dts = "// (no declaration output)";
          }
        } catch (error) {
          outputCacheRef.current.dts = `// DTS emit error: ${error.message}`;
        }
      }
      setDtsOutput(outputCacheRef.current.dts);
      dtsEditorRef.current.setValue(outputCacheRef.current.dts);
    }
  }

  async function runCheck() {
    if (!wasmRef.current || !editorRef.current) return;

    checkTimeoutRef.current = null;
    const options = getCurrentCompilerOptions();
    resetOutputCache();

    debugDiagnosticsLog("runCheck:start", {
      example: selectedExampleKey,
      strict: options.strict,
      soundMode: options.soundMode,
      code: codeRef.current,
    });

    setStatus({ text: "checking...", className: "status-checking" });

    const startedAt = performance.now();

    try {
      const program = createCheckProgram(codeRef.current, options);
      const parsedDiagnostics = normalizeDiagnostics(program, codeRef.current);
      let userDiagnostics = parsedDiagnostics.filter(diagnostic => !(diagnostic.code === 2318 && diagnostic.start === 0));
      if (options.soundMode) {
        const selectedExample = getExampleByKey(selectedExampleKey);
        const baselineOptions = { ...options, soundMode: false };
        const baselineProgram = createCheckProgram(codeRef.current, baselineOptions);
        const baselineDiagnostics = normalizeDiagnostics(baselineProgram, codeRef.current)
          .filter(diagnostic => !(diagnostic.code === 2318 && diagnostic.start === 0));
        userDiagnostics = withSoundDiagnosticDisplayCodes(
          userDiagnostics,
          baselineDiagnostics,
          selectedExample?.soundDiagnosticCode
        );
        if (typeof baselineProgram.dispose === "function") {
          baselineProgram.dispose();
        }
      }
      const elapsed = `${(performance.now() - startedAt).toFixed(0)}ms`;

      debugDiagnosticsLog("runCheck:raw-diagnostics", parsedDiagnostics);

      setDiagnostics(userDiagnostics);

      const errorCount = userDiagnostics.filter(diagnostic => diagnostic.category === 1).length;
      const warningCount = userDiagnostics.filter(diagnostic => diagnostic.category === 0).length;

      if (errorCount === 0 && warningCount === 0) {
        setStatus({ text: elapsed, className: "status-ready" });
      } else {
        const parts = [];
        if (errorCount) parts.push(`${errorCount} error${errorCount > 1 ? "s" : ""}`);
        if (warningCount) parts.push(`${warningCount} warning${warningCount > 1 ? "s" : ""}`);
        setStatus({ text: `${parts.join(", ")} ${elapsed}`, className: "status-count" });
      }

      const model = editorRef.current.getModel();
      const markers = userDiagnostics.map(diagnostic => {
        const start = model.getPositionAt(diagnostic.start);
        const end = model.getPositionAt(diagnostic.start + (diagnostic.length || 1));
        return {
          severity: diagnostic.category === 1
            ? monacoRef.current.MarkerSeverity.Error
            : diagnostic.category === 0
              ? monacoRef.current.MarkerSeverity.Warning
              : monacoRef.current.MarkerSeverity.Info,
          message: diagnostic.messageText,
          startLineNumber: start.lineNumber,
          startColumn: start.column,
          endLineNumber: end.lineNumber,
          endColumn: end.column,
          code: formatDiagnosticCode(diagnostic),
        };
      });
      monacoRef.current.editor.setModelMarkers(model, "tsz", markers);
      if (typeof program.dispose === "function") {
        program.dispose();
      }
    } catch (error) {
      setStatus({ text: `error: ${error.message}`, className: "status-error" });
      console.error("Check failed:", error);
    }
  }

  function scheduleCheck(delay = 250) {
    if (checkTimeoutRef.current) {
      clearTimeout(checkTimeoutRef.current);
    }
    checkTimeoutRef.current = window.setTimeout(() => {
      runCheck();
    }, delay);
  }

  useEffect(() => {
    let cancelled = false;

    async function boot() {
      try {
        setStatus({ text: "loading editor...", className: "status-loading" });
        await loadMonaco();
        if (cancelled) return;

        setStatus({ text: "loading WASM...", className: "status-loading" });
        await Promise.all([loadWasm(), loadLibFiles()]);
        if (cancelled) return;

        setWasmReady(true);
        setStatus({ text: "ready", className: "status-ready" });
      } catch (error) {
        console.error("Playground bootstrap failed:", error);
        setLoadError(error.message || "Failed to load playground");
        setStatus({ text: "failed to load", className: "status-error" });
      }
    }

    boot();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!monacoRef.current || !editorContainerRef.current || editorRef.current) {
      return;
    }

    const monaco = monacoRef.current;
    const isDark = window.matchMedia("(prefers-color-scheme: dark)").matches;

    editorRef.current = monaco.editor.create(editorContainerRef.current, {
      value: codeRef.current,
      language: "typescript",
      theme: isDark ? "vs-dark" : "vs",
      minimap: { enabled: false },
      fontSize: 14,
      fontFamily: "'SF Mono', 'Cascadia Code', 'JetBrains Mono', 'Fira Code', Menlo, Consolas, monospace",
      lineNumbers: "on",
      scrollBeyondLastLine: false,
      automaticLayout: true,
      tabSize: 2,
      renderLineHighlight: "all",
      padding: { top: 12 },
      smoothScrolling: true,
      cursorBlinking: "smooth",
    });

    monaco.languages.typescript.typescriptDefaults.setDiagnosticsOptions({
      noSemanticValidation: true,
      noSyntaxValidation: true,
    });

    if (typeof monaco.languages.typescript.typescriptDefaults.setModeConfiguration === "function") {
      monaco.languages.typescript.typescriptDefaults.setModeConfiguration({
        completionItems: false,
        hovers: false,
        signatureHelp: false,
        documentSymbols: false,
        definitions: false,
        references: false,
        documentHighlights: false,
        rename: false,
        diagnostics: false,
        selectionRanges: false,
        inlayHints: false,
        semanticTokens: false,
        codeActions: false,
      });
    }
    configureTsconfigJsonLanguage(monaco);

    jsEditorRef.current = monaco.editor.create(jsContainerRef.current, {
      value: "",
      language: "javascript",
      theme: isDark ? "vs-dark" : "vs",
      minimap: { enabled: false },
      fontSize: 14,
      fontFamily: "'SF Mono', 'Cascadia Code', 'JetBrains Mono', 'Fira Code', Menlo, Consolas, monospace",
      lineNumbers: "on",
      scrollBeyondLastLine: false,
      automaticLayout: true,
      tabSize: 2,
      readOnly: true,
      renderLineHighlight: "none",
      padding: { top: 12 },
      smoothScrolling: true,
    });

    dtsEditorRef.current = monaco.editor.create(dtsContainerRef.current, {
      value: "",
      language: "typescript",
      theme: isDark ? "vs-dark" : "vs",
      minimap: { enabled: false },
      fontSize: 14,
      fontFamily: "'SF Mono', 'Cascadia Code', 'JetBrains Mono', 'Fira Code', Menlo, Consolas, monospace",
      lineNumbers: "on",
      scrollBeyondLastLine: false,
      automaticLayout: true,
      tabSize: 2,
      readOnly: true,
      renderLineHighlight: "none",
      padding: { top: 12 },
      smoothScrolling: true,
    });

    tsconfigModelRef.current = monaco.editor.createModel(
      tsconfigRef.current,
      "json",
      monaco.Uri.parse(TSCONFIG_MODEL_URI)
    );
    tsconfigEditorRef.current = monaco.editor.create(tsconfigContainerRef.current, {
      model: tsconfigModelRef.current,
      theme: isDark ? "vs-dark" : "vs",
      minimap: { enabled: false },
      fontSize: 14,
      fontFamily: "'SF Mono', 'Cascadia Code', 'JetBrains Mono', 'Fira Code', Menlo, Consolas, monospace",
      lineNumbers: "on",
      scrollBeyondLastLine: false,
      automaticLayout: true,
      tabSize: 2,
      renderLineHighlight: "all",
      padding: { top: 12 },
      smoothScrolling: true,
      cursorBlinking: "smooth",
      quickSuggestions: true,
      suggestOnTriggerCharacters: true,
    });

    editorRef.current.onDidChangeModelContent(() => {
      setCode(editorRef.current.getValue());
    });

    tsconfigEditorRef.current.onDidChangeModelContent(() => {
      const nextText = tsconfigEditorRef.current.getValue();
      tsconfigRef.current = nextText;
      setTsconfigText(nextText);
      if (tsconfigSyncingRef.current) return;

      const nextStrict = readStrictFromTsconfig(nextText);
      if (typeof nextStrict === "boolean" && nextStrict !== strictModeRef.current) {
        setStrictMode(nextStrict);
        resetOutputCache();
      }
    });

    monaco.languages.registerHoverProvider("typescript", {
      provideHover(model, position) {
        if (!editorRef.current || model !== editorRef.current.getModel()) return null;
        const parser = ensureLspParser();
        if (!parser) return null;

        try {
          const pos = toLspPosition(position);
          const hover = parser.getHoverAtPosition(pos.line, pos.character);
          if (!hover) return null;
          return {
            range: toMonacoRange(hover.range),
            contents: Array.isArray(hover.contents)
              ? hover.contents.map(content => ({ value: String(content) }))
              : [],
          };
        } catch (error) {
          console.warn("TSZ hover failed:", error);
          return null;
        }
      },
    });

    monaco.languages.registerCompletionItemProvider("typescript", {
      triggerCharacters: [".", "\"", "'", "/", "@", "<"],
      provideCompletionItems(model, position) {
        if (!editorRef.current || model !== editorRef.current.getModel()) {
          return { suggestions: [] };
        }

        const parser = ensureLspParser();
        if (!parser) {
          return { suggestions: [] };
        }

        try {
          const pos = toLspPosition(position);
          const result = parser.getCompletionsAtPosition(pos.line, pos.character);
          const entries = Array.isArray(result)
            ? result
            : result && Array.isArray(result.entries)
              ? result.entries
              : [];
          return {
            suggestions: entries.map(entry => ({
              label: entry.label,
              kind: completionKindToMonaco(monaco, entry.kind),
              detail: entry.detail || undefined,
              documentation: entry.documentation ? { value: String(entry.documentation) } : undefined,
              sortText: entry.sort_text || undefined,
              filterText: entry.label,
              insertText: entry.insert_text || entry.label,
              insertTextRules: entry.is_snippet
                ? monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet
                : monaco.languages.CompletionItemInsertTextRule.None,
            })),
          };
        } catch (error) {
          console.warn("TSZ completions failed:", error);
          return { suggestions: [] };
        }
      },
    });

    monaco.languages.registerSignatureHelpProvider("typescript", {
      signatureHelpTriggerCharacters: ["(", ","],
      signatureHelpRetriggerCharacters: [","],
      provideSignatureHelp(model, position) {
        if (!editorRef.current || model !== editorRef.current.getModel()) {
          return {
            value: { signatures: [], activeSignature: 0, activeParameter: 0 },
            dispose: () => {},
          };
        }

        const parser = ensureLspParser();
        if (!parser) {
          return {
            value: { signatures: [], activeSignature: 0, activeParameter: 0 },
            dispose: () => {},
          };
        }

        try {
          const pos = toLspPosition(position);
          const help = parser.getSignatureHelpAtPosition(pos.line, pos.character);
          if (!help || !Array.isArray(help.signatures) || help.signatures.length === 0) {
            return {
              value: { signatures: [], activeSignature: 0, activeParameter: 0 },
              dispose: () => {},
            };
          }

          return {
            value: {
              signatures: help.signatures.map(signature => ({
                label: signature.label,
                documentation: signature.documentation ? { value: String(signature.documentation) } : undefined,
                parameters: Array.isArray(signature.parameters)
                  ? signature.parameters.map(parameter => ({
                      label: parameter.label,
                      documentation: parameter.documentation ? { value: String(parameter.documentation) } : undefined,
                    }))
                  : [],
              })),
              activeSignature: help.active_signature || 0,
              activeParameter: help.active_parameter || 0,
            },
            dispose: () => {},
          };
        } catch (error) {
          console.warn("TSZ signature help failed:", error);
          return {
            value: { signatures: [], activeSignature: 0, activeParameter: 0 },
            dispose: () => {},
          };
        }
      },
    });

    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    const themeListener = event => {
      monaco.editor.setTheme(event.matches ? "vs-dark" : "vs");
    };
    mediaQuery.addEventListener("change", themeListener);

    setEditorsReady(true);

    return () => {
      mediaQuery.removeEventListener("change", themeListener);
      disposeLspParser();
      editorRef.current?.dispose();
      jsEditorRef.current?.dispose();
      dtsEditorRef.current?.dispose();
      tsconfigEditorRef.current?.dispose();
      tsconfigModelRef.current?.dispose();
    };
  }, [monacoRef.current]);

  useEffect(() => {
    if (!editorRef.current) return;
    if (editorRef.current.getValue() === code) return;
    editorRef.current.setValue(code);
  }, [code]);

  useEffect(() => {
    disposeLspParser();
  }, [code, strictMode, soundMode, tsconfigText]);

  useEffect(() => {
    if (!editorsReady || !wasmReady) return;

    if (!hasRunInitialCheckRef.current) {
      hasRunInitialCheckRef.current = true;
      runCheck();
      return undefined;
    }

    scheduleCheck();

    return () => {
      if (checkTimeoutRef.current) {
        clearTimeout(checkTimeoutRef.current);
        checkTimeoutRef.current = null;
      }
    };
  }, [code, strictMode, soundMode, tsconfigText, editorsReady, wasmReady]);

  useEffect(() => {
    if (!editorsReady || !wasmReady) return;
    if (activePanel === "js" || activePanel === "dts") {
      updateActiveOutputPanel();
    }
  }, [activePanel, tsconfigText, editorsReady, wasmReady]);

  function handleExampleChange(event) {
    const nextKey = event.target.value;
    const example = getExampleByKey(nextKey);
    if (!example) return;

    // Force a full page navigation so Monaco, wasm state, and cached parser/program
    // objects are rebuilt from scratch for each example switch.
    window.location.assign(getExampleUrl(nextKey));
  }

  function handleStrictChange(event) {
    setStrictEverywhere(event.target.checked);
  }

  function handleSoundChange(event) {
    setSoundMode(event.target.checked);
    resetOutputCache();
  }

  function handleDiagnosticClick(start) {
    if (!editorRef.current) return;
    const position = editorRef.current.getModel().getPositionAt(start);
    editorRef.current.revealLineInCenter(position.lineNumber);
    editorRef.current.setPosition(position);
    editorRef.current.focus();
  }

  const groupedExamples = playgroundExamples.reduce((groups, example) => {
    if (!groups[example.category]) {
      groups[example.category] = [];
    }
    groups[example.category].push(example);
    return groups;
  }, {});

  const showFallback = Boolean(loadError);
  return showFallback ? (
    <div className="fallback-box">
      <p><strong>WASM module not available.</strong></p>
      <p>{loadError || "The playground requires the tsz WASM build."}</p>
      <div className="install-block" style={{ justifyContent: "center" }}>
        <span className="prompt">$</span>
        <span className="cmd">npm install -g @mohsen-azimi/tsz-dev</span>
      </div>
    </div>
  ) : (
    <>
      <div className="playground-toolbar">
        <div className="toolbar-left">
          <select value={selectedExampleKey} onChange={handleExampleChange}>
            {Object.entries(groupedExamples).map(([category, examples]) => (
              <optgroup key={category} label={category}>
                {examples.map(example => (
                  <option key={example.key} value={example.key}>{example.title}</option>
                ))}
              </optgroup>
            ))}
          </select>
          <label className="toolbar-check">
            <input type="checkbox" checked={strictMode} onChange={handleStrictChange} />
            <span>strict</span>
          </label>
          <label className="toolbar-check">
            <input type="checkbox" checked={soundMode} onChange={handleSoundChange} />
            <span>sound</span>
          </label>
        </div>
        <div className="toolbar-right">
          <span id="playground-status" className={status.className}>{status.text}</span>
        </div>
      </div>

      <div className="playground-panels">
        <div id="editor-container" ref={editorContainerRef} />
        <div className="playground-divider" />
        <div className="playground-output">
          <div className="output-tabs">
            <button
              className={`output-tab${activePanel === "diagnostics" ? " active" : ""}`}
              onClick={() => setActivePanel("diagnostics")}
              type="button"
            >
              Diagnostics <span className={`tab-badge${diagnostics.length === 0 ? " zero" : ""}`}>{diagnostics.length}</span>
            </button>
            <button
              className={`output-tab${activePanel === "js" ? " active" : ""}`}
              onClick={() => setActivePanel("js")}
              type="button"
            >
              JS Output
            </button>
            <button
              className={`output-tab${activePanel === "dts" ? " active" : ""}`}
              onClick={() => setActivePanel("dts")}
              type="button"
            >
              DTS Output
            </button>
            <button
              className={`output-tab${activePanel === "tsconfig" ? " active" : ""}`}
              onClick={() => setActivePanel("tsconfig")}
              type="button"
            >
              tsconfig
            </button>
          </div>

          <div className={`output-panel${activePanel === "diagnostics" ? " active" : ""}`}>
            {diagnostics.length === 0 ? (
              <div className={status.className === "status-loading" ? "diagnostics-empty" : "diagnostics-ok"}>
                {status.className === "status-loading" ? "Type-check results appear here" : "No errors"}
              </div>
            ) : (
              diagnostics.map(diagnostic => {
                const position = editorRef.current?.getModel().getPositionAt(diagnostic.start);
                const category = diagnostic.category === 1 ? "error" : diagnostic.category === 0 ? "warning" : "suggestion";
                return (
                  <div
                    key={`${formatDiagnosticCode(diagnostic)}-${diagnostic.code}-${diagnostic.start}-${diagnostic.length}`}
                    className="diag-item"
                    onClick={() => handleDiagnosticClick(diagnostic.start)}
                  >
                    <div className="diag-header">
                      <span className={`diag-code ${category}`}>{formatDiagnosticCode(diagnostic)}</span>
                      <span className="diag-message">{diagnostic.messageText}</span>
                    </div>
                    <div className="diag-location">
                      input.ts:{position?.lineNumber ?? 1}:{position?.column ?? 1}
                    </div>
                  </div>
                );
              })
            )}
          </div>

          <div className={`output-panel${activePanel === "js" ? " active" : ""}`}>
            <div id="js-output-editor" ref={jsContainerRef} data-output={jsOutput} />
          </div>

          <div className={`output-panel${activePanel === "dts" ? " active" : ""}`}>
            <div id="dts-output-editor" ref={dtsContainerRef} data-output={dtsOutput} />
          </div>

          <div className={`output-panel${activePanel === "tsconfig" ? " active" : ""}`}>
            <div id="tsconfig-editor" ref={tsconfigContainerRef} data-output={tsconfigText} />
          </div>
        </div>
      </div>
    </>
  );
}

const root = document.getElementById("playground-root");
createRoot(root).render(<PlaygroundApp />);
