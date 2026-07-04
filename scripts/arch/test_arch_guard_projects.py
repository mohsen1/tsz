"""Compatibility entrypoint for the singular project guard test modules."""

import unittest


def _compat_suite():
    suite = unittest.TestSuite()
    for module_name in ("test_arch_guard_lsp", "test_arch_guard_project"):
        suite.addTests(unittest.defaultTestLoader.loadTestsFromName(module_name))
    return suite

if __name__ == "__main__":
    raise SystemExit(not unittest.TextTestRunner().run(_compat_suite()).wasSuccessful())
