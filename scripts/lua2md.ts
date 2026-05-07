/**
 * lua2md — Transform LuaLS doc.json export into VitePress API reference pages.
 *
 * Runs lua-language-server --doc to extract structured documentation,
 * then generates a Markdown page per module.
 *
 * Usage:
 *   pnpm lua2md
 *   tsx scripts/lua2md.ts lua/rootbeer -o docs/api
 */
import { readFile, writeFile, mkdir, access, constants } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { execSync } from "node:child_process";
import { exit } from "node:process";
import {
  modulesSection,
  referenceSection,
  type NavSection,
  type NavCategory,
} from "../.vitepress/nav.ts";

// ── LuaLS doc.json types ──────────────────────────────────────────

interface DocArg {
  name: string;
  view: string;
  desc?: string;
  rawdesc?: string;
}

interface DocReturn {
  name?: string;
  view: string;
  desc?: string;
  rawdesc?: string;
}

interface DocExtends {
  type: string;
  view?: string;
  args?: DocArg[];
  returns?: DocReturn[];
}

interface DocField {
  name: string;
  view: string;
  desc?: string;
  rawdesc?: string;
  extends?: DocExtends;
  visible?: string;
  deprecated?: boolean;
  file?: string;
}

interface DocDefine {
  type: string;
  file?: string;
  view?: string;
  desc?: string;
  rawdesc?: string;
  extends?: DocExtends;
}

interface DocEntry {
  name: string;
  type: string;
  view?: string;
  defines?: DocDefine[];
  fields?: DocField[];
}

// ── Internal model ────────────────────────────────────────────────

interface Param {
  name: string;
  view: string;
  desc: string;
  optional: boolean;
}

interface Return {
  view: string;
  desc: string;
}

interface Func {
  /** Fully-qualified name as the user calls it, e.g. `zsh.config`. */
  qualified: string;
  /** Short name, e.g. `config`. */
  name: string;
  desc: string;
  params: Param[];
  returns: Return[];
}

interface ClassField {
  name: string;
  view: string;
  desc: string;
  optional: boolean;
}

interface Klass {
  name: string;
  desc: string;
  fields: ClassField[];
}

interface ModulePage {
  /** Module key: file stem (e.g. `zsh`). */
  key: string;
  functions: Func[];
  classes: Klass[];
}

// ── Description normalization ─────────────────────────────────────

/**
 * Strip the leading single space prefix that LuaLS adds to every line
 * of a `--- foo` doc comment, while preserving the original line breaks.
 *
 * Multi-paragraph descriptions, fenced code blocks, lists, etc. all
 * survive intact so VitePress can render them as Markdown.
 */
function normalizeDesc(raw: string | undefined | null): string {
  if (!raw) return "";
  // LuaLS sometimes appends a "@*param* ..." block re-stating params.
  // Strip everything from the first such marker onwards — it's noise
  // that duplicates information we already render structurally.
  const cut = raw.search(/\n\s*@\*(?:param|return)\*/);
  const body = cut === -1 ? raw : raw.slice(0, cut);
  return body
    .split("\n")
    .map((line) => line.replace(/^ /, ""))
    .join("\n")
    .trim();
}

/** Single-line variant for short contexts (param/field cards). */
function inlineDesc(raw: string | undefined | null): string {
  return normalizeDesc(raw).replace(/\s+/g, " ").trim();
}

// ── Type view normalization ───────────────────────────────────────

/** Strip a single pair of balanced outer parentheses. */
function stripOuterParens(view: string): string {
  if (!view.startsWith("(") || !view.endsWith(")")) return view;
  let depth = 0;
  for (let i = 0; i < view.length; i++) {
    const c = view[i];
    if (c === "(") depth++;
    else if (c === ")") {
      depth--;
      if (depth === 0 && i !== view.length - 1) return view;
    }
  }
  return view.slice(1, -1);
}

/**
 * Normalize a type view as displayed to the user:
 *   - separate trailing `?` (used as the optional badge)
 *   - strip spurious wrapping parens
 *   - resolve LuaLS-truncated aliases (`...(+N)`) via the alias table
 */
