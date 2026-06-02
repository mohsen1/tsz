const path = require("node:path");
const { createRequire } = require("node:module");

const configPath = path.resolve(process.argv[1]);
const projectRoot = path.dirname(configPath);
const projectRequire = createRequire(path.join(projectRoot, "package.json"));
const ts = projectRequire("typescript");

function flattenMessage(messageText) {
  return ts.flattenDiagnosticMessageText(messageText, "\n");
}

function categoryName(category) {
  switch (category) {
    case ts.DiagnosticCategory.Warning:
      return "warning";
    case ts.DiagnosticCategory.Error:
      return "error";
    case ts.DiagnosticCategory.Suggestion:
      return "suggestion";
    case ts.DiagnosticCategory.Message:
      return "message";
    default:
      return "message";
  }
}

function toComparableDiagnostic(diagnostic) {
  let file = null;
  let line = null;
  let column = null;
  if (diagnostic.file) {
    file = diagnostic.file.fileName;
    if (typeof diagnostic.start === "number") {
      const pos = diagnostic.file.getLineAndCharacterOfPosition(diagnostic.start);
      line = pos.line + 1;
      column = pos.character + 1;
    }
  }
  return {
    file,
    start: typeof diagnostic.start === "number" ? diagnostic.start : null,
    length: typeof diagnostic.length === "number" ? diagnostic.length : null,
    line,
    column,
    code: diagnostic.code,
    category: categoryName(diagnostic.category),
    message: flattenMessage(diagnostic.messageText),
  };
}

const config = ts.readConfigFile(configPath, ts.sys.readFile);
let diagnostics = [];
if (config.error) {
  diagnostics.push(config.error);
} else {
  const parsed = ts.parseJsonConfigFileContent(
    config.config,
    ts.sys,
    projectRoot,
    { noEmit: true },
    configPath,
  );
  diagnostics.push(...parsed.errors);
  if (parsed.errors.length === 0) {
    const program = ts.createProgram({
      rootNames: parsed.fileNames,
      options: { ...parsed.options, noEmit: true },
      projectReferences: parsed.projectReferences,
    });
    diagnostics.push(...ts.getPreEmitDiagnostics(program));
  }
}

process.stdout.write(JSON.stringify({
  typescript_version: ts.version,
  diagnostics: diagnostics.map(toComparableDiagnostic),
}));
