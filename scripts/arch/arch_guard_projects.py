"""Compatibility shim for the singular project guard module.

Project-row guard logic lives in `arch_guard_project.py`. Keep this module as a
thin re-export so older local commands and imports continue to use the owner.
"""

from arch_guard_project import *  # noqa: F401,F403
