#!/usr/bin/env python3
"""Direction check for emit row status and oracle-clean diagnostics (#16171).

The emit gate in ``scripts/ci/full-ci.sh`` compares summed pass *counts*
against ``scripts/emit/emit-snapshot.json``. Two things that comparison
structurally cannot see:

1. **A swap.** One row fixed and another broken leaves ``jsPass`` unchanged,
   so a real regression passes the gate.
2. **A ratchet-down.** ``cap_positive_baseline`` computes ``min(baseline,
   floor)``, so ``TSZ_CI_JS_ACCEPTED_FLOOR`` is a *ceiling* — an
   anti-unsatisfiability valve — not a floor. If ``emit-snapshot.json`` is
   ever hand-refreshed while emit is regressed, the count bar follows the
   snapshot down and the constant that looks like a backstop does not stop it.

``emit-snapshot.json``'s ``detailFingerprint`` / ``detailResultCount`` pin that
the retired ``emit-detail.json`` matches its own summary. That is an
internal-consistency check, not a rewrite direction check. This script defaults
to the separately pinned rewrite-era regression baseline.

This script is the direction check, modelled on how conformance diffs its
failure set rather than its pass count: a row that is failing now and was not
failing in the committed baseline is reported by name and fails the gate,
whatever the counts say.

Status is not the whole row. ``TSZ_NONZERO_OUTCOME`` is emitted only after the
exact TypeScript 7 invocation exited successfully, so every listed TSZ
diagnostic is oracle-absent for that authored emit case. A persistently
incomplete row may therefore regress while its status stays diagonal. The
accepted baseline sequence may shrink, but it may not gain codes or reorder.
The detail schema stores codes rather than full diagnostic fingerprints; exact
span/message review remains part of semantic PR evidence.

Schema-v2 rows also preserve product selection and raw path-to-bytes parity.
An unselected JS or DTS surface is product-neutral and must be `skip`; a
selected surface is validated against the invocation artifact state. Once a
selected surface proves raw product agreement, later terminal-state changes
cannot hide loss of that agreement.

Deliberately asymmetric:

* **Named pass losses, new mismatches, and terminal escalations are fatal.** A
  status swap cannot hide them behind a stable total.
* **A terminal row may become typed unsupported.** Honest fail-closed
  containment removes a crash/timeout without claiming parity; the reverse is
  fatal, and measured complete/incomplete work cannot withdraw this way.
* **Baseline rows absent from the run are reported.** Rewrite CI makes them
  fatal so a truncated artifact or same-change baseline edit cannot erase a
  regression; ad-hoc corpus-migration comparisons may leave them as warnings.
* **Rows that are failing in the baseline and still failing are silent.** They
  are the accepted set; shrinking it is an improvement and never blocks.
* **Oracle-clean TSZ diagnostic sequences may only shrink as ordered
  subsequences.** Additions and reordering are fatal even when status is
  unchanged.
"""

import argparse
from collections import Counter
import hashlib
import json
import os
import pathlib
import re
import subprocess
import sys
import tempfile


FAILING_STATUSES = ("fail", "timeout", "crash")
TERMINAL_STATUSES = ("timeout", "crash")
TSZ_NONZERO_PREFIX = "TSZ_NONZERO_OUTCOME:"
ARTIFACT_STATUSES = frozenset(
    ("complete", "crash", "incomplete", "timeout", "unsupported")
)
JS_STATUSES = frozenset(
    ("crash", "fail", "incomplete", "pass", "skip", "timeout", "unsupported")
)
DTS_STATUSES = JS_STATUSES
PRODUCT_STATUS_FIELDS = (
    ("Artifact", "artifactState", ARTIFACT_STATUSES),
    ("JS", "jsStatus", JS_STATUSES),
    ("DTS", "dtsStatus", DTS_STATUSES),
)


def row_key(row):
    """Stable identity for one emit result row.

    ``(testPath, baselineFile, name)`` is unique across the committed corpus
    (13806/13806 distinct in the pinned rewrite baseline).
    """
    return (
        row.get("testPath") or "",
        row.get("baselineFile") or "",
        row.get("name") or "",
    )


