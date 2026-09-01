#!/usr/bin/env python3
"""Small, transparent architecture guard for the TSZ rewrite.

The reset intentionally has few architectural rules.  Keep each rule here
mechanical and actionable; semantic compatibility belongs in oracle tests.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import json
from pathlib import Path
import os
import re
import sys
from typing import Iterable, Iterator


ALLOWED_WORKSPACE_MEMBERS = {
    "crates/conformance": "tsz-conformance",
    "crates/tsz-cli": "tsz-cli",
    "crates/tsz-core": "tsz-core",
}

RETIRED_CRATES = {
    "tsz-binder",
    "tsz-checker",
    "tsz-common",
    "tsz-emitter",
    "tsz-lowering",
    "tsz-lsp",
    "tsz-parser",
    "tsz-scanner",
    "tsz-solver",
    "tsz-wasm",
    "tsz-website",
}

SKIP_DIR_NAMES = {
    ".cache",
    ".git",
    ".next",
    ".target",
    "TypeScript",
    "__pycache__",
    "baselines",
    "cache",
    "node_modules",
    "snapshots",
    "target",
    "vendor",
}

SOUND_SCAN_ROOTS = (
    "crates",
    "docs",
    "tests/legacy-internal",
)

# The legacy tests are a byte-preserved oracle corpus with Cargo autotest
# discovery disabled.  File-size policy applies to executable rewrite code,
# not to those archived specifications.
ACTIVE_RUST_ROOTS = (
    "crates/tsz-core/src",
    "crates/tsz-cli/src",
    "crates/conformance",
)

# The retired compiler accumulated 111 TSZ_* behavior knobs.  The rewrite's
# semantic result must be a function of typed program/service inputs, not the
# parent process.  These CLI-only names may expose logging or counters; they
# must never select compiler behavior.
ACTIVE_COMPILER_RUST_ROOTS = (
    "crates/tsz-core/src",
    "crates/tsz-cli/src",
)

REWRITE_COMPILER_SIZE_MANIFEST_PATH = "scripts/arch/rewrite-compiler-size.json"
R0_COMPILER_PHYSICAL_LINE_LIMIT = 15_000
REWRITE_COMPILER_INCLUDE_PATTERNS = (
    "crates/tsz-cli/src/**/*.rs",
    "crates/tsz-core/src/**/*.rs",
)
REWRITE_COMPILER_EXCLUDE_PATHS = (
    "crates/tsz-core/src/program/capabilities/tests.rs",
)

CLI_OBSERVABILITY_ENV_NAMES = frozenset(
    {
        "TSZ_LOG",
        "TSZ_LOG_FORMAT",
        "TSZ_PERF_COUNTERS",
    }
)

# Process environment ownership is deliberately narrower than crate ownership.
# Keeping the reads in one adapter module makes it reviewable that they only
# configure observability and never influence compiler inputs or semantics.
CLI_OBSERVABILITY_ENV_PATH = "crates/tsz-cli/src/telemetry.rs"

# These documents explain why the retired surface must not return.  They are
# the only places under the scanned roots where the historical name is useful.
SOUND_HISTORY_ALLOWLIST = {
    "docs/architecture/RESET.md",
    "docs/plan/ROADMAP.md",
}

TEXT_SUFFIXES = {
    ".cjs",
    ".css",
    ".html",
    ".js",
    ".json",
    ".json5",
    ".jsx",
    ".md",
    ".mjs",
    ".py",
    ".rs",
    ".sh",
    ".toml",
    ".ts",
    ".tsx",
    ".yaml",
    ".yml",
}

SOUND_MARKERS = (
    re.compile(r"(?i)\bsound(?:[ _-]?mode)\b"),
    re.compile(r"\bsound_report_only\b"),
    re.compile(r"\bsound_declaration_projection\b"),
    re.compile(r"\bsoundCheckDeclarations\b"),
    re.compile(r"\bsoundPedantic\b"),
    re.compile(r"\bstrict_subtype_checking\b"),
    re.compile(r"\bstrict_any_propagation\b"),
    re.compile(r"\bSoundLawyer\b"),
    re.compile(r"\bSoundModeConfig\b"),
    re.compile(r"\bSoundDiagnosticCode\b"),
    re.compile(r"\baudit-unsoundness\b"),
    re.compile(r"--sound\b"),
)

SEMANTIC_PATH_WORDS = {
    "checker",
    "flow",
    "inference",
    "narrowing",
    "relation",
    "types",
}

# These patterns are not a Rust parser.  They only reject obvious production
# semantic decisions made from user spellings, fixture paths, or rendered
# output.  Oracle tests and review remain responsible for subtler cases.
HARDCODING_PATTERNS = (
    (
        "literal-string-predicate",
        re.compile(
            r"\b(?:if|while|match)\b[^\n]{0,240}"
            r"\.(?:contains|starts_with|ends_with)\s*\(\s*r?#{0,8}\""
        ),
    ),
    (
        "user-name-or-path-comparison",
        re.compile(
            r"\b(?:file_name|fixture|identifier|property_name|source_text|"
            r"symbol_name|type_text|rendered_type|formatted_diagnostic)\b"
            r"[^\n;]{0,120}(?:==|!=)\s*r?(?:#+)?\""
        ),
    ),
    (
        "rendered-value-predicate",
        re.compile(
            r"\b(?:if|while|match)\b[^\n]{0,240}"
            r"(?:format!\s*\(|to_string\s*\(|render(?:ed)?_type|"
            r"formatted_diagnostic)"
        ),
    ),
    (
        "regex-in-semantics",
        re.compile(r"\b(?:Regex|RegexBuilder)::(?:new|default)\s*\("),
    ),
)

PROCESS_ENV_ACCESS = re.compile(
    r"\b(?:std\s*::\s*)?env\s*::\s*(?:var|var_os|vars|vars_os)\b"
)

PROCESS_ENV_IMPORTS = (
    re.compile(r"\buse\s+(?:::)?std\s*::\s*env\b"),
    re.compile(r"\buse\s+(?:::)?std\s*::\s*\{[^;]{0,300}\benv\b", re.DOTALL),
)

TSZ_ENV_NAME = re.compile(r"\bTSZ_[A-Za-z0-9_]+\b")


@dataclass(frozen=True, order=True)
class Violation:
    path: str
    line: int
    code: str
    message: str

    def render(self) -> str:
        return f"{self.path}:{self.line}: {self.code}: {self.message}"


@dataclass(frozen=True)
class Dependency:
    name: str
    package: str
    path: str | None
    workspace: bool
    section: str
    line: int


@dataclass(frozen=True)
class Manifest:
    package: str | None
    members: tuple[str, ...]
    members_line: int
    dependencies: tuple[Dependency, ...]


@dataclass(frozen=True)
class RewriteCompilerSizeManifest:
    r0_physical_line_limit: int
    include: tuple[str, ...]
    exclude: tuple[str, ...]


@dataclass(frozen=True)
class RewriteCompilerSize:
    physical_lines: int
    r0_physical_line_limit: int
    included_paths: tuple[str, ...]
    excluded_paths: tuple[str, ...]

    @property
    def r0_ready(self) -> bool:
        return self.physical_lines < self.r0_physical_line_limit


def _relative(root: Path, path: Path) -> str:
    return path.resolve().relative_to(root.resolve()).as_posix()


def _walk_files(root: Path, start: Path) -> Iterator[Path]:
    if not start.exists():
        return
    for directory, dir_names, file_names in os.walk(start, followlinks=False):
        dir_names[:] = sorted(name for name in dir_names if name not in SKIP_DIR_NAMES)
        for file_name in sorted(file_names):
            path = Path(directory, file_name)
            if path.is_file() and not path.is_symlink():
                yield path


def _read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def _physical_lines(text: str) -> int:
    if not text:
        return 0
    return text.count("\n") + (not text.endswith("\n"))


def _rewrite_compiler_size_manifest(root: Path) -> RewriteCompilerSizeManifest:
    path = root / REWRITE_COMPILER_SIZE_MANIFEST_PATH
    try:
        value = json.loads(_read_text(path))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"cannot read size manifest: {error}") from error
    expected_keys = {
        "schema_version",
        "r0_physical_line_limit",
        "include",
        "exclude",
    }
    if not isinstance(value, dict) or set(value) != expected_keys:
        raise ValueError(
            "size manifest must be an object with exactly schema_version, "
            "r0_physical_line_limit, include, and exclude"
        )
    if type(value["schema_version"]) is not int or value["schema_version"] != 1:
        raise ValueError("size manifest schema_version must be 1")
    if (
        type(value["r0_physical_line_limit"]) is not int
        or value["r0_physical_line_limit"] != R0_COMPILER_PHYSICAL_LINE_LIMIT
    ):
        raise ValueError(
            "R0 compiler size limit must remain 15000 physical lines"
        )

    def string_tuple(field: str) -> tuple[str, ...]:
        entries = value[field]
        if (
            not isinstance(entries, list)
            or any(not isinstance(entry, str) or not entry for entry in entries)
            or entries != sorted(set(entries))
        ):
            raise ValueError(
                f"size manifest {field} must be a sorted list of unique paths"
            )
        return tuple(entries)

    include = string_tuple("include")
    exclude = string_tuple("exclude")
    if include != REWRITE_COMPILER_INCLUDE_PATTERNS:
        raise ValueError(
            "size manifest include list must cover exactly tsz-core/src and "
            "tsz-cli/src Rust sources"
        )
    if exclude != REWRITE_COMPILER_EXCLUDE_PATHS:
        raise ValueError(
            "size manifest exclude list must contain exactly the test-only "
            "Rust sources under the compiler src roots"
        )
    return RewriteCompilerSizeManifest(
        r0_physical_line_limit=value["r0_physical_line_limit"],
        include=include,
        exclude=exclude,
    )


def rewrite_compiler_size(root: Path) -> RewriteCompilerSize:
    manifest = _rewrite_compiler_size_manifest(root)
    included: set[Path] = set()
    for pattern in manifest.include:
        matches = {
            path.resolve()
            for path in root.glob(pattern)
            if path.is_file() and not path.is_symlink()
        }
        if not matches:
            raise ValueError(
                f"size manifest include pattern matched no files: {pattern}"
            )
        included.update(matches)

    excluded: set[Path] = set()
    for relative in manifest.exclude:
        path = (root / relative).resolve()
        if not path.is_file() or path.is_symlink():
            raise ValueError(f"size manifest excluded source is missing: {relative}")
        if path not in included:
            raise ValueError(
                f"size manifest exclusion is outside include roots: {relative}"
            )
        excluded.add(path)

    selected = sorted(included - excluded)
    return RewriteCompilerSize(
        physical_lines=sum(_physical_lines(_read_text(path)) for path in selected),
        r0_physical_line_limit=manifest.r0_physical_line_limit,
        included_paths=tuple(_relative(root, path) for path in selected),
        excluded_paths=tuple(_relative(root, path) for path in sorted(excluded)),
    )


def _line_of(text: str, pattern: re.Pattern[str]) -> int:
    match = pattern.search(text)
    return text.count("\n", 0, match.start()) + 1 if match else 1


def _manifest_line(text: str, name: str) -> int:
    pattern = re.compile(rf"(?m)^\s*{re.escape(name)}\s*=")
    return _line_of(text, pattern)


_TABLE = re.compile(r"^\s*\[{1,2}([^]]+)]{1,2}\s*(?:#.*)?$")
_ASSIGNMENT = re.compile(r'^\s*(?:"([^"]+)"|\'([^\']+)\'|([A-Za-z0-9_.-]+))\s*=\s*(.*)$')
_STRING_FIELD = r'\b{field}\s*=\s*(?:"([^"]*)"|\'([^\']*)\')'


def _assignment(line: str) -> tuple[str, str] | None:
    match = _ASSIGNMENT.match(line)
    if not match:
        return None
    return next(value for value in match.groups()[:3] if value is not None), match.group(4)


def _string_field(value: str, field: str) -> str | None:
    match = re.search(_STRING_FIELD.format(field=re.escape(field)), value)
    if not match:
        return None
    return match.group(1) if match.group(1) is not None else match.group(2)


def _dependency_section(section: str) -> bool:
    return section in {"dependencies", "dev-dependencies", "build-dependencies"} or any(
        section.endswith("." + name)
        for name in ("dependencies", "dev-dependencies", "build-dependencies")
    )


def _dependency_table(section: str) -> tuple[str, str] | None:
    match = re.match(
        r"^(.*(?:^|\.)(?:dependencies|dev-dependencies|build-dependencies))\."
        r'(?:"([^"]+)"|\'([^\']+)\'|([A-Za-z0-9_.-]+))$',
        section,
    )
    if not match:
        return None
    name = next(value for value in match.groups()[1:] if value is not None)
    return match.group(1), name


def _balanced_value(lines: list[str], start: int, value: str, opening: str, closing: str) -> str:
    combined = value
    balance = value.count(opening) - value.count(closing)
    index = start + 1
    while balance > 0 and index < len(lines):
        combined += "\n" + lines[index]
        balance += lines[index].count(opening) - lines[index].count(closing)
        index += 1
    return combined


def _parse_manifest(path: Path) -> Manifest:
    """Read only the Cargo fields used by this guard.

    This deliberately is not a general TOML parser.  Cargo validates TOML;
    the guard recognizes package names, workspace members, and every standard
    dependency table, including target-qualified and table-form dependencies.
    Keeping this reader local preserves Python 3.9 compatibility without a
    third-party parser.
    """

    lines = _read_text(path).splitlines()
    section = ""
    package: str | None = None
    members: tuple[str, ...] = ()
    members_line = 1
    dependencies: list[Dependency] = []
    table_dependency: dict[str, object] | None = None

    def finish_table_dependency() -> None:
        nonlocal table_dependency
        if table_dependency is not None:
            dependencies.append(Dependency(**table_dependency))
            table_dependency = None

    def record_dependency(dependency: Dependency) -> None:
        for existing_index, existing in enumerate(dependencies):
            if existing.name != dependency.name or existing.section != dependency.section:
                continue
            dependencies[existing_index] = Dependency(
                name=existing.name,
                package=(
                    dependency.package
                    if dependency.package != dependency.name
                    else existing.package
                ),
                path=dependency.path if dependency.path is not None else existing.path,
                workspace=existing.workspace or dependency.workspace,
                section=existing.section,
                line=min(existing.line, dependency.line),
            )
            return
        dependencies.append(dependency)

    for index, line in enumerate(lines):
        table = _TABLE.match(line)
        if table:
            finish_table_dependency()
            section = table.group(1).strip()
            dependency_table = _dependency_table(section)
            if dependency_table:
                parent, name = dependency_table
                table_dependency = {
                    "name": name,
                    "package": name,
                    "path": None,
                    "workspace": False,
                    "section": parent,
                    "line": index + 1,
                }
            continue

        assignment = _assignment(line)
        if not assignment:
            continue
        key, value = assignment
        if table_dependency is not None:
            if key in {"package", "path"}:
                parsed = _string_field(f"{key} = {value}", key)
                if parsed is not None:
                    table_dependency[key] = parsed
            elif key == "workspace":
                table_dependency[key] = value.split("#", 1)[0].strip() == "true"
            continue

        if section == "package" and key == "name":
            package = _string_field(f"name = {value}", "name")
        elif section == "workspace" and key == "members":
            members_line = index + 1
            array = _balanced_value(lines, index, value, "[", "]")
            members = tuple(
                first if first is not None else second
                for first, second in re.findall(r'"([^"]*)"|\'([^\']*)\'', array)
            )
        elif _dependency_section(section):
            field = None
            dependency_name = key
            if "." in key and key.rsplit(".", 1)[1] in {"package", "path", "workspace"}:
                dependency_name, field = key.rsplit(".", 1)
            spec = _balanced_value(lines, index, value, "{", "}")
            dependency_package = _string_field(spec, "package") or dependency_name
            path_value = _string_field(spec, "path")
            workspace = bool(re.search(r"\bworkspace\s*=\s*true\b", spec))
            if field == "package":
                dependency_package = (
                    _string_field(f"package = {value}", "package") or dependency_name
                )
            elif field == "path":
                path_value = _string_field(f"path = {value}", "path")
            elif field == "workspace":
                workspace = value.split("#", 1)[0].strip() == "true"
            record_dependency(
                Dependency(
                    name=dependency_name,
                    package=dependency_package,
                    path=path_value,
                    workspace=workspace,
                    section=section,
                    line=index + 1,
                )
            )
    finish_table_dependency()
    return Manifest(package, members, members_line, tuple(dependencies))


def _workspace_dependency_package(workspace: Manifest, name: str) -> str:
    for dependency in workspace.dependencies:
        if dependency.section == "workspace.dependencies" and dependency.name == name:
            return dependency.package
    return name


def check_workspace(root: Path) -> list[Violation]:
    violations: list[Violation] = []
    manifest = root / "Cargo.toml"
    if not manifest.is_file():
        return [Violation("Cargo.toml", 1, "workspace-manifest", "root manifest is missing")]
    data = _parse_manifest(manifest)
    members = {member.removeprefix("./").rstrip("/") for member in data.members}
    expected = set(ALLOWED_WORKSPACE_MEMBERS)
    for member in sorted(members - expected):
        violations.append(
            Violation(
                "Cargo.toml",
                data.members_line,
                "workspace-member",
                f"unexpected workspace member {member!r}; internal phases are tsz-core modules",
            )
        )
    for member in sorted(expected - members):
        violations.append(
            Violation(
                "Cargo.toml",
                data.members_line,
                "workspace-member",
                f"required workspace member {member!r} is missing",
            )
        )

    for member, expected_package in ALLOWED_WORKSPACE_MEMBERS.items():
        member_manifest = root / member / "Cargo.toml"
        if not member_manifest.is_file():
            violations.append(
                Violation(member + "/Cargo.toml", 1, "workspace-manifest", "member manifest is missing")
            )
            continue
        actual = _parse_manifest(member_manifest).package
        if actual != expected_package:
            violations.append(
                Violation(
                    _relative(root, member_manifest),
                    1,
                    "workspace-package",
                    f"expected package name {expected_package!r}, found {actual!r}",
                )
            )
    return violations


def check_manifests(root: Path) -> list[Violation]:
    violations: list[Violation] = []
    manifests = sorted(_walk_files(root, root / "crates"))
    root_manifest = root / "Cargo.toml"
    if root_manifest.is_file():
        manifests.insert(0, root_manifest)

    root_data = _parse_manifest(root_manifest) if root_manifest.is_file() else Manifest(None, (), 1, ())
    for manifest in manifests:
        if manifest.name != "Cargo.toml":
            continue
        data = _parse_manifest(manifest)
        text = _read_text(manifest)
        package_name = data.package
        if package_name in RETIRED_CRATES:
            violations.append(
                Violation(
                    _relative(root, manifest),
                    _manifest_line(text, "name"),
                    "retired-crate",
                    f"retired implementation package {package_name!r} must stay in git history",
                )
            )
        for dependency in data.dependencies:
            if dependency.name in RETIRED_CRATES or dependency.package in RETIRED_CRATES:
                violations.append(
                    Violation(
                        _relative(root, manifest),
                        dependency.line,
                        "retired-dependency",
                        f"dependency {dependency.package!r} belongs to the retired crate graph",
                    )
                )

    cli_manifest = root / "crates/tsz-cli/Cargo.toml"
    if cli_manifest.is_file():
        cli_data = _parse_manifest(cli_manifest)
        saw_core = False
        for dependency in cli_data.dependencies:
            package = dependency.package
            if dependency.workspace:
                package = _workspace_dependency_package(root_data, dependency.name)
            local_path = (
                (cli_manifest.parent / dependency.path).resolve() if dependency.path is not None else None
            )
            internal = package.startswith("tsz-") or dependency.name.startswith("tsz-")
            internal = internal or (local_path is not None and root.resolve() in local_path.parents)
            if not internal:
                continue
            if package == "tsz-core" or local_path == (root / "crates/tsz-core").resolve():
                saw_core = True
            else:
                violations.append(
                    Violation(
                        _relative(root, cli_manifest),
                        dependency.line,
                        "cli-boundary",
                        f"tsz-cli may consume tsz-core, not internal dependency {package!r}",
                    )
                )
        if not saw_core:
            violations.append(
                Violation(
                    _relative(root, cli_manifest),
                    1,
                    "cli-boundary",
                    "tsz-cli must depend directly on tsz-core",
                )
            )
    return violations


def check_rust_line_limits(root: Path) -> list[Violation]:
    violations: list[Violation] = []
    for relative_root in ACTIVE_RUST_ROOTS:
        for path in _walk_files(root, root / relative_root):
            if path.suffix != ".rs":
                continue
            try:
                line_count = _physical_lines(_read_text(path))
            except (OSError, UnicodeDecodeError) as error:
                violations.append(
                    Violation(_relative(root, path), 1, "rust-read", f"cannot count lines: {error}")
                )
                continue
            if line_count > 2_000:
                violations.append(
                    Violation(
                        _relative(root, path),
                        2_001,
                        "rust-file-size",
                        f"Rust source has {line_count} physical lines; maximum is 2000",
                    )
                )
    return violations


def check_rewrite_compiler_size_manifest(root: Path) -> list[Violation]:
    manifest_path = root / REWRITE_COMPILER_SIZE_MANIFEST_PATH
    guard_is_installed = (root / "scripts/arch/arch_guard.py").is_file()
    if not manifest_path.is_file() and not guard_is_installed:
        # Small unit-test repositories need not install the rewrite guard.
        return []
    try:
        rewrite_compiler_size(root)
    except (OSError, UnicodeDecodeError, ValueError) as error:
        return [
            Violation(
                REWRITE_COMPILER_SIZE_MANIFEST_PATH,
                1,
                "rewrite-compiler-size-manifest",
                str(error),
            )
        ]
    return []


def check_sound_mode(root: Path) -> list[Violation]:
    violations: list[Violation] = []
    paths: list[Path] = []
    for relative_root in SOUND_SCAN_ROOTS:
        paths.extend(_walk_files(root, root / relative_root))
    if (root / "Cargo.toml").is_file():
        paths.append(root / "Cargo.toml")

    for path in sorted(set(paths)):
        relative = _relative(root, path)
        if relative in SOUND_HISTORY_ALLOWLIST or path.suffix not in TEXT_SUFFIXES:
            continue
        try:
            text = _read_text(path)
        except (OSError, UnicodeDecodeError):
            continue
        for marker in SOUND_MARKERS:
            match = marker.search(text)
            if match:
                violations.append(
                    Violation(
                        relative,
                        text.count("\n", 0, match.start()) + 1,
                        "sound-mode",
                        "retired Sound Mode API/configuration marker remains",
                    )
                )
                break
    return violations


def _is_semantic_source(path: Path, source_root: Path) -> bool:
    relative = path.relative_to(source_root)
    words = {part.removesuffix(".rs").replace("-", "_") for part in relative.parts}
    return bool(words & SEMANTIC_PATH_WORDS)


def _mask_non_newlines(characters: list[str], start: int, end: int) -> None:
    for index in range(start, end):
        if characters[index] != "\n":
            characters[index] = " "


def _rust_code_and_string_literals(text: str) -> tuple[str, list[tuple[int, str]]]:
    """Return comment/string-masked Rust code and source string literals.

    This is deliberately a small lexical scan, not a Rust parser.  It handles
    nested block comments, ordinary strings, raw strings, and the character
    literals that could otherwise make a quote look like a string opener.
    Keeping offsets and newlines intact makes violations actionable.
    """

    characters = list(text)
    literals: list[tuple[int, str]] = []
    index = 0
    length = len(text)
    raw_start = re.compile(r'(?:br|r)(?P<hashes>#{0,255})"')

    while index < length:
        if text.startswith("//", index):
            end = text.find("\n", index + 2)
            end = length if end < 0 else end
            _mask_non_newlines(characters, index, end)
            index = end
            continue

        if text.startswith("/*", index):
            depth = 1
            end = index + 2
            while end < length and depth:
                if text.startswith("/*", end):
                    depth += 1
                    end += 2
                elif text.startswith("*/", end):
                    depth -= 1
                    end += 2
                else:
                    end += 1
            _mask_non_newlines(characters, index, end)
            index = end
            continue

        raw = raw_start.match(text, index)
        if raw and (index == 0 or not (text[index - 1].isalnum() or text[index - 1] == "_")):
            hashes = raw.group("hashes")
            content_start = raw.end()
            terminator = '"' + hashes
            content_end = text.find(terminator, content_start)
            end = length if content_end < 0 else content_end + len(terminator)
            literal_end = length if content_end < 0 else content_end
            literals.append((content_start, text[content_start:literal_end]))
            _mask_non_newlines(characters, index, end)
            index = end
            continue

        if text[index] == '"':
            content_start = index + 1
            end = content_start
            while end < length:
                if text[end] == "\\":
                    end += 2
                    continue
                if text[end] == '"':
                    break
                end += 1
            literal_end = min(end, length)
            literals.append((content_start, text[content_start:literal_end]))
            end = length if end >= length else end + 1
            _mask_non_newlines(characters, index, end)
            index = end
            continue

        if text[index] == "'":
            # Mask ordinary one-codepoint and escaped character literals.  A
            # lifetime such as 'a has no nearby closing quote and is retained.
            end = index + 1
            if end < length and text[end] == "\\":
                end += 1
                while end < min(length, index + 16) and text[end] != "'":
                    end += 1
            else:
                end += 1
            if end < length and text[end] == "'":
                end += 1
                _mask_non_newlines(characters, index, end)
                index = end
                continue

        index += 1

    return "".join(characters), literals


_CFG_TEST_MODULE = re.compile(
    r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]\s*"
    r"(?:#\s*\[[^]]*\]\s*)*"
    r"(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+[A-Za-z_][A-Za-z0-9_]*\s*"
)


def _without_cfg_test_modules(text: str) -> str:
    """Mask inline `cfg(test)` modules without hiding later production items.

    Keeping byte offsets and newlines stable lets every architecture check
    report the original line.  The brace walk uses comment/string-masked Rust,
    so nested test modules and braces inside fixtures cannot end the region
    early.  External `mod tests;` declarations are masked as well; their files
    are excluded separately by `_is_test_source`.
    """

    code, _ = _rust_code_and_string_literals(text)
    characters = list(text)
    search_from = 0
    while match := _CFG_TEST_MODULE.search(code, search_from):
        item_end = match.end()
        if item_end < len(code) and code[item_end] == ";":
            item_end += 1
        elif item_end < len(code) and code[item_end] == "{":
            depth = 1
            item_end += 1
            while item_end < len(code) and depth:
                if code[item_end] == "{":
                    depth += 1
                elif code[item_end] == "}":
                    depth -= 1
                item_end += 1
        else:
            # A malformed attribute/item is compiler-invalid.  Advance rather
            # than masking unrelated production code after it.
            search_from = match.end()
            continue
        _mask_non_newlines(characters, match.start(), item_end)
        search_from = item_end
    return "".join(characters)


def _is_test_source(path: Path, source_root: Path) -> bool:
    parts = {
        part.removesuffix(".rs").replace("-", "_")
        for part in path.relative_to(source_root).parts
    }
    return bool(parts & {"test", "tests", "fixtures"})


def check_ambient_behavior_switches(root: Path) -> list[Violation]:
    violations: list[Violation] = []
    for relative_root in ACTIVE_COMPILER_RUST_ROOTS:
        source_root = root / relative_root
        is_core = relative_root == "crates/tsz-core/src"
        for path in _walk_files(root, source_root):
            if path.suffix != ".rs" or _is_test_source(path, source_root):
                continue
            relative = _relative(root, path)
            text = _without_cfg_test_modules(_read_text(path))
            code, literals = _rust_code_and_string_literals(text)

            env_accesses = list(PROCESS_ENV_ACCESS.finditer(code))
            env_imports = [
                match
                for pattern in PROCESS_ENV_IMPORTS
                for match in pattern.finditer(code)
            ]
            if is_core:
                for match in [*env_accesses, *env_imports]:
                    violations.append(
                        Violation(
                            relative,
                            text.count("\n", 0, match.start()) + 1,
                            "ambient-env",
                            "tsz-core may not read or import process environment variables; "
                            "pass typed inputs through the service/program boundary",
                        )
                    )
            else:
                # Imports and aliases make the selected variable impossible to
                # audit with this deliberately small lexical guard.  Require a
                # direct call with one literal name in the sole telemetry owner.
                for match in env_imports:
                    violations.append(
                        Violation(
                            relative,
                            text.count("\n", 0, match.start()) + 1,
                            "ambient-env",
                            "tsz-cli process environment reads must be direct calls in "
                            f"{CLI_OBSERVABILITY_ENV_PATH}",
                        )
                    )
                for match in env_accesses:
                    call_start = match.end()
                    while call_start < len(code) and code[call_start].isspace():
                        call_start += 1
                    call_end = (
                        code.find(")", call_start + 1)
                        if call_start < len(code) and code[call_start] == "("
                        else -1
                    )
                    literal = next(
                        (
                            value
                            for start, value in literals
                            if call_end >= 0
                            and call_start < start < call_end
                            and code[call_start + 1 : start - 1].strip() == ""
                        ),
                        None,
                    )
                    if (
                        relative == CLI_OBSERVABILITY_ENV_PATH
                        and literal in CLI_OBSERVABILITY_ENV_NAMES
                    ):
                        continue
                    violations.append(
                        Violation(
                            relative,
                            text.count("\n", 0, match.start()) + 1,
                            "ambient-env",
                            "tsz-cli may read only literal TSZ_LOG, TSZ_LOG_FORMAT, or "
                            f"TSZ_PERF_COUNTERS in {CLI_OBSERVABILITY_ENV_PATH}",
                        )
                    )

            for literal_start, literal in literals:
                for match in TSZ_ENV_NAME.finditer(literal):
                    name = match.group(0)
                    if (
                        not is_core
                        and relative == CLI_OBSERVABILITY_ENV_PATH
                        and name in CLI_OBSERVABILITY_ENV_NAMES
                    ):
                        continue
                    violations.append(
                        Violation(
                            relative,
                            text.count("\n", 0, literal_start + match.start()) + 1,
                            "behavior-switch",
                            f"ambient compiler switch {name!r} is forbidden; "
                            "tsz-cli permits only observability-only logging/counter names",
                        )
                    )
    return violations


def check_semantic_hardcoding(root: Path) -> list[Violation]:
    violations: list[Violation] = []
    source_root = root / "crates/tsz-core/src"
    for path in _walk_files(root, source_root):
        if path.suffix != ".rs" or not _is_semantic_source(path, source_root):
            continue
        relative_parts = {
            part.removesuffix(".rs").replace("-", "_")
            for part in path.relative_to(source_root).parts
        }
        if relative_parts & {"test", "tests", "fixtures"}:
            continue
        text = _without_cfg_test_modules(_read_text(path))
        for code, pattern in HARDCODING_PATTERNS:
            for match in pattern.finditer(text):
                violations.append(
                    Violation(
                        _relative(root, path),
                        text.count("\n", 0, match.start()) + 1,
                        "semantic-hardcoding",
                        f"{code}: semantic decisions must use structure and stable identities",
                    )
                )
    return violations


def check_semantic_api_boundary(root: Path) -> list[Violation]:
    """Keep allocation-order semantic handles inside the owning core universe."""

    source_root = root / "crates/tsz-core/src"
    violations: list[Violation] = []
    public_module = re.compile(r"\bpub(?:\s*\([^)]*\))?\s+mod\s+semantics\b")
    any_use = re.compile(
        r"\b(?:pub(?:\s*\([^)]*\))?\s+)?use\b(?P<body>[^;]*);", re.DOTALL
    )
    any_type_alias = re.compile(
        r"\b(?:pub(?:\s*\([^)]*\))?\s+)?type\s+"
        r"(?P<name>[A-Za-z_][A-Za-z0-9_]*)\b[^=;]*=(?P<body>[^;]*);",
        re.DOTALL,
    )
    public_use = re.compile(r"\bpub(?:\s*\([^)]*\))?\s+use\b(?P<body>[^;]*);", re.DOTALL)
    public_signature = re.compile(
        r"\bpub(?:\s*\([^)]*\))?\s+"
        r"(?:type|fn|const|static|struct)\b(?P<body>[^;{]*)(?:;|\{)",
        re.DOTALL,
    )
    public_field = re.compile(
        r"\bpub(?:\s*\([^)]*\))?\s+[A-Za-z_][A-Za-z0-9_]*\s*:"
        r"(?P<body>[^,;}]+)"
    )
    semantic_handle = re.compile(r"\b(?:TypeId|TypeKind|TypeStore)\b")
    semantic_path = re.compile(r"\bsemantics\b")

    def introduced_names(body: str) -> set[str]:
        aliases = set(re.findall(r"\bas\s+([A-Za-z_][A-Za-z0-9_]*)\b", body))
        # A simple `use path::Item;` introduces its final segment. Braced trees
        # that rename an item are covered by `as`; direct handle leaves remain
        # visible to `semantic_handle` at the eventual public surface.
        if "{" not in body and "*" not in body and not aliases:
            identifiers = re.findall(r"[A-Za-z_][A-Za-z0-9_]*", body)
            if identifiers and identifiers[-1] not in {"crate", "self", "super"}:
                aliases.add(identifiers[-1])
        return aliases

    def mentions_tainted(body: str, tainted: set[str]) -> bool:
        return semantic_path.search(body) is not None or semantic_handle.search(body) is not None or any(
            re.search(rf"\b{re.escape(name)}\b", body) for name in tainted
        )

    for path in _walk_files(root, source_root):
        if path.suffix != ".rs" or _is_test_source(path, source_root):
            continue
        text = _without_cfg_test_modules(_read_text(path))
        code, _ = _rust_code_and_string_literals(text)
        use_matches = list(any_use.finditer(code))
        type_alias_matches = list(any_type_alias.finditer(code))
        tainted_names: set[str] = set()
        changed = True
        while changed:
            changed = False
            for use_match in use_matches:
                body = use_match.group("body")
                if not mentions_tainted(body, tainted_names):
                    continue
                before = len(tainted_names)
                tainted_names.update(introduced_names(body))
                changed = changed or len(tainted_names) != before
            for type_alias_match in type_alias_matches:
                if not mentions_tainted(type_alias_match.group("body"), tainted_names):
                    continue
                before = len(tainted_names)
                tainted_names.add(type_alias_match.group("name"))
                changed = changed or len(tainted_names) != before

        matches = list(public_module.finditer(code))
        matches.extend(
            match
            for match in public_use.finditer(code)
            if mentions_tainted(match.group("body"), tainted_names)
        )
        # Re-exporting through `pub type`, a public return type, or a public
        # field crosses the same universe boundary as `pub use`. Definitions
        # inside the deliberately private semantics module are not external
        # API, so only scan these surfaces outside that module.
        if "semantics" not in path.relative_to(source_root).parts:
            matches.extend(
                match
                for pattern in (public_signature, public_field)
                for match in pattern.finditer(code)
                if mentions_tainted(match.group("body"), tainted_names)
            )
        for match in matches:
            violations.append(
                Violation(
                    _relative(root, path),
                    text.count("\n", 0, match.start()) + 1,
                    "semantic-universe-api",
                    "semantic allocation handles must remain inside tsz-core; expose only "
                    "program/service value artifacts",
                )
            )
    return violations


ARCHITECTURE_RATCHET_PATH = "scripts/arch/rewrite-architecture-ratchet.json"


class ArchitectureMetricAnchorError(ValueError):
    """Raised when a lexical ratchet can no longer find its semantic owner."""


def _rust_struct_body(text: str, name: str) -> str:
    code, _ = _rust_code_and_string_literals(_without_cfg_test_modules(text))
    declaration = re.search(
        rf"\bstruct\s+{re.escape(name)}(?:\s*<[^{{;]+>)?\s*\{{",
        code,
    )
    if declaration is None:
        return ""
    start = declaration.end()
    depth = 1
    end = start
    while end < len(code) and depth:
        if code[end] == "{":
            depth += 1
        elif code[end] == "}":
            depth -= 1
        end += 1
    return code[start : end - 1] if depth == 0 else ""


def _field_count(body: str, type_pattern: str) -> int:
    return len(
        re.findall(
            rf"(?m)(?:^|,)\s*(?:pub(?:\([^)]*\))?\s+)?"
            rf"[A-Za-z_][A-Za-z0-9_]*\s*:\s*(?:{type_pattern})\b",
            body,
        )
    )


def _rust_method_call_arguments(text: str, names: tuple[str, ...]) -> list[str]:
    """Return balanced argument text for selected Rust method calls."""

    calls: list[str] = []
    pattern = re.compile(rf"\.(?:{'|'.join(map(re.escape, names))})\s*\(")
    for matched in pattern.finditer(text):
        start = matched.end()
        depth = 1
        end = start
        while end < len(text) and depth:
            if text[end] == "(":
                depth += 1
            elif text[end] == ")":
                depth -= 1
            end += 1
        if depth == 0:
            calls.append(text[start : end - 1])
    return calls


def _top_level_boolean_term_count(condition: str) -> int:
    """Count top-level `||` terms in already string/comment-stripped Rust."""

    depth = 0
    operators = 0
    index = 0
    while index < len(condition):
        character = condition[index]
        if character in "([{":
            depth += 1
        elif character in ")]}" and depth:
            depth -= 1
        elif character == "|" and condition[index : index + 2] == "||" and depth == 0:
            operators += 1
            index += 1
        index += 1
    return operators + 1 if condition.strip() else 0


def _required_metric_condition(
    text: str,
    pattern: str,
    metric: str,
) -> str:
    """Return one owner's condition or fail rather than reporting a false zero."""

    matches = list(re.finditer(pattern, text, re.DOTALL))
    if len(matches) != 1:
        raise ArchitectureMetricAnchorError(
            f"{metric} requires exactly one program owner anchor; found {len(matches)}"
        )
    return matches[0].group("condition")


