#!/usr/bin/env python3
"""Reject unreachable tests in every active workspace test root.

The clean-slate workspace sets `autotests = false`, so a Rust file under an
active test root only participates in a harness when an explicit Cargo
`[[test]]` target or a crate-source `#[path]` module reaches it. A test-bearing
file outside both graphs is silent: Cargo and nextest do not report that its
tests never ran (see #16013).

Workspace membership and active test roots come from Cargo manifests. This
keeps `tsz-core/rewrite-tests`, `tsz-cli/rewrite-tests`, and
`conformance/tests` covered without reviving retired no-manifest crate trees or
the unregistered legacy porting corpus. Active roots have no orphan baseline:
test-bearing files are reachable or rejected.
"""

from __future__ import annotations

from collections import deque
from dataclasses import dataclass
from pathlib import Path
import re
import sys


ROOT_DIR = Path(__file__).resolve().parents[2]
CRATES_DIR = ROOT_DIR / "crates"

TEST_ATTRIBUTE_RE = re.compile(
    r"(?m)^[ \t]*#[ \t]*\[[ \t]*test[ \t]*\]"
)
MODULE_DECLARATION_RE = re.compile(
    r"(?m)^[ \t]*(?:pub(?:\([^\r\n)]*\))?[ \t]+)?"
    r"mod[ \t]+(?P<name>[A-Za-z_][A-Za-z0-9_]*)[ \t]*;"
)
PATH_ATTRIBUTE_RE = re.compile(
    r'^\s*#\s*\[\s*path\s*=\s*"(?P<path>[^"\r\n]+)"\s*\]\s*$'
)


class ReachabilityConfigurationError(RuntimeError):
    """The active crate layout cannot be checked deterministically."""


@dataclass(frozen=True)
class ModuleRoot:
    path: Path
    module_dir: Path


@dataclass(frozen=True)
class ModuleDeclaration:
    name: str
    explicit_path: str | None


@dataclass(frozen=True)
class ActiveManifest:
    autotests_false: bool
    test_paths: tuple[str, ...]


def _blank_non_newlines(chars: list[str], start: int, end: int) -> None:
    for index in range(start, end):
        if chars[index] not in "\r\n":
            chars[index] = " "


def _char_literal_end(source: str, start: int) -> int | None:
    index = start + 1
    if index >= len(source) or source[index] in "\r\n":
        return None
    if source[index] == "\\":
        index += 1
        if index >= len(source):
            return None
        if source[index] == "u" and source[index + 1 : index + 2] == "{":
            closing_brace = source.find("}", index + 2)
            if closing_brace == -1:
                return None
            index = closing_brace + 1
        elif source[index] == "x":
            index += 3
        else:
            index += 1
    else:
        index += 1
    if index < len(source) and source[index] == "'":
        return index + 1
    return None


def mask_rust_comments_and_literals(source: str) -> str:
    """Keep offsets/newlines while hiding fake attributes and module items."""
    chars = list(source)
    index = 0
    while index < len(source):
        if source.startswith("//", index):
            end = source.find("\n", index + 2)
            end = len(source) if end == -1 else end
            _blank_non_newlines(chars, index, end)
            index = end
            continue
        if source.startswith("/*", index):
            start = index
            depth = 1
            index += 2
            while index < len(source) and depth:
                if source.startswith("/*", index):
                    depth += 1
                    index += 2
                elif source.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
            _blank_non_newlines(chars, start, index)
            continue
        if source[index] == "r":
            opening_quote = index + 1
            while opening_quote < len(source) and source[opening_quote] == "#":
                opening_quote += 1
            if opening_quote < len(source) and source[opening_quote] == '"':
                terminator = '"' + "#" * (opening_quote - index - 1)
                end = source.find(terminator, opening_quote + 1)
                end = len(source) if end == -1 else end + len(terminator)
                _blank_non_newlines(chars, index, end)
                index = end
                continue
        if source[index] == '"':
            start = index
            index += 1
            while index < len(source):
                if source[index] == "\\":
                    index = min(index + 2, len(source))
                elif source[index] == '"':
                    index += 1
                    break
                else:
                    index += 1
            _blank_non_newlines(chars, start, index)
            continue
        if source[index] == "'":
            end = _char_literal_end(source, index)
            if end is not None:
                _blank_non_newlines(chars, index, end)
                index = end
                continue
        index += 1
    return "".join(chars)


