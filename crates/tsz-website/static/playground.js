/**
 * TSZ Playground
 *
 * Loads Monaco editor + tsz WASM module.
 * Type-checks on edit (debounced) and renders diagnostics.
 */

const EXAMPLES = {
  hello: `const greeting: string = "Hello, tsz!";
console.log(greeting);

function add(a: number, b: number): number {
  return a + b;
}

const result = add(1, 2);
`,
  generics: `function identity<T>(value: T): T {
  return value;
}

const str = identity("hello");  // string
const num = identity(42);       // number

interface Container<T> {
  value: T;
  map<U>(fn: (val: T) => U): Container<U>;
}

function wrap<T>(value: T): Container<T> {
  return {
    value,
    map(fn) {
      return wrap(fn(value));
    }
  };
}

const boxed = wrap(42).map(n => n.toString());
`,
  narrowing: `type Shape =
  | { kind: "circle"; radius: number }
  | { kind: "rectangle"; width: number; height: number };

function area(shape: Shape): number {
  switch (shape.kind) {
    case "circle":
      return Math.PI * shape.radius ** 2;
    case "rectangle":
      return shape.width * shape.height;
    default:
      const _exhaustive: never = shape;
      return _exhaustive;
  }
}

function processValue(value: string | number | null) {
  if (value === null) {
    return "null";
  }
  if (typeof value === "string") {
    return value.toUpperCase();
  }
  return value.toFixed(2);
}
`,
  dts: `export type Id = string | number;

export interface User {
  id: Id;
  name: string;
  tags?: readonly string[];
}

export class UserStore<T extends User> {
  #items: T[] = [];

  add(user: T): void {
    this.#items.push(user);
  }

  getById(id: Id): T | undefined {
    return this.#items.find(item => item.id === id);
  }

  all(): readonly T[] {
    return this.#items;
  }
}

export function createUser(name: string): User {
  return { id: name.toLowerCase(), name };
}
`,
  sound_mode: `// ⚠️ Sound mode is experimental.
// Uncheck "sound" to see these errors disappear!

// 1. Sticky freshness — excess properties via indirection
//    tsc allows this because freshness is "widened away"
interface Point2D { x: number; y: number }
const point3d = { x: 1, y: 2, z: 3 };
const p: Point2D = point3d; // Sound: excess 'z'

// 2. Method bivariance
//    tsc allows subclass methods to narrow parameter types unsafely
class Animal {
  feed(food: string | number) {}
}

class Dog extends Animal {
  feed(food: string) {
    console.log(food.toUpperCase());
  }
}

// 3. Nested any escape
//    sound mode is stricter when any leaks through structure
interface Payload {
  user: { name: string };
}

const payload: { user: any } = { user: "oops" };
const safePayload: Payload = payload; // Sound: nested any escape

// 4. Excess properties in function args via indirection
interface Config { host: string; port: number }
const cfg = { host: "localhost", port: 8080, debug: true };
function startServer(c: Config) {}
startServer(cfg); // Sound: excess 'debug'
`,
  errors: `// Intentional type errors — tsz should catch all of these

let x: string = 42;

function greet(name: string): string {
  return "Hello, " + name;
}

greet(123);

interface User {
  name: string;
  age: number;
}

const user: User = {
  name: "Alice",
  age: "thirty",
};
`,
};

let editor = null;
let jsEditor = null;
let dtsEditor = null;
let wasm = null;
let libFiles = {};
let checkTimeout = null;
let lspParser = null;
let lspParserState = null;

const statusEl = document.getElementById("playground-status");
const exampleSelect = document.getElementById("example-select");
const strictCheck = document.getElementById("strict-mode");
const soundCheck = document.getElementById("sound-mode");
const diagPanel = document.getElementById("diagnostics-panel");
const diagBadge = document.getElementById("diag-badge");

function getValidExampleKey(key) {
  if (!key || typeof key !== "string") return null;
  return Object.prototype.hasOwnProperty.call(EXAMPLES, key) ? key : null;
}

function getExampleFromUrl() {
  const params = new URLSearchParams(window.location.search);
  return getValidExampleKey(params.get("example"));
}

function setExampleInUrl(key) {
  const valid = getValidExampleKey(key);
  const url = new URL(window.location.href);
  if (valid) {
    url.searchParams.set("example", valid);
  } else {
    url.searchParams.delete("example");
  }
  window.history.replaceState({}, "", `${url.pathname}${url.search}${url.hash}`);
}

// ── Tab switching ──

