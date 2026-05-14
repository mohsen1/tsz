import { getExampleByKey } from "../playground-app/examples.js";

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

const PAGE_EXAMPLE_KEYS = [
  "sound_mode",
  "sound_mode_argument",
  "sound_mode_array",
];

function isDarkTheme() {
  return window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches;
}

function loadMonaco() {
  if (window.monaco?.editor) {
    return Promise.resolve(window.monaco);
  }

  if (window.__tszMonacoPromise) {
    return window.__tszMonacoPromise;
  }

  window.__tszMonacoPromise = new Promise((resolve, reject) => {
    const loadEditor = () => {
      window.require.config({
        paths: { vs: "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/min/vs" },
      });
      window.require(["vs/editor/editor.main"], () => resolve(window.monaco), reject);
    };

    if (window.require?.config) {
      loadEditor();
      return;
    }

    const script = document.createElement("script");
    script.src = "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/min/vs/loader.js";
    script.onload = loadEditor;
    script.onerror = reject;
    document.head.appendChild(script);
  });

  return window.__tszMonacoPromise;
}

async function loadWasm() {
  if (window.__tszSoundModeWasm) {
    return window.__tszSoundModeWasm;
  }

  const module = await import("/wasm/tsz_wasm.js");
  await module.default();
  window.__tszSoundModeWasm = module;
  return module;
}

async function loadLibFiles() {
  if (window.__tszSoundModeLibFiles) {
    return window.__tszSoundModeLibFiles;
  }

  const libs = {};
  await Promise.all(
    LIB_FILES.map(async name => {
      const response = await fetch(`/lib/${name}`);
      if (response.ok) {
        libs[name] = await response.text();
      }
    })
  );
  window.__tszSoundModeLibFiles = libs;
  return libs;
}

function createCheckProgram(wasm, libFiles, code, options) {
  if (!wasm.WasmProgram) {
    throw new Error("WasmProgram is required for Sound Mode examples");
  }

  const program = new wasm.WasmProgram();
  program.setCompilerOptions(JSON.stringify(options));
  for (const [name, content] of Object.entries(libFiles)) {
    program.addLibFile(name, content);
  }
  if (typeof program.addFile === "function") {
    program.addFile("input.ts", code);
  } else {
    program.addSourceFile("input.ts", code);
  }
  return program;
}

