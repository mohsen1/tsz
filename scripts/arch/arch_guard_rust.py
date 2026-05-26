import pathlib
import re

from arch_guard_shared import (
    EXCLUDE_DIRS,
    ROOT,
    SOLVER_TYPEDATA_QUARANTINE_ALLOWLIST,
    VALID_CHECKER_CONTEXT_CAPABILITIES,
    VALID_CHECKER_CONTEXT_LIFETIMES,
    iter_rs_files,
)

def scan_struct_field_count(
    path: pathlib.Path, struct_name: str, max_fields: int
) -> list[str]:
    """Count fields in `pub struct <struct_name>` and report when over `max_fields`.

    Field counting is intentionally regex-based (not syn/AST): the goal is a
    cheap, repeatable arch metric, not a perfect reflection.  Lines that look
    like a field declaration (`name: Type,`) inside the struct body are
    counted; doc comments, empty lines, and `}` terminators are skipped.
    Comments are stripped first via `strip_rust_comments` so commented-out
    fields don't inflate the count.
    """
    if not path.exists():
        return []
    rel = relative_path(path)
    body = find_struct_body(path, struct_name)
    if body is None:
        return [f"{rel}:0 struct {struct_name!r} not found"]

    field_count = len(extract_struct_field_names_from_body(body))

    if field_count > max_fields:
        return [
            f"{rel}:struct {struct_name} has {field_count} fields "
            f"(cap {max_fields}; bump cap intentionally and update ROADMAP.md)"
        ]
    return []


def scan_trait_method_count(
    path: pathlib.Path, trait_name: str, max_methods: int
) -> list[str]:
    """Count method declarations in `pub trait <trait_name>`.

    This is a cheap architecture metric for broad capability traits.  It counts
    every `fn name...` declaration in the trait body, including default-method
    bodies, because both expand the capability surface exposed to algorithms.
    Comments are stripped first so doc examples or commented-out signatures do
    not affect the ratchet.
    """
    if not path.exists():
        return []
    rel = relative_path(path)
    body = find_trait_body(path, trait_name)
    if body is None:
        return [f"{rel}:0 trait {trait_name!r} not found"]

    method_count = len(extract_trait_method_names_from_body(body))

    if method_count > max_methods:
        return [
            f"{rel}:trait {trait_name} has {method_count} methods "
            f"(cap {max_methods}; split onto a narrower trait or bump cap "
            f"intentionally and update #8205)"
        ]
    return []


def relative_path(path: pathlib.Path) -> str:
    try:
        return path.relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def find_struct_body(path: pathlib.Path, struct_name: str):
    text = path.read_text(encoding="utf-8", errors="ignore")
    stripped = strip_rust_comments(text)
    header_pattern = re.compile(
        rf"\bpub\s+struct\s+{re.escape(struct_name)}\b[^{{]*\{{",
        re.MULTILINE,
    )
    match = header_pattern.search(stripped)
    if match is None:
        return None

    body_start = match.end()
    depth = 1
    body_end = body_start
    for i in range(body_start, len(stripped)):
        ch = stripped[i]
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                body_end = i
                break
    return stripped[body_start:body_end]


def find_trait_body(path: pathlib.Path, trait_name: str):
    text = path.read_text(encoding="utf-8", errors="ignore")
    stripped = strip_rust_comments(text)
    header_pattern = re.compile(
        rf"\bpub\s+trait\s+{re.escape(trait_name)}\b[^{{]*\{{",
        re.MULTILINE,
    )
    match = header_pattern.search(stripped)
    if match is None:
        return None

    body_start = match.end()
    depth = 1
    body_end = body_start
    for i in range(body_start, len(stripped)):
        ch = stripped[i]
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                body_end = i
                break
    return stripped[body_start:body_end]


STRUCT_FIELD_PATTERN = re.compile(
    r"^\s*(?:pub(?:\s*\([^)]*\))?\s+)?(?P<name>[a-z_][a-zA-Z0-9_]*)\s*:"
)

TRAIT_METHOD_PATTERN = re.compile(
    r"^\s*(?:async\s+|unsafe\s+|const\s+)?fn\s+"
    r"(?P<name>[a-zA-Z_][a-zA-Z0-9_]*)\s*[<(]"
)


def extract_struct_field_names_from_body(body: str) -> list[str]:
    names = []
    for line in body.splitlines():
        match = STRUCT_FIELD_PATTERN.match(line)
        if match:
            names.append(match.group("name"))
    return names


def extract_trait_method_names_from_body(body: str) -> list[str]:
    names = []
    for line in body.splitlines():
        match = TRAIT_METHOD_PATTERN.match(line)
        if match:
            names.append(match.group("name"))
    return names


def extract_struct_field_names(path: pathlib.Path, struct_name: str) -> list[str]:
    if not path.exists():
        return []
    body = find_struct_body(path, struct_name)
    if body is None:
        return []
    return extract_struct_field_names_from_body(body)


