"""Contract tests for emit-runner compiler binary selection."""

import os
import pathlib
import subprocess
import tempfile
import unittest


RUNNER = pathlib.Path(__file__).with_name("run.sh")


class EmitBinaryResolutionTests(unittest.TestCase):
    def setUp(self):
        self.temporary_directory = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.temporary_directory.name)
        self.dist = self.root / ".target" / "dist-fast" / "tsz"
        self.release = self.root / ".target" / "release" / "tsz"
        self.dist.parent.mkdir(parents=True)
        self.release.parent.mkdir(parents=True)

    def tearDown(self):
        self.temporary_directory.cleanup()

    @staticmethod
    def make_executable(path: pathlib.Path, payload: bytes) -> None:
        path.write_bytes(payload)
        path.chmod(0o755)

    def resolve(self, command: str) -> subprocess.CompletedProcess[str]:
        environment = os.environ.copy()
        environment.pop("TSZ_BIN", None)
        environment.pop("CARGO_TARGET_DIR", None)
        return subprocess.run(
            [
                "bash",
                "-c",
                'source "$1"; ROOT_DIR="$2"; ' + command,
                "emit-binary-resolution-test",
                str(RUNNER),
                str(self.root),
            ],
            check=False,
            capture_output=True,
            text=True,
            env=environment,
        )

    def test_skip_build_main_rejects_older_implicit_binary(self):
        self.make_executable(self.dist, b"old dist-fast binary")
        self.make_executable(self.release, b"new release binary")
        os.utime(self.dist, (1_000_000_000, 1_000_000_000))
        os.utime(self.release, (2_000_000_000, 2_000_000_000))
        (self.root / "TypeScript" / "tests" / "baselines" / "reference").mkdir(
            parents=True
        )

        result = self.resolve("main --skip-build --max=1")

        self.assertEqual(result.returncode, 2)
        self.assertIn("multiple different tsz binaries", result.stderr)
        self.assertIn(str(self.dist), result.stderr)
        self.assertIn(str(self.release), result.stderr)
        self.assertIn("Set TSZ_BIN", result.stderr)

    def test_explicit_binary_is_authoritative(self):
        self.make_executable(self.dist, b"old dist-fast binary")
        self.make_executable(self.release, b"new release binary")

        result = self.resolve(
            'TSZ_BIN="$ROOT_DIR/.target/release/tsz"; export TSZ_BIN; '
            'resolve_tsz_binary 1; printf "%s" "$TSZ_BIN"'
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(result.stdout.endswith(str(self.release)), result.stdout)

    def test_invalid_explicit_binary_does_not_fall_back(self):
        self.make_executable(self.dist, b"implicit binary")

        result = self.resolve(
            'TSZ_BIN="$ROOT_DIR/missing/tsz"; export TSZ_BIN; resolve_tsz_binary 1'
        )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("explicit TSZ_BIN is not executable", result.stderr)

    def test_identical_implicit_binaries_are_unambiguous(self):
        payload = b"same compiler binary"
        self.make_executable(self.dist, payload)
        self.make_executable(self.release, payload)

        result = self.resolve('resolve_tsz_binary 1; printf "%s" "$TSZ_BIN"')

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(result.stdout.endswith(str(self.dist)), result.stdout)


if __name__ == "__main__":
    unittest.main()
