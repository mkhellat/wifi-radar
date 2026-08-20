"""Sphinx configuration for wifi-radar documentation."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent / "src"))

project = "wifi-radar"
copyright = "2026, wifi-radar contributors"  # noqa: A001
author = "wifi-radar contributors"
release = "0.1.0"

extensions = [
    "sphinx.ext.autodoc",
    "sphinx.ext.napoleon",
    "sphinx.ext.viewcode",
    "sphinx.ext.intersphinx",
    "sphinx_autodoc_typehints",
    "myst_parser",
]

templates_path = ["_templates"]
exclude_patterns = ["_build", "Thumbs.db", ".DS_Store"]

html_theme = "sphinx_rtd_theme"
html_static_path = ["_static"]
html_title = "wifi-radar documentation"

autodoc_member_order = "bysource"
autodoc_typehints = "description"
always_document_param_types = True

intersphinx_mapping = {
    "python": ("https://docs.python.org/3", None),
}

myst_enable_extensions = [
    "colon_fence",
    "fieldlist",
    "tasklist",
]

source_suffix = {
    ".rst": "restructuredtext",
    ".md": "markdown",
}