def _path_attribute_before(
    source_lines: list[str], masked_lines: list[str], module_line: int
) -> str | None:
    """Return the nearest rustfmt-shaped `#[path]` on this module item."""
    index = module_line - 1
    while index >= 0:
        masked = masked_lines[index].strip()
        if not masked:
            index -= 1
            continue
        if not (masked.startswith("#[") and masked.endswith("]")):
            break
        match = PATH_ATTRIBUTE_RE.match(source_lines[index])
        if match is not None:
            return match.group("path")
        index -= 1
    return None


def module_declarations(path: Path) -> list[ModuleDeclaration]:
    source = path.read_text(encoding="utf-8")
    masked = mask_rust_comments_and_literals(source)
    source_lines = source.splitlines()
    masked_lines = masked.splitlines()
    declarations = []
    for match in MODULE_DECLARATION_RE.finditer(masked):
        module_line = masked.count("\n", 0, match.start())
        declarations.append(
            ModuleDeclaration(
                name=match.group("name"),
                explicit_path=_path_attribute_before(
                    source_lines, masked_lines, module_line
                ),
            )
        )
    return declarations


def file_has_test_attribute(path: Path) -> bool:
    source = path.read_text(encoding="utf-8")
    return TEST_ATTRIBUTE_RE.search(mask_rust_comments_and_literals(source)) is not None


def _toml_without_comment(raw_line: str) -> str:
    """Strip a TOML comment without treating `#` inside a string as one."""
    in_string = False
    escaped = False
    for index, char in enumerate(raw_line):
        if escaped:
            escaped = False
            continue
        if in_string and char == "\\":
            escaped = True
            continue
        if char == '"':
            in_string = not in_string
            continue
        if char == "#" and not in_string:
            return raw_line[:index]
    return raw_line


def _quoted_toml_value(line: str, key: str) -> str | None:
    match = re.fullmatch(
        rf'{re.escape(key)}\s*=\s*"(?P<value>[^"\\]*)"\s*', line
    )
    if match is None:
        return None
    return match.group("value")


def _manifest(crate_dir: Path) -> ActiveManifest:
    manifest_path = crate_dir / "Cargo.toml"
    if not manifest_path.is_file():
        raise ReachabilityConfigurationError(f"missing manifest: {manifest_path}")
    section = ""
    package_autotests_false = False
    test_paths = []
    for raw_line in manifest_path.read_text(encoding="utf-8").splitlines():
        line = _toml_without_comment(raw_line).strip()
        if not line:
            continue
        if line == "[[test]]":
            section = "test"
            continue
        if line.startswith("[") and line.endswith("]"):
            section = "package" if line == "[package]" else ""
            continue
        if section == "package" and re.fullmatch(
            r"autotests\s*=\s*false", line
        ):
            package_autotests_false = True
        elif section == "test":
            path = _quoted_toml_value(line, "path")
            if path is not None:
                test_paths.append(path)
    return ActiveManifest(
        autotests_false=package_autotests_false,
        test_paths=tuple(test_paths),
    )


