import pathlib
import re
import argparse
import importlib.util
import json
import shlex
import subprocess
import sys
from collections import Counter
from pathlib import Path
from typing import BinaryIO, Iterable, Optional

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - exercised on Python < 3.11.
    tomllib = None

ROOT = pathlib.Path(__file__).resolve().parents[2]
POLICY_PATH = pathlib.Path(__file__).resolve().parent / "arch_guard_policy.toml"


def _strip_toml_comment(line: str) -> str:
    in_basic = False
    in_literal = False
    escaped = False

    for index, char in enumerate(line):
        if in_basic:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_basic = False
            continue
        if in_literal:
            if char == "'":
                in_literal = False
            continue
        if char == '"':
            in_basic = True
        elif char == "'":
            in_literal = True
        elif char == "#":
            return line[:index]
    return line


def _parse_toml_string(value: str) -> str:
    value = value.strip()
    if value.startswith("'") and value.endswith("'"):
        return value[1:-1]
    if value.startswith('"') and value.endswith('"'):
        return json.loads(value)
    raise ValueError(f"unsupported TOML string value: {value!r}")


def _parse_toml_string_array(lines: list[str]) -> list[str]:
    text = "\n".join(_strip_toml_comment(line) for line in lines)
    start = text.find("[")
    end = text.rfind("]")
    if start == -1 or end == -1 or end < start:
        raise ValueError(f"unsupported TOML array value: {text!r}")

    items: list[str] = []
    index = start + 1
    while index < end:
        while index < end and text[index] in " \t\r\n,":
            index += 1
        if index >= end:
            break

        quote = text[index]
        if quote not in {"'", '"'}:
            raise ValueError(f"unsupported TOML array item near: {text[index:end]!r}")
        index += 1
        item_start = index
        if quote == "'":
            while index < end and text[index] != "'":
                index += 1
            if index >= end:
                raise ValueError("unterminated TOML literal string in array")
            items.append(text[item_start:index])
            index += 1
            continue

        escaped = False
        buffer: list[str] = []
        while index < end:
            char = text[index]
            if escaped:
                buffer.append("\\" + char)
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                break
            else:
                buffer.append(char)
            index += 1
        if index >= end:
            raise ValueError("unterminated TOML basic string in array")
        items.append(json.loads(f'"{"".join(buffer)}"'))
        index += 1

    return items


def _parse_toml_value(value: str, array_lines: list[str]) -> object:
    value = _strip_toml_comment(value).strip()
    if value == "true":
        return True
    if value == "false":
        return False
    if value.startswith("["):
        return _parse_toml_string_array(array_lines)
    return _parse_toml_string(value)


def _parse_arch_guard_policy_toml(text: str) -> dict:
    """Parse the arch guard policy subset when stdlib `tomllib` is unavailable."""
    data: dict[str, list[dict]] = {"pattern_checks": [], "manifest_checks": []}
    current: Optional[dict] = None
    lines = text.splitlines()
    index = 0

    while index < len(lines):
        raw_line = lines[index]
        line = _strip_toml_comment(raw_line).strip()
        index += 1

        if not line:
            continue
        if line.startswith("[[") and line.endswith("]]"):
            table_name = line[2:-2].strip()
            if table_name not in data:
                raise ValueError(f"unsupported TOML table: {table_name!r}")
            current = {}
            data[table_name].append(current)
            continue
        if current is None or "=" not in line:
            raise ValueError(f"unsupported TOML line: {raw_line!r}")

        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip()
        array_lines = [raw_line]
        if value.startswith("[") and "]" not in _strip_toml_comment(value):
            while index < len(lines):
                array_lines.append(lines[index])
                if "]" in _strip_toml_comment(lines[index]):
                    index += 1
                    break
                index += 1
        current[key] = _parse_toml_value(value, array_lines)

    return data


def _load_policy_toml(file: BinaryIO) -> dict:
    if tomllib is not None:
        return tomllib.load(file)
    return _parse_arch_guard_policy_toml(file.read().decode("utf-8"))


def _build_excludes(entry: dict) -> dict:
    excludes: dict = {}
    if entry.get("exclude_dirs") is not None:
        excludes["exclude_dirs"] = set(entry["exclude_dirs"])
    if entry.get("exclude_files") is not None:
        excludes["exclude_files"] = set(entry["exclude_files"])
    if entry.get("exclude_test_files"):
        excludes["exclude_test_files"] = True
    if entry.get("ignore_comment_lines"):
        excludes["ignore_comment_lines"] = True
    return excludes


def _parse_pattern_checks(data: dict) -> list[tuple[str, pathlib.Path, re.Pattern, dict]]:
    return [
        (entry["name"], ROOT / entry["base"], re.compile(entry["pattern"]), _build_excludes(entry))
        for entry in data.get("pattern_checks", [])
    ]


def _parse_manifest_checks(data: dict) -> list[tuple[str, pathlib.Path, re.Pattern]]:
    return [
        (entry["name"], ROOT / entry["file"], re.compile(entry["pattern"], re.MULTILINE))
        for entry in data.get("manifest_checks", [])
    ]


def _load_pattern_checks(
    policy_path: pathlib.Path = POLICY_PATH,
) -> list[tuple[str, pathlib.Path, re.Pattern, dict]]:
    """Load [[pattern_checks]] entries from the declarative policy TOML."""
    with policy_path.open("rb") as f:
        return _parse_pattern_checks(_load_policy_toml(f))


def _load_manifest_checks(
    policy_path: pathlib.Path = POLICY_PATH,
) -> list[tuple[str, pathlib.Path, re.Pattern]]:
    """Load [[manifest_checks]] entries from the declarative policy TOML.

    Patterns are compiled with ``re.MULTILINE`` so ``^`` and ``$`` match
    at line boundaries within Cargo.toml files.
    """
    with policy_path.open("rb") as f:
        return _parse_manifest_checks(_load_policy_toml(f))


def _load_all_checks(
    policy_path: pathlib.Path = POLICY_PATH,
) -> tuple[list[tuple[str, pathlib.Path, re.Pattern, dict]], list[tuple[str, pathlib.Path, re.Pattern]]]:
    """Parse the policy TOML once and return both check lists."""
    with policy_path.open("rb") as f:
        data = _load_policy_toml(f)
    return _parse_pattern_checks(data), _parse_manifest_checks(data)


CHECKS, MANIFEST_CHECKS = _load_all_checks()

