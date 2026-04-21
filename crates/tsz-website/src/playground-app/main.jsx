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
  const editorRef = useRef(null);
  const jsEditorRef = useRef(null);
  const dtsEditorRef = useRef(null);
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

  const [selectedExampleKey, setSelectedExampleKey] = useState(initialExampleKey);
  const [code, setCode] = useState(initialExample.source);
  const [strictMode, setStrictMode] = useState(true);
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

  function getCurrentCompilerOptions() {
    return {
      strict: strictModeRef.current,
      module: 99,
    };
  }

  function resetOutputCache() {
    outputCacheRef.current = { key: null, js: null, dts: null };
    setJsOutput("");
    setDtsOutput("");
  }

  function getOutputStateKey(nextCode, options) {
    return JSON.stringify({
      code: nextCode,
      strict: options.strict,
      module: options.module,
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
    const ProgramCtor = wasmRef.current.WasmProgram || wasmRef.current.TsProgram;
    const program = new ProgramCtor();
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
      strict: options.strict,
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
    const module = await import("/wasm/tsz_wasm.js");
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
            JSON.stringify({ declaration: true })
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
      code: codeRef.current,
    });

    setStatus({ text: "checking...", className: "status-checking" });

    const startedAt = performance.now();

    try {
      const program = createCheckProgram(codeRef.current, options);
      const parsedDiagnostics = normalizeDiagnostics(program, codeRef.current);
      const userDiagnostics = parsedDiagnostics.filter(diagnostic => !(diagnostic.code === 2318 && diagnostic.start === 0));
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
          code: `TS${diagnostic.code}`,
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

    editorRef.current.onDidChangeModelContent(() => {
      setCode(editorRef.current.getValue());
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
    };
  }, [monacoRef.current]);

  useEffect(() => {
    if (!editorRef.current) return;
    if (editorRef.current.getValue() === code) return;
    editorRef.current.setValue(code);
  }, [code]);

  useEffect(() => {
    disposeLspParser();
  }, [code, strictMode]);

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
  }, [code, strictMode, editorsReady, wasmReady]);

  useEffect(() => {
    if (!editorsReady || !wasmReady) return;
    if (activePanel === "js" || activePanel === "dts") {
      updateActiveOutputPanel();
    }
  }, [activePanel, editorsReady, wasmReady]);

  function handleExampleChange(event) {
    const nextKey = event.target.value;
    const example = getExampleByKey(nextKey);
    if (!example) return;

    // Force a full page navigation so Monaco, wasm state, and cached parser/program
    // objects are rebuilt from scratch for each example switch.
    window.location.assign(getExampleUrl(nextKey));
  }

  function handleStrictChange(event) {
    setStrictMode(event.target.checked);
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
          <span className="toolbar-title">Playground</span>
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
                    key={`${diagnostic.code}-${diagnostic.start}-${diagnostic.length}`}
                    className="diag-item"
                    onClick={() => handleDiagnosticClick(diagnostic.start)}
                  >
                    <div className="diag-header">
                      <span className={`diag-code ${category}`}>TS{diagnostic.code}</span>
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
        </div>
      </div>
    </>
  );
}

const root = document.getElementById("playground-root");
createRoot(root).render(<PlaygroundApp />);