function renderType(
  view: string,
  aliases: Map<string, string>,
): { view: string; optional: boolean } {
  let v = view.trim();
  let optional = false;
  if (v.endsWith("?")) {
    optional = true;
    v = v.slice(0, -1);
  }
  v = stripOuterParens(v).trim();
  // Resolve a single-token alias name to its expansion.
  if (aliases.has(v)) v = aliases.get(v)!;
  // LuaLS truncation marker `...(+N)` — the alias map already has the
  // full form, but if the view itself is the truncated string we leave
  // it alone (no alias name to look up).
  return { view: v, optional };
}

// ── Alias resolution ──────────────────────────────────────────────

/**
 * Build a map of view-string → expanded union view from `doc.alias`
 * entries. LuaLS truncates the inline `view` for long unions but stores
 * the full definition in `rawdesc` as a fenced code block.
 *
 * We index the map by both the alias's name (e.g. `mac.HotCornerAction`)
 * AND the truncated view (e.g. `"a"|"b"|"c"...(+6)`) because LuaLS
 * substitutes the alias inline at the field site, leaving us only the
 * truncated form to reverse.
 */
function buildAliasMap(entries: DocEntry[]): Map<string, string> {
  const map = new Map<string, string>();
  for (const entry of entries) {
    if (entry.type !== "type") continue;
    for (const def of entry.defines ?? []) {
      if (def.type !== "doc.alias") continue;
      const raw = def.rawdesc ?? def.desc ?? "";
      const lines = raw
        .split("\n")
        .map((l) => l.trim())
        .filter((l) => l.startsWith("|"));
      if (!lines.length) continue;
      const full = lines.map((l) => l.replace(/^\|\s*/, "").trim()).join(" | ");
      map.set(entry.name, full);
      if (def.view) map.set(def.view, full);
    }
  }
  return map;
}

// ── Function signature parsing ────────────────────────────────────

/**
 * Extract the fully-qualified function name from a LuaLS function view
 * like `function amp.config(cfg: amp.Config)`. Falls back to the raw
 * field name if the view doesn't match.
 */
