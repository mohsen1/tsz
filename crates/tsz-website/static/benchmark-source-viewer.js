(function () {
  const viewers = Array.from(document.querySelectorAll(".benchmark-source-viewer"));
  if (!viewers.length) return;

  function parseFiles(viewer) {
    try {
      const files = JSON.parse(viewer.getAttribute("data-files") || "[]");
      return Array.isArray(files) ? files.filter((file) => file?.name && file?.source) : [];
    } catch {
      return [];
    }
  }

  function escapeText(value) {
    return String(value)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");
  }

  function renderFallback(viewer, files, activeIndex) {
    const editor = viewer.querySelector(".benchmark-source-editor");
    if (!editor || !files[activeIndex]) return;
    editor.innerHTML = `<pre class="bench-snippet"><code>${escapeText(files[activeIndex].source)}</code></pre>`;
  }

  function setupTabs(viewer, files, onSelect) {
    const tabs = viewer.querySelector(".benchmark-source-tabs");
    if (!tabs) return;

    tabs.innerHTML = "";
    files.forEach((file, index) => {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = file.name;
      button.setAttribute("role", "tab");
      button.setAttribute("aria-selected", index === 0 ? "true" : "false");
      button.addEventListener("click", () => {
        tabs.querySelectorAll("button").forEach((tab, tabIndex) => {
          tab.setAttribute("aria-selected", tabIndex === index ? "true" : "false");
        });
        onSelect(index);
      });
      tabs.append(button);
    });
  }

  function loadMonaco() {
    if (window.monaco?.editor) {
      return Promise.resolve(window.monaco);
    }
    if (window.__tszBenchmarkMonacoPromise) {
      return window.__tszBenchmarkMonacoPromise;
    }

    window.__tszBenchmarkMonacoPromise = new Promise((resolve, reject) => {
      function configureAndLoad() {
        if (!window.require?.config) {
          reject(new Error("Monaco loader did not initialize"));
          return;
        }
        window.require.config({
          paths: {
            vs: "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/min/vs",
          },
        });
        window.require(["vs/editor/editor.main"], resolve, reject);
      }

      if (window.require?.config) {
        configureAndLoad();
        return;
      }

      const script = document.createElement("script");
      script.src = "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/min/vs/loader.js";
      script.onload = configureAndLoad;
      script.onerror = () => reject(new Error("Failed to load Monaco"));
      document.head.append(script);
    });

    return window.__tszBenchmarkMonacoPromise;
  }

  function mountViewer(viewer, files, monaco) {
    const editorContainer = viewer.querySelector(".benchmark-source-editor");
    if (!editorContainer) return;

    editorContainer.innerHTML = "";
    const models = files.map((file) => {
      const uri = monaco.Uri.parse(`benchmark:///${file.name}`);
      return monaco.editor.createModel(file.source, file.language || "typescript", uri);
    });
    const editor = monaco.editor.create(editorContainer, {
      model: models[0],
      readOnly: true,
      minimap: { enabled: false },
      automaticLayout: true,
      scrollBeyondLastLine: false,
      lineNumbers: "off",
      folding: false,
      renderLineHighlight: "none",
      overviewRulerLanes: 0,
      wordWrap: "on",
      fontSize: 14,
      lineHeight: 22,
    });

    setupTabs(viewer, files, (index) => {
      editor.setModel(models[index]);
      editor.layout();
    });

    const media = window.matchMedia("(prefers-color-scheme: dark)");
    monaco.editor.setTheme(media.matches ? "vs-dark" : "vs");
    media.addEventListener?.("change", (event) => {
      monaco.editor.setTheme(event.matches ? "vs-dark" : "vs");
    });
  }

  for (const viewer of viewers) {
    const files = parseFiles(viewer);
    if (!files.length) continue;

    setupTabs(viewer, files, (index) => renderFallback(viewer, files, index));
    renderFallback(viewer, files, 0);

    loadMonaco()
      .then((monaco) => mountViewer(viewer, files, monaco))
      .catch(() => {});
  }
})();
