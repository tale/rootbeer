/**
 * Single source of truth for the Modules and Reference sections —
 * consumed by both `.vitepress/config.ts` (for nav/sidebar generation)
 * and `scripts/lua2md.ts` (for the `/modules/` and `/reference/`
 * index pages).
 */

export interface NavEntry {
  /** URL slug under the section root (e.g. `zsh` → `/modules/zsh`). */
  slug: string;
  /** Display text in sidebar / index table. */
  text: string;
  /** One-line description shown in the section index table. */
  desc: string;
}

export interface NavCategory {
  text: string;
  entries: NavEntry[];
}

export interface NavSection {
  /** Root path, with trailing slash (e.g. `/modules/`). */
  root: string;
  /** Section page title. */
  title: string;
  /** Lead paragraph for the index page (markdown). */
  lead: string;
  categories: NavCategory[];
}

export const modulesSection: NavSection = {
  root: "/modules/",
  title: "Modules",
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
  categories: [
    {
      text: "Shell",
      entries: [
        {
          slug: "zsh",
          text: "`zsh`",
          desc: "`~/.zshenv`, `.zprofile`, `.zshrc` from one table.",
        },
      ],
    },
    {
      text: "Developer Tools",
      entries: [
        { slug: "git", text: "`git`", desc: "`~/.gitconfig` and global gitignore." },
        { slug: "ssh", text: "`ssh`", desc: "`~/.ssh/config` host blocks and includes." },
      ],
    },
    {
      text: "AI Coding",
      entries: [
        { slug: "amp", text: "`amp`", desc: "Amp settings, MCP servers, AGENTS.md." },
        {
          slug: "claude-code",
          text: "`claude_code`",
          desc: "Claude Code settings, permissions, LSP.",
        },
      ],
    },
    {
      text: "Package Managers",
      entries: [
        {
          slug: "brew",
          text: "`brew`",
          desc: "Brewfile + `brew bundle` for formulae, casks, MAS.",
        },
      ],
    },
    {
      text: "System",
      entries: [
        {
          slug: "mac",
          text: "`mac`",
          desc: "macOS Dock, Finder, hot corners, hostname, Touch ID.",
        },
      ],
    },
  ],
};

export const referenceSection: NavSection = {
  root: "/reference/",
  title: "Reference",
  lead: `The low-level surface that every [module](/modules/) is built on. Reach
for these primitives when you're writing your own integration, gluing
together modules, or doing one-off file/symlink/exec work.

\`\`\`lua
local rb = require("rootbeer")
\`\`\``,
  categories: [
    {
      text: "Core",
      entries: [
        {
          slug: "core",
          text: "Core API",
          desc: "File ops, symlinks, exec, secrets, profile system.",
        },
        {
          slug: "host",
          text: "Host",
          desc: "The `rb.host` table — OS, arch, hostname, user.",
        },
      ],
    },
    {
      text: "Data Formats",
      entries: [
        {
          slug: "/formats/json",
          text: "`json`",
          desc: "Pretty-printed with 2-space indent.",
        },
        {
          slug: "/formats/toml",
          text: "`toml`",
          desc: "Datetimes decode as strings.",
        },
        {
          slug: "/formats/yaml",
          text: "`yaml`",
          desc: "Tags decode transparently to scalars.",
        },
        {
          slug: "/formats/plist",
          text: "`plist`",
          desc: "XML output; decode accepts XML or binary.",
        },
      ],
    },
    {
      text: "Script Writers",
      entries: [
        {
          slug: "/scripts/",
          text: "Script writers",
          desc: "`rb.scripts.bash` / `python` / `script` …",
        },
      ],
    },
  ],
};

/** Strip surrounding backticks for sidebar plain-text rendering. */
function plain(text: string): string {
  return text.replace(/`/g, "");
}

/** Build a VitePress sidebar item array for a section. */
export function sidebarFromSection(section: NavSection) {
  const items: any[] = [{ text: "Overview", link: section.root }];
  for (const cat of section.categories) {
    items.push({
      text: cat.text,
      collapsed: true,
      items: cat.entries.map((e) => ({
        text: plain(e.text),
        link: linkFor(section, e),
      })),
    });
  }
  return items;
}

/** Resolve an entry's full URL — slugs starting with `/` are absolute. */
export function linkFor(section: NavSection, entry: NavEntry): string {
  return entry.slug.startsWith("/") ? entry.slug : section.root + entry.slug;
}
