#!/usr/bin/env python3
"""Validate the checked-in TypeScript cache and candidate-domain manifest."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from lib.cache_domain import (
    CacheDomainValidationError,
    load_json_object,
    resolve_pinned_typescript_version,
    validate_cache_domain,
)


HERE = Path(__file__).resolve().parent


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Validate TypeScript cache/domain version and partition invariants."
    )
    parser.add_argument(
        "--cache",
        type=Path,
        default=HERE / "tsc-cache-full.json",
        help="TSC cache JSON path",
    )
    parser.add_argument(
        "--domain",
        type=Path,
        default=HERE / "conformance-domain.json",
        help="Conformance domain JSON path",
    )
    parser.add_argument(
        "--versions",
        type=Path,
        default=HERE / "typescript-versions.json",
        help="Pinned TypeScript version manifest",
    )
    args = parser.parse_args(argv)

    try:
        cache = load_json_object(args.cache, "TSC cache")
        domain = load_json_object(args.domain, "conformance domain")
        versions = load_json_object(args.versions, "TypeScript version manifest")
        pinned_version = resolve_pinned_typescript_version(versions)
        summary = validate_cache_domain(cache, domain, pinned_version)
    except CacheDomainValidationError as error:
        for message in error.errors:
            print(f"error: {message}", file=sys.stderr)
        return 1

    print(
        f"ok: TypeScript {summary.typescript_version} domain has "
        f"{summary.candidates} candidates = {summary.runnable} runnable + "
        f"{summary.unsupported} unsupported + {summary.skipped} skipped"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
