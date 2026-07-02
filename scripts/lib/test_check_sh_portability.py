#!/usr/bin/env python3
"""Unit tests for the Bash 3.2 portability guard.

Validates that `check-sh-portability.py` flags each Bash 4+ construct, ignores
documentation/comment mentions, scans extensionless Bash-shebang scripts, and
reports the live tree as clean.
"""

import importlib.util
import pathlib
import sys
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
GUARD_PATH = ROOT / "scripts" / "lib" / "check-sh-portability.py"


def _load_guard():
    spec = importlib.util.spec_from_file_location("check_sh_portability", GUARD_PATH)
    module = importlib.util.module_from_spec(spec)
    # Register before exec so dataclass forward-ref annotations resolve.
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


guard = _load_guard()


class PortabilityGuardTests(unittest.TestCase):
    def _scan_snippet(self, body, name="probe.sh"):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / name).write_text(body, encoding="utf-8")
            return guard.find_violations(root)

    def _rule_names(self, violations):
        return {v.rule.name for v in violations}

    def test_live_tree_is_clean(self):
        """The committed scripts/ tree must stay 3.2-compatible."""
        violations = guard.find_violations()
        detail = "\n".join(
            f"{v.path}:{v.lineno}: {v.rule.name}: {v.text.strip()}"
            for v in violations
        )
        self.assertEqual(violations, [], f"unexpected violations:\n{detail}")

    def test_flags_mapfile(self):
        v = self._scan_snippet("#!/usr/bin/env bash\nmapfile -t arr < <(gen)\n")
        self.assertIn("mapfile/readarray builtin", self._rule_names(v))

    def test_flags_readarray(self):
        v = self._scan_snippet("#!/usr/bin/env bash\nreadarray -t arr <file\n")
        self.assertIn("mapfile/readarray builtin", self._rule_names(v))

    def test_flags_associative_array(self):
        v = self._scan_snippet("#!/usr/bin/env bash\ndeclare -A table\n")
        self.assertIn("associative array (declare -A)", self._rule_names(v))

    def test_flags_associative_array_flag_cluster(self):
        v = self._scan_snippet("#!/usr/bin/env bash\nlocal -rA table\n")
        self.assertIn("associative array (declare -A)", self._rule_names(v))

    def test_flags_nameref(self):
        v = self._scan_snippet("#!/usr/bin/env bash\nlocal -n ref=target\n")
        self.assertIn("nameref (declare -n)", self._rule_names(v))

    def test_flags_case_modification(self):
        for expr in ("${name^^}", "${name,,}", "${name^}", "${name,}"):
            with self.subTest(expr=expr):
                v = self._scan_snippet(f"#!/usr/bin/env bash\necho {expr}\n")
                self.assertIn(
                    "case-modifying expansion (${v^^}/${v,,})",
                    self._rule_names(v),
                )

    def test_flags_wait_n(self):
        v = self._scan_snippet("#!/usr/bin/env bash\nwait -n\n")
        self.assertIn("wait -n", self._rule_names(v))

    def test_flags_coproc(self):
        v = self._scan_snippet("#!/usr/bin/env bash\ncoproc worker { run; }\n")
        self.assertIn("coproc", self._rule_names(v))

    def test_flags_append_both_redirect(self):
        v = self._scan_snippet("#!/usr/bin/env bash\nrun &>> log.txt\n")
        self.assertIn("&>> redirect", self._rule_names(v))

    def test_flags_pipe_both(self):
        v = self._scan_snippet("#!/usr/bin/env bash\ngen |& filter\n")
        self.assertIn("|& pipe", self._rule_names(v))

    def test_flags_negative_index(self):
        v = self._scan_snippet("#!/usr/bin/env bash\necho ${arr[-1]}\n")
        self.assertIn("negative array index", self._rule_names(v))

    def test_ignores_full_line_comment(self):
        v = self._scan_snippet(
            "#!/usr/bin/env bash\n# mapfile -t arr is Bash 4 only\necho ok\n"
        )
        self.assertEqual(v, [])

    def test_ignores_trailing_comment(self):
        v = self._scan_snippet(
            "#!/usr/bin/env bash\necho ok  # avoid mapfile here\n"
        )
        self.assertEqual(v, [])

    def test_does_not_cut_hash_inside_string(self):
        # A `#` inside quotes must not be treated as a comment start, so a real
        # construct after it is still flagged.
        v = self._scan_snippet(
            "#!/usr/bin/env bash\necho \"count #\" && mapfile -t arr <f\n"
        )
        self.assertIn("mapfile/readarray builtin", self._rule_names(v))

    def test_scans_extensionless_bash_shebang(self):
        v = self._scan_snippet("#!/bin/bash\nmapfile -t arr <f\n", name="pre-commit")
        self.assertIn("mapfile/readarray builtin", self._rule_names(v))

    def test_skips_extensionless_non_bash(self):
        v = self._scan_snippet("#!/bin/sh\nmapfile -t arr <f\n", name="posix-tool")
        # POSIX sh files are out of scope for this Bash guard.
        self.assertEqual(v, [])

    def test_skips_non_shell_extension(self):
        v = self._scan_snippet(
            "mapfile is a word in prose\n", name="notes.md"
        )
        self.assertEqual(v, [])


if __name__ == "__main__":
    unittest.main()