def rewrite_architecture_metrics(root: Path) -> dict[str, int]:
    """Measure distributed rewrite ownership that must not grow.

    These are intentionally coarse ratchets, not semantic validators.  Their
    purpose is to force a review when a feature would add another mirrored
    capability flag, whole-program suppression term, force call/reset,
    recursion constructor, required-type prepass, checker collection, or line
    to an already near-cap central module.
    """

    source_root = root / "crates/tsz-core/src"
    production_parts: list[str] = []
    for path in _walk_files(root, source_root):
        if path.suffix != ".rs" or _is_test_source(path, source_root):
            continue
        text = _without_cfg_test_modules(_read_text(path))
        production_parts.append(_rust_code_and_string_literals(text)[0])
    production = "\n".join(production_parts)

    ast_text = _read_text(source_root / "syntax/ast.rs")
    modifiers_text = _read_text(source_root / "syntax/parser/modifiers.rs")
    checker_text = _read_text(source_root / "semantics/checker.rs")
    emit_paths_path = source_root / "emit_paths.rs"
    emit_paths_text = (
        _rust_code_and_string_literals(
            _without_cfg_test_modules(_read_text(emit_paths_path))
        )[0]
        if emit_paths_path.is_file()
        else ""
    )
    program_text = _rust_code_and_string_literals(
        _without_cfg_test_modules(_read_text(source_root / "program.rs"))
    )[0]

    source_unit = _rust_struct_body(ast_text, "SourceUnit")
    product_capabilities = _rust_struct_body(modifiers_text, "ProductCapabilities")
    checker = _rust_struct_body(checker_text, "Checker")
    emit_plan = _rust_struct_body(emit_paths_text, "EmitPlan")
    check_condition = _required_metric_condition(
        program_text,
        r"\blet\s+CheckResult\s*\{[^{}]*\}\s*=\s*if\s+"
        r"(?P<condition>options\.no_check[^{}]*)\s*\{",
        "program_whole_check_skip_terms",
    )
    completion_condition = _required_metric_condition(
        program_text,
        r"\bif\s+(?P<condition>[^{}]*"
        r"!capabilities\.semantic_diagnostics_are_claimed\s*\(\s*options\s*\)"
        r"[^{}]*)\s*\{\s*checker_completion\s*=\s*"
        r"checker_completion\.combine\s*\(\s*SemanticCompletion::Deferred\s*\)\s*;",
        "program_completion_gate_terms",
    )
    force_type_calls = _rust_method_call_arguments(production, ("force_type",))
    force_deferred_calls = _rust_method_call_arguments(production, ("force_deferred",))
    force_calls = force_type_calls + force_deferred_calls

    def physical_repo_lines(relative: str) -> int:
        path = root / relative
        return len(_read_text(path).splitlines()) if path.is_file() else 0

    def physical_lines(relative: str) -> int:
        return physical_repo_lines(f"crates/tsz-core/src/{relative}")

    try:
        compiler_rust_lines = rewrite_compiler_size(root).physical_lines
    except (OSError, UnicodeDecodeError, ValueError):
        # The manifest check reports the actionable error. Keeping a numeric
        # value here makes the ratchet fail closed without aborting the guard.
        compiler_rust_lines = 0

    return {
        "checker_collection_fields": _field_count(
            checker,
            r"(?:BTreeMap|BTreeSet|FxHashMap|FxHashSet|HashMap|HashSet|Vec)",
        ),
        "caller_depth_force_call_sites": sum(
            bool(
                re.search(
                    r",\s*depth(?:\s*\+\s*1)?\s*,?\s*$",
                    arguments,
                    re.DOTALL,
                )
            )
            for arguments in force_calls
        ),
        "capability_policy_mentions": len(
            re.findall(
                r"\b(?:functions|classes|declarations?|javascript|"
                r"[A-Za-z_][A-Za-z0-9_]*_(?:products|hosts|classes|declarations?|"
                r"completion|program_options|program_sources))"
                r"_supported\b",
                production,
            )
        ),
        "checker_rs_lines": physical_lines("semantics/checker.rs"),
        "config_rs_lines": physical_lines("config.rs"),
        "emit_plan_boolean_fields": _field_count(emit_plan, "bool"),
        "emit_plan_incomplete_assignments": len(
            re.findall(r"\bincomplete_products\s*=\s*true\b", emit_paths_text)
        ),
        "emit_plan_program_wide_promotions": len(
            re.findall(
                r"incomplete_file_products\.extend\s*\(\s*"
                r"files\.iter\(\)\.map\s*\(",
                emit_paths_text,
            )
        ),
        "emit_rs_lines": physical_lines("emit.rs"),
        "force_deferred_call_sites": len(force_deferred_calls),
        "force_type_call_sites": len(force_type_calls),
        "foundation_rewrite_test_lines": physical_repo_lines(
            "crates/tsz-core/rewrite-tests/foundation.rs"
        ),
        "parser_rs_lines": physical_lines("syntax/parser.rs"),
        "parser_product_capability_boolean_fields": _field_count(
            product_capabilities,
            "bool",
        ),
        "program_empty_check_result_sites": len(
            re.findall(
                r"CheckResult\s*\{[^{}]{0,400}?diagnostics:\s*Vec::new\(\)"
                r"[^{}]{0,400}?type_count:\s*0\b",
                program_text,
                re.DOTALL,
            )
        ),
        "program_whole_check_skip_terms": _top_level_boolean_term_count(
            check_condition
        ),
        "program_completion_deferred_assignments": len(
            re.findall(
                r"\b(?P<completion>[A-Za-z_][A-Za-z0-9_]*_completion)\s*=\s*"
                r"(?P=completion)\.combine\s*\("
                r"SemanticCompletion::Deferred\s*\)",
                program_text,
            )
        ),
        "program_completion_gate_terms": _top_level_boolean_term_count(
            completion_condition
        ),
        "r0_handwritten_compiler_rust_lines": compiler_rust_lines,
        "reference_stack_constructors": len(
            re.findall(r"\bReferenceExpansionStack::new\s*\(", production)
        ),
        "required_type_rs_lines": physical_lines(
            "semantics/checker/required_type.rs"
        ),
        "source_unit_boolean_fields": _field_count(source_unit, "bool"),
        "type_members_rewrite_test_lines": physical_repo_lines(
            "crates/tsz-core/rewrite-tests/type_members.rs"
        ),
        "unmodeled_policy_mentions": len(
            re.findall(r"\bhas_unmodeled_[A-Za-z_][A-Za-z0-9_]*\b", production)
        ),
        "whole_required_type_prepass_call_sites": len(
            re.findall(r"\.require_explicit_type_positions\s*\(", production)
        ),
        "zero_depth_force_call_sites": sum(
            bool(re.search(r",\s*0\s*,?\s*$", arguments, re.DOTALL))
            for arguments in force_calls
        ),
    }