def format_key(key):
    test_path, baseline_file, name = key
    return "%s [%s] (%s)" % (name, baseline_file, test_path)


def load_detail(path):
    """Return ``(results, oracle_fingerprint)`` from an emit detail document."""
    with open(path, "r", encoding="utf-8") as handle:
        data = json.load(handle)
    if not isinstance(data, dict):
        raise ValueError("%s is not an emit detail object" % path)
    results = data.get("results")
    if not isinstance(results, list):
        raise ValueError("%s has no 'results' array" % path)
    if "detailResultCount" not in data:
        raise ValueError("%s omits required detailResultCount" % path)
    declared_count = data["detailResultCount"]
    if type(declared_count) is not int or declared_count < 0:
        raise ValueError("%s has invalid detailResultCount=%r" % (path, declared_count))
    if declared_count != len(results):
        raise ValueError(
            "%s detailResultCount=%r does not match %d result rows"
            % (path, declared_count, len(results))
        )
    oracle = data.get("oracle")
    fingerprint = oracle.get("fingerprint") if isinstance(oracle, dict) else None
    if fingerprint is not None and not isinstance(fingerprint, str):
        raise ValueError("%s has a non-string oracle fingerprint" % path)
    return results, fingerprint


def trusted_oracle_fingerprints(path):
    """Compute every platform fingerprint allowed by the pinned manifest."""
    with open(path, "r", encoding="utf-8") as handle:
        manifest = json.load(handle)
    if not isinstance(manifest, dict) or not isinstance(manifest.get("platforms"), dict):
        raise ValueError("%s is not a pinned emit oracle manifest" % path)
    try:
        shared = {
            "schemaVersion": manifest["schemaVersion"],
            "packageName": manifest["packageName"],
            "version": manifest["version"],
            "gitHead": manifest["gitHead"],
            "wrapperIntegrity": manifest["wrapperIntegrity"],
            "wrapperPackageJsonSha256": manifest["wrapperPackageJsonSha256"],
            "wrapperBinSha256": manifest["wrapperBinSha256"],
        }
        prefix = manifest["platformPackagePrefix"]
    except KeyError as exc:
        raise ValueError("%s is missing oracle field %s" % (path, exc)) from exc

    fingerprints = set()
    for suffix, platform in manifest["platforms"].items():
        if not isinstance(platform, dict):
            raise ValueError("%s has invalid platform %s" % (path, suffix))
        package_name = "%s%s" % (prefix, suffix)
        binary_name = "tsc.exe" if suffix.startswith("win32-") else "tsc"
        try:
            provenance = {
                "schemaVersion": shared["schemaVersion"],
                "packageName": shared["packageName"],
                "platformPackageName": package_name,
                "version": shared["version"],
                "gitHead": shared["gitHead"],
                "wrapperIntegrity": shared["wrapperIntegrity"],
                "platformIntegrity": platform["packageIntegrity"],
                "wrapperPackageJsonSha256": shared["wrapperPackageJsonSha256"],
                "wrapperBinSha256": shared["wrapperBinSha256"],
                "platformPackageJsonSha256": platform["packageJsonSha256"],
                "platformPackageTreeSha256": platform["packageTreeSha256"],
                "binarySha256": platform["binarySha256"],
                "binaryPath": "scripts/node_modules/%s/lib/%s"
                % (package_name, binary_name),
            }
        except KeyError as exc:
            raise ValueError(
                "%s platform %s is missing oracle field %s" % (path, suffix, exc)
            ) from exc
        encoded = json.dumps(provenance, separators=(",", ":")).encode("utf-8")
        fingerprints.add("sha256:%s" % hashlib.sha256(encoded).hexdigest())
    if not fingerprints:
        raise ValueError("%s has no trusted oracle platforms" % path)
    return fingerprints


