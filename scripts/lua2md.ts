/**
 * lua2md — Extract LDoc-style comments from Lua modules and emit Markdown.
 *
 * Parses `---` doc comments with @class/@field/@param/@return/@usage tags,
 * then generates includable Markdown snippets per function for VitePress.
 *
 * Output structure (in docs/_generated/):
 *   shells.zsh.config.md   — snippet for zsh.config()
 *
 * Handwritten docs use: <!--@include: ./_generated/shells.zsh.config.md-->
 *
 * Usage:
 *   pnpm lua2md
 *   tsx scripts/lua2md.ts lua/rootbeer -o docs/_generated
 */
import { readFileSync, writeFileSync, mkdirSync, readdirSync, statSync } from "node:fs"
import { join, relative, dirname, extname, sep } from "node:path"

// ── Types ──────────────────────────────────────────────────────────

interface Param {
	name: string
	type: string
	desc: string
}

interface Return {
	type: string
	desc: string
}

interface Field {
	name: string
	type: string
	desc: string
}

interface ClassDef {
	name: string
	summary: string[]
	fields: Field[]
}

interface DocBlock {
	summary: string[]
	params: Param[]
	returns: Return[]
	fields: Field[]
	usage: string[]
	funcName?: string
}

interface Module {
	name: string
	path: string
	classes: Map<string, ClassDef>
	blocks: DocBlock[]
}

// ── Parsing ────────────────────────────────────────────────────────

/** Match a type that may contain generics and union pipes: table<string, string>, string|string[] */
const TYPE_RE = /(?:\S+<[^>]+>|\S+(?:\|(?:\S+<[^>]+>|\S+))*)/