function qualifiedName(view: string | undefined, fallback: string): string {
  if (!view) return fallback;
  const match = view.match(/^function\s+([\w.:]+)\s*\(/);
  return match ? match[1] : fallback;
}

/** Returns whose name is an English article are LuaLS misparses of
 *  `--- @return string The decoded value` — fold the name into desc. */
const ARTICLE_RE = /^(the|a|an)$/i;

function makeReturn(r: DocReturn, aliases: Map<string, string>): Return {
  const t = renderType(r.view, aliases);
  let desc = inlineDesc(r.rawdesc ?? r.desc);
  if (r.name && ARTICLE_RE.test(r.name)) {
    desc = `${r.name} ${desc}`.trim();
  } else if (r.name && desc) {
    desc = `\`${r.name}\` — ${desc}`;
  } else if (r.name) {
    desc = `\`${r.name}\``;
  }
  return { view: t.view, desc };
}

function makeParam(a: DocArg, aliases: Map<string, string>): Param {
  const t = renderType(a.view, aliases);
  return {
    name: a.name,
    view: t.view,
    desc: inlineDesc(a.rawdesc ?? a.desc),
    optional: t.optional,
  };
}

function makeFunc(
  qualified: string,
  name: string,
  rawdesc: string | undefined,
  ext: DocExtends,
  aliases: Map<string, string>,
): Func {
  const args = (ext.args ?? []).filter((a) => a.name !== "self");
  const params = args.map((a) => makeParam(a, aliases));
  const returns = (ext.returns ?? []).map((r) => makeReturn(r, aliases));
  return {
    qualified,
    name,
    desc: normalizeDesc(rawdesc),
    params,
    returns,
  };
}

// ── Module assembly ───────────────────────────────────────────────

function sourceFile(entry: DocEntry): string | null {
  for (const def of entry.defines ?? []) {
    if (def.file) return def.file;
  }
  return null;
}

function buildModules(entries: DocEntry[]): Map<string, ModulePage> {
  const aliases = buildAliasMap(entries);
  const modules = new Map<string, ModulePage>();

  function getModule(key: string): ModulePage {
    let mod = modules.get(key);
    if (!mod) {
      mod = { key, functions: [], classes: [] };
      modules.set(key, mod);
    }
    return mod;
  }

  // Module-table class names (the @class entry whose name matches the
  // file stem) — these are the "container" classes whose function fields
  // become module functions, and which themselves are not documented.
  const moduleNames = new Set<string>();
  for (const entry of entries) {
    if (entry.type !== "type") continue;
    const file = sourceFile(entry);
    if (!file) continue;
    const key = file.replace(/\.lua$/, "");
    if (entry.name === key) moduleNames.add(entry.name);
  }

  for (const entry of entries) {
    if (entry.type === "luals.config") continue;
    const file = sourceFile(entry);
    if (!file) continue;
    const key = file.replace(/\.lua$/, "");
    const mod = getModule(key);

    if (entry.type === "variable") {
      // Global function defined as a value, e.g. `rootbeer.file`.
      const def = entry.defines?.[0];
      if (!def?.extends || def.extends.type !== "function") continue;
      const short = entry.name.split(".").pop()!;
      const qual = qualifiedName(def.extends.view, entry.name);
      mod.functions.push(makeFunc(qual, short, def.rawdesc, def.extends, aliases));
    } else if (entry.type === "type") {
      // Skip the alias-only `doc.alias` entries — they're already
      // inlined wherever they appear by `renderType`.
      const onlyAlias = entry.defines?.length && entry.defines.every((d) => d.type === "doc.alias");
      if (onlyAlias) continue;

      const isModuleTable = moduleNames.has(entry.name);
      const fns: Func[] = [];
      const fields: ClassField[] = [];

      for (const f of entry.fields ?? []) {
        if (f.extends?.type === "function") {
          const qual = qualifiedName(f.extends.view, `${entry.name}.${f.name}`);
          fns.push(makeFunc(qual, f.name, f.rawdesc, f.extends, aliases));
        } else {
          const t = renderType(f.view, aliases);
          fields.push({
            name: f.name,
            view: t.view,
            desc: inlineDesc(f.rawdesc ?? f.desc),
            optional: t.optional,
          });
        }
      }

      mod.functions.push(...fns);
      if (!isModuleTable && fields.length) {
        mod.classes.push({
          name: entry.name,
          desc: normalizeDesc(entry.defines?.find((d) => d.type === "doc.class")?.rawdesc),
          fields,
        });
      }
    }
  }

  // Deduplicate functions by qualified name within each module.
  for (const mod of modules.values()) {
    const seen = new Set<string>();
    mod.functions = mod.functions.filter((f) => {
      if (seen.has(f.qualified)) return false;
      seen.add(f.qualified);
      return true;
    });
    // Stable, predictable order.
    mod.functions.sort((a, b) => a.qualified.localeCompare(b.qualified));
    mod.classes.sort((a, b) => a.name.localeCompare(b.name));
  }

  return modules;
}

// ── Rendering ─────────────────────────────────────────────────────

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/**
 * In-page anchor slug. We emit explicit `<a id="…">` markers at each
 * function/class heading so the slug is stable regardless of which
 * markdown engine VitePress uses.
 */
function slug(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
}

/**
 * Render a type view as escaped HTML, wrapping any in-page class
 * references in anchor links so users can jump straight from a field's
 * type to its definition further down the page.
 */
function renderTypeHtml(view: string, anchors: Map<string, string>): string {
  if (!anchors.size) return escapeHtml(view);
  // Longest names first so `mac.HotCornerAction` wins over `mac.HotCorner`.
  const names = Array.from(anchors.keys()).sort((a, b) => b.length - a.length);
  const pattern = new RegExp(`(${names.map((n) => n.replace(/\./g, "\\.")).join("|")})`, "g");
  let out = "";
  let last = 0;
  for (const m of view.matchAll(pattern)) {
    const idx = m.index!;
    const name = m[0];
    // Boundary check: the match shouldn't be in the middle of a
    // longer identifier (e.g. `xmac.HotCornery`).
    const before = view[idx - 1] ?? "";
    const after = view[idx + name.length] ?? "";
    if (/[A-Za-z0-9_]/.test(before) || /[A-Za-z0-9_]/.test(after)) continue;
    out += escapeHtml(view.slice(last, idx));
    out += `<a href="#${anchors.get(name)}">${escapeHtml(name)}</a>`;
    last = idx + name.length;
  }
  out += escapeHtml(view.slice(last));
  return out;
}

/**
 * Convert short inline text (param descs) to HTML.
 * Supports backtick `code` spans and HTML-escapes everything else.
 */
function inlineMarkdownToHtml(s: string): string {
  if (!s) return "";
  return s.replace(/`([^`]+)`|([^`]+)/g, (_, code, text) => {
    if (code !== undefined) return `<code>${escapeHtml(code)}</code>`;
    return escapeHtml(text);
  });
}