def validate_rewrite_baseline_metadata(path):
    """Validate the provenance envelope of the committed projection."""
    with open(path, "r", encoding="utf-8") as handle:
        data = json.load(handle)
    if data.get("schemaVersion") != 1:
        raise ValueError("%s has unsupported rewrite baseline schemaVersion" % path)
    source_hash = data.get("sourceArtifactSha256")
    if not isinstance(source_hash, str) or re.fullmatch(
        r"sha256:[0-9a-f]{64}", source_hash
    ) is None:
        raise ValueError("%s has invalid sourceArtifactSha256 provenance" % path)
    git_sha = data.get("git_sha")
    if not isinstance(git_sha, str) or re.fullmatch(r"[0-9a-f]{40}", git_sha) is None:
        raise ValueError("%s has invalid git_sha provenance" % path)


def index_rows(results, source="emit detail"):
    """Map unique row key -> row, rejecting ambiguous stable identities."""
    indexed = {}
    for index, row in enumerate(results):
        if not isinstance(row, dict):
            raise ValueError("%s result %d is not an object" % (source, index))
        key = row_key(row)
        if not all(key):
            raise ValueError("%s result %d has an incomplete stable key" % (source, index))
        if key in indexed:
            raise ValueError("%s has duplicate emit row %s" % (source, format_key(key)))
        for kind, field, domain in PRODUCT_STATUS_FIELDS:
            if field not in row:
                raise ValueError(
                    "%s result %d omits required %s status field %s"
                    % (source, index, kind, field)
                )
            status = row[field]
            if status not in domain:
                raise ValueError(
                    "%s result %d has unknown %s status %r"
                    % (source, index, kind, status)
                )
        artifact = row["artifactState"]
        js_status = row["jsStatus"]
        dts_status = row["dtsStatus"]
        has_selection = "jsSelected" in row or "dtsSelected" in row
        if has_selection and not all(field in row for field in ("jsSelected", "dtsSelected")):
            raise ValueError(
                "%s result %d must serialize both jsSelected and dtsSelected"
                % (source, index)
            )
        selections = {}
        for surface, status in (("js", js_status), ("dts", dts_status)):
            selected = row.get(f"{surface}Selected", status != "skip")
            if not isinstance(selected, bool):
                raise ValueError(
                    "%s result %d has non-boolean %sSelected=%r"
                    % (source, index, surface, selected)
                )
            selections[surface] = selected
            expected_statuses = (
                ("pass", "fail") if artifact == "complete" else (artifact,)
            ) if selected else ("skip",)
            if status not in expected_statuses:
                raise ValueError(
                    "%s result %d has inconsistent %s artifact product status: "
                    "selected=%s artifact=%s status=%s"
                    % (source, index, surface, selected, artifact, status)
                )

        if has_selection:
            if "outcomeMatch" not in row or not isinstance(
                row.get("outcomeMatch"), (bool, type(None))
            ):
                raise ValueError(
                    "%s result %d has invalid or missing outcomeMatch"
                    % (source, index)
                )
            if artifact == "complete" and not isinstance(row.get("outcomeMatch"), bool):
                raise ValueError(
                    "%s result %d has complete artifact without measured outcomeMatch"
                    % (source, index)
                )
            for surface in ("js", "dts"):
                selected = selections[surface]
                match_field = f"{surface}Match"
                product_field = f"{surface}ProductMatch"
                product_error_field = f"{surface}ProductError"
                if match_field not in row or product_field not in row:
                    raise ValueError(
                        "%s result %d omits schema-v2 %s parity fields"
                        % (source, index, surface.upper())
                    )
                match = row.get(match_field)
                product_match = row.get(product_field)
                if selected:
                    if not isinstance(match, bool):
                        raise ValueError(
                            "%s result %d has selected %s product without boolean %s"
                            % (source, index, surface.upper(), match_field)
                        )
                    if not isinstance(product_match, (bool, type(None))):
                        raise ValueError(
                            "%s result %d has invalid %s=%r"
                            % (source, index, product_field, product_match)
                        )
                    if artifact == "complete" and not isinstance(product_match, bool):
                        raise ValueError(
                            "%s result %d has complete selected %s product without raw parity"
                            % (source, index, surface.upper())
                        )
                    expected_match = row.get("outcomeMatch") is True and product_match is True
                    if match is not expected_match:
                        raise ValueError(
                            "%s result %d has inconsistent %s effective parity"
                            % (source, index, surface.upper())
                        )
                elif match is not None or product_match is not None:
                    raise ValueError(
                        "%s result %d has unselected %s product with measured parity"
                        % (source, index, surface.upper())
                    )
                product_error = row.get(product_error_field)
                if product_match is False:
                    if not isinstance(product_error, str) or not product_error:
                        raise ValueError(
                            "%s result %d has %s=false without %s"
                            % (source, index, product_field, product_error_field)
                        )
                elif product_error is not None:
                    raise ValueError(
                        "%s result %d has %s=%r with a %s payload"
                        % (source, index, product_field, product_match, product_error_field)
                    )

        for surface, status in (("js", js_status), ("dts", dts_status)):
            error_field = f"{surface}Error"
            if status in ("pass", "skip") and row.get(error_field) is not None:
                raise ValueError(
                    "%s result %d has %sStatus=%s with a %s payload"
                    % (source, index, surface, status, error_field)
                )
        for field in ("jsError", "dtsError"):
            tsz_nonzero_diagnostics(row.get(field))
        indexed[key] = row
    return indexed


