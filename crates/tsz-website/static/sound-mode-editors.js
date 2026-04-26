(function () {
  const sectionAnchor = document.getElementById("sound-mode-catch-examples");
  if (!sectionAnchor) {
    return;
  }

  const sectionHeading = sectionAnchor.closest("h2");
  if (!sectionHeading) {
    return;
  }

  const sectionBlocks = [];
  let node = sectionHeading.nextElementSibling;
  while (node && node.tagName !== "H1" && node.tagName !== "H2") {
    if (node.tagName === "PRE") {
      sectionBlocks.push(node);
    }
    node = node.nextElementSibling;
  }

  const containers = [];
  const blocks = Array.from(sectionBlocks).flatMap((pre) => Array.from(pre.querySelectorAll("code")));

  if (blocks.length === 0) {
    return;
  }

  function ensureContainerStyle() {
    if (document.querySelector("style[data-sound-mode-editors]")) return;
    const style = document.createElement("style");
    style.setAttribute("data-sound-mode-editors", "1");
    style.textContent = `
      .sound-mode-monaco-editor {
        width: 100%;
        border: 0;
        margin: 0 0 1rem 0;
      }
    `;
    document.head.appendChild(style);
  }

  function detectLanguage(node) {
    const match = /language-([\w+#-]+)/.exec(node.className || "");
    const lang = match ? match[1].toLowerCase() : "";
    if (lang === "ts" || lang === "typescript" || lang === "tsx") {
      return "typescript";
    }
    if (lang === "js" || lang === "javascript") {
      return "javascript";
    }
    return "typescript";
  }

  function replaceWithEditors() {
    blocks.forEach((codeBlock) => {
      const pre = codeBlock.parentElement;
      if (!pre || !pre.parentElement) return;

      const container = document.createElement("div");
      container.className = "sound-mode-monaco-editor";
      const lineHeight = 22;
      const lineCount = Math.max(codeTextLineCount(codeBlock.textContent || ""), 1);
      container.style.height = `${lineCount * lineHeight}px`;
      pre.parentElement.replaceChild(container, pre);

      const codeText = (codeBlock.textContent || "").replace(/\s+$/, "");
      const language = detectLanguage(codeBlock);
      containers.push({ container, codeText, language });
    });
  }

  function codeTextLineCount(text) {
    return text
      .replace(/\s+$/, "")
      .split("\n")
      .length;
  }

  function isDarkTheme() {
    return window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches;
  }

  function mountEditors(monaco) {
    ensureContainerStyle();
    containers.forEach(({ container, codeText, language }) => {
      monaco.editor.create(container, {
        value: codeText,
        language,
        readOnly: true,
        minimap: { enabled: false },
        fontSize: 14,
        lineNumbers: "on",
        scrollBeyondLastLine: false,
        scrollbar: {
          vertical: "auto",
          horizontal: "auto",
        },
        overviewRulerLanes: 0,
        folding: false,
        automaticLayout: true,
        contextmenu: false,
        renderLineHighlight: "none",
        suggestOnTriggerCharacters: false,
        quickSuggestions: false,
        glyphMargin: false,
        theme: isDarkTheme() ? "vs-dark" : "vs",
      });
    });
  }

  function loadMonacoAndInit() {
    if (window.monaco && window.monaco.editor) {
      mountEditors(window.monaco);
      return;
    }

    if (!window.require || typeof window.require.config !== "function") {
      window.__tszSoundModeEditorError = "Monaco loader did not initialize";
      return;
    }

    if (window.__tszSoundModeMonacoPromise) {
      window.__tszSoundModeMonacoPromise
        .then((monaco) => mountEditors(monaco))
        .catch(() => {});
      return;
    }

    window.require.config({
      paths: {
        vs: "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/min/vs",
      },
    });

    window.__tszSoundModeMonacoPromise = new Promise((resolve, reject) => {
      window.require(
        ["vs/editor/editor.main"],
        (monaco) => resolve(monaco),
        (error) => reject(error),
      );
    });

    window.__tszSoundModeMonacoPromise
      .then((monaco) => mountEditors(monaco))
      .catch(() => {});
  }

  replaceWithEditors();
  loadMonacoAndInit();
})();