function renderField(f: ClassField, anchors: Map<string, string>): string {
  const out: string[] = [];
  out.push(`<div class="api-field">`);
  out.push(`  <div class="api-field-header">`);
  out.push(`    <code class="api-field-name">${escapeHtml(f.name)}</code>`);
  out.push(`    <code class="api-field-type">${renderTypeHtml(f.view, anchors)}</code>`);
  if (f.optional) {
    out.push(`    <span class="api-field-badge">optional</span>`);
  }
  out.push(`  </div>`);
  if (f.desc) {
    out.push(`  <div class="api-field-desc">${inlineMarkdownToHtml(f.desc)}</div>`);
  }
  out.push(`</div>`);
  return out.join("\n");
}

function renderFieldList(fields: ClassField[], anchors: Map<string, string>): string {
  const out: string[] = [];
  out.push(`<div class="api-fields">`);
  out.push("");
  for (let i = 0; i < fields.length; i++) {
    out.push(renderField(fields[i], anchors));
    if (i < fields.length - 1) out.push("");
  }
  out.push("");
  out.push(`</div>`);
  return out.join("\n");
}

function paramsToFields(params: Param[]): ClassField[] {
  return params.map((p) => ({
    name: p.name,
    view: p.view,
    desc: p.desc,
    optional: p.optional,
  }));
}

function renderReturns(returns: Return[], anchors: Map<string, string>): string {
  const out: string[] = [];
  for (const r of returns) {
    const desc = r.desc ? ` — ${inlineMarkdownToHtml(r.desc)}` : "";
    out.push(
      `<div class="api-returns"><code>${renderTypeHtml(r.view, anchors)}</code>${desc}</div>`,
    );
  }
  return out.join("\n");
}

function renderFunc(
  fn: Func,
  classByName: Map<string, Klass>,
  anchors: Map<string, string>,
): string {
  const out: string[] = [];
  const sig = `${fn.qualified}(${fn.params.map((p) => p.name).join(", ")})`;
  out.push(`<a id="${slug(fn.qualified)}"></a>`);
  out.push(`### \`${sig}\``);
  out.push("");
  if (fn.desc) {
    out.push(fn.desc);
    out.push("");
  }

  if (fn.params.length) {
    // Common pattern: a single param whose type is a sibling class.
    // Inline the class fields directly so the user sees the actual
    // shape of the table they pass.
    const inlinable = fn.params.length === 1 && classByName.has(fn.params[0].view);
    if (inlinable) {
      const cls = classByName.get(fn.params[0].view)!;
      out.push(`**Parameters** — \`${fn.params[0].name}: ${cls.name}\``);
      out.push("");
      out.push(renderFieldList(cls.fields, anchors));
      out.push("");
    } else {
      out.push(`**Parameters**`);
      out.push("");
      out.push(renderFieldList(paramsToFields(fn.params), anchors));
      out.push("");
    }
  }
  if (fn.returns.length) {
    out.push(`**Returns**`);
    out.push("");
    out.push(renderReturns(fn.returns, anchors));
    out.push("");
  }
  return (
    out
      .join("\n")
      .replace(/\n{3,}/g, "\n\n")
      .trimEnd() + "\n"
  );
}

