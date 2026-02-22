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
let wasm = null;
let libFiles = {};
let checkTimeout = null;

const statusEl = document.getElementById("playground-status");
const exampleSelect = document.getElementById("example-select");
const strictCheck = document.getElementById("strict-mode");
const diagPanel = document.getElementById("diagnostics-panel");
const diagBadge = document.getElementById("diag-badge");

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

        editor.onDidChangeModelContent(() => scheduleCheck());

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
  const strict = strictCheck.checked;

  statusEl.textContent = "checking...";
  statusEl.className = "status-checking";

  const start = performance.now();

  try {
    const program = new wasm.TsProgram();
    program.setCompilerOptions(JSON.stringify({ strict }));

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
  const code = EXAMPLES[exampleSelect.value];
  if (code && editor) editor.setValue(code);
});

strictCheck.addEventListener("change", () => scheduleCheck());

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

    runCheck();
  } catch (e) {
    statusEl.textContent = `failed: ${e.message}`;
    statusEl.className = "status-error";
    console.error("Init failed:", e);
  }
}

init();