def check_rewrite_architecture_ratchet(root: Path) -> list[Violation]:
    baseline_path = root / ARCHITECTURE_RATCHET_PATH
    if not baseline_path.is_file():
        # Small unit-test repositories do not carry the rewrite guard itself.
        if not (root / "scripts/arch/arch_guard.py").is_file():
            return []
        return [
            Violation(
                ARCHITECTURE_RATCHET_PATH,
                1,
                "architecture-ratchet",
                "rewrite architecture metric baseline is missing",
            )
        ]
    try:
        baseline = json.loads(_read_text(baseline_path))
    except (OSError, json.JSONDecodeError) as error:
        return [
            Violation(
                ARCHITECTURE_RATCHET_PATH,
                1,
                "architecture-ratchet",
                f"cannot read metric baseline: {error}",
            )
        ]
    if not isinstance(baseline, dict):
        return [
            Violation(
                ARCHITECTURE_RATCHET_PATH,
                1,
                "architecture-ratchet",
                "metric baseline must be a JSON object",
            )
        ]
    try:
        actual = rewrite_architecture_metrics(root)
    except ArchitectureMetricAnchorError as error:
        return [
            Violation(
                ARCHITECTURE_RATCHET_PATH,
                1,
                "architecture-ratchet",
                f"cannot measure rewrite ownership: {error}",
            )
        ]
    violations: list[Violation] = []
    if set(baseline) != set(actual):
        violations.append(
            Violation(
                ARCHITECTURE_RATCHET_PATH,
                1,
                "architecture-ratchet",
                "metric keys differ from the guard; regenerate the exact baseline",
            )
        )
        return violations
    for name, measured in actual.items():
        expected = baseline[name]
        if type(expected) is not int or expected != measured:
            direction = "grew" if type(expected) is int and measured > expected else "fell"
            action = (
                "consolidate an existing owner instead of raising the baseline"
                if direction == "grew"
                else "lower the baseline in the same consolidation change"
            )
            violations.append(
                Violation(
                    ARCHITECTURE_RATCHET_PATH,
                    1,
                    "architecture-ratchet",
                    f"{name} {direction}: baseline={expected!r}, actual={measured}; {action}",
                )
            )
    return violations