function renderClass(cls: Klass, anchors: Map<string, string>): string {
  const out: string[] = [];
  out.push(`<a id="${slug(cls.name)}"></a>`);
  out.push(`### \`${cls.name}\``);
  out.push("");
  if (cls.desc) {
    out.push(cls.desc);
    out.push("");
  }
  if (cls.fields.length) {
    out.push(renderFieldList(cls.fields, anchors));
  }
  return (
    out
      .join("\n")
      .replace(/\n{3,}/g, "\n\n")
      .trimEnd() + "\n"
  );
}

function renderModule(mod: ModulePage): string {
  const classByName = new Map<string, Klass>();
  for (const cls of mod.classes) classByName.set(cls.name, cls);

  // Track classes that get inlined into a single-param function so we
  // can omit their standalone duplicate section below.
  const inlined = new Set<string>();
  for (const fn of mod.functions) {
    if (fn.params.length === 1 && classByName.has(fn.params[0].view)) {
      inlined.add(fn.params[0].view);
    }
  }

  // Anchor map: every class on the page (including inlined ones, since
  // they may still be referenced by other functions' returns or fields)
  // gets a stable in-page anchor we link to from type cells.
  const anchors = new Map<string, string>();
  for (const cls of mod.classes) anchors.set(cls.name, slug(cls.name));

  const sections: string[] = [];
  for (const fn of mod.functions) sections.push(renderFunc(fn, classByName, anchors));
  for (const cls of mod.classes) {
    if (inlined.has(cls.name)) continue;
    sections.push(renderClass(cls, anchors));
  }
  return sections.join("\n");
}

// ── Doc-coverage lint ────────────────────────────────────────────

interface LintWarning {
  module: string;
  message: string;
}

/**
 * Walk the assembled module model and collect warnings about missing
 * documentation. Reported to stderr; never fatal.
 */
function lintModules(modules: Map<string, ModulePage>): LintWarning[] {
  const warnings: LintWarning[] = [];
  for (const mod of modules.values()) {
    const classNames = new Set(mod.classes.map((c) => c.name));
    for (const fn of mod.functions) {
      if (!fn.desc) {
        warnings.push({
          module: mod.key,
          message: `function \`${fn.qualified}\` has no description`,
        });
      }
      for (const p of fn.params) {
        // Single-class-typed params get inlined as the table shape;
        // the class fields carry the docs, so the param-level desc
        // is redundant. Skip those.
        const inlined = fn.params.length === 1 && classNames.has(p.view);
        if (!p.desc && !inlined) {
          warnings.push({
            module: mod.key,
            message: `param \`${fn.qualified}#${p.name}\` has no description`,
          });
        }
      }
    }
    for (const cls of mod.classes) {
      for (const f of cls.fields) {
        if (!f.desc) {
          warnings.push({
            module: mod.key,
            message: `field \`${cls.name}.${f.name}\` has no description`,
          });
        }
      }
    }
  }
  return warnings;
}

function reportLint(warnings: LintWarning[]) {
  if (!warnings.length) return;
  console.error(`\nlua2md: ${warnings.length} doc-coverage warning(s):`);
  for (const w of warnings) {
    console.error(`  [${w.module}] ${w.message}`);
  }
}

// ── Section index pages ──────────────────────────────────────────

function renderCategoryTable(section: NavSection, cat: NavCategory): string {
  const rows = cat.entries
    .map((e) => {
      const link = e.slug.startsWith("/") ? e.slug : section.root + e.slug;
      return `| [${e.text}](${link}) | ${e.desc} |`;
    })
    .join("\n");
  return `## ${cat.text}\n\n| Page | Description |\n| ---- | ----------- |\n${rows}`;
}

function renderSectionIndex(section: NavSection): string {
  const parts: string[] = [];
  parts.push(`# ${section.title}`);
  parts.push("");
  parts.push(section.lead);
  parts.push("");
  for (const cat of section.categories) {
    parts.push(renderCategoryTable(section, cat));
    parts.push("");
  }
  // Banner so it's clear this file is generated.
  const banner =
    "<!-- AUTO-GENERATED by scripts/lua2md.ts — edit .vitepress/nav.ts and re-run `pnpm lua2md` -->\n";
  return banner + parts.join("\n").trimEnd() + "\n";
}

