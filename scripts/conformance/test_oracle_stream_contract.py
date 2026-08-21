#!/usr/bin/env python3
"""Executable contract for the manual oracle wrapper's process streams."""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
ORACLE_WRAPPER = ROOT / "scripts/conformance/oracle.sh"


class OracleStreamContractTests(unittest.TestCase):
    def test_stdout_is_only_the_native_compiler_stream(self) -> None:
        with tempfile.TemporaryDirectory(prefix="tsz-oracle-stream-") as raw_root:
            root = Path(raw_root)
            wrapper = root / "scripts/conformance/oracle.sh"
            ensure = root / "scripts/setup/ensure-pinned-typescript.sh"
            resolver = root / "scripts/emit/resolve-oracle.mjs"
            native = root / "native oracle/bin/tsc"
            for path in (wrapper, ensure, resolver, native):
                path.parent.mkdir(parents=True, exist_ok=True)

            shutil.copy2(ORACLE_WRAPPER, wrapper)
            ensure.write_text(
                "#!/usr/bin/env bash\n"
                "set -euo pipefail\n"
                'echo "$1 TypeScript version: 7.0.2"\n'
                'echo "$1 TypeScript lib dir: $1/node_modules/typescript/lib"\n'
                'echo "package verification detail" >&2\n',
                encoding="utf-8",
            )
            oracle_payload = json.dumps(
                {
                    "binaryPath": str(native),
                    "provenance": {"version": "7.0.2"},
                }
            ) + "\n"
            resolver.write_text(
                "import process from 'node:process';\n"
                "process.stderr.write('resolver provenance detail\\n');\n"
                f"process.stdout.write({json.dumps(oracle_payload)});\n",
                encoding="utf-8",
            )
            native.write_text(
                "#!/usr/bin/env bash\n"
                "printf '%s\\n' 'case.ts(1,1): error TS2322: compiler stdout'\n"
                "printf 'compiler argv:' >&2\n"
                "printf ' <%s>' \"$@\" >&2\n"
                "printf '\\n' >&2\n"
                "printf '%s\\n' 'compiler stderr' >&2\n"
                "exit 1\n",
                encoding="utf-8",
            )
            for executable in (wrapper, ensure, native):
                executable.chmod(0o755)
            (root / "case.ts").write_text("const value: string = 1;\n", encoding="utf-8")

            result = subprocess.run(
                [str(wrapper), "case.ts", "--strict"],
                cwd=root,
                capture_output=True,
                text=True,
                encoding="utf-8",
                check=False,
            )

            self.assertEqual(1, result.returncode, result.stderr)
            self.assertEqual(
                "case.ts(1,1): error TS2322: compiler stdout\n", result.stdout
            )
            self.assertNotIn("TypeScript version", result.stdout)
            self.assertNotIn("TypeScript lib dir", result.stdout)
            self.assertNotIn("resolver provenance", result.stdout)
            self.assertIn("TypeScript version: 7.0.2", result.stderr)
            self.assertIn("TypeScript lib dir:", result.stderr)
            self.assertIn("package verification detail", result.stderr)
            self.assertIn("resolver provenance detail", result.stderr)
            self.assertIn("# oracle: typescript@7.0.2", result.stderr)
            self.assertIn(
                "compiler argv: <--noEmit> <--pretty> <false> <--singleThreaded> "
                "<--stableTypeOrdering> <true> <--strict> <case.ts>",
                result.stderr,
            )
            self.assertIn("compiler stderr", result.stderr)


if __name__ == "__main__":
    unittest.main()