def check(root: Path) -> list[Violation]:
    checks: Iterable = (
        check_workspace,
        check_manifests,
        check_rust_line_limits,
        check_rewrite_compiler_size_manifest,
        check_sound_mode,
        check_ambient_behavior_switches,
        check_semantic_hardcoding,
        check_semantic_api_boundary,
        check_rewrite_architecture_ratchet,
    )
    violations: list[Violation] = []
    for checker in checks:
        violations.extend(checker(root))
    return sorted(set(violations))


def _parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
        help="repository root (defaults to the root containing this script)",
    )
    parser.add_argument(
        "--require-r0-ready",
        action="store_true",
        help="fail unless hand-written compiler Rust is below the R0 15000-line limit",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = _parse_args(sys.argv[1:] if argv is None else argv)
    root = args.root.resolve()
    violations = check(root)
    size: RewriteCompilerSize | None = None
    try:
        size = rewrite_compiler_size(root)
    except (OSError, UnicodeDecodeError, ValueError):
        # The regular manifest check already owns the actionable diagnostic.
        pass
    if size is not None:
        readiness = "READY" if size.r0_ready else "NOT READY"
        comparison = "<" if size.r0_ready else ">="
        print(
            f"R0 size readiness: {readiness} "
            f"({size.physical_lines} {comparison} {size.r0_physical_line_limit} "
            "hand-written compiler Rust physical lines)"
        )
        if args.require_r0_ready and not size.r0_ready:
            violations.append(
                Violation(
                    REWRITE_COMPILER_SIZE_MANIFEST_PATH,
                    1,
                    "r0-compiler-size",
                    f"R0 requires fewer than {size.r0_physical_line_limit} physical "
                    f"lines; measured {size.physical_lines}",
                )
            )
            violations = sorted(set(violations))
    else:
        print("R0 size readiness: UNAVAILABLE (size manifest is missing or invalid)")
        if args.require_r0_ready:
            violations.append(
                Violation(
                    REWRITE_COMPILER_SIZE_MANIFEST_PATH,
                    1,
                    "r0-compiler-size",
                    "strict R0 readiness requires a valid compiler size manifest",
                )
            )
            violations = sorted(set(violations))
    if violations:
        for violation in violations:
            print(violation.render())
        print(f"architecture guard: {len(violations)} violation(s)")
        return 1
    print("architecture guard: enforced checks pass; rewrite debt ratchet unchanged")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