LINE_LIMIT_CHECKS = [
    (
        "Checker boundary: src files must stay under 2000 LOC",
        ROOT / "crates" / "tsz-checker" / "src",
        2000,
        # Exclusion list pruned 2026-05-01: removed 15 entries for files
        # that no longer exist (split or renamed) and 16 entries for files
        # that have since dropped below 2000 lines.
        #
        # Refreshed 2026-05-12: removed entries that had since dropped below
        # 2000 raw lines. The set below is the audited current set of files
        # above 2000 raw lines.
        {
            # ≥2000 LOC, real files. When a file drops below the limit,
            # delete it from this set in the same diff and the
            # `test_excluded_files_actually_exceed_limit` test will catch
            # any regression.
            "crates/tsz-checker/src/assignability/assignability_diagnostics.rs",
            "crates/tsz-checker/src/declarations/import/declaration.rs",
            "crates/tsz-checker/src/error_reporter/properties.rs",
            "crates/tsz-checker/src/flow/control_flow/core.rs",
            "crates/tsz-checker/src/jsdoc/diagnostics.rs",
            "crates/tsz-checker/src/jsdoc/params.rs",
            "crates/tsz-checker/src/state/state_checking/property.rs",
            "crates/tsz-checker/src/state/state_checking_members/interface_checks.rs",
            "crates/tsz-checker/src/state/type_analysis/core.rs",
            "crates/tsz-checker/src/state/type_environment/core.rs",
            "crates/tsz-checker/src/state/type_resolution/module.rs",
            "crates/tsz-checker/src/state/variable_checking/core.rs",
            "crates/tsz-checker/src/state/variable_checking/destructuring.rs",
            "crates/tsz-checker/src/types/class_type/constructor.rs",
            "crates/tsz-checker/src/types/property_access_type/resolve.rs",
            "crates/tsz-checker/src/types/type_checking/duplicate_identifiers.rs",
            "crates/tsz-checker/src/types/type_checking/duplicate_identifiers_helpers.rs",
            "crates/tsz-checker/src/types/utilities/core.rs",
        },
    ),
    (
        "Checker computation boundary: type-computation monoliths must stay below 3100 LOC (#8226)",
        ROOT / "crates" / "tsz-checker" / "src" / "types" / "computation",
        3100,
    ),
]