def is_failing(status):
    return status in FAILING_STATUSES


def is_status_regression(kind, baseline_status, current_status):
    """Whether a named row's product status moved in a blocking direction."""
    if baseline_status == current_status:
        return False
    if kind == "Artifact":
        if current_status == "complete":
            return False
        if baseline_status == "complete":
            return True
        if baseline_status == "incomplete" and current_status == "unsupported":
            return True
        if current_status in TERMINAL_STATUSES:
            return True
        return False

    if current_status == "pass":
        return False
    if baseline_status == "pass":
        return True
    # Once a product was selected, `skip` is a coverage withdrawal rather
    # than an improvement. Likewise, measured fail/incomplete work may not be
    # hidden by reclassifying it as unsupported.
    if current_status == "skip":
        return baseline_status != "skip"
    if current_status == "unsupported" and baseline_status in (
        "fail",
        "incomplete",
    ):
        return True
    if current_status == "fail":
        return baseline_status not in ("fail", "crash", "timeout")
    if current_status in TERMINAL_STATUSES:
        # A different terminal mechanism is not a demonstrated improvement.
        return True
    if baseline_status == "fail" and current_status == "incomplete":
        return True
    if baseline_status == "incomplete":
        return True
    return False


def tsz_nonzero_diagnostics(error):
    """Return the ordered TSZ diagnostic codes, or ``None`` for another error.

    ``TSZ_NONZERO_OUTCOME`` is only selected when the oracle invocation was
    clean. ``<none>`` represents a typed nonclaim exit with no fabricated
    diagnostic and therefore returns an empty tuple.
    """
    if not isinstance(error, str) or not error.startswith(TSZ_NONZERO_PREFIX):
        return None
    marker = "diagnostics="
    if marker not in error:
        raise ValueError("malformed TSZ_NONZERO_OUTCOME: missing diagnostics=")
    payload = error.split(marker, 1)[1].strip()
    if payload == "<none>":
        return ()
    if not payload:
        raise ValueError("malformed TSZ_NONZERO_OUTCOME: empty diagnostics payload")
    codes = tuple(code.strip() for code in payload.split(",") if code.strip())
    if not codes or any(re.fullmatch(r"TS\d+", code) is None for code in codes):
        raise ValueError("malformed TSZ_NONZERO_OUTCOME diagnostic codes")
    return codes


def is_ordered_subsequence(candidate, baseline):
    """Whether ``candidate`` can be obtained only by deleting baseline items."""
    baseline_iter = iter(baseline)
    return all(any(code == prior for prior in baseline_iter) for code in candidate)