async function writeSectionIndex(docsRoot: string, section: NavSection) {
  const path = join(docsRoot, section.root.replace(/^\/|\/$/g, ""), "index.md");
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, renderSectionIndex(section));
  console.error(`  wrote ${path}`);
}

// ── README sync ──────────────────────────────────────────────────

/**
 * Sync the "Quick look" Lua sample from `docs/index.md` into README.md
 * between `<!-- LUA_QUICKLOOK_START -->` and `<!-- LUA_QUICKLOOK_END -->`
 * markers. No-op if either side is missing.
 */
async function syncReadmeQuicklook(repoRoot: string) {
  const indexPath = join(repoRoot, "docs/index.md");
  const readmePath = join(repoRoot, "README.md");

  let homepage: string;
  let readme: string;
  try {
    homepage = await readFile(indexPath, "utf8");
    readme = await readFile(readmePath, "utf8");
  } catch {
    return;
  }

  const blockMatch = homepage.match(/##\s+Quick look[\s\S]*?(```lua[\s\S]*?```)/);
  if (!blockMatch) return;
  const block = blockMatch[1];

  const startTag = "<!-- LUA_QUICKLOOK_START -->";
  const endTag = "<!-- LUA_QUICKLOOK_END -->";
  const re = new RegExp(`${startTag}[\\s\\S]*?${endTag}`);
  if (!re.test(readme)) return;

  const updated = readme.replace(re, `${startTag}\n\n${block}\n\n${endTag}`);
  if (updated === readme) return;
  await writeFile(readmePath, updated);
  console.error(`  wrote ${readmePath} (quicklook)`);
}

// ── Main ──────────────────────────────────────────────────────────

const args = process.argv.slice(2);
let root: string | null = null;
let outDir: string | null = null;

for (let i = 0; i < args.length; i++) {
  if (args[i] === "-o" || args[i] === "--output") {
    outDir = args[++i];
  } else if (!root) {
    root = args[i];
  }
}

if (!root) {
  console.error("Usage: lua2md <lua-root> [-o <output-dir>]");
  process.exit(1);
}

const absRoot = resolve(root);
const tmpDir = execSync("mktemp -d", { encoding: "utf8" }).trim();

try {
  execSync(`lua-language-server --doc=${absRoot} --doc_out_path=${tmpDir}`, {
    stdio: ["pipe", "pipe", "pipe"],
  });
} catch (e: any) {
  console.error("Failed to run lua-language-server:", e.message);
  exit(1);
}

const jsonPath = join(tmpDir, "doc.json");
try {
  await access(jsonPath, constants.R_OK);
} catch {
  console.error("lua-language-server did not produce doc.json");
  exit(1);
}

const jsonData = await readFile(jsonPath, "utf8");
const entries: DocEntry[] = JSON.parse(jsonData);
const modules = buildModules(entries);

if (!modules.size) {
  console.error("No documented modules found.");
  exit(1);
}

if (!outDir) {
  for (const mod of modules.values()) {
    process.stdout.write(renderModule(mod));
    process.stdout.write("\n---\n\n");
  }
  exit(0);
}

await mkdir(outDir, { recursive: true });
for (const mod of modules.values()) {
  const file = `${mod.key}.md`;
  await writeFile(join(outDir, file), renderModule(mod));
  console.error(`  wrote ${join(outDir, file)}`);
}

// Generated section index pages: derive the docs root from outDir.
// outDir is conventionally `<docs>/api/_generated`, so the docs root
// is two directories up.
const docsRoot = resolve(outDir, "..", "..");
await writeSectionIndex(docsRoot, modulesSection);
await writeSectionIndex(docsRoot, referenceSection);

// Sync README "quick look" from docs/index.md (repo root is one above docs).
await syncReadmeQuicklook(resolve(docsRoot, ".."));

// Doc-coverage report (always runs, never fatal).
reportLint(lintModules(modules));