def _workspace_string_array(key: str, *, required: bool) -> tuple[str, ...]:
    manifest_path = CRATES_DIR.parent / "Cargo.toml"
    if not manifest_path.is_file():
        raise ReachabilityConfigurationError(
            f"missing workspace manifest: {manifest_path}"
        )

    section = ""
    collecting = False
    fragments = []
    balance = 0
    for raw_line in manifest_path.read_text(encoding="utf-8").splitlines():
        line = _toml_without_comment(raw_line).strip()
        if collecting:
            fragments.append(line)
            balance += line.count("[") - line.count("]")
            if balance <= 0:
                break
            continue
        if line.startswith("[") and line.endswith("]"):
            section = line[1:-1].strip()
            continue
        if section != "workspace":
            continue
        match = re.fullmatch(rf"{re.escape(key)}\s*=\s*(?P<value>.*)", line)
        if match is None:
            continue
        value = match.group("value")
        fragments.append(value)
        balance = value.count("[") - value.count("]")
        collecting = balance > 0
        if not collecting:
            break

    if not fragments:
        if required:
            raise ReachabilityConfigurationError(
                f"workspace manifest has no {key!r} array: {manifest_path}"
            )
        return ()

    array = "\n".join(fragments)
    if balance != 0 or not array.lstrip().startswith("[") or not array.rstrip().endswith("]"):
        raise ReachabilityConfigurationError(
            f"malformed workspace {key!r} array: {manifest_path}"
        )
    values = tuple(
        double if double else single
        for double, single in re.findall(r'"([^"\\]*)"|\'([^\'\\]*)\'', array)
    )
    residue = re.sub(r'"[^"\\]*"|\'[^\'\\]*\'', "", array)
    residue = residue.replace("[", "").replace("]", "").replace(",", "")
    if residue.strip():
        raise ReachabilityConfigurationError(
            f"unsupported value in workspace {key!r} array: {manifest_path}"
        )
    return values


def workspace_member_dirs() -> list[Path]:
    workspace_root = CRATES_DIR.parent.resolve()
    member_patterns = _workspace_string_array("members", required=True)
    exclude_patterns = _workspace_string_array("exclude", required=False)
    excluded = {
        candidate.resolve()
        for pattern in exclude_patterns
        for candidate in workspace_root.glob(pattern)
    }
    members = set()
    for pattern in member_patterns:
        candidates = sorted(workspace_root.glob(pattern))
        if not candidates:
            raise ReachabilityConfigurationError(
                f"workspace member pattern matches nothing: {pattern!r}"
            )
        for candidate in candidates:
            resolved = candidate.resolve()
            if resolved in excluded:
                continue
            if not _inside(resolved, workspace_root):
                raise ReachabilityConfigurationError(
                    f"workspace member escapes repository: {pattern!r}"
                )
            if not (resolved / "Cargo.toml").is_file():
                raise ReachabilityConfigurationError(
                    f"workspace member has no manifest: {resolved}"
                )
            members.add(resolved)
    return sorted(members)


def active_crate_dirs() -> list[Path]:
    return [
        crate_dir
        for crate_dir in workspace_member_dirs()
        if _manifest(crate_dir).autotests_false
    ]


def _inside(path: Path, directory: Path) -> bool:
    try:
        path.relative_to(directory)
    except ValueError:
        return False
    return True


def _external_module_dir(path: Path) -> Path:
    if path.name == "mod.rs":
        return path.parent
    return path.with_suffix("")


def _external_test_directory(crate_dir: Path, path: Path) -> Path | None:
    """Return the crate-local top-level directory containing an external test."""
    resolved_crate = crate_dir.resolve()
    resolved_path = path.resolve()
    if not _inside(resolved_path, resolved_crate):
        raise ReachabilityConfigurationError(
            f"active test path escapes workspace crate: {resolved_path}"
        )
    relative = resolved_path.relative_to(resolved_crate)
    if len(relative.parts) < 2 or relative.parts[0] == "src":
        return None
    return resolved_crate / relative.parts[0]


def active_test_dirs(crate_dir: Path) -> list[Path]:
    """Manifest-owned test directories for one active workspace member."""
    roots = set()
    for raw_path in _manifest(crate_dir).test_paths:
        path = (crate_dir / raw_path).resolve()
        if not path.is_file():
            raise ReachabilityConfigurationError(
                f"Cargo test target does not exist: {path}"
            )
        root = _external_test_directory(crate_dir, path)
        if root is None:
            raise ReachabilityConfigurationError(
                f"Cargo test target must live in a dedicated test root: {path}"
            )
        roots.add(root)

    source_root = crate_dir / "src"
    for source_file in sorted(source_root.rglob("*.rs")):
        for declaration in module_declarations(source_file):
            if declaration.explicit_path is None:
                continue
            path = (source_file.parent / declaration.explicit_path).resolve()
            if not path.is_file() or _inside(path, source_root.resolve()):
                continue
            root = _external_test_directory(crate_dir, path)
            if root is not None:
                roots.add(root)

    missing = [root for root in sorted(roots) if not root.is_dir()]
    if missing:
        raise ReachabilityConfigurationError(
            "missing active test root(s): " + ", ".join(str(root) for root in missing)
        )
    return sorted(roots)