def find_diagnostic_payload_regressions(baseline_rows, current_rows):
    """Find oracle-clean TSZ diagnostic sequences that grew or reordered.

    Returns ``(key, kind, baseline_codes, current_codes, added)`` tuples.
    ``None`` means the error field is absent or no longer a recognized TSZ
    outcome; an empty tuple means the explicit ``diagnostics=<none>`` outcome.
    """
    regressions = []
    for key, current in sorted(current_rows.items()):
        baseline = baseline_rows.get(key)
        for kind, field, status_field in (
            ("JS", "jsError", "jsStatus"),
            ("DTS", "dtsError", "dtsStatus"),
        ):
            current_codes = tsz_nonzero_diagnostics(current.get(field))
            baseline_codes = (
                tsz_nonzero_diagnostics(baseline.get(field))
                if baseline is not None
                else None
            )
            if current_codes is None:
                # Once a row established a typed oracle-clean TSZ outcome,
                # silently dropping/changing that payload while it remains
                # non-passing is a schema or coverage regression. A real pass
                # may of course remove the old error payload.
                if baseline_codes is not None and current[status_field] != "pass":
                    regressions.append(
                        (key, kind, baseline_codes, current_codes, ())
                    )
                continue
            if not current_codes:
                continue
            if baseline_codes is not None and is_ordered_subsequence(
                current_codes, baseline_codes
            ):
                continue
            prior_counts = Counter(baseline_codes or ())
            added = tuple(
                sorted((Counter(current_codes) - prior_counts).elements())
            )
            regressions.append(
                (key, kind, baseline_codes, current_codes, added)
            )
    return regressions


def find_regressions(baseline_rows, current_rows):
    """Named row status transitions that regress the baseline.

    Returns a list of ``(key, kind, baseline_status, current_status)`` tuples
    sorted by name, where ``kind`` is ``"JS"`` or ``"DTS"``.
    """
    regressions = []
    for key, current in sorted(current_rows.items()):
        baseline = baseline_rows.get(key)
        if baseline is None:
            # A row the baseline has never seen (new corpus test). It cannot be
            # a regression against a baseline that does not describe it.
            continue
        for kind, field, _domain in PRODUCT_STATUS_FIELDS:
            if field not in baseline and field not in current:
                continue
            current_status = current.get(field)
            baseline_status = baseline.get(field)
            if is_status_regression(kind, baseline_status, current_status):
                regressions.append((key, kind, baseline_status, current_status))
    return regressions


def find_product_parity_regressions(baseline_rows, current_rows):
    """Selected schema-v2 products that lost previously proven byte parity."""
    regressions = []
    for key, current in sorted(current_rows.items()):
        baseline = baseline_rows.get(key)
        if baseline is None:
            continue
        for kind, surface in (("JS", "js"), ("DTS", "dts")):
            baseline_match = baseline.get(f"{surface}ProductMatch")
            if baseline_match is not True:
                continue
            current_match = current.get(f"{surface}ProductMatch")
            if current_match is not True:
                regressions.append((key, kind, baseline_match, current_match))
    return regressions


def find_absent(baseline_rows, current_rows):
    """Baseline rows the run did not report at all."""
    return sorted(key for key in baseline_rows if key not in current_rows)


