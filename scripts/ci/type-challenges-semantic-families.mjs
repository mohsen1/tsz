import fs from "node:fs";
import path from "node:path";

const SEMANTIC_FAMILY_RULES = [
  {
    id: "template literal inference",
    test: (text) =>
      /`[^`]*\$\{/.test(text) || /\binfer\s+\w+\s+extends\s+string\b/.test(text),
  },
  {
    id: "mapped/key-remapped types",
    test: (text) =>
      /\[\s*(?:readonly\s+)?[A-Za-z_$][\w$]*\s+in\s+/.test(text) ||
      /\bas\s+keyof\b/.test(text),
  },
  {
    id: "indexed access",
    test: (text) =>
      /\bkeyof\b/.test(text) || /[A-Za-z_$][\w$]*(?:<[^>\n]+>)?\s*\[[^\]\n]+\]/.test(text),
  },
  {
    id: "tuple recursion",
    test: (text) =>
      /\[\s*(?:\.\.\.|infer\b)/.test(text) ||
      /\binfer\s+\w+\s*,/.test(text) ||
      /\.\.\.\s*[A-Za-z_$][\w$]*/.test(text),
  },
  {
    id: "recursive conditionals",
    test: (text) =>
      /\bextends\b/.test(text) &&
      (/\binfer\b/.test(text) || /\b[A-Za-z_$][\w$]*<[^>]+>/.test(text)),
  },
  {
    id: "distributive conditionals",
    test: (text) => /\b[A-Za-z_$][\w$]*\s+extends\s+/.test(text) && /\?/.test(text),
  },
  {
    id: "inference cache/session behavior",
    test: (text) => /\binfer\b/.test(text) || /<[^>]*\bextends\b/.test(text),
  },
];

export function normalizePath(file) {
  return String(file || "").split(/[\\/]+/).join("/");
}

export function semanticFamiliesForText(text) {
  const families = SEMANTIC_FAMILY_RULES.filter((rule) => rule.test(text)).map(
    (rule) => rule.id,
  );
  return families.length > 0 ? families : ["unclassified"];
}

export function semanticFamiliesForFile(file, root, sourceCache = new Map()) {
  if (!file || !root) {
    return ["unknown"];
  }

  const normalized = normalizePath(file).replace(/^\.\//, "");
  const resolvedRoot = path.resolve(root);
  const candidatePath = path.resolve(resolvedRoot, normalized);
  if (
    candidatePath === resolvedRoot ||
    !candidatePath.startsWith(`${resolvedRoot}${path.sep}`) ||
    !fs.existsSync(candidatePath)
  ) {
    return ["unknown"];
  }
  if (!fs.statSync(candidatePath).isFile()) {
    return ["unknown"];
  }

  let source = sourceCache.get(candidatePath);
  if (source === undefined) {
    source = fs.readFileSync(candidatePath, "utf8");
    sourceCache.set(candidatePath, source);
  }

  return semanticFamiliesForText(source);
}