def parse_checker_context_lifetime_manifest(
    path: pathlib.Path,
) -> tuple[dict[str, dict[str, object]], list[str]]:
    rel = relative_path(path)
    if not path.exists():
        return {}, [f"{rel}:0 lifetime manifest is missing"]

    entries: dict[str, dict[str, object]] = {}
    errors: list[str] = []
    current = None
    section_pattern = re.compile(r"^\s*\[([A-Za-z_][A-Za-z0-9_]*)\]\s*(?:#.*)?$")
    inline_entry_pattern = re.compile(
        r'^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*\{\s*'
        r'lifetime\s*=\s*"([^"]*)"\s*,\s*'
        r'capability\s*=\s*"([^"]*)"\s*,\s*'
        r'reason\s*=\s*"([^"]*)"\s*'
        r'\}\s*(?:#.*)?$'
    )
    key_value_pattern = re.compile(
        r'^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*"([^"]*)"\s*(?:#.*)?$'
    )

    for line_no, line in enumerate(
        path.read_text(encoding="utf-8", errors="ignore").splitlines(),
        start=1,
    ):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue

        inline_entry_match = inline_entry_pattern.match(line)
        if inline_entry_match and current is None:
            field, lifetime, capability, reason = inline_entry_match.groups()
            if field in entries:
                errors.append(f"{rel}:{line_no} duplicate manifest entry [{field}]")
            else:
                entries[field] = {
                    "line": line_no,
                    "lifetime": lifetime,
                    "capability": capability,
                    "reason": reason,
                }
            continue

        section_match = section_pattern.match(line)
        if section_match:
            current = section_match.group(1)
            if current in entries:
                errors.append(f"{rel}:{line_no} duplicate manifest section [{current}]")
            else:
                entries[current] = {"line": line_no}
            continue

        key_value_match = key_value_pattern.match(line)
        if key_value_match and current is not None:
            key, value = key_value_match.groups()
            entries[current][key] = value
            continue

        if key_value_match:
            errors.append(f"{rel}:{line_no} key/value entry appears before any section")
        else:
            errors.append(f"{rel}:{line_no} unsupported manifest line")

    return entries, errors


def scan_checker_context_lifetime_manifest(
    struct_path: pathlib.Path,
    struct_name: str,
    manifest_path: pathlib.Path,
) -> list[str]:
    if not struct_path.exists():
        return []
    struct_rel = relative_path(struct_path)
    manifest_rel = relative_path(manifest_path)
    body = find_struct_body(struct_path, struct_name)
    if body is None:
        return [f"{struct_rel}:0 struct {struct_name!r} not found"]

    fields = extract_struct_field_names_from_body(body)
    field_set = set(fields)
    entries, hits = parse_checker_context_lifetime_manifest(manifest_path)
    entry_set = set(entries.keys())

    for field in fields:
        if field not in entries:
            hits.append(
                f"{manifest_rel}:0 missing CheckerContext lifetime for field [{field}]"
            )

    for field in sorted(entry_set - field_set):
        line = entries[field].get("line", 0)
        hits.append(
            f"{manifest_rel}:{line} stale manifest entry [{field}] "
            f"not found in {struct_name}"
        )

    for field, entry in sorted(
        entries.items(), key=lambda item: item[1].get("line", 0)
    ):
        line = entry.get("line", 0)
        lifetime = entry.get("lifetime")
        capability = entry.get("capability")
        reason = entry.get("reason")
        if lifetime is None:
            hits.append(f"{manifest_rel}:{line} [{field}] missing lifetime")
        elif lifetime == "Unknown":
            hits.append(f"{manifest_rel}:{line} [{field}] lifetime must not be Unknown")
        elif lifetime not in VALID_CHECKER_CONTEXT_LIFETIMES:
            hits.append(
                f"{manifest_rel}:{line} [{field}] invalid lifetime {lifetime!r}"
            )
        if capability is None:
            hits.append(f"{manifest_rel}:{line} [{field}] missing capability")
        elif capability == "Unknown":
            hits.append(f"{manifest_rel}:{line} [{field}] capability must not be Unknown")
        elif capability not in VALID_CHECKER_CONTEXT_CAPABILITIES:
            hits.append(
                f"{manifest_rel}:{line} [{field}] invalid capability {capability!r}"
            )
        if not isinstance(reason, str) or not reason.strip():
            hits.append(f"{manifest_rel}:{line} [{field}] missing reason")

    return hits


def escape_markdown_cell(value: object) -> str:
    return str(value).replace("|", "\\|").replace("\n", " ")


def checker_context_lifetime_markdown(
    struct_path: pathlib.Path,
    struct_name: str,
    manifest_path: pathlib.Path,
) -> str:
    fields = extract_struct_field_names(struct_path, struct_name)
    entries, _errors = parse_checker_context_lifetime_manifest(manifest_path)
    lines = [
        "| Field | Lifetime | Capability | Reason |",
        "| --- | --- | --- | --- |",
    ]
    for field in fields:
        entry = entries.get(field, {})
        lifetime = escape_markdown_cell(entry.get("lifetime", "MISSING"))
        capability = escape_markdown_cell(entry.get("capability", "MISSING"))
        reason = escape_markdown_cell(entry.get("reason", "MISSING"))
        lines.append(f"| `{field}` | `{lifetime}` | `{capability}` | {reason} |")
    return "\n".join(lines)


