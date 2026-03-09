/**
 * Replace code blocks on the Sound Mode page with read-only Monaco editors.
 */

(function () {
  const langMap = {
    "language-typescript": "typescript",
    "language-bash": "shell",
    "language-json": "json",
  };

  const blocks = document.querySelectorAll(
    Object.keys(langMap).map(c => `pre > code.${c}`).join(", ")
  );
  if (!blocks.length) return;

  const isDark = window.matchMedia("(prefers-color-scheme: dark)").matches;

  const script = document.createElement("script");
  script.src = "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/min/vs/loader.js";
  script.onload = () => {
    window.require.config({
      paths: { vs: "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/min/vs" },
    });

    window.require(["vs/editor/editor.main"], () => {
      monaco.languages.typescript.typescriptDefaults.setDiagnosticsOptions({
        noSemanticValidation: true,
        noSyntaxValidation: true,
      });

      for (const code of blocks) {
        const pre = code.parentElement;
        const text = code.textContent;

        // Detect language
        let lang = "typescript";
        for (const [cls, monacoLang] of Object.entries(langMap)) {
          if (code.classList.contains(cls)) { lang = monacoLang; break; }
        }

        const lineCount = text.split("\n").length;
        const height = Math.min(Math.max(lineCount * 20 + 16, 60), 500);

        const container = document.createElement("div");
        container.className = "monaco-code-block";
        container.style.height = `${height}px`;
        pre.replaceWith(container);

        monaco.editor.create(container, {
          value: text,
          language: lang,
          theme: isDark ? "vs-dark" : "vs",
          readOnly: true,
          minimap: { enabled: false },
          fontSize: 13,
          fontFamily: "'SF Mono', 'Cascadia Code', 'JetBrains Mono', 'Fira Code', Menlo, Consolas, monospace",
          lineNumbers: "off",
          scrollBeyondLastLine: false,
          automaticLayout: true,
          renderLineHighlight: "none",
          scrollbar: { vertical: "hidden", horizontal: "auto", handleMouseWheel: false },
          overviewRulerLanes: 0,
          hideCursorInOverviewRuler: true,
          overviewRulerBorder: false,
          guides: { indentation: false },
          padding: { top: 8, bottom: 8 },
          domReadOnly: true,
          contextmenu: false,
        });
      }

      window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", e => {
        monaco.editor.setTheme(e.matches ? "vs-dark" : "vs");
      });
    });
  };
  document.head.appendChild(script);
})();
