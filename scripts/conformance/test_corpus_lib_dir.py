"""Tests for `corpus-lib-dir.sh` — the deterministic conformance lib resolver.

Conformance fingerprints only match the checked-in tsc cache when tsz checks
against the same pinned-version `lib.*.d.ts` set. `corpus-lib-dir.sh` picks that
directory deterministically so local and CI agree (#13400). These tests pin the
candidate priority order, the fail-loud behavior, and the `TSZ_LIB_DIR`
override contract without needing the full TypeScript corpus.
"""

import os
import platform
import json
import subprocess
import tempfile
import unittest
from pathlib import Path

SCRIPT_PATH = Path(__file__).with_name("corpus-lib-dir.sh")


def run_resolver(repo_root, env_overrides=None):
    """Invoke the resolver against `repo_root`. Returns (returncode, stdout, stderr)."""
    env = dict(os.environ)
    # Start from a clean slate so an ambient TSZ_LIB_DIR can't leak into cases
    # that mean to leave it unset.
    env.pop("TSZ_LIB_DIR", None)
    if env_overrides:
        env.update(env_overrides)
    proc = subprocess.run(
        ["bash", str(SCRIPT_PATH), "--repo-root", str(repo_root)],
        capture_output=True,
        text=True,
        env=env,
    )
    return proc.returncode, proc.stdout.strip(), proc.stderr


def make_dir(root, rel):
    path = Path(root) / rel
    path.mkdir(parents=True, exist_ok=True)
    return path


def make_compiled_lib_dir(root, rel):
    path = make_dir(root, rel)
    path.joinpath("lib.d.ts").write_text("/// <reference lib=\"es5\" />\n")
    path.joinpath("lib.es5.d.ts").write_text("interface Array<T> {}\n")
    return path


def node_platform_arch():
    system = {"Darwin": "darwin", "Linux": "linux", "Windows": "win32"}[platform.system()]
    machine = platform.machine().lower()
    arch = {"arm64": "arm64", "aarch64": "arm64", "x86_64": "x64", "amd64": "x64"}[machine]
    return system, arch


def make_typescript_install(root, wrapper_version="7.0.2", platform_version="7.0.2"):
    wrapper = make_dir(root, "scripts/node_modules/typescript")
    wrapper.joinpath("package.json").write_text(
        json.dumps({"name": "typescript", "version": wrapper_version})
    )
    make_dir(root, "scripts/node_modules/typescript/lib")
    system, arch = node_platform_arch()
    package = make_compiled_lib_dir(
        root, f"scripts/node_modules/@typescript/typescript-{system}-{arch}/lib"
    ).parent
    package.joinpath("package.json").write_text(
        json.dumps({"name": f"@typescript/typescript-{system}-{arch}", "version": platform_version})
    )
    return package / "lib"


class CorpusLibDirTests(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        self.addCleanup(self._tmp.cleanup)

    def test_prefers_built_local(self):
        built = make_compiled_lib_dir(self.root, "TypeScript/built/local")
        # Lower-priority candidates also present; built/local must still win.
        make_compiled_lib_dir(self.root, "TypeScript/lib")
        code, out, _ = run_resolver(self.root)
        self.assertEqual(code, 0)
        self.assertEqual(out, str(built))

    def test_falls_back_to_typescript_lib(self):
        lib = make_compiled_lib_dir(self.root, "TypeScript/lib")
        code, out, _ = run_resolver(self.root)
        self.assertEqual(code, 0)
        self.assertEqual(out, str(lib))

    def test_falls_back_to_scripts_node_modules(self):
        scripts_lib = make_compiled_lib_dir(self.root, "scripts/node_modules/typescript/lib")
        code, out, _ = run_resolver(self.root)
        self.assertEqual(code, 0)
        self.assertEqual(out, str(scripts_lib))

    def test_resolves_typescript_7_platform_package(self):
        platform_lib = make_typescript_install(self.root)
        code, out, err = run_resolver(self.root)
        self.assertEqual(code, 0, msg=err)
        self.assertEqual(out, str(platform_lib.resolve()))

    def test_rejects_typescript_7_wrapper_without_platform_package(self):
        wrapper = make_dir(self.root, "scripts/node_modules/typescript")
        wrapper.joinpath("package.json").write_text(
            json.dumps({"name": "typescript", "version": "7.0.2"})
        )
        make_dir(self.root, "scripts/node_modules/typescript/lib")
        code, out, err = run_resolver(self.root)
        self.assertEqual(code, 1)
        self.assertEqual(out, "")
        self.assertIn("requires its platform optional dependency", err)

    def test_rejects_wrapper_platform_version_mismatch(self):
        make_typescript_install(self.root, platform_version="7.0.1")
        code, out, err = run_resolver(self.root)
        self.assertEqual(code, 1)
        self.assertEqual(out, "")
        self.assertIn("wrapper/platform version mismatch", err)

    def test_ignores_stray_root_node_modules(self):
        # A root node_modules/typescript at an arbitrary version is the exact
        # divergence source from #13400: it must NOT be selected. With no real
        # corpus candidate present, the resolver fails loudly instead.
        make_dir(self.root, "node_modules/typescript/lib")
        code, out, err = run_resolver(self.root)
        self.assertEqual(code, 1, msg=err)
        self.assertEqual(out, "")
        self.assertIn("no pinned-version TypeScript lib directory", err)

    def test_fails_loudly_when_nothing_present(self):
        code, out, err = run_resolver(self.root)
        self.assertEqual(code, 1)
        self.assertEqual(out, "")
        # Message is actionable: it names the three candidates and the npm hint.
        self.assertIn("TypeScript/built/local", err)
        self.assertIn("scripts/node_modules/@typescript/typescript-<platform>-<arch>/lib", err)
        self.assertIn("cd scripts && npm install", err)

    def test_does_not_select_src_lib(self):
        # src/lib uses the unprefixed `es2021.intl.d.ts` layout and would
        # mismatch every cached `lib.*.d.ts` fingerprint, so it is not a
        # candidate even when it is the only TypeScript dir present.
        make_dir(self.root, "TypeScript/src/lib")
        code, _, err = run_resolver(self.root)
        self.assertEqual(code, 1, msg=err)

    def test_explicit_override_wins(self):
        override = make_compiled_lib_dir(self.root, "custom-lib")
        # built/local is present but the explicit override must take precedence.
        make_compiled_lib_dir(self.root, "TypeScript/built/local")
        code, out, _ = run_resolver(
            self.root, env_overrides={"TSZ_LIB_DIR": str(override)}
        )
        self.assertEqual(code, 0)
        self.assertEqual(out, str(override))

    def test_stale_override_is_rejected(self):
        # A TSZ_LIB_DIR pointing at a missing path must fail loudly rather than
        # silently falling back (which would reintroduce the divergence).
        code, out, err = run_resolver(
            self.root,
            env_overrides={"TSZ_LIB_DIR": str(self.root / "does-not-exist")},
        )
        self.assertEqual(code, 1)
        self.assertEqual(out, "")
        self.assertIn("does not contain compiled TypeScript libs", err)

    def test_rejects_unknown_argument(self):
        proc = subprocess.run(
            ["bash", str(SCRIPT_PATH), "--bogus"],
            capture_output=True,
            text=True,
        )
        self.assertEqual(proc.returncode, 2)
        self.assertIn("unknown argument", proc.stderr)


if __name__ == "__main__":
    unittest.main()