FILE_LINE_LIMIT_CHECKS = [
    (
        "Core boundary: tsz-core lib facade must stay at current 365 LOC baseline",
        ROOT / "crates" / "tsz-core" / "src" / "lib.rs",
        365,
    ),
    (
        "Checker query boundary: common quarantine must not grow (#8225)",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "query_boundaries"
        / "common.rs",
        1920,
    ),
    (
        "Solver engine boundary: generic call resolver must stay at current 3381 LOC baseline (#8209)",
        ROOT
        / "crates"
        / "tsz-solver"
        / "src"
        / "operations"
        / "generic_call"
        / "resolve.rs",
        3381,
    ),
    # Pin the async ES5 IR transformer file size while #8277 splits the
    # monolith into staged lowering modules. The cap should ratchet down
    # as more phases (helper scheduling, temp/hoist planning, suspended
    # target lowering, ...) are extracted into sibling submodules.
    # Ratcheted 5150→4918 after submodule extraction reduced the core engine.
    (
        "Emitter boundary: async ES5 IR engine size ratchet (#8277)",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "transforms"
        / "async_es5_ir.rs",
        4918,
    ),
    # Emitter ES decorators: PR #10778 tracks sharding into 7 focused submodules.
    # Ratchet down as submodules land.
    (
        "Emitter boundary: es_decorators monolith size ratchet (#10778)",
        ROOT / "crates" / "tsz-emitter" / "src" / "transforms" / "es_decorators.rs",
        5755,
    ),
    # Config monolith: tsconfig/compiler-options parser. Issue #8280 tracks
    # splitting into option-domain submodules. Ratchet down as each domain lands.
    (
        "Core boundary: tsconfig/config monolith size ratchet (#8280)",
        ROOT / "crates" / "tsz-core" / "src" / "config" / "mod.rs",
        8206,
    ),
    # LSP signature-help: carries TypeData and direct lookup() baseline debt
    # (see arch_guard_policy.toml exclusions). Ratchet down per §19 splitting
    # and arch-debt burn-down in Track 10.
    (
        "LSP boundary: signature_help monolith size ratchet",
        ROOT / "crates" / "tsz-lsp" / "src" / "signature_help.rs",
        4808,
    ),
    # Scanner main loop: issue #9431 tracks splitting by token family.
    (
        "Scanner boundary: scanner_impl monolith size ratchet (#9431)",
        ROOT / "crates" / "tsz-scanner" / "src" / "scanner_impl.rs",
        4190,
    ),
    # CLI driver resolution: split into discovery/exports_imports/package_resolution/
    # path_resolution/type_packages submodules; ratchet holds the orchestrator at 301.
    (
        "CLI boundary: driver/resolution monolith size ratchet",
        ROOT / "crates" / "tsz-cli" / "src" / "driver" / "resolution.rs",
        301,
    ),
    # Emitter class declarations: split by emit feature family per §19.
    (
        "Emitter boundary: class declaration emitter size ratchet",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "emitter"
        / "declarations"
        / "class"
        / "emit_es6.rs",
        4191,
    ),
    # CLI driver check-utils: ProgramData construction. Issue #9412 tracks
    # extracting the source-resolution phase.
    (
        "CLI boundary: driver/check_utils monolith size ratchet (#9412)",
        ROOT / "crates" / "tsz-cli" / "src" / "driver" / "check_utils.rs",
        3949,
    ),
    # LSP module-specifier resolution: split by resolution family per §19.
    (
        "LSP boundary: module_specifiers monolith size ratchet",
        ROOT / "crates" / "tsz-lsp" / "src" / "project" / "module_specifiers.rs",
        3669,
    ),
    # LSP import candidate collection: issue #9420 tracks splitting collection
    # from ranking and rendering.
    (
        "LSP boundary: project/imports monolith size ratchet (#9420)",
        ROOT / "crates" / "tsz-lsp" / "src" / "project" / "imports.rs",
        3384,
    ),
    # Binder declaration binding: split by declaration family per §19.
    (
        "Binder boundary: binder/declaration monolith size ratchet",
        ROOT / "crates" / "tsz-binder" / "src" / "binding" / "declaration.rs",
        3038,
    ),
    # Emitter class ES5 AST-to-IR: issue #10638 tracks splitting alongside
    # async_es5_ir.rs. Partially split (comments/control-flow/expressions/for-in-of
    # submodules already extracted); ratchet holds orchestrator at 1869.
    (
        "Emitter boundary: class ES5 AST-to-IR engine size ratchet (#10638)",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "transforms"
        / "class_es5_ast_to_ir.rs",
        1869,
    ),
    # CLI LSP server: completions handler — split by completion kind per §19.
    (
        "CLI LSP server: handlers_completions monolith size ratchet",
        ROOT
        / "crates"
        / "tsz-cli"
        / "src"
        / "bin"
        / "tsz_server"
        / "handlers_completions.rs",
        3577,
    ),
    # CLI main binary: split by command family per §19.
    (
        "CLI boundary: tsz main binary size ratchet",
        ROOT / "crates" / "tsz-cli" / "src" / "bin" / "tsz.rs",
        3573,
    ),
    # CLI driver core: orchestrates check/emit/resolve pipeline. Ratchet down
    # as pipeline stages are extracted per §19.
    (
        "CLI boundary: driver/core monolith size ratchet",
        ROOT / "crates" / "tsz-cli" / "src" / "driver" / "core.rs",
        3215,
    ),
    # CLI LSP server: structure/outline handler — split by request kind per §19.
    (
        "CLI LSP server: handlers_structure monolith size ratchet",
        ROOT
        / "crates"
        / "tsz-cli"
        / "src"
        / "bin"
        / "tsz_server"
        / "handlers_structure.rs",
        3075,
    ),
    # CLI LSP server: hover/signature/semantic handler — split by feature per §19.
    (
        "CLI LSP server: handlers_info monolith size ratchet",
        ROOT
        / "crates"
        / "tsz-cli"
        / "src"
        / "bin"
        / "tsz_server"
        / "handlers_info.rs",
        2881,
    ),
    # CLI LSP server: editing/refactor handler — split by action family per §19.
    (
        "CLI LSP server: handlers_editing monolith size ratchet",
        ROOT
        / "crates"
        / "tsz-cli"
        / "src"
        / "bin"
        / "tsz_server"
        / "handlers_editing.rs",
        2332,
    ),
    # LSP project core: orchestrates multi-file state. Ratchet down as file
    # management is delegated to ProjectFileSet/CompilationGroup per §19.
    (
        "LSP boundary: project/core monolith size ratchet",
        ROOT / "crates" / "tsz-lsp" / "src" / "project" / "core.rs",
        2916,
    ),
    # LSP fourslash: language-service test protocol runner. Ratchet down as
    # test helpers are extracted into focused sub-modules per §19.
    (
        "LSP boundary: fourslash test protocol size ratchet",
        ROOT / "crates" / "tsz-lsp" / "src" / "fourslash.rs",
        2268,
    ),
    # Emitter DTS portability resolver: split by portability family per §19.
    (
        "Emitter boundary: declaration_emitter/helpers/portability_resolve size ratchet",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "declaration_emitter"
        / "helpers"
        / "portability_resolve.rs",
        3178,
    ),
    # Emitter DTS type-inference helper: issue #8276 tracks migrating inference
    # output to structured declaration summary facts.
    (
        "Emitter boundary: declaration_emitter/helpers/type_inference size ratchet (#8276)",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "declaration_emitter"
        / "helpers"
        / "type_inference.rs",
        2846,
    ),
    # Emitter using/disposable region: issue #8276 tracks migrating the 16
    # output-surgery rewrites to structured resource-region IR.
    (
        "Emitter boundary: source_file/top_level_using size ratchet (#8276)",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "emitter"
        / "source_file"
        / "top_level_using.rs",
        2537,
    ),
    # Emitter property/element access: split by access kind per §19.
    (
        "Emitter boundary: emitter/expressions/access size ratchet",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "emitter"
        / "expressions"
        / "access.rs",
        2554,
    ),
    # --- Blanket coverage batch: all production files > 2000 lines per §19 ---
    # These entries pin the current baseline and prevent silent growth.
    # Each file is a candidate for splitting; ratchet down as submodules land.
    (
        "Checker boundary: types/property_access_type/resolve.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "types"
        / "property_access_type"
        / "resolve.rs",
        3152,
    ),
    (
        "Checker boundary: types/type_checking/duplicate_identifiers_helpers.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "types"
        / "type_checking"
        / "duplicate_identifiers_helpers.rs",
        3150,
    ),
    (
        "Checker boundary: error_reporter/properties.rs size ratchet",
        ROOT / "crates" / "tsz-checker" / "src" / "error_reporter" / "properties.rs",
        3107,
    ),
    (
        "Checker boundary: declarations/import/declaration.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "declarations"
        / "import"
        / "declaration.rs",
        3066,
    ),
    (
        "Checker boundary: state/state_checking/property.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "state"
        / "state_checking"
        / "property.rs",
        3036,
    ),
    (
        "Parser boundary: parser/state_expressions_literals.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-parser"
        / "src"
        / "parser"
        / "state_expressions_literals.rs",
        3027,
    ),
    (
        "Checker boundary: jsdoc/params.rs size ratchet",
        ROOT / "crates" / "tsz-checker" / "src" / "jsdoc" / "params.rs",
        2941,
    ),
    (
        "Checker boundary: types/type_checking/duplicate_identifiers.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "types"
        / "type_checking"
        / "duplicate_identifiers.rs",
        2916,
    ),
    (
        "Checker boundary: flow/control_flow/core.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "flow"
        / "control_flow"
        / "core.rs",
        2886,
    ),
    (
        "Solver boundary: type_queries/flow.rs size ratchet",
        ROOT / "crates" / "tsz-solver" / "src" / "type_queries" / "flow.rs",
        2874,
    ),
    (
        "Checker boundary: types/utilities/core.rs size ratchet",
        ROOT / "crates" / "tsz-checker" / "src" / "types" / "utilities" / "core.rs",
        2779,
    ),
    (
        "Checker boundary: state/type_analysis/core.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "state"
        / "type_analysis"
        / "core.rs",
        2764,
    ),
    (
        "CLI boundary: driver/tests.rs size ratchet",
        ROOT / "crates" / "tsz-cli" / "src" / "driver" / "tests.rs",
        2736,
    ),
    (
        "Checker boundary: types/class_type/constructor.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "types"
        / "class_type"
        / "constructor.rs",
        2688,
    ),
    (
        "Solver boundary: diagnostics/format/compound.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-solver"
        / "src"
        / "diagnostics"
        / "format"
        / "compound.rs",
        2602,
    ),
    (
        "Checker boundary: assignability/assignability_diagnostics.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "assignability"
        / "assignability_diagnostics.rs",
        2600,
    ),
    (
        "Checker boundary: state/type_resolution/module.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "state"
        / "type_resolution"
        / "module.rs",
        2596,
    ),
    (
        "Parser boundary: parser/state_statements_class_members.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-parser"
        / "src"
        / "parser"
        / "state_statements_class_members.rs",
        2587,
    ),
    (
        "Checker boundary: state/type_environment/core.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "state"
        / "type_environment"
        / "core.rs",
        2568,
    ),
    (
        "Conformance boundary: conformance runner size ratchet",
        ROOT / "crates" / "conformance" / "src" / "runner.rs",
        2485,
    ),
    (
        "Emitter boundary: emitter/module_emission/core/mod.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "emitter"
        / "module_emission"
        / "core"
        / "mod.rs",
        2484,
    ),
    (
        "Emitter boundary: emitter/source_file/emit.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "emitter"
        / "source_file"
        / "emit.rs",
        2462,
    ),
    (
        "Checker boundary: jsdoc/diagnostics.rs size ratchet",
        ROOT / "crates" / "tsz-checker" / "src" / "jsdoc" / "diagnostics.rs",
        2437,
    ),
    (
        "Checker boundary: state/variable_checking/destructuring.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "state"
        / "variable_checking"
        / "destructuring.rs",
        2250,
    ),
    (
        "Checker boundary: state/state_checking_members/interface_checks.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "state"
        / "state_checking_members"
        / "interface_checks.rs",
        2250,
    ),
    (
        "Solver boundary: operations/constraints/walker.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-solver"
        / "src"
        / "operations"
        / "constraints"
        / "walker.rs",
        2230,
    ),
    (
        "Emitter boundary: emitter/es5/helpers_async.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "emitter"
        / "es5"
        / "helpers_async.rs",
        2224,
    ),
    (
        "Checker boundary: state/variable_checking/core.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "state"
        / "variable_checking"
        / "core.rs",
        2207,
    ),
    (
        "Emitter boundary: emitter/helpers.rs size ratchet",
        ROOT / "crates" / "tsz-emitter" / "src" / "emitter" / "helpers.rs",
        2202,
    ),
    (
        "Emitter boundary: declaration_emitter/usage_analyzer.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "declaration_emitter"
        / "usage_analyzer.rs",
        2154,
    ),
    (
        "Emitter boundary: emitter/transform_dispatch.rs size ratchet",
        ROOT / "crates" / "tsz-emitter" / "src" / "emitter" / "transform_dispatch.rs",
        2124,
    ),
    (
        "Solver boundary: visitors/visitor_predicates.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-solver"
        / "src"
        / "visitors"
        / "visitor_predicates.rs",
        2123,
    ),
    (
        "Solver boundary: operations/call_args.rs size ratchet",
        ROOT / "crates" / "tsz-solver" / "src" / "operations" / "call_args.rs",
        2122,
    ),
    (
        "LSP boundary: navigation/definition.rs size ratchet",
        ROOT / "crates" / "tsz-lsp" / "src" / "navigation" / "definition.rs",
        2121,
    ),
    (
        "Solver boundary: relations/subtype/rules/functions/checking.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-solver"
        / "src"
        / "relations"
        / "subtype"
        / "rules"
        / "functions"
        / "checking.rs",
        2118,
    ),
    (
        "LSP boundary: completions/member.rs size ratchet",
        ROOT / "crates" / "tsz-lsp" / "src" / "completions" / "member.rs",
        2117,
    ),
    (
        "Emitter boundary: emitter/module_wrapper/system_emit.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "emitter"
        / "module_wrapper"
        / "system_emit.rs",
        2093,
    ),
    (
        "LSP boundary: hierarchy/call_hierarchy.rs size ratchet",
        ROOT / "crates" / "tsz-lsp" / "src" / "hierarchy" / "call_hierarchy.rs",
        2091,
    ),
    (
        "Solver boundary: intern/core/interner.rs size ratchet",
        ROOT / "crates" / "tsz-solver" / "src" / "intern" / "core" / "interner.rs",
        2086,
    ),
    (
        "CLI boundary: bin/tsz_server/tests_navigation.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-cli"
        / "src"
        / "bin"
        / "tsz_server"
        / "tests_navigation.rs",
        2044,
    ),
    (
        "Solver boundary: operations/core/call_resolution.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-solver"
        / "src"
        / "operations"
        / "core"
        / "call_resolution.rs",
        2031,
    ),
    (
        "LSP boundary: hover/core.rs size ratchet",
        ROOT / "crates" / "tsz-lsp" / "src" / "hover" / "core.rs",
        2029,
    ),
    (
        "Emitter boundary: emitter/statements/control_flow.rs size ratchet",
        ROOT
        / "crates"
        / "tsz-emitter"
        / "src"
        / "emitter"
        / "statements"
        / "control_flow.rs",
        2029,
    ),
    (
        "Solver boundary: evaluation/evaluate.rs size ratchet",
        ROOT / "crates" / "tsz-solver" / "src" / "evaluation" / "evaluate.rs",
        2019,
    ),
    (
        "Emitter boundary: transforms/module_commonjs.rs size ratchet",
        ROOT / "crates" / "tsz-emitter" / "src" / "transforms" / "module_commonjs.rs",
        2016,
    ),
]