def run_git(root, *arguments):
    result = subprocess.run(
        ["git", "-C", str(root), *arguments],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise ValueError("git %r failed: %s" % (" ".join(arguments), detail))
    return result.stdout


def git_is_ancestor(root, ancestor, descendant):
    result = subprocess.run(
        ["git", "-C", str(root), "merge-base", "--is-ancestor", ancestor, descendant],
        capture_output=True,
        text=True,
    )
    if result.returncode not in (0, 1):
        detail = result.stderr.strip() or result.stdout.strip()
        raise ValueError("cannot inspect emit baseline history ancestry: %s" % detail)
    return result.returncode == 0


def relevant_history_is_shallow(root, head):
    if run_git(root, "rev-parse", "--is-shallow-repository").strip() == "false":
        return False
    shallow_path = pathlib.Path(
        run_git(root, "rev-parse", "--git-path", "shallow").strip()
    )
    if not shallow_path.is_absolute():
        shallow_path = root / shallow_path
    try:
        boundaries = shallow_path.read_text(encoding="utf-8").splitlines()
    except OSError as exc:
        raise ValueError("cannot read git shallow boundaries: %s" % exc) from exc
    return any(git_is_ancestor(root, boundary, head) for boundary in boundaries)


def check_baseline_history(repo_root, baseline_path, oracle_manifest, checker_path=None):
    """Compare the current baseline with every reachable committed version.

    This makes the baseline itself a monotonic history ratchet. It cannot be
    reset by advancing the PR base, deleting/recreating the file, or merging a
    looser parallel branch.
    """
    root_input = pathlib.Path(os.path.abspath(str(repo_root)))
    supplied = pathlib.Path(baseline_path)
    supplied_absolute = pathlib.Path(
        os.path.abspath(
            str(supplied if supplied.is_absolute() else root_input / supplied)
        )
    )
    try:
        relative_path = supplied_absolute.relative_to(root_input)
    except ValueError as exc:
        raise ValueError("emit baseline must be inside the repository") from exc
    root = root_input.resolve()
    baseline = root / relative_path
    relative = relative_path.as_posix()
    if baseline.resolve() != baseline:
        raise ValueError("rewrite emit baseline path must not contain a symlink")
    head = run_git(root, "rev-parse", "--verify", "HEAD^{commit}").strip()
    if relevant_history_is_shallow(root, head):
        raise ValueError("emit baseline history check requires complete history")
    current_entries = run_git(root, "ls-files", "--stage", "--", relative).splitlines()
    if len(current_entries) != 1 or not current_entries[0].startswith("100644 "):
        raise ValueError("rewrite emit baseline must be one tracked regular file")
    commits = run_git(
        root,
        "rev-list",
        "--full-history",
        "--reverse",
        "--topo-order",
        head,
        "--",
        relative,
    ).splitlines()
    if not commits:
        raise ValueError("committed rewrite emit baseline has no reachable history")

    checker = pathlib.Path(checker_path or __file__).resolve()
    checked = 0
    with tempfile.TemporaryDirectory() as temporary:
        temporary_path = pathlib.Path(temporary)
        for commit in commits:
            entries = run_git(root, "ls-tree", commit, "--", relative).splitlines()
            if not entries:
                continue
            if len(entries) != 1 or not entries[0].startswith("100644 blob "):
                raise ValueError(
                    "rewrite emit baseline at %s is not one regular file" % commit
                )
            historical = temporary_path / ("%s.json" % commit)
            payload = subprocess.run(
                ["git", "-C", str(root), "show", "%s:%s" % (commit, relative)],
                capture_output=True,
            )
            if payload.returncode != 0:
                raise ValueError(
                    "cannot read rewrite emit baseline at %s: %s"
                    % (commit, payload.stderr.decode("utf-8", errors="replace").strip())
                )
            historical.write_bytes(payload.stdout)
            compared = subprocess.run(
                [
                    sys.executable,
                    str(checker),
                    "--baseline",
                    str(historical),
                    "--require-oracle-provenance",
                    "--reject-absent-baseline-rows",
                    "--oracle-manifest",
                    str(oracle_manifest),
                    str(baseline),
                ],
                capture_output=True,
                text=True,
            )
            if compared.stdout:
                sys.stdout.write(compared.stdout)
            if compared.stderr:
                sys.stderr.write(compared.stderr)
            if compared.returncode != 0:
                return 1
            checked += 1
    if checked == 0:
        raise ValueError("committed rewrite emit baseline has no readable snapshot")
    print("Rewrite emit baseline history: %d committed floor(s) passed" % checked)
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--baseline",
        default="scripts/emit/rewrite-regression-baseline.json",
        help="committed emit detail JSON to diff against",
    )
    parser.add_argument(
        "detail",
        nargs="*",
        help="per-shard emit detail JSON produced by scripts/emit/run.sh --json-out",
    )
    parser.add_argument(
        "--check-baseline-history",
        action="store_true",
        help="compare --baseline against every committed version reachable from HEAD",
    )
    parser.add_argument(
        "--max-report",
        type=int,
        default=50,
        help="cap the number of named rows printed (all are counted)",
    )
    parser.add_argument(
        "--oracle-manifest",
        default="scripts/emit/oracle-manifest.json",
        help="pinned oracle manifest used to validate artifact provenance",
    )
    parser.add_argument(
        "--require-oracle-provenance",
        action="store_true",
        help="fail unless every current detail identifies a manifest-trusted oracle",
    )
    parser.add_argument(
        "--reject-absent-baseline-rows",
        action="store_true",
        help="fail if a committed baseline row is absent from the current details",
    )
    args = parser.parse_args(argv)

    baseline_path = pathlib.Path(args.baseline)
    if not baseline_path.is_file():
        print(
            "error: emit regression set check needs a baseline at %s" % baseline_path,
            file=sys.stderr,
        )
        return 1

    if args.check_baseline_history:
        if args.detail:
            print(
                "error: --check-baseline-history does not accept detail arguments",
                file=sys.stderr,
            )
            return 2
        try:
            return check_baseline_history(
                pathlib.Path(__file__).resolve().parents[2],
                baseline_path,
                args.oracle_manifest,
            )
        except ValueError as exc:
            print("error: %s" % exc, file=sys.stderr)
            return 1

    if not args.detail:
        print("error: at least one current emit detail is required", file=sys.stderr)
        return 2

    try:
        baseline_results, baseline_oracle = load_detail(baseline_path)
        baseline_rows = index_rows(baseline_results, str(baseline_path))
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print("error: cannot read emit baseline %s: %s" % (baseline_path, exc), file=sys.stderr)
        return 1

    current_rows = {}
    current_oracles = set()
    current_missing_oracle = False
    for detail in args.detail:
        try:
            results, oracle = load_detail(detail)
            indexed = index_rows(results, detail)
            duplicates = sorted(set(current_rows).intersection(indexed))
            if duplicates:
                raise ValueError(
                    "%s repeats emit row already reported by another detail: %s"
                    % (detail, format_key(duplicates[0]))
                )
            current_rows.update(indexed)
            if oracle is None:
                current_missing_oracle = True
            else:
                current_oracles.add(oracle)
        except (OSError, ValueError, json.JSONDecodeError) as exc:
            print("error: cannot read emit detail %s: %s" % (detail, exc), file=sys.stderr)
            return 1

    if not current_rows:
        print(
            "error: emit regression set check found no result rows across %d detail file(s)"
            % len(args.detail),
            file=sys.stderr,
        )
        return 1

    if len(current_oracles) > 1:
        print(
            "error: emit details use multiple oracle fingerprints: %s"
            % ", ".join(sorted(current_oracles)),
            file=sys.stderr,
        )
        return 1
    if current_oracles and current_missing_oracle:
        print(
            "error: some emit details omit oracle provenance while other shards provide it",
            file=sys.stderr,
        )
        return 1
    if args.require_oracle_provenance:
        try:
            validate_rewrite_baseline_metadata(baseline_path)
            trusted_oracles = trusted_oracle_fingerprints(args.oracle_manifest)
        except (OSError, ValueError, json.JSONDecodeError) as exc:
            print(
                "error: cannot validate emit baseline/oracle provenance: %s" % exc,
                file=sys.stderr,
            )
            return 1
        if baseline_oracle is None:
            print(
                "error: committed emit baseline omits required oracle provenance",
                file=sys.stderr,
            )
            return 1
        if not current_oracles:
            print(
                "error: current emit details omit required oracle provenance",
                file=sys.stderr,
            )
            return 1
        untrusted = current_oracles - trusted_oracles
        if baseline_oracle not in trusted_oracles:
            untrusted.add(baseline_oracle)
        if untrusted:
            print(
                "error: emit detail uses untrusted oracle fingerprint(s): %s"
                % ", ".join(sorted(untrusted)),
                file=sys.stderr,
            )
            return 1

    failed = False
    absent = find_absent(baseline_rows, current_rows)
    if absent:
        level = "error" if args.reject_absent_baseline_rows else "warning"
        print(
            "%s: %d baseline emit row(s) were not reported by this run "
            "(corpus drift, truncation, or a shard that produced incomplete detail)"
            % (level, len(absent)),
            file=sys.stderr,
        )
        for key in absent[: args.max_report]:
            print("%s:   absent %s" % (level, format_key(key)), file=sys.stderr)
        failed = args.reject_absent_baseline_rows

    regressions = find_regressions(baseline_rows, current_rows)
    product_regressions = find_product_parity_regressions(
        baseline_rows, current_rows
    )
    payload_regressions = find_diagnostic_payload_regressions(
        baseline_rows, current_rows
    )
    if regressions:
        failed = True
        print(
            "error: emit regression: %d named product status transition(s) "
            "regress %s"
            % (len(regressions), baseline_path),
            file=sys.stderr,
        )
        for key, kind, baseline_status, current_status in regressions[: args.max_report]:
            print(
                "error:   %s %s: %s -> %s"
                % (kind, format_key(key), baseline_status, current_status),
                file=sys.stderr,
            )
        if len(regressions) > args.max_report:
            print(
                "error:   ... and %d more" % (len(regressions) - args.max_report),
                file=sys.stderr,
            )
    if product_regressions:
        failed = True
        print(
            "error: emit product parity regression: %d selected surface(s) "
            "lost previously proven raw product agreement"
            % len(product_regressions),
            file=sys.stderr,
        )
        for key, kind, baseline_match, current_match in product_regressions[
            : args.max_report
        ]:
            print(
                "error:   %s %s: productMatch=%s -> %s"
                % (kind, format_key(key), baseline_match, current_match),
                file=sys.stderr,
            )
        if len(product_regressions) > args.max_report:
            print(
                "error:   ... and %d more"
                % (len(product_regressions) - args.max_report),
                file=sys.stderr,
            )
    if payload_regressions:
        failed = True
        print(
            "error: emit diagnostic regression: %d oracle-clean row payload(s) "
            "gained or reordered TSZ diagnostics"
            % len(payload_regressions),
            file=sys.stderr,
        )
        for key, kind, baseline_codes, current_codes, added in payload_regressions[
            : args.max_report
        ]:
            prior = ",".join(baseline_codes) if baseline_codes else "<none>"
            if current_codes is None:
                current = "<missing-or-unrecognized-outcome>"
                suffix = " erased typed oracle-clean outcome"
            else:
                current = ",".join(current_codes) if current_codes else "<none>"
                suffix = (
                    " added=%s" % ",".join(added) if added else " reordered"
                )
            print(
                "error:   %s %s: diagnostics [%s] -> [%s]%s"
                % (kind, format_key(key), prior, current, suffix),
                file=sys.stderr,
            )
        if len(payload_regressions) > args.max_report:
            print(
                "error:   ... and %d more"
                % (len(payload_regressions) - args.max_report),
                file=sys.stderr,
            )
    if failed:
        return 1

    baseline_failing = sum(
        1
        for row in baseline_rows.values()
        if is_failing(row.get("jsStatus")) or is_failing(row.get("dtsStatus"))
    )
    current_failing = sum(
        1
        for row in current_rows.values()
        if is_failing(row.get("jsStatus")) or is_failing(row.get("dtsStatus"))
    )
    print(
        "Emit regression set OK: %d row(s) compared, failing rows %d -> %d "
        "(no row newly failing; no oracle-clean TSZ diagnostic grew or reordered)"
        % (len(current_rows), baseline_failing, current_failing)
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