def cargo_test_roots(crate_dir: Path, active_root: Path) -> set[ModuleRoot]:
    roots = set()
    for raw_path in _manifest(crate_dir).test_paths:
        path = (crate_dir / raw_path).resolve()
        if not _inside(path, active_root):
            continue
        if not path.is_file():
            raise ReachabilityConfigurationError(
                f"Cargo test target does not exist: {path}"
            )
        # A Cargo target is a crate root: sibling `mod part;` declarations are
        # resolved relative to the directory containing the target file.
        roots.add(ModuleRoot(path=path, module_dir=path.parent))
    return roots


def source_path_roots(crate_dir: Path, active_root: Path) -> set[ModuleRoot]:
    roots = set()
    source_root = crate_dir / "src"
    for source_file in sorted(source_root.rglob("*.rs")):
        for declaration in module_declarations(source_file):
            if declaration.explicit_path is None:
                continue
            path = (source_file.parent / declaration.explicit_path).resolve()
            if _inside(path, active_root) and path.is_file():
                roots.add(
                    ModuleRoot(path=path, module_dir=_external_module_dir(path))
                )
    return roots


def _module_children(root: ModuleRoot) -> list[ModuleRoot]:
    children = []
    for declaration in module_declarations(root.path):
        if declaration.explicit_path is not None:
            path = (root.path.parent / declaration.explicit_path).resolve()
            if path.is_file():
                children.append(
                    ModuleRoot(path=path, module_dir=_external_module_dir(path))
                )
            continue

        candidates = (
            root.module_dir / f"{declaration.name}.rs",
            root.module_dir / declaration.name / "mod.rs",
        )
        existing = [candidate.resolve() for candidate in candidates if candidate.is_file()]
        if len(existing) > 1:
            raise ReachabilityConfigurationError(
                f"ambiguous module {declaration.name!r} from {root.path}: "
                + ", ".join(str(path) for path in existing)
            )
        if existing:
            path = existing[0]
            children.append(ModuleRoot(path=path, module_dir=_external_module_dir(path)))
    return children


def reachable_files(roots: set[ModuleRoot]) -> set[Path]:
    reachable = set()
    visited_states = set()
    queue = deque(sorted(roots, key=lambda root: (str(root.path), str(root.module_dir))))
    while queue:
        root = queue.popleft()
        if root in visited_states:
            continue
        visited_states.add(root)
        if not root.path.is_file():
            continue
        reachable.add(root.path.resolve())
        queue.extend(_module_children(root))
    return reachable


def find_unreachable_active_test_files() -> list[str]:
    offenders = []
    for crate_dir in active_crate_dirs():
        for active_root in active_test_dirs(crate_dir):
            roots = cargo_test_roots(crate_dir, active_root)
            roots.update(source_path_roots(crate_dir, active_root))
            reachable = reachable_files(roots)
            for path in sorted(active_root.rglob("*.rs")):
                resolved = path.resolve()
                if file_has_test_attribute(resolved) and resolved not in reachable:
                    relative = resolved.relative_to(CRATES_DIR.parent.resolve())
                    offenders.append(relative.as_posix())
    return offenders


def main() -> int:
    try:
        offenders = find_unreachable_active_test_files()
    except (OSError, UnicodeError, ReachabilityConfigurationError) as error:
        print(f"test-file-reachability configuration error: {error}", file=sys.stderr)
        return 1
    if offenders:
        print(
            "Unreachable active test file(s): these contain #[test] "
            "but no Cargo [[test]] or src #[path] module graph reaches them:",
            file=sys.stderr,
        )
        for offender in offenders:
            print(f"  {offender}", file=sys.stderr)
        print(
            "Register each active test through Cargo or an existing source/test "
            "module graph, or delete it if it is dead. No orphan baseline is allowed.",
            file=sys.stderr,
        )
        return 1
    print("test-file-reachability: active workspace roots have 0 unreachable tests.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