def scan_file_line_limit(path: pathlib.Path, limit: int):
    if not path.exists():
        return []

    try:
        rel = path.relative_to(ROOT).as_posix()
    except ValueError:
        rel = path.as_posix()

    line_count = 0
    try:
        with path.open("r", encoding="utf-8", errors="ignore") as handle:
            for line_count, _line in enumerate(handle, start=1):
                pass
    except OSError:
        return []

    if line_count > limit:
        return [f"{rel}:{line_count} lines (limit {limit})"]
    return []


def strip_rust_comments(text: str) -> str:
    chars = list(text)
    i = 0
    n = len(chars)
    out = []
    state = "code"
    block_depth = 0
    raw_hash_count = 0

    while i < n:
        ch = chars[i]
        nxt = chars[i + 1] if i + 1 < n else ""

        if state == "line_comment":
            if ch == "\n":
                out.append("\n")
                state = "code"
            else:
                out.append(" ")
            i += 1
            continue

        if state == "block_comment":
            if ch == "/" and nxt == "*":
                block_depth += 1
                out.extend([" ", " "])
                i += 2
                continue
            if ch == "*" and nxt == "/":
                block_depth -= 1
                out.extend([" ", " "])
                i += 2
                if block_depth == 0:
                    state = "code"
                continue
            out.append("\n" if ch == "\n" else " ")
            i += 1
            continue

        if state == "string":
            out.append(ch)
            if ch == "\\" and i + 1 < n:
                out.append(chars[i + 1])
                i += 2
                continue
            if ch == '"':
                state = "code"
            i += 1
            continue

        if state == "char":
            out.append(ch)
            if ch == "\\" and i + 1 < n:
                out.append(chars[i + 1])
                i += 2
                continue
            if ch == "'":
                state = "code"
            i += 1
            continue

        if state == "raw_string":
            out.append(ch)
            if ch == '"' and raw_hash_count == 0:
                state = "code"
                i += 1
                continue
            if ch == '"' and raw_hash_count > 0:
                hashes = 0
                j = i + 1
                while j < n and chars[j] == "#" and hashes < raw_hash_count:
                    hashes += 1
                    j += 1
                if hashes == raw_hash_count:
                    out.extend(["#"] * hashes)
                    i = j
                    state = "code"
                    continue
            i += 1
            continue

        if ch == "/" and nxt == "/":
            out.extend([" ", " "])
            i += 2
            state = "line_comment"
            continue
        if ch == "/" and nxt == "*":
            out.extend([" ", " "])
            i += 2
            state = "block_comment"
            block_depth = 1
            continue
        if ch == '"':
            out.append(ch)
            i += 1
            state = "string"
            continue
        if ch == "'":
            out.append(ch)
            i += 1
            state = "char"
            continue
        if ch == "r":
            j = i + 1
            hashes = 0
            while j < n and chars[j] == "#":
                hashes += 1
                j += 1
            if j < n and chars[j] == '"':
                out.append("r")
                out.extend(["#"] * hashes)
                out.append('"')
                i = j + 1
                state = "raw_string"
                raw_hash_count = hashes
                continue

        out.append(ch)
        i += 1

    return "".join(out)


def scan_solver_typedata_quarantine(base: pathlib.Path):
    hits = set()
    alias_re = re.compile(r"\bTypeData\s+as\s+([A-Za-z_]\w*)\b")
    type_alias_re = re.compile(r"\btype\s+([A-Za-z_]\w*)\s*=\s*[^;]*\bTypeData\b[^;]*;")
    direct_intern_re = re.compile(
        r"\.intern\s*\(\s*(?:crate::types::TypeData|tsz_solver::TypeData|TypeData)\s*::",
        re.MULTILINE,
    )

    for path, rel in iter_rs_files(base):
        if "/tests/" in rel or any(rel.endswith(allow) for allow in SOLVER_TYPEDATA_QUARANTINE_ALLOWLIST):
            continue

        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        text_without_comments = strip_rust_comments(text)

        aliases = {"TypeData"}
        for alias_match in alias_re.finditer(text_without_comments):
            aliases.add(alias_match.group(1))
        for statement in text_without_comments.split(";"):
            normalized = " ".join(statement.split())
            type_alias_match = type_alias_re.search(f"{normalized};")
            if type_alias_match:
                aliases.add(type_alias_match.group(1))

        for match in direct_intern_re.finditer(text_without_comments):
            line_idx = text_without_comments.count("\n", 0, match.start())
            hits.add(f"{rel}:{line_idx + 1}")

        for alias in aliases:
            if alias == "TypeData":
                continue
            alias_re_intern = re.compile(
                rf"\.intern\s*\(\s*{re.escape(alias)}\s*::",
                re.MULTILINE,
            )
            for match in alias_re_intern.finditer(text_without_comments):
                line_idx = text_without_comments.count("\n", 0, match.start())
                hits.add(f"{rel}:{line_idx + 1}")

    return sorted(hits)
