import fs from "node:fs";
import path from "node:path";
import { execSync } from "node:child_process";

const FILE_GLOBS = [
  "crates/*/src/*.rs",
  "crates/*/build.rs",
  "crates/tsz-website/rust/*.rs",
  "crates/*/tests/*.rs",
];

const CC = {
  NL: 0x0a,
  HASH: 0x23,
  LBRACK: 0x5b,
  RBRACK: 0x5d,
  LBRACE: 0x7b,
  RBRACE: 0x7d,
  LPAREN: 0x28,
  RPAREN: 0x29,
  SEMI: 0x3b,
  SLASH: 0x2f,
  STAR: 0x2a,
  BSLASH: 0x5c,
  DQUOTE: 0x22,
  SQUOTE: 0x27,
  LOWER_B: 0x62,
  LOWER_R: 0x72,
};

export function fmt(n) {
  return Number(n).toLocaleString("en-US");
}

export function isTestFile(relPath) {
  const p = relPath.replace(/\\/g, "/");
  if (/(?:^|\/)tests\//.test(p)) return true;
  if (/(?:^|\/)benches\//.test(p)) return true;
  const base = p.slice(p.lastIndexOf("/") + 1);
  if (base === "tests.rs") return true;
  if (/_tests?\.rs$/.test(base)) return true;
  if (base.startsWith("test_")) return true;
  return false;
}

function isIdentCode(cc) {
  return (
    (cc >= 0x30 && cc <= 0x39) ||
    (cc >= 0x41 && cc <= 0x5a) ||
    (cc >= 0x61 && cc <= 0x7a) ||
    cc === 0x5f
  );
}

function isWsCode(cc) {
  return cc === 0x20 || cc === 0x09 || cc === 0x0a || cc === 0x0d;
}

function skipWs(src, i, len) {
  while (i < len && isWsCode(src.charCodeAt(i))) i++;
  return i;
}

// If `src[i..]` starts a string, char literal, or comment, advance past it and
// invoke `onNewline(offset)` for each contained newline. Returns the offset
// just after the lexical token, or -1 if `src[i]` doesn't start one of these
// constructs.
function skipNonCode(src, i, len, onNewline) {
  const c = src.charCodeAt(i);
  const c1 = i + 1 < len ? src.charCodeAt(i + 1) : -1;

  if (c === CC.SLASH && c1 === CC.SLASH) {
    let j = i + 2;
    while (j < len && src.charCodeAt(j) !== CC.NL) j++;
    return j;
  }

  if (c === CC.SLASH && c1 === CC.STAR) {
    let blockDepth = 1;
    let j = i + 2;
    while (j < len && blockDepth > 0) {
      const cc = src.charCodeAt(j);
      if (cc === CC.NL) {
        if (onNewline) onNewline(j);
        j++;
      } else if (cc === CC.SLASH && src.charCodeAt(j + 1) === CC.STAR) {
        blockDepth++;
        j += 2;
      } else if (cc === CC.STAR && src.charCodeAt(j + 1) === CC.SLASH) {
        blockDepth--;
        j += 2;
      } else {
        j++;
      }
    }
    return j;
  }

  const prevCc = i > 0 ? src.charCodeAt(i - 1) : 0;
  const leadOk = !isIdentCode(prevCc);

  let rawPrefix = 0;
  if (leadOk && c === CC.LOWER_R && (c1 === CC.DQUOTE || c1 === CC.HASH)) {
    rawPrefix = 1;
  } else if (
    leadOk &&
    c === CC.LOWER_B &&
    c1 === CC.LOWER_R &&
    (src.charCodeAt(i + 2) === CC.DQUOTE || src.charCodeAt(i + 2) === CC.HASH)
  ) {
    rawPrefix = 2;
  }
  if (rawPrefix > 0) {
    let j = i + rawPrefix;
    let hashes = 0;
    while (j < len && src.charCodeAt(j) === CC.HASH) {
      hashes++;
      j++;
    }
    if (src.charCodeAt(j) !== CC.DQUOTE) return -1;
    j++;
    while (j < len) {
      const cc = src.charCodeAt(j);
      if (cc === CC.NL && onNewline) onNewline(j);
      if (cc === CC.DQUOTE) {
        let k = j + 1;
        let h = 0;
        while (h < hashes && k < len && src.charCodeAt(k) === CC.HASH) {
          k++;
          h++;
        }
        if (h === hashes) {
          j = k;
          break;
        }
      }
      j++;
    }
    return j;
  }

  if (
    c === CC.DQUOTE ||
    (c === CC.LOWER_B && c1 === CC.DQUOTE && leadOk)
  ) {
    let j = c === CC.LOWER_B ? i + 2 : i + 1;
    while (j < len) {
      const cc = src.charCodeAt(j);
      if (cc === CC.BSLASH) {
        if (j + 1 < len && src.charCodeAt(j + 1) === CC.NL && onNewline) {
          onNewline(j + 1);
        }
        j += 2;
        continue;
      }
      if (cc === CC.NL && onNewline) onNewline(j);
      if (cc === CC.DQUOTE) {
        j++;
        break;
      }
      j++;
    }
    return j;
  }

  if (
    c === CC.SQUOTE ||
    (c === CC.LOWER_B && c1 === CC.SQUOTE && leadOk)
  ) {
    const p = c === CC.LOWER_B ? i + 2 : i + 1;
    const pc = src.charCodeAt(p);
    if (pc === CC.BSLASH) {
      let q = p + 1;
      while (q < len && src.charCodeAt(q) !== CC.SQUOTE && src.charCodeAt(q) !== CC.NL) q++;
      if (src.charCodeAt(q) === CC.SQUOTE) return q + 1;
      return p;
    }
    if (p + 1 < len && src.charCodeAt(p + 1) === CC.SQUOTE) {
      return p + 2;
    }
    // Lifetime such as `'a`; only consume the leading quote so the caller
    // re-enters the main code path on the next iteration.
    return i + 1;
  }

  return -1;
}

// Returns the byte length consumed if `src[start..]` is a `#[cfg(test)]` style
// attribute whose enabling predicate is satisfied whenever `cfg(test)` is —
// `#[cfg(test)]` or `#[cfg(all(test, ...))]`. Returns 0 otherwise.
function matchCfgTestAttr(src, start, len) {
  let i = start;
  if (src.charCodeAt(i) !== CC.HASH || src.charCodeAt(i + 1) !== CC.LBRACK) return 0;
  i += 2;
  i = skipWs(src, i, len);
  if (
    src.charCodeAt(i) !== 0x63 ||
    src.charCodeAt(i + 1) !== 0x66 ||
    src.charCodeAt(i + 2) !== 0x67
  ) {
    return 0;
  }
  i += 3;
  i = skipWs(src, i, len);
  if (src.charCodeAt(i) !== CC.LPAREN) return 0;
  i++;
  i = skipWs(src, i, len);

  let parenDepth = 1;
  if (
    src.charCodeAt(i) === 0x61 &&
    src.charCodeAt(i + 1) === 0x6c &&
    src.charCodeAt(i + 2) === 0x6c &&
    !isIdentCode(src.charCodeAt(i + 3))
  ) {
    i += 3;
    i = skipWs(src, i, len);
    if (src.charCodeAt(i) !== CC.LPAREN) return 0;
    i++;
    i = skipWs(src, i, len);
    parenDepth++;
  }

  if (
    src.charCodeAt(i) !== 0x74 ||
    src.charCodeAt(i + 1) !== 0x65 ||
    src.charCodeAt(i + 2) !== 0x73 ||
    src.charCodeAt(i + 3) !== 0x74 ||
    isIdentCode(src.charCodeAt(i + 4))
  ) {
    return 0;
  }
  i += 4;

  while (i < len && parenDepth > 0) {
    const cc = src.charCodeAt(i);
    if (cc === CC.LPAREN) parenDepth++;
    else if (cc === CC.RPAREN) parenDepth--;
    i++;
  }
  i = skipWs(src, i, len);
  if (src.charCodeAt(i) !== CC.RBRACK) return 0;
  return i + 1 - start;
}

function skipAttribute(src, i, len) {
  if (src.charCodeAt(i) !== CC.HASH || src.charCodeAt(i + 1) !== CC.LBRACK) return i;
  let j = i + 2;
  let bracketDepth = 1;
  while (j < len && bracketDepth > 0) {
    const cc = src.charCodeAt(j);
    if (cc === CC.LBRACK) bracketDepth++;
    else if (cc === CC.RBRACK) bracketDepth--;
    j++;
  }
  return j;
}

// Single-pass scanner. Counts total file newlines and the subset of those
// newlines that fall inside a top-level `#[cfg(test)]` gated item, honoring
// Rust comment and string lexing so braces/`;` inside them never confuse the
// item-body terminator search. Bracket/paren depth is tracked so that `;` in
// array types like `[T; N]` and `{` in macros like `vec![{1}]` don't trigger
// the item terminator while the cfg(test) item is still being parsed.
export function scanRust(src) {
  const len = src.length;
  const counts = { totalNl: 0, testNl: 0 };
  let depth = 0;
  let bracketDepth = 0;
  let parenDepth = 0;
  let inTestRegion = false;
  let testPendingSemi = false;
  let testTrailingNlEnd = -1;
  let i = 0;

  const noteNl = (at) => {
    counts.totalNl++;
    if (inTestRegion) counts.testNl++;
    if (testTrailingNlEnd !== -1 && at >= testTrailingNlEnd) {
      inTestRegion = false;
      testTrailingNlEnd = -1;
    }
  };

  while (i < len) {
    if (testTrailingNlEnd !== -1 && i >= testTrailingNlEnd) {
      inTestRegion = false;
      testTrailingNlEnd = -1;
    }

    const skipped = skipNonCode(src, i, len, noteNl);
    if (skipped > i) {
      i = skipped;
      continue;
    }

    const c = src.charCodeAt(i);
    if (c === CC.NL) {
      noteNl(i);
      i++;
      continue;
    }

    const atTopNesting = bracketDepth === 0 && parenDepth === 0;

    if (c === CC.LBRACK) {
      bracketDepth++;
      i++;
      continue;
    }
    if (c === CC.RBRACK) {
      if (bracketDepth > 0) bracketDepth--;
      i++;
      continue;
    }
    if (c === CC.LPAREN) {
      parenDepth++;
      i++;
      continue;
    }
    if (c === CC.RPAREN) {
      if (parenDepth > 0) parenDepth--;
      i++;
      continue;
    }
    if (c === CC.LBRACE) {
      depth++;
      if (testPendingSemi && atTopNesting && depth === 1) {
        testPendingSemi = false;
      }
      i++;
      continue;
    }
    if (c === CC.RBRACE) {
      if (depth > 0) depth--;
      i++;
      if (inTestRegion && !testPendingSemi && depth === 0) {
        testTrailingNlEnd = endOfLineAfter(src, i, len);
      }
      continue;
    }
    if (c === CC.SEMI && inTestRegion && testPendingSemi && atTopNesting && depth === 0) {
      i++;
      testPendingSemi = false;
      testTrailingNlEnd = endOfLineAfter(src, i, len);
      continue;
    }

    if (
      !inTestRegion &&
      depth === 0 &&
      atTopNesting &&
      c === CC.HASH &&
      src.charCodeAt(i + 1) === CC.LBRACK
    ) {
      const cfgLen = matchCfgTestAttr(src, i, len);
      if (cfgLen > 0) {
        inTestRegion = true;
        testPendingSemi = true;
        continue;
      }
    }

    i++;
  }

  return counts;
}

function endOfLineAfter(src, i, len) {
  let j = i;
  while (j < len) {
    const cc = src.charCodeAt(j);
    if (cc !== 0x20 && cc !== 0x09 && cc !== 0x0d) break;
    j++;
  }
  if (j < len && src.charCodeAt(j) === CC.NL) j++;
  return j;
}

function listFiles(root, globs) {
  try {
    const output = execSync(
      `git ls-files -z ${globs.map((g) => `'${g}'`).join(" ")}`,
      { cwd: root, encoding: "utf8", maxBuffer: 50 * 1024 * 1024 },
    );
    return output.split("\0").filter(Boolean);
  } catch {
    return [];
  }
}

export function computeLocSplit(root) {
  const files = listFiles(root, FILE_GLOBS).sort();

  let sourceLines = 0;
  let testLines = 0;

  for (const rel of files) {
    let content;
    try {
      content = fs.readFileSync(path.join(root, rel), "utf8");
    } catch {
      continue;
    }
    if (isTestFile(rel)) {
      let nl = 0;
      for (let k = 0; k < content.length; k++) {
        if (content.charCodeAt(k) === CC.NL) nl++;
      }
      testLines += nl;
      continue;
    }
    const { totalNl, testNl } = scanRust(content);
    const inlineTest = Math.min(totalNl, testNl);
    sourceLines += totalNl - inlineTest;
    testLines += inlineTest;
  }

  let crateCount = 0;
  try {
    crateCount = fs
      .readdirSync(path.join(root, "crates"), { withFileTypes: true })
      .filter((d) => d.isDirectory())
      .length;
  } catch {
    crateCount = 0;
  }

  const total = sourceLines + testLines;
  return {
    source_lines: sourceLines,
    test_lines: testLines,
    total_lines: total,
    crate_count: crateCount,
    source_loc: fmt(sourceLines),
    test_loc: fmt(testLines),
    total_loc: fmt(total),
    num_crates: String(crateCount),
  };
}

export function unavailableLocSplit() {
  return {
    source_lines: 0,
    test_lines: 0,
    total_lines: 0,
    crate_count: 0,
    source_loc: "N/A",
    test_loc: "N/A",
    total_loc: "N/A",
    num_crates: "N/A",
  };
}