function parseFile(filepath: string, root: string): Module | null {
	const rel = relative(root, filepath)
	const parts = rel.replace(/\.lua$/, "").split(sep)
	if (parts.at(-1) === "init") parts.pop()
	const modName = parts.join(".")

	const lines = readFileSync(filepath, "utf8").split("\n")
	const classes = new Map<string, ClassDef>()
	const blocks: DocBlock[] = []
	let cur: DocBlock | null = null
	let curClass: ClassDef | null = null
	let inUsage = false

	const fieldRe = new RegExp(`^@field\\s+(\\w+\\??)\\s+(${TYPE_RE.source})\\s*(.*)`)
	const paramRe = new RegExp(`^@param\\s+(\\w+)\\s+(${TYPE_RE.source})\\s*(.*)`)
	const returnRe = new RegExp(`^@return\\s+(${TYPE_RE.source})\\s*(.*)`)

	for (const raw of lines) {
		const stripped = raw.trim()
		const dm = stripped.match(/^---\s?(.*)/)

		if (dm) {
			const content = dm[1]

			let m: RegExpMatchArray | null
			if ((m = content.match(/^@class\s+(\S+)/))) {
				if (cur) { blocks.push(cur); cur = null }
				curClass = { name: m[1], summary: [], fields: [] }
				inUsage = false
				continue
			}

			if (curClass && (m = content.match(fieldRe))) {
				curClass.fields.push({ name: m[1], type: m[2], desc: m[3] })
				continue
			}

			if (curClass && !content.match(/^@field/)) {
				classes.set(curClass.name, curClass)
				curClass = null
			}

			if (!cur) {
				cur = { summary: [], params: [], returns: [], fields: [], usage: [] }
				inUsage = false
			}

			if ((m = content.match(paramRe))) {
				inUsage = false
				cur.params.push({ name: m[1], type: m[2], desc: m[3] })
			} else if ((m = content.match(returnRe))) {
				inUsage = false
				cur.returns.push({ type: m[1], desc: m[2] })
			} else if ((m = content.match(fieldRe))) {
				inUsage = false
				cur.fields.push({ name: m[1], type: m[2], desc: m[3] })
			} else if ((m = content.match(/^@usage\s*(.*)/))) {
				inUsage = true
				if (m[1]) cur.usage.push(m[1])
			} else if (inUsage) {
				cur.usage.push(content)
			} else {
				cur.summary.push(content)
			}
			continue
		}

		if (curClass) {
			classes.set(curClass.name, curClass)
			curClass = null
		}

		if (cur) {
			inUsage = false
			const fm = stripped.match(/^function\s+([\w.:]+)\s*\(/)
			if (fm) {
				let name = fm[1]
				if (name.includes(".")) name = name.split(".").slice(1).join(".")
				if (name.includes(":")) name = name.split(":").slice(1).join(":")
				cur.funcName = name
			}
			blocks.push(cur)
			cur = null
		}
	}

	if (curClass) classes.set(curClass.name, curClass)
	if (!blocks.length && !classes.size) return null
	return { name: modName, path: rel, classes, blocks }
}

function discover(root: string): Module[] {
	const modules: Module[] = []
	function walk(dir: string) {
		for (const entry of readdirSync(dir).sort()) {
			const full = join(dir, entry)
			if (statSync(full).isDirectory()) {
				walk(full)
			} else if (extname(entry) === ".lua") {
				const mod = parseFile(full, root)
				if (mod) modules.push(mod)
			}
		}
	}
	walk(root)
	return modules
}

// ── Rendering ──────────────────────────────────────────────────────

/** Escape pipe characters inside markdown table cells */
function escapeCell(s: string): string {
	return s.replace(/\|/g, "\\|")
}

function renderFields(out: string[], fields: Field[]) {
	out.push("| Name | Type | Description |")
	out.push("|------|------|-------------|")
	for (const f of fields) {
		const optional = f.name.endsWith("?")
		const name = optional ? f.name.slice(0, -1) : f.name
		const badge = optional ? " *(optional)*" : ""
		out.push(`| \`${name}\`${badge} | \`${escapeCell(f.type)}\` | ${escapeCell(f.desc)} |`)
	}
}

/** Render a single function's doc block as an includable snippet (no page title). */
function renderBlock(block: DocBlock, mod: Module): string {
	const out: string[] = []

	if (block.funcName) {
		const paramNames = block.params.map((p) => p.name).join(", ")
		out.push(`### \`${block.funcName}(${paramNames})\``)
		out.push("")
	}

	const summary = block.summary.join(" ").trim()
	if (summary) {
		out.push(summary)
		out.push("")
	}

	if (block.params.length) {
		for (const p of block.params) {
			const classDef = mod.classes.get(p.type)
			if (classDef) {
				out.push(`**\`${p.name}\`** \`${escapeCell(p.type)}\` — ${escapeCell(p.desc)}`)
				out.push("")
				renderFields(out, classDef.fields)
				out.push("")
			} else {
				out.push("**Parameters:**")
				out.push("")
				out.push("| Name | Type | Description |")
				out.push("|------|------|-------------|")
				out.push(`| \`${p.name}\` | \`${escapeCell(p.type)}\` | ${escapeCell(p.desc)} |`)
				out.push("")
			}
		}
	}

	if (block.returns.length) {
		out.push("**Returns:** ", "")
		for (const r of block.returns) {
			const desc = r.desc ? ` — ${r.desc}` : ""
			out.push(`- \`${escapeCell(r.type)}\`${desc}`)
		}
		out.push("")
	}

	if (block.fields.length) {
		out.push("**Fields:**", "")
		renderFields(out, block.fields)
		out.push("")
	}

	if (block.usage.length) {
		out.push("**Usage:**", "")
		out.push("```lua")
		out.push(...block.usage)
		out.push("```")
		out.push("")
	}

	return out.join("\n")
}

// ── Public API ─────────────────────────────────────────────────────

export interface GeneratedSnippet {
	/** e.g. "shells.zsh.config" */
	id: string
	/** relative path inside outDir, e.g. "shells.zsh.config.md" */
	file: string
	/** The module this belongs to */
	module: string
	/** Function name */
	func: string
}

/**
 * Discovers Lua modules, writes per-function snippet files, returns metadata.
 * Handwritten docs include these via <!--@include: ./_generated/shells.zsh.config.md-->
 */
export function generateDocs(luaRoot: string, outDir: string): GeneratedSnippet[] {
	const modules = discover(luaRoot)
	const snippets: GeneratedSnippet[] = []

	mkdirSync(outDir, { recursive: true })

	for (const mod of modules) {
		for (const block of mod.blocks) {
			if (!block.funcName) continue
			const id = `${mod.name}.${block.funcName}`
			const file = `${id}.md`
			const md = renderBlock(block, mod)
			writeFileSync(join(outDir, file), md)
			snippets.push({ id, file, module: mod.name, func: block.funcName })
		}
	}

	return snippets
}

// ── CLI entrypoint ─────────────────────────────────────────────────

function main() {
	const args = process.argv.slice(2)
	let root: string | null = null
	let outDir: string | null = null

	for (let i = 0; i < args.length; i++) {
		if (args[i] === "-o" || args[i] === "--output") {
			outDir = args[++i]
		} else if (!root) {
			root = args[i]
		}
	}

	if (!root) {
		console.error("Usage: lua2md <lua-root> [-o <output-dir>]")
		process.exit(1)
	}

	if (!outDir) {
		// Default: print all blocks to stdout for preview
		const modules = discover(root)
		if (!modules.length) {
			console.error("No documented Lua modules found.")
			process.exit(1)
		}
		for (const mod of modules) {
			for (const block of mod.blocks) {
				process.stdout.write(renderBlock(block, mod))
				process.stdout.write("\n---\n\n")
			}
		}
		return
	}

	const snippets = generateDocs(root, outDir)
	for (const s of snippets) {
		console.error(`  wrote ${join(outDir, s.file)}`)
	}
}

const isDirectRun = process.argv[1]?.endsWith("lua2md.ts")
if (isDirectRun) main()