# Pin field counts on giant coordination structs so workstream-4 (Checker
# State / Speculation) extraction work shows up as visible metric drift in
# the diff.  Each entry: (description, file_path, struct_name, max_fields).
#
# When a field is added intentionally, bump the cap in the same PR.  This is
# the same convention as `FILE_LINE_LIMIT_CHECKS` — it makes architecture
# health metric drift visible at review time (Operating Principle 8 in
# `docs/plan/ROADMAP.md`).
STRUCT_FIELD_COUNT_CHECKS = [
    (
        "Checker boundary: CheckerContext field count (architecture health metric 1)",
        ROOT / "crates" / "tsz-checker" / "src" / "context" / "mod.rs",
        "CheckerContext",
        239,
    ),
]

# Pin the size of the solver's full database capability trait while #8205
# splits it into narrower storage/config/provenance traits.  The live count is
# tolerated as baseline debt, but new methods must either land on a narrower
# trait or deliberately bump this cap with a roadmap/issue explanation.
#
# Each entry: (description, file_path, trait_name, max_methods).
TRAIT_METHOD_COUNT_CHECKS = [
    (
        "Solver boundary: TypeDatabase method count (#8205)",
        ROOT / "crates" / "tsz-solver" / "src" / "caches" / "db.rs",
        "TypeDatabase",
        77,
    ),
]

VALID_CHECKER_CONTEXT_LIFETIMES = {
    "ProgramStable",
    "WorkerReusable",
    "FileLocalReset",
    "SpeculationScoped",
    "DiagnosticsOnly",
    "LspPersistent",
}