document.querySelectorAll(".output-tab").forEach(tab => {
  tab.addEventListener("click", () => {
    document.querySelectorAll(".output-tab").forEach(t => t.classList.remove("active"));
    document.querySelectorAll(".output-panel").forEach(p => p.classList.remove("active"));
    tab.classList.add("active");
    const panel = document.getElementById(`${tab.dataset.panel}-panel`);
    if (panel) panel.classList.add("active");
  });
});

// ── Load Monaco ──

async function loadMonaco() {
  return new Promise((resolve, reject) => {
    const script = document.createElement("script");
    script.src = "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/min/vs/loader.js";
    script.onload = () => {
      window.require.config({
        paths: { vs: "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/min/vs" },
      });

      const isDark = window.matchMedia("(prefers-color-scheme: dark)").matches;

      window.require(["vs/editor/editor.main"], () => {
        editor = monaco.editor.create(document.getElementById("editor-container"), {
          value: EXAMPLES.hello,
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

        // Disable built-in TS diagnostics
        monaco.languages.typescript.typescriptDefaults.setDiagnosticsOptions({
          noSemanticValidation: true,
          noSyntaxValidation: true,
        });
        // Disable Monaco/TS worker language features so editor intelligence comes from TSZ.
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

        jsEditor = monaco.editor.create(document.getElementById("js-output-editor"), {
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

        const dtsContainer = document.getElementById("dts-output-editor");
        if (dtsContainer) {
          dtsEditor = monaco.editor.create(dtsContainer, {
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
        }

        editor.onDidChangeModelContent(() => scheduleCheck());

        registerTszProviders();

        // Track dark mode changes
        window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", e => {
          monaco.editor.setTheme(e.matches ? "vs-dark" : "vs");
        });

        resolve();
      });
    };
    script.onerror = reject;
    document.head.appendChild(script);
  });
}

function toLspPosition(position) {
  return {
    line: Math.max(0, position.lineNumber - 1),
    character: Math.max(0, position.column - 1),
  };
}

function toMonacoRange(range) {
  if (!range || !range.start || !range.end) return undefined;
  return new monaco.Range(
    range.start.line + 1,
    range.start.character + 1,
    range.end.line + 1,
    range.end.character + 1
  );
}

function getCurrentCompilerOptions() {
  return {
    strict: strictCheck.checked,
    soundMode: soundCheck.checked,
    module: 99,
  };
}

function disposeLspParser() {
  if (lspParser && typeof lspParser.free === "function") {
    lspParser.free();
  }
  lspParser = null;
  lspParserState = null;
}

function ensureLspParser() {
  if (!wasm || !editor) return null;

  const code = editor.getValue();
  const options = getCurrentCompilerOptions();
  const state = JSON.stringify({
    code,
    strict: options.strict,
    soundMode: options.soundMode,
    libCount: Object.keys(libFiles).length,
  });

  if (lspParser && lspParserState === state) {
    return lspParser;
  }

  disposeLspParser();

  try {
    const parser = new wasm.Parser("input.ts", code);
    parser.setCompilerOptions(JSON.stringify(options));
    for (const [name, content] of Object.entries(libFiles)) {
      parser.addLibFile(name, content);
    }
    parser.parseSourceFile();

    lspParser = parser;
    lspParserState = state;
    return lspParser;
  } catch (e) {
    console.warn("Failed to build TSZ parser for LSP features:", e);
    disposeLspParser();
    return null;
  }
}

function completionKindToMonaco(kind) {
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
    case "Variable":
    default:
      return monaco.languages.CompletionItemKind.Variable;
  }
}

function registerTszProviders() {
  monaco.languages.registerHoverProvider("typescript", {
    provideHover(model, position) {
      if (!editor || model !== editor.getModel()) return null;
      const parser = ensureLspParser();
      if (!parser) return null;

      try {
        const pos = toLspPosition(position);
        const hover = parser.getHoverAtPosition(pos.line, pos.character);
        if (!hover) return null;

        const contents = Array.isArray(hover.contents)
          ? hover.contents.map(c => ({ value: String(c) }))
          : [];
        return {
          range: toMonacoRange(hover.range),
          contents,
        };
      } catch (e) {
        console.warn("TSZ hover failed:", e);
        return null;
      }
    },
  });

  monaco.languages.registerCompletionItemProvider("typescript", {
    triggerCharacters: [".", "\"", "'", "/", "@", "<"],
    provideCompletionItems(model, position) {
      if (!editor || model !== editor.getModel()) return { suggestions: [] };
      const parser = ensureLspParser();
      if (!parser) return { suggestions: [] };

      try {
        const pos = toLspPosition(position);
        const result = parser.getCompletionsAtPosition(pos.line, pos.character);
        const entries = Array.isArray(result)
          ? result
          : result && Array.isArray(result.entries)
            ? result.entries
            : [];
        const suggestions = entries.map(entry => {
          const insertText = entry.insert_text || entry.label;
          return {
            label: entry.label,
            kind: completionKindToMonaco(entry.kind),
            detail: entry.detail || undefined,
            documentation: entry.documentation ? { value: String(entry.documentation) } : undefined,
            sortText: entry.sort_text || undefined,
            filterText: entry.label,
            insertText,
            insertTextRules: entry.is_snippet
              ? monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet
              : monaco.languages.CompletionItemInsertTextRule.None,
            range: undefined,
          };
        });
        return { suggestions };
      } catch (e) {
        console.warn("TSZ completions failed:", e);
        return { suggestions: [] };
      }
    },
  });

  monaco.languages.registerSignatureHelpProvider("typescript", {
    signatureHelpTriggerCharacters: ["(", ","],
    signatureHelpRetriggerCharacters: [","],
    provideSignatureHelp(model, position) {
      if (!editor || model !== editor.getModel()) {
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

        const signatures = help.signatures.map(sig => ({
          label: sig.label,
          documentation: sig.documentation ? { value: String(sig.documentation) } : undefined,
          parameters: Array.isArray(sig.parameters)
            ? sig.parameters.map(p => ({
                label: p.label,
                documentation: p.documentation ? { value: String(p.documentation) } : undefined,
              }))
            : [],
        }));

        return {
          value: {
            signatures,
            activeSignature: help.active_signature || 0,
            activeParameter: help.active_parameter || 0,
          },
          dispose: () => {},
        };
      } catch (e) {
        console.warn("TSZ signature help failed:", e);
        return {
          value: { signatures: [], activeSignature: 0, activeParameter: 0 },
          dispose: () => {},
        };
      }
    },
  });
}

// ── Load WASM ──

async function loadWasm() {
  try {
    const module = await import("/wasm/tsz_wasm.js");
    await module.default();
    wasm = module;
    return true;
  } catch (e) {
    console.warn("WASM load failed:", e);
    return false;
  }
}

// ── Load lib files ──

async function loadLibFiles() {
  const libs = [
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

  const results = await Promise.allSettled(libs.map(async (name) => {
    const resp = await fetch(`/lib/${name}`);
    if (resp.ok) {
      libFiles[name] = await resp.text();
    }
  }));

  console.log(`Loaded ${Object.keys(libFiles).length}/${libs.length} lib files`);
}

// ── Check code ──

function scheduleCheck() {
  if (checkTimeout) clearTimeout(checkTimeout);
  checkTimeout = setTimeout(runCheck, 250);
}

function runCheck() {
  if (!wasm || !editor) return;

  const code = editor.getValue();
  const options = getCurrentCompilerOptions();

  statusEl.textContent = "checking...";
  statusEl.className = "status-checking";

  const start = performance.now();

  try {
    ensureLspParser();

    const program = new wasm.TsProgram();
    program.setCompilerOptions(JSON.stringify(options));

    for (const [name, content] of Object.entries(libFiles)) {
      program.addLibFile(name, content);
    }

    program.addSourceFile("input.ts", code);

    const diagJson = program.getPreEmitDiagnosticsJson();
    const diagnostics = JSON.parse(diagJson);
    const elapsed = (performance.now() - start).toFixed(0);

    // Filter out "Cannot find global type" noise if libs partially loaded
    const userDiags = diagnostics.filter(d => {
      // Keep all diagnostics from user file that aren't global-type complaints at pos 0
      if (d.code === 2318 && d.start === 0) return false;
      return true;
    });

    const errCount = userDiags.filter(d => d.category === 1).length;
    const warnCount = userDiags.filter(d => d.category === 0).length;

    // Status
    if (errCount === 0 && warnCount === 0) {
      statusEl.innerHTML = `<span class="time">${elapsed}ms</span>`;
      statusEl.className = "status-ready";
    } else {
      const parts = [];
      if (errCount) parts.push(`<span class="err-count">${errCount} error${errCount > 1 ? "s" : ""}</span>`);
      if (warnCount) parts.push(`${warnCount} warning${warnCount > 1 ? "s" : ""}`);
      statusEl.innerHTML = `${parts.join(", ")} <span class="time">${elapsed}ms</span>`;
      statusEl.className = "status-count";
    }

    // Badge
    diagBadge.style.display = "";
    diagBadge.textContent = userDiags.length;
    diagBadge.className = userDiags.length === 0 ? "tab-badge zero" : "tab-badge";

    // Monaco markers
    const model = editor.getModel();
    const markers = userDiags.map(d => {
      const startPos = model.getPositionAt(d.start);
      const endPos = model.getPositionAt(d.start + (d.length || 1));
      return {
        severity: d.category === 1
          ? monaco.MarkerSeverity.Error
          : d.category === 0
            ? monaco.MarkerSeverity.Warning
            : monaco.MarkerSeverity.Info,
        message: d.messageText,
        startLineNumber: startPos.lineNumber,
        startColumn: startPos.column,
        endLineNumber: endPos.lineNumber,
        endColumn: endPos.column,
        code: `TS${d.code}`,
      };
    });
    monaco.editor.setModelMarkers(model, "tsz", markers);

    renderDiagnostics(userDiags, model);

    // Emit JS output
    try {
      const jsOutput = program.emitFile("input.ts");
      jsEditor.setValue(jsOutput || "// (empty output)");
    } catch (e) {
      jsEditor.setValue(`// Emit error: ${e.message}`);
    }

    // Emit .d.ts output
    if (dtsEditor) {
      try {
        const transpileResultJson = wasm.transpileModule(code, JSON.stringify({ declaration: true }));
        const transpileResult = JSON.parse(transpileResultJson || "{}");
        const dtsOutput = transpileResult && typeof transpileResult.declarationText === "string"
          ? transpileResult.declarationText
          : transpileResult && typeof transpileResult.declaration_text === "string"
            ? transpileResult.declaration_text
          : "";
        dtsEditor.setValue(dtsOutput || "// (no declaration output)");
      } catch (e) {
        dtsEditor.setValue(`// DTS emit error: ${e.message}`);
      }
    }

    program.dispose();
  } catch (e) {
    statusEl.textContent = `error: ${e.message}`;
    statusEl.className = "status-error";
    console.error("Check failed:", e);
  }
}

function renderDiagnostics(diagnostics, model) {
  if (diagnostics.length === 0) {
    diagPanel.innerHTML = `<div class="diagnostics-ok">No errors</div>`;
    return;
  }

  diagPanel.innerHTML = diagnostics.map(d => {
    const pos = model.getPositionAt(d.start);
    const cat = d.category === 1 ? "error" : d.category === 0 ? "warning" : "suggestion";
    return `<div class="diag-item" data-start="${d.start}">
      <div class="diag-header">
        <span class="diag-code ${cat}">TS${d.code}</span>
        <span class="diag-message">${escapeHtml(d.messageText)}</span>
      </div>
      <div class="diag-location">input.ts:${pos.lineNumber}:${pos.column}</div>
    </div>`;
  }).join("");

  diagPanel.querySelectorAll(".diag-item").forEach(item => {
    item.addEventListener("click", () => {
      const start = Number(item.dataset.start);
      const pos = model.getPositionAt(start);
      editor.revealLineInCenter(pos.lineNumber);
      editor.setPosition(pos);
      editor.focus();
    });
  });
}

function escapeHtml(s) {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

// ── Example selector ──

exampleSelect.addEventListener("change", () => {
  const val = exampleSelect.value;
  const code = EXAMPLES[val];
  if (code && editor) {
    setExampleInUrl(val);
    // Auto-toggle sound mode checkbox for the sound_mode example
    if (val === "sound_mode") {
      soundCheck.checked = true;
    } else {
      soundCheck.checked = false;
    }
    editor.setValue(code);
    disposeLspParser();
  }
});

strictCheck.addEventListener("change", () => {
  disposeLspParser();
  scheduleCheck();
});
soundCheck.addEventListener("change", () => {
  disposeLspParser();
  scheduleCheck();
});

// ── Init ──

async function init() {
  try {
    statusEl.textContent = "loading editor...";
    await loadMonaco();

    statusEl.textContent = "loading WASM...";
    const [wasmOk] = await Promise.all([loadWasm(), loadLibFiles()]);

    if (!wasmOk) {
      document.getElementById("playground-root").style.display = "none";
      document.getElementById("playground-fallback").style.display = "block";
      return;
    }

    statusEl.textContent = "ready";
    statusEl.className = "status-ready";

    const urlExample = getExampleFromUrl();
    if (urlExample) {
      exampleSelect.value = urlExample;
      editor.setValue(EXAMPLES[urlExample]);
      if (urlExample === "sound_mode") {
        soundCheck.checked = true;
      } else {
        soundCheck.checked = false;
      }
    } else {
      setExampleInUrl(exampleSelect.value);
    }

    runCheck();
  } catch (e) {
    statusEl.textContent = `failed: ${e.message}`;
    statusEl.className = "status-error";
    console.error("Init failed:", e);
  }
}

init();
