#!/usr/bin/env python3
"""Small, transparent architecture guard for the TSZ rewrite.

The reset intentionally has few architectural rules.  Keep each rule here
mechanical and actionable; semantic compatibility belongs in oracle tests.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
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


def _production_prefix(text: str) -> str:
    # Tests may assert exact messages and user spellings.  Only inspect the
    # production portion before a conventional inline cfg(test) module.
    marker = re.search(r"(?m)^\s*#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]\s*$", text)
    return text[: marker.start()] if marker else text


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
        text = _production_prefix(_read_text(path))
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


def check(root: Path) -> list[Violation]:
    checks: Iterable = (
        check_workspace,
        check_manifests,
        check_rust_line_limits,
        check_sound_mode,
        check_semantic_hardcoding,
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
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = _parse_args(sys.argv[1:] if argv is None else argv)
    root = args.root.resolve()
    violations = check(root)
    if violations:
        for violation in violations:
            print(violation.render())
        print(f"architecture guard: {len(violations)} violation(s)")
        return 1
    print("architecture guard: reset invariants pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