VALID_CHECKER_CONTEXT_CAPABILITIES = {
    "CheckerInputs",
    "DiagnosticState",
    "EmitSummaryState",
    "FileTypeCache",
    "FlowSessionState",
    "ProgramLookupContext",
    "RelationSessionState",
    "SpeculationState",
}

CHECKER_CONTEXT_LIFETIME_MANIFEST_CHECKS = [
    (
        "Checker boundary: CheckerContext lifetime inventory (T2.1.A)",
        ROOT / "crates" / "tsz-checker" / "src" / "context" / "mod.rs",
        "CheckerContext",
        ROOT
        / "crates"
        / "tsz-checker"
        / "src"
        / "context"
        / "checker_context_lifetimes.toml",
    ),
]

# Pin the count of files that construct full independent parse→bind→check
# pipelines (architecture health metric 4 in `docs/plan/ROADMAP.md`).  A
# "full pipeline" is any non-test source file that calls all three of
# `ParserState::new`, `BinderState::new`, `CheckerState::new` — that is, a
# frontend reaching past the compiler service into the raw crate APIs.
#
# Workstream 3 ("Compiler Service Front Door") exit criterion is "There is
# one blessed parse-bind-check path."  Pinning the count makes new
# independent pipelines fail pre-commit and consolidation work show up as
# a cap reduction in the same diff.
#
# Each entry: (description, search_roots, max_pipelines).
INDEPENDENT_PIPELINE_CHECKS = [
    (
        "Frontend boundary: independent parse-bind-check pipelines (architecture health metric 4)",
        [
            ROOT / "crates" / "tsz-cli" / "src",
            ROOT / "crates" / "tsz-core" / "src",
            ROOT / "crates" / "tsz-lsp" / "src",
            ROOT / "crates" / "tsz-wasm" / "src",
        ],
        4,
    ),
]

# Pin the count of non-test source files that import `tsz_solver` outside the
# solver/checker boundary (architecture health metric 7 in
# `docs/plan/ROADMAP.md`).  The checker crate contains the canonical
# `query_boundaries` modules and is the one architecturally allowed consumer
# of solver internals; every other crate (`tsz-cli`, `tsz-core`, `tsz-lsp`,
# `tsz-wasm`, `tsz-emitter`, `tsz-lowering`) reaching directly into the solver
# weakens the front door story (workstream 3) and shows up as drift on this
# metric.
#
# A file "imports tsz_solver" if a non-comment line contains one of:
#   - `use tsz_solver::...`
#   - `pub use tsz_solver` (re-export, including `pub use tsz_solver;`)
#   - `extern crate tsz_solver`
#
# Each entry: (description, search_roots, exclude_path_prefixes, max_imports).
SOLVER_IMPORT_COUNT_CHECKS = [
    (
        "Frontend/emitter boundary: direct tsz_solver imports outside solver/checker (architecture health metric 7)",
        [ROOT / "crates"],
        (
            "crates/tsz-solver/",
            "crates/tsz-checker/",
        ),
        36,
    ),
]

# Pin the count of flat root-level solver computation API references outside
# the approved checker query-boundary layer. Existing references are
# transitional compatibility debt from `tsz_solver::*` root re-exports; new
# references should go through a named solver facade, a checker
# `query_boundaries` helper, or intentionally bump this cap.
#
# Each entry:
#   (description, search_roots, exclude_path_prefixes, max_references).
ROOT_SOLVER_COMPUTATION_IMPORT_COUNT_CHECKS = [
    (
        "Solver API boundary: flat root computation imports outside query boundaries (#8204)",
        [
            ROOT / "crates" / "tsz-checker" / "src",
            ROOT / "crates" / "tsz-emitter" / "src",
            ROOT / "crates" / "tsz-lsp" / "src",
            ROOT / "crates" / "tsz-cli" / "src",
        ],
        ("crates/tsz-checker/src/query_boundaries/",),
        0,
    ),
]

# Pin the producer-side compatibility surface that still re-exports solver
# computation/construction APIs from the crate root. The zero wildcard guard
# below prevents broad `pub use module::*` growth; this count makes explicit
# root re-export growth visible too.
#
# Each entry:
#   (description, file_path, root_module_prefixes, max_reexports).
ROOT_SOLVER_EXPLICIT_REEXPORT_COUNT_CHECKS = [
    (
        "Solver API boundary: flat root explicit computation re-exports (#8204)",
        ROOT / "crates" / "tsz-solver" / "src" / "lib.rs",
        (
            "caches",
            "canonicalize",
            "classes",
            "contextual",
            "evaluation",
            "instantiation",
            "intern",
            "narrowing",
            "objects",
            "operations",
            "relations",
            "widening",
        ),
        0,
    ),
]

# Pin direct checker call sites into `query_boundaries::common`, the broad
# compatibility/quarantine barrel tracked by #8225. Existing sites are
# tolerated as migration debt; new checker code should prefer a narrower
# request-shaped boundary module, or intentionally bump this cap.
#
# Each entry:
#   (description, search_roots, exclude_path_prefixes, max_references).
QUERY_BOUNDARY_COMMON_REFERENCE_COUNT_CHECKS = [
    (
        "Checker query boundary: direct common quarantine references outside query_boundaries (#8225)",
        [ROOT / "crates" / "tsz-checker" / "src"],
        ("crates/tsz-checker/src/query_boundaries/",),
        # Bumped by 2 for the deferred-conditional diagnostic-display fix
        # (`is_conditional_type` guards in the assignment-target display path,
        # matching the existing direct-call pattern in type_display.rs).
        #
        # Ratcheted down by 5 after literal alias / literal widening
        # diagnostic display probes moved through query_boundaries::diagnostics.
        #
        # Ratcheted down by 14 after branch refresh removed stale direct
        # common references.
        #
        # Ratcheted down by 1 during the #9281 current-main refresh after
        # the split guard tests caught slack in the live count.
        #
        # Ratcheted down by 1 after the interface heritage `this`-type helper
        # moved to `query_boundaries::type_predicates`.
        #
        # Ratcheted down by 8 after rebasing on main removed additional direct
        # common references.
        #
        # Refreshed #9852 on current main for contextual-wrapper excess-property
        # diagnostics; this records the merged live count.
        #
        # Ratcheted down after current-main guard tests caught slack in the
        # live direct-reference count.
        #
        # Ratcheted down to the live merged count after #10311 and #10359
        # narrowed checker-side direct common references; removal condition
        # remains #8225 narrowing this quarantine.
        #
        # Ratcheted down after current-main guard tests caught slack in the
        # live direct-reference count.
        #
        # Ratcheted down after arch-smoke caught current stacked-branch slack.
        3266,
    ),
]

