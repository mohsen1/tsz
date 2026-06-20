import importlib.util
import pathlib
import sys
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
ARCH_GUARD_PATH = ROOT / "scripts" / "arch" / "arch_guard.py"


def load_arch_script_module(module_name: str, path: pathlib.Path):
    """Load a `scripts/arch` Python module by file path.

    Registers the module in `sys.modules` before executing it so module-level
    `@dataclass` definitions can resolve their own `__module__`.
    """
    spec = importlib.util.spec_from_file_location(module_name, path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def load_arch_guard_module():
    return load_arch_script_module("arch_guard", ARCH_GUARD_PATH)