function normalizeDiagnostics(program) {
  if (typeof program.getPreEmitDiagnosticsJson === "function") {
    return JSON.parse(program.getPreEmitDiagnosticsJson() || "[]");
  }

  if (typeof program.checkAll !== "function") {
    return [];
  }

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

  return [
    ...parseDiagnostics.map(diagnostic => ({
      start: diagnostic.start ?? 0,
      length: diagnostic.length ?? 0,
      messageText: diagnostic.messageText || diagnostic.message || "",
      category: 1,
      code: diagnostic.code,
    })),
    ...checkDiagnostics.map(diagnostic => ({
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
    })),
  ];
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

function withSoundDiagnosticDisplayCodes(soundDiagnostics, baselineDiagnostics, forcedDisplayCode) {
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

function visibleDiagnostics(diagnostics) {
  return diagnostics.filter(diagnostic => !(diagnostic.code === 2318 && diagnostic.start === 0));
}

function runDiagnostics(wasm, libFiles, example, soundMode) {
  const options = { strict: true, soundMode, module: 99 };
  const program = createCheckProgram(wasm, libFiles, example.source, options);
  let diagnostics = visibleDiagnostics(normalizeDiagnostics(program));
  if (typeof program.dispose === "function") {
    program.dispose();
  }

  if (soundMode) {
    const baselineProgram = createCheckProgram(wasm, libFiles, example.source, {
      ...options,
      soundMode: false,
    });
    const baselineDiagnostics = visibleDiagnostics(normalizeDiagnostics(baselineProgram));
    if (typeof baselineProgram.dispose === "function") {
      baselineProgram.dispose();
    }
    diagnostics = withSoundDiagnosticDisplayCodes(
      diagnostics,
      baselineDiagnostics,
      example.soundDiagnosticCode
    );
  }

  return diagnostics;
}

function editorHeightForSource(source) {
  const lineCount = Math.max(source.replace(/\s+$/, "").split("\n").length, 1);
  return lineCount * 20 + 22;
}

function languageFromCodeBlock(codeBlock) {
  const match = /language-([\w+#-]+)/.exec(codeBlock.className || "");
  const language = match ? match[1].toLowerCase() : "";
  if (language === "ts" || language === "typescript" || language === "tsx") {
    return "typescript";
  }
  if (language === "js" || language === "javascript") {
    return "javascript";
  }
  if (language === "json") {
    return "json";
  }
  if (language === "bash" || language === "sh" || language === "shell") {
    return "shell";
  }
  return "typescript";
}

function mountStaticCodeBlock(pre, codeBlock, monaco) {
  const source = (codeBlock.textContent || "").replace(/\s+$/, "");
  const host = document.createElement("div");
  host.className = "sound-mode-static-editor";
  host.style.height = `${editorHeightForSource(source)}px`;
  pre.replaceWith(host);

  return monaco.editor.create(host, {
    value: source,
    language: languageFromCodeBlock(codeBlock),
    theme: isDarkTheme() ? "vs-dark" : "vs",
    readOnly: true,
    domReadOnly: true,
    minimap: { enabled: false },
    fontSize: 14,
    fontFamily: "'SF Mono', 'Cascadia Code', 'JetBrains Mono', 'Fira Code', Menlo, Consolas, monospace",
    lineNumbers: "on",
    scrollBeyondLastLine: false,
    automaticLayout: true,
    tabSize: 2,
    renderLineHighlight: "none",
    contextmenu: false,
    folding: false,
    glyphMargin: false,
    overviewRulerLanes: 0,
    scrollbar: {
      vertical: "hidden",
      horizontal: "auto",
      alwaysConsumeMouseWheel: false,
    },
    quickSuggestions: false,
    suggestOnTriggerCharacters: false,
    padding: { top: 10, bottom: 8 },
  });
}

function mountExample(root, example, monaco, wasm, libFiles) {
  root.textContent = "";
  root.classList.add("sound-mode-inline-playground");

  const header = document.createElement("div");
  header.className = "sound-mode-inline-header";

  const heading = document.createElement("div");
  heading.className = "sound-mode-inline-title";

  const title = document.createElement("strong");
  title.textContent = example.title.replace(/^Sound Mode:\s*/, "");

  const description = document.createElement("span");
  description.textContent = example.description;

  heading.append(title, description);

  const label = document.createElement("label");
  label.className = "sound-mode-inline-check";
  const checkbox = document.createElement("input");
  checkbox.type = "checkbox";
  checkbox.checked = true;
  const checkboxText = document.createElement("span");
  checkboxText.textContent = "sound";
  label.append(checkbox, checkboxText);

  const controls = document.createElement("div");
  controls.className = "sound-mode-inline-controls";
  controls.append(label);

  header.append(heading, controls);

  const editorFrame = document.createElement("div");
  editorFrame.className = "sound-mode-inline-editor-frame";
  const editorHost = document.createElement("div");
  editorHost.className = "sound-mode-inline-editor";
  editorHost.style.height = `${editorHeightForSource(example.source)}px`;

  editorFrame.append(editorHost);

  root.append(header, editorFrame);

  const editor = monaco.editor.create(editorHost, {
    value: example.source.replace(/\s+$/, ""),
    language: "typescript",
    theme: isDarkTheme() ? "vs-dark" : "vs",
    readOnly: true,
    domReadOnly: true,
    minimap: { enabled: false },
    fontSize: 14,
    fontFamily: "'SF Mono', 'Cascadia Code', 'JetBrains Mono', 'Fira Code', Menlo, Consolas, monospace",
    lineNumbers: "on",
    scrollBeyondLastLine: false,
    automaticLayout: true,
    tabSize: 2,
    renderLineHighlight: "none",
    contextmenu: false,
    folding: false,
    glyphMargin: false,
    overviewRulerLanes: 0,
    scrollbar: {
      vertical: "hidden",
      horizontal: "auto",
      alwaysConsumeMouseWheel: false,
    },
    quickSuggestions: false,
    suggestOnTriggerCharacters: false,
    padding: { top: 10, bottom: 8 },
  });
  const diagnosticDecorations = typeof editor.createDecorationsCollection === "function"
    ? editor.createDecorationsCollection([])
    : null;
  let diagnosticDecorationIds = [];

  function check() {
    root.dataset.status = "checking";
    try {
      const diagnostics = runDiagnostics(wasm, libFiles, example, checkbox.checked);
      const model = editor.getModel();
      const markers = diagnostics.map(diagnostic => {
        const start = model.getPositionAt(diagnostic.start);
        const end = model.getPositionAt(diagnostic.start + (diagnostic.length || 1));
        return {
          severity: diagnostic.category === 1
            ? monaco.MarkerSeverity.Error
            : diagnostic.category === 0
              ? monaco.MarkerSeverity.Warning
              : monaco.MarkerSeverity.Info,
          message: diagnostic.messageText,
          startLineNumber: start.lineNumber,
          startColumn: start.column,
          endLineNumber: end.lineNumber,
          endColumn: end.column,
          code: formatDiagnosticCode(diagnostic),
        };
      });
      monaco.editor.setModelMarkers(model, "tsz", markers);
      const decorations = diagnostics.map(diagnostic => {
        const start = model.getPositionAt(diagnostic.start);
        const end = model.getPositionAt(diagnostic.start + (diagnostic.length || 1));
        return {
          range: new monaco.Range(start.lineNumber, start.column, end.lineNumber, end.column),
          options: {
            inlineClassName: "sound-mode-inline-error-range",
            hoverMessage: {
              value: `**${formatDiagnosticCode(diagnostic)}** ${diagnostic.messageText}`,
            },
          },
        };
      });
      if (diagnosticDecorations) {
        diagnosticDecorations.set(decorations);
      } else {
        diagnosticDecorationIds = editor.deltaDecorations(diagnosticDecorationIds, decorations);
      }
      root.dataset.status = diagnostics.length === 0 ? "ok" : "error";
      root.dataset.diagnosticCount = String(diagnostics.length);
    } catch (error) {
      monaco.editor.setModelMarkers(editor.getModel(), "tsz", []);
      if (diagnosticDecorations) {
        diagnosticDecorations.clear();
      } else {
        diagnosticDecorationIds = editor.deltaDecorations(diagnosticDecorationIds, []);
      }
      root.dataset.status = "failed";
    }
  }

  checkbox.addEventListener("change", check);
  check();

  return editor;
}

async function main() {
  const roots = Array.from(document.querySelectorAll("[data-sound-mode-example]"));
  const staticCodeBlocks = Array.from(document.querySelectorAll("main.sound-mode pre > code"))
    .map(codeBlock => ({ pre: codeBlock.parentElement, codeBlock }))
    .filter(({ pre }) => pre);

  if (roots.length === 0 && staticCodeBlocks.length === 0) {
    return;
  }

  roots.forEach(root => {
    root.classList.add("sound-mode-inline-playground");
    root.textContent = "Loading example...";
  });

  const needsWasm = roots.length > 0;
  const [monaco, wasm, libFiles] = await Promise.all([
    loadMonaco(),
    needsWasm ? loadWasm() : Promise.resolve(null),
    needsWasm ? loadLibFiles() : Promise.resolve(null),
  ]);

  monaco.languages.typescript.typescriptDefaults.setDiagnosticsOptions({
    noSemanticValidation: true,
    noSyntaxValidation: true,
  });

  const mountedEditors = [];
  staticCodeBlocks.forEach(({ pre, codeBlock }) => {
    mountedEditors.push(mountStaticCodeBlock(pre, codeBlock, monaco));
  });

  roots.forEach(root => {
    const key = root.getAttribute("data-sound-mode-example");
    const example = getExampleByKey(key);
    if (!example || !PAGE_EXAMPLE_KEYS.includes(example.key)) {
      root.textContent = "Sound Mode example not found.";
      return;
    }
    mountedEditors.push(mountExample(root, example, monaco, wasm, libFiles));
  });

  const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
  mediaQuery.addEventListener("change", event => {
    monaco.editor.setTheme(event.matches ? "vs-dark" : "vs");
  });

  window.addEventListener("pagehide", () => {
    mountedEditors.forEach(editor => editor.dispose());
  }, { once: true });
}

main().catch(error => {
  document.querySelectorAll("[data-sound-mode-example]").forEach(root => {
    root.classList.add("sound-mode-inline-playground");
    root.textContent = `Could not load Sound Mode example: ${error.message}`;
  });
});
