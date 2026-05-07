/**
 * Section-level metadata for the auto-generated docs sections.
 * Rarely changes. Per-page metadata lives in `lua/rootbeer/<name>.intro.md`
 * frontmatter and is consumed by `scripts/lua2md.ts`.
 *
 * `categoryOrder` controls the order in which categories are rendered on
 * the index page and in the sidebar; categories not listed are appended
 * in alphabetical order.
 */

export interface SectionMeta {
  title: string;
  root: string;
  lead: string;
  categoryOrder?: string[];
}

export const sections: Record<string, SectionMeta> = {
  modules: {
    title: "Modules",
    root: "/modules/",
    lead: `Modules are the high-level, opinionated wrappers that turn a Lua table
into the configuration files, settings, and side effects you actually
want on disk. Each module is a thin layer over the [core API](/reference/core)
— \`rb.file()\`, \`rb.exec()\`, the format codecs — exposed as a small,
typed surface so your dotfiles read like declarations instead of glue.

Pull a module in with \`require("rootbeer.<name>")\`:

\`\`\`lua
local zsh = require("rootbeer.zsh")
local git = require("rootbeer.git")
\`\`\`

For per-machine variants, see [Profiles](/guide/profiles). To wire up
something rootbeer doesn't ship a module for, drop down to the
[core API](/reference/core).`,
    categoryOrder: ["Shell", "Developer Tools", "AI Coding", "Package Managers", "System"],
  },
  reference: {
    title: "Reference",
    root: "/reference/",
    lead: `The low-level surface that every [module](/modules/) is built on. Reach
for these primitives when you're writing your own integration, gluing
together modules, or doing one-off file/symlink/exec work.

\`\`\`lua
local rb = require("rootbeer")
\`\`\`

See also [data formats](/formats/) for codecs and [script writers](/scripts/)
for executable script helpers.`,
    categoryOrder: ["Core"],
  },
  formats: {
    title: "Data Formats",
    root: "/formats/",
    lead: `Rootbeer ships built-in codecs for the configuration formats you
actually encounter in dotfiles. Every codec is a sub-table on \`rb\` with
the same four-function shape:

\`\`\`lua
rb.<fmt>.encode(t)        -- table  → string
rb.<fmt>.decode(s)        -- string → table
rb.<fmt>.read(path)       -- path   → table   (slurp ∘ decode)
rb.<fmt>.write(path, t)   -- path, table → () (encode ∘ file)
\`\`\`

\`encode\` and \`decode\` are pure transformations. \`read\` slurps a file
synchronously at plan time — the file must exist when your script runs.
\`write\` is **deferred**: it appends a \`WriteFile\` op that runs during
\`rb apply\`, exactly like \`rb.file()\`. A trailing newline is always
added on \`write\` so the output is well-formed.

## Encoding rules

These apply to every codec. Format-specific behaviour is documented on
each codec's page.

- Tables with consecutive integer keys starting at \`1\` become arrays /
  sequences. All other tables become objects / maps / dictionaries.
- \`nil\` values are omitted from the output.
- \`NaN\` and \`Infinity\` are rejected by formats that cannot represent
  them (JSON, plist).
- Lua functions, threads, and userdata error on encode — they have no
  serialized form.`,
    categoryOrder: ["Codecs"],
  },
};
