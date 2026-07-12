#!/usr/bin/env python3
"""Focused repair tests for ensure-pinned-typescript.sh."""

from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
ENSURE_SCRIPT = ROOT / "scripts" / "setup" / "ensure-pinned-typescript.sh"
LIB_RESOLVER = ROOT / "scripts" / "setup" / "resolve-typescript-lib-dir.mjs"
VERSIONS_FILE = ROOT / "scripts" / "conformance" / "typescript-versions.json"


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value) + "\n", encoding="utf-8")


class EnsurePinnedTypeScriptTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        versions = json.loads(VERSIONS_FILE.read_text(encoding="utf-8"))
        current = versions["current"]
        cls.pinned_version = versions["mappings"][current]["npm"]
        platform = subprocess.check_output(
            ["node", "-p", "process.platform + ' ' + process.arch"],
            cwd=ROOT,
            text=True,
        ).strip()
        cls.platform, cls.arch = platform.split()
        cls.platform_package = (
            f"@typescript/typescript-{cls.platform}-{cls.arch}"
        )

    def make_fixture(self, initial_platform_version: str | None):
        temp = tempfile.TemporaryDirectory(prefix="tsz-ensure-typescript-")
        fixture_root = Path(temp.name)
        setup_dir = fixture_root / "scripts" / "setup"
        conformance_dir = fixture_root / "scripts" / "conformance"
        setup_dir.mkdir(parents=True)
        conformance_dir.mkdir(parents=True)
        shutil.copy2(ENSURE_SCRIPT, setup_dir / ENSURE_SCRIPT.name)
        shutil.copy2(LIB_RESOLVER, setup_dir / LIB_RESOLVER.name)
        shutil.copy2(VERSIONS_FILE, conformance_dir / VERSIONS_FILE.name)

        project = fixture_root / "project"
        wrapper = project / "node_modules" / "typescript"
        write_json(project / "package.json", {"private": True})
        write_json(
            wrapper / "package.json",
            {
                "name": "typescript",
                "version": self.pinned_version,
                "optionalDependencies": {
                    self.platform_package: self.pinned_version,
                },
            },
        )
        wrapper.joinpath("lib").mkdir(parents=True)
        wrapper.joinpath("lib", "tsc.js").write_text(
            f'console.log("Version {self.pinned_version}");\n',
            encoding="utf-8",
        )

        platform_root = project / "node_modules" / Path(self.platform_package)
        if initial_platform_version is not None:
            write_json(
                platform_root / "package.json",
                {
                    "name": self.platform_package,
                    "version": initial_platform_version,
                    "exports": {"./package.json": "./package.json"},
                },
            )
            platform_root.joinpath("lib").mkdir(parents=True)
            platform_root.joinpath("lib", "lib.d.ts").write_text("\n")
            platform_root.joinpath("lib", "lib.es5.d.ts").write_text("\n")

        fake_bin = fixture_root / "fake-bin"
        fake_bin.mkdir()
        npm_log = fixture_root / "npm.log"
        fake_npm = fake_bin / "npm"
        fake_npm.write_text(
            """#!/usr/bin/env node
const fs = require("fs");
const path = require("path");
const spec = process.argv[process.argv.length - 1];
const separator = spec.lastIndexOf("@");
const name = spec.slice(0, separator);
const version = spec.slice(separator + 1);
if (!name.startsWith("@typescript/typescript-")) process.exit(9);
const packageRoot = path.join(process.cwd(), "node_modules", ...name.split("/"));
fs.mkdirSync(path.join(packageRoot, "lib"), { recursive: true });
fs.writeFileSync(path.join(packageRoot, "package.json"), JSON.stringify({
  name,
  version,
  exports: { "./package.json": "./package.json" },
}) + "\\n");
fs.writeFileSync(path.join(packageRoot, "lib", "lib.d.ts"), "\\n");
fs.writeFileSync(path.join(packageRoot, "lib", "lib.es5.d.ts"), "\\n");
fs.appendFileSync(process.env.FAKE_NPM_LOG, spec + "\\n");
""",
            encoding="utf-8",
        )
        fake_npm.chmod(0o755)
        env = os.environ.copy()
        env["PATH"] = f"{fake_bin}{os.pathsep}{env['PATH']}"
        env["FAKE_NPM_LOG"] = str(npm_log)
        return temp, project, platform_root, npm_log, env

    def test_repairs_missing_platform_optional_dependency(self) -> None:
        temp, project, platform_root, npm_log, env = self.make_fixture(None)
        with temp:
            result = subprocess.run(
                [str(project.parents[0] / "scripts" / "setup" / ENSURE_SCRIPT.name), str(project)],
                text=True,
                capture_output=True,
                env=env,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            installed = json.loads(
                platform_root.joinpath("package.json").read_text(encoding="utf-8")
            )
            self.assertEqual(installed["version"], self.pinned_version)
            self.assertEqual(
                npm_log.read_text(encoding="utf-8").strip(),
                f"{self.platform_package}@{self.pinned_version}",
            )

    def test_repairs_mismatched_platform_optional_dependency(self) -> None:
        temp, project, platform_root, npm_log, env = self.make_fixture("0.0.0")
        with temp:
            result = subprocess.run(
                [str(project.parents[0] / "scripts" / "setup" / ENSURE_SCRIPT.name), str(project)],
                text=True,
                capture_output=True,
                env=env,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            installed = json.loads(
                platform_root.joinpath("package.json").read_text(encoding="utf-8")
            )
            self.assertEqual(installed["version"], self.pinned_version)
            self.assertEqual(
                npm_log.read_text(encoding="utf-8").strip(),
                f"{self.platform_package}@{self.pinned_version}",
            )


if __name__ == "__main__":
    unittest.main()