# Pin root-level lint allowance entries in the query-boundary module map. #8225
# tracks turning this layer from migration quarantine into narrower APIs, and
# broad module-level allowances are part of that quarantine debt. The cap should
# ratchet down as modules no longer need blanket suppressions.
QUERY_BOUNDARY_MODULE_ALLOWANCE_COUNT_CHECKS = [
    (
        "Checker query boundary: module-level lint allowances must not grow (#8225)",
        ROOT / "crates" / "tsz-checker" / "src" / "query_boundaries" / "mod.rs",
        0,
    ),
]

WORKSPACE_CLIPPY_ALLOW_COUNT_CHECKS = [
    (
        "Workspace Clippy suppressions must not grow (#9446)",
        [ROOT / "crates"],
        10,
    ),
]

SNAPSHOT_ROLLBACK_FILE_COUNT_CHECKS = [
    (
        "Checker speculation boundary: snapshot-rollback call sites outside speculation.rs (architecture health metric 5)",
        [ROOT / "crates" / "tsz-checker" / "src"],
        ("crates/tsz-checker/src/context/speculation.rs",),
        6,
    ),
]

# Pin architecture health metric 6 ("Speculation APIs with surprising
# non-RAII behavior") in `docs/plan/ROADMAP.md`.
#
# After PR #1213 renamed `DiagnosticSpeculationGuard → DiagnosticSpeculationSnapshot`
# the speculation surface no longer carries `…Guard` types whose name
# implies RAII rollback-on-drop while the implementation is implicit-commit.
# This guard pins the rename: any new `pub(crate) struct …Guard` on the
# speculation surface re-introduces the same ambiguity and must update
# the cap (deliberately) or use a `…Snapshot` name (preferred). The
# scan looks at the speculation file directly so the check is local
# and cheap.
#
# Each entry: (description, file_path, max_guard_struct_count).
SPECULATION_GUARD_NAME_CHECKS = [
    (
        "Checker speculation boundary: number of `…Guard` structs in speculation.rs (architecture health metric 6)",
        ROOT / "crates" / "tsz-checker" / "src" / "context" / "speculation.rs",
        0,
    ),
]

DEBUG_PRINT_REPORT_PATH = ROOT / "scripts" / "perf" / "debug-print-report.py"
DEBUG_PRINT_MACRO_CHECKS = [
    (
        "Performance boundary: compiler-internal debug print macros (Track 10)",
        ROOT,
        (
            "crates/tsz-binder/src",
            "crates/tsz-checker/src",
            "crates/tsz-common/src",
            "crates/tsz-core/src",
            "crates/tsz-emitter/src",
            "crates/tsz-lowering/src",
            "crates/tsz-parser/src",
            "crates/tsz-scanner/src",
            "crates/tsz-solver/src",
        ),
    ),
]

