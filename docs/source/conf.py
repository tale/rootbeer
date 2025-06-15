# Configuration file for the Sphinx documentation builder.
#
# For the full list of built-in configuration values, see the documentation:
# https://www.sphinx-doc.org/en/master/usage/configuration.html

# -- Project information -----------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#project-information

project = 'Rootbeer'
copyright = '2025, Aarnav Tale'
author = 'Aarnav Tale'

# -- General configuration ---------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#general-configuration

extensions = [
	'breathe',
	'myst_parser',
	'sphinx_lua_ls',
	'sphinx.ext.autodoc',
	'sphinx.ext.viewcode'
]
breathe_projects = {"Rootbeer": "../.doxygen/xml"}
breathe_default_project = "Rootbeer"
lua_ls_project_root = "../../"

templates_path = ['_templates']
exclude_patterns = ['_build', 'Thumbs.db', '.DS_Store']

myst_enable_extensions = [
	# "amsmath",
	"colon_fence",
	# "deflist",
	# "fieldlist",
	# "html_admonition",
	# "html_image",
	# "linkify",
	# "replacements",
	# "smartquotes",
	"substitution"
]

myst_substitutions = {
	"ghdir": "https://github.com/tale/rootbeer/tree/main",
}



# -- Options for HTML output -------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#options-for-html-output

html_theme = 'furo'
html_static_path = ['_static']