# Pin Track 10's diagnostic-debt ratchets in the shared architecture guard.
# These are count metrics, not new semantic bans: the current baselines still
# contain legacy fingerprint rewrites, source-text snippets, and rendered-type
# decisions. Any new line must bump the cap intentionally; cleanup PRs should
# lower the cap in the same diff.
#
# Each entry: (description, search_roots, pattern, max_lines).
REGEX_LINE_COUNT_CHECKS = [
    (
        "Checker diagnostic boundary: post-check rewrite_*_fingerprints functions (Track 10)",
        [ROOT / "crates" / "tsz-checker" / "src"],
        re.compile(r"^\s*fn\s+rewrite_\w+_fingerprints\s*\("),
        7,
    ),
    (
        "Checker diagnostic boundary: source_text.contains decisions (Track 10)",
        [ROOT / "crates" / "tsz-checker" / "src"],
        re.compile(r"\bsource_text\.contains\s*\("),
        25,
    ),
    (
        "Checker diagnostic boundary: file-name/path substring decisions (Track 10)",
        [ROOT / "crates" / "tsz-checker" / "src"],
        re.compile(r"\b(?:\w+\.)?file_name\.contains\s*\(|\bsource_path\.contains\s*\("),
        1,
    ),
    (
        "Checker diagnostic boundary: rendered type strings as semantic input (Track 10)",
        [ROOT / "crates" / "tsz-checker" / "src"],
        re.compile(
            r"\bformat_type(?:_diagnostic)?\s*\([^\n]*"
            r"(?:\.contains\s*\(|\.starts_with\s*\(|\.ends_with\s*\(|\.as_str\s*\(\))"
        ),
        0,
    ),
    (
        "Checker diagnostic boundary: rendered message predicates (Track 10)",
        [
            ROOT / "crates" / "tsz-checker" / "src" / "checkers" / "jsx",
            ROOT / "crates" / "tsz-checker" / "src" / "checkers" / "call_checker",
            ROOT / "crates" / "tsz-checker" / "src" / "types" / "type_checking",
        ],
        re.compile(
            r"\b(?:display|source_display|target_display|stripped_display|"
            r"diagnostic\.message_text|raw|evaluated)"
            r"\.(?:contains|starts_with|ends_with|as_str)\s*\("
        ),
        14,
    ),
    (
        # Ratcheted from 3→1: two calls removed (bang-module and mixin-intersection
        # decisions migrated to structured AST facts in #8406 / #8276 cycle).
        # Remaining call: variable_decl.rs intersection-arm detection; issue #8276
        # tracks migrating it to a structured declaration summary.
        "Emitter boundary: source_text.contains recovery decisions (Track 9/10)",
        [ROOT / "crates" / "tsz-emitter" / "src"],
        re.compile(r"\bsource_text\.contains\s*\("),
        1,
    ),
    (
        "Solver API boundary: flat root wildcard compatibility re-exports (#8204)",
        [ROOT / "crates" / "tsz-solver" / "src" / "lib.rs"],
        re.compile(r"^pub use (?:[A-Za-z_][A-Za-z0-9_]*::)+\*;"),
        0,
    ),
    (
        "Solver API boundary: root judge convenience re-export (#8204)",
        [ROOT / "crates" / "tsz-solver" / "src" / "lib.rs"],
        re.compile(r"^\s*pub\s+mod\s+judge\s*\{"),
        0,
    ),
    (
        "Solver relation boundary: legacy relation flag bridge surface (#8207)",
        [ROOT / "crates" / "tsz-solver" / "src"],
        re.compile(
            r"\b(?:from_checker_flags_u16|from_legacy_u8|to_legacy_u8|"
            r"subtype_cache_config_from_legacy_flags|"
            r"assignability_cache_config_from_legacy_flags)\b"
        ),
        0,
    ),
    (
        "Solver relation boundary: RelationPolicy must store typed flags (#8207)",
        [ROOT / "crates" / "tsz-solver" / "src" / "relations" / "relation_queries.rs"],
        re.compile(r"^\s*(?:pub\s+)?flags\s*:\s*u16\s*,"),
        0,
    ),
    (
        "Solver relation boundary: RelationPolicy must not expose packed flags (#8207)",
        [ROOT / "crates" / "tsz-solver" / "src" / "relations" / "relation_queries.rs"],
        re.compile(r"\bfn\s+legacy_packed_flags\s*\([^)]*\)\s*->\s*u16\b"),
        0,
    ),
    (
        "Checker relation boundary: raw diagnostic assignability predicates (#8227)",
        [
            ROOT
            / "crates"
            / "tsz-checker"
            / "src"
            / "assignability"
            / "assignability_diagnostics.rs",
            ROOT / "crates" / "tsz-checker" / "src" / "error_reporter",
            ROOT / "crates" / "tsz-checker" / "src" / "checkers" / "jsx",
        ],
        re.compile(
            r"\b(?:self|self\.ctx\.types|self\.interner)"
            r"\.is_assignable_to(?:_[A-Za-z0-9_]+)?\s*\("
        ),
        0,
    ),
    (
        "Checker residency boundary: with_parent_cache_attributed migration callsites (Track 10)",
        [ROOT / "crates" / "tsz-checker" / "src"],
        re.compile(
            r"^(?!\s*(?:pub(?:\([^)]*\))?\s+)?fn\b)"
            r".*\bwith_parent_cache_attributed\s*\("
        ),
        33,
    ),
    (
        "Checker residency boundary: copy_symbol_file_targets_to_attributed migration callsites (Track 10)",
        [ROOT / "crates" / "tsz-checker" / "src"],
        re.compile(
            r"^(?!\s*(?:pub(?:\([^)]*\))?\s+)?fn\b)"
            r".*\bcopy_symbol_file_targets_to_attributed\s*\("
        ),
        23,
    ),
    (
        "Checker relation boundary: diagnostic-local RelationRequest constructors (#8227)",
        [
            ROOT
            / "crates"
            / "tsz-checker"
            / "src"
            / "assignability"
            / "assignability_diagnostics.rs",
            ROOT / "crates" / "tsz-checker" / "src" / "error_reporter",
            ROOT / "crates" / "tsz-checker" / "src" / "checkers" / "jsx",
        ],
        re.compile(r"\bRelationRequest::[A-Za-z_][A-Za-z0-9_]*\s*\("),
        0,
    ),
    (
        "Solver relation boundary: legacy packed relation flag bridges (#8207)",
        [ROOT / "crates" / "tsz-solver" / "src"],
        re.compile(
            r'^(?:[^"\n]|"[^"\n]*")*?'
            r"\b(?:subtype_cache_config_from_legacy_flags\s*\(|"
            r"assignability_cache_config_from_legacy_flags\s*\(|"
            r"from_checker_flags_u16\s*\(|from_legacy_u8\s*\(|to_legacy_u8\s*\(|"
            r"RelationCacheKey::(?:subtype|assignability)\s*\(|"
            r"RelationFlags::from_bits_truncate\s*\(|"
            r"CachedAnyMode::from_legacy_u8\s*\()"
        ),
        0,
    ),
    (
        "Solver relation boundary: relation engines avoid packed apply_flags (#8207)",
        [ROOT / "crates" / "tsz-solver" / "src" / "relations"],
        re.compile(
            r"\bfn\s+apply_flags\s*\([^)]*\bflags\s*:\s*u16"
            r"|\.\s*apply_flags\s*\(\s*policy\.flags\s*\)"
        ),
        0,
    ),
    (
        "Solver relation boundary: legacy flag decoder avoids cache-key constants (#8207)",
        [ROOT / "crates" / "tsz-solver" / "src" / "relations" / "relation_queries.rs"],
        re.compile(r"\bRelationCacheKey::FLAG_[A-Z0-9_]+\b"),
        0,
    ),
    (
        "Solver relation boundary: legacy RelationPolicy::from_flags calls stay at boundary (#8207)",
        [ROOT / "crates" / "tsz-solver" / "src"],
        re.compile(
            r'^\s*(?!//)(?:[^"\n]|"[^"\n]*")*?'
            r"\bRelationPolicy::from_flags\s*\("
        ),
        0,
    ),
    (
        "Solver relation boundary: query cache uses relation facade (#8207)",
        [ROOT / "crates" / "tsz-solver" / "src" / "caches" / "query_cache.rs"],
        re.compile(r"\b(?:configured_compat_checker|configured_subtype_checker)\s*\("),
        0,
    ),
    (
        "Solver relation boundary: query cache trace labels use typed policy names (#8207)",
        [ROOT / "crates" / "tsz-solver" / "src" / "caches" / "query_cache.rs"],
        re.compile(r'"is_(?:subtype_of|assignable_to)_with_flags"'),
        0,
    ),
    (
        "Solver relation boundary: query cache legacy flag overrides (#8207)",
        [ROOT / "crates" / "tsz-solver" / "src" / "caches" / "query_cache.rs"],
        re.compile(r"\bfn\s+is_(?:subtype_of|assignable_to)_with_flags\s*\("),
        0,
    ),
    (
        "Solver relation boundary: query database legacy flag methods (#8207)",
        [
            ROOT / "crates" / "tsz-solver" / "src" / "caches" / "db.rs",
            ROOT / "crates" / "tsz-solver" / "src" / "caches" / "query_cache.rs",
        ],
        re.compile(r"\bfn\s+is_(?:subtype_of|assignable_to)_with_flags\s*\("),
        0,
    ),
]

# Track 10 performance guardrail: branch-local `visited.clone()` traversal
# clones are a known scale-cliff risk for graph predicates.  Existing sites are
# pinned by file plus statement text so normal line movement does not churn the
# guard, while new clone sites must either replace an existing one with a
# memoized/worklist traversal or extend this allowlist intentionally.
BRANCH_LOCAL_VISITED_CLONE_CHECKS = [
    (
        "Performance boundary: branch-local visited.clone() graph traversal sites (Track 10)",
        [
            ROOT / "crates" / "tsz-checker" / "src",
            ROOT / "crates" / "tsz-lsp" / "src",
            ROOT / "crates" / "tsz-solver" / "src",
        ],
        (
            (
                "crates/tsz-checker/src/state/type_environment/lazy.rs",
                "let mut branch_visited = visited.clone();",
            ),
            (
                "crates/tsz-checker/src/state/type_resolution/module.rs",
                "let mut inner_visited = visited.clone();",
            ),
            (
                "crates/tsz-checker/src/types/queries/type_only.rs",
                "let mut exists_visited = visited.clone();",
            ),
            (
                "crates/tsz-checker/src/types/queries/type_only.rs",
                "let mut type_only_visited = visited.clone();",
            ),
            (
                "crates/tsz-lsp/src/completions/member.rs",
                "let mut member_visited = visited.clone();",
            ),
            (
                "crates/tsz-solver/src/evaluation/evaluate_rules/infer_pattern.rs",
                "let mut alias_visited = visited.clone();",
            ),
            (
                "crates/tsz-solver/src/evaluation/evaluate_rules/infer_pattern_object_helpers.rs",
                "let mut alias_visited = visited.clone();",
            ),
        ),
    ),
]

# Pin the count of LSP feature-dispatch methods in
# `crates/tsz-lsp/src/project/features.rs` (architecture health metric 7
# in `docs/plan/ROADMAP.md` — "LSP/WASM semantic features implemented
# outside the compiler service layer").
#
# Every `pub fn` on `Project` whose name starts with one of `get_`,
# `provide_`, `prepare_`, `handle_`, `on_`, `find_`, or `resolve_` is an
# LSP feature dispatched directly from `Project` rather than through a
# service-trait abstraction. Workstream 6 ("LSP And WASM As Service
# Clients") exit criterion 3 is that "LSP request handling mostly maps
# protocol inputs to service queries and service outputs to protocol
# DTOs"; the raw count tracks how far the live code is from that
# state. Each new feature dispatch must bump the cap with a roadmap
# entry; consolidation onto a service trait shows up as a cap reduction
# in the same diff.
#
# Each entry: (description, file_path, max_methods).
LSP_FEATURE_METHOD_COUNT_CHECKS = [
    (
        "LSP boundary: feature-dispatch method count in project/features.rs (architecture health metric 7)",
        ROOT / "crates" / "tsz-lsp" / "src" / "project" / "features.rs",
        32,
    ),
]

PROJECT_DASHBOARD_ROW_CHECKS = [
    (
        "Project corpus dashboard: shared project row manifest must cover dashboard rows (Track 1)",
        ROOT / "scripts" / "bench" / "project-rows.mjs",
    ),
]

PROJECT_FIXTURE_SOURCE_CHECKS = [
    (
        "Project corpus fixtures: pinned rows must record fixture source refs (Track 1)",
        ROOT / "scripts" / "bench" / "project-rows.mjs",
        ROOT / "scripts" / "bench" / "project-fixtures.sh",
    ),
]

PROJECT_INCLUSION_POLICY_CHECKS = [
    (
        "Project corpus inclusion: row manifest must match compile guard and benchmark rows (Track 1)",
        ROOT / "scripts" / "bench" / "project-rows.mjs",
        ROOT / "scripts" / "ci" / "project-compile-guard.sh",
        ROOT / "scripts" / "bench" / "bench-vs-tsgo.sh",
    ),
]

PROJECT_CONFIG_WRITER_CHECKS = [
    (
        "Project corpus config shape: shared rows must use shared config writers (Track 1)",
        ROOT / "scripts" / "bench" / "project-fixtures.sh",
        ROOT / "scripts" / "ci" / "project-compile-guard.sh",
        ROOT / "scripts" / "bench" / "bench-vs-tsgo.sh",
    ),
]

PROJECT_CONFIG_WRITERS = {
    "utility-types-project": "tsz_write_utility_types_config",
    "ts-toolbelt-project": "tsz_write_ts_toolbelt_config",
    "ts-essentials-project": "tsz_write_ts_essentials_config",
    "rxjs-project": "tsz_write_rxjs_config",
    "type-fest-project": "tsz_write_type_fest_config",
    "zod-project": "tsz_write_zod_config",
    "kysely-project": "tsz_write_kysely_config",
    "nextjs": "tsz_write_nextjs_config",
}

GENERATED_PROJECT_ROWS_WITHOUT_PINNED_SOURCE = {
    "vite-vanilla-ts-app",
    "nextjs-fresh-app",
}

COMPILE_GUARD_ONLY_PROJECT_ROWS = {
    "type-challenges-solutions-project",
}

BENCHMARK_ONLY_PROJECT_ROWS = {
    "nextjs",
    "large-ts-repo",
}

EXCLUDE_DIRS = {".git", "target", "node_modules"}
SOLVER_TYPEDATA_QUARANTINE_ALLOWLIST = {
    "crates/tsz-solver/src/intern/mod.rs",
    "crates/tsz-solver/src/intern/core/constructors.rs",
    "crates/tsz-solver/src/intern/intersection.rs",
    "crates/tsz-solver/src/intern/normalize.rs",
    "crates/tsz-solver/src/intern/template.rs",
}


def iter_rs_files(base: pathlib.Path):
    for path in base.rglob("*.rs"):
        rel = path.relative_to(ROOT).as_posix()
        parts = set(rel.split("/"))
        if EXCLUDE_DIRS.intersection(parts):
            continue
        yield path, rel


def find_matches(file_text: str, pattern: re.Pattern[str], rel: str, excludes: dict):
    matches = []
    excluded_files = set(excludes.get("exclude_files", ()))
    if rel in excluded_files:
        return matches

    exclude_dirs = set(excludes.get("exclude_dirs", ()))
    part_set = set(rel.split("/"))
    if exclude_dirs and exclude_dirs.intersection(part_set):
        return matches

    if excludes.get("exclude_test_files") and is_test_file(rel):
        return matches

    for i, line in enumerate(file_text.splitlines(), start=1):
        if excludes.get("ignore_comment_lines", False):
            if line.lstrip().startswith("//"):
                continue
        if pattern.search(line):
            matches.append(i)
    return matches


def is_test_file(rel: str) -> bool:
    """Check if a file path looks like a test file."""
    parts = rel.split("/")
    filename = parts[-1] if parts else ""
    return filename.endswith("_tests.rs") or filename.startswith("test_")


def scan(base, pattern, excludes):
    hits = []
    for path, rel in iter_rs_files(base):
        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        for ln in find_matches(text, pattern, rel, excludes):
            hits.append(f"{rel}:{ln}")
    return hits


def scan_line_limits(base: pathlib.Path, limit: int, exclude_files=None):
    hits = []
    for path, rel in iter_rs_files(base):
        if exclude_files and rel in exclude_files:
            continue
        line_count = 0
        try:
            with path.open("r", encoding="utf-8", errors="ignore") as handle:
                for line_count, _line in enumerate(handle, start=1):
                    pass
        except OSError:
            continue
        if line_count > limit:
            hits.append(f"{rel}:{line_count} lines (limit {limit})")
    return hits
