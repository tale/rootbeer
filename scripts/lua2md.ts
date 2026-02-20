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
import { readFileSync, writeFileSync, mkdirSync, existsSync } from "node:fs"
import { join, resolve } from "node:path"
import { execSync } from "node:child_process"

// ── Types (matching LuaLS doc.json structure) ──────────────────────

interface DocArg {
	name: string
	view: string
	desc?: string
}

interface DocReturn {
	name?: string
	view: string
	desc?: string
	type: string
}

interface DocExtends {
	type: string
	view: string
	args?: DocArg[]
	returns?: DocReturn[]
}

interface DocField {
	name: string
	type: string
	view: string
	desc?: string
	rawdesc?: string
	extends?: DocExtends
	visible?: string
	deprecated?: boolean
	file?: string
}

interface DocDefine {
	type: string
	file?: string
	view?: string
	desc?: string
	rawdesc?: string
	extends?: DocExtends
	visible?: string
	deprecated?: boolean
}

interface DocEntry {
	name: string
	type: string
	view?: string
	desc?: string
	defines?: DocDefine[]
	fields?: DocField[]
}

// ── Module grouping ────────────────────────────────────────────────

interface ModuleFunc {
	name: string
	desc: string
	args: DocArg[]
	returns: DocReturn[]
}

interface ModuleClass {
	name: string
	fields: { name: string; view: string; desc: string; optional: boolean }[]
}

interface ModulePage {
	/** Module key: file stem without .lua */
	key: string
	functions: ModuleFunc[]
	classes: ModuleClass[]
}

function sourceFile(entry: DocEntry): string | null {
	if (entry.defines?.length) {
		const file = entry.defines[0].file
		if (file) return file
	}
	return null
}

/** Collapse LuaLS multi-line rawdesc (leading spaces on continuation lines) into a single line. */
function cleanDesc(raw: string): string {
	return raw.replace(/\n\s*/g, " ").trim()
}

function extractFunctionsFromType(entry: DocEntry): ModuleFunc[] {
	const funcs: ModuleFunc[] = []
	for (const field of entry.fields ?? []) {
		if (field.extends?.type !== "function") continue
		funcs.push({
			name: field.name,
			desc: cleanDesc(field.rawdesc ?? ""),
			args: (field.extends.args ?? []).filter((a) => a.name !== "self"),
			returns: field.extends.returns ?? [],
		})
	}
	return funcs
}

/**
 * Extract a class definition from a type entry (fields that are NOT functions).
 * Skip the module's own class entry (e.g., "zsh" itself) — only document
 * parameter/return types like "zsh.Config" or "rootbeer.SystemData".
 */
function extractClass(entry: DocEntry, moduleKeys: Set<string>): ModuleClass | null {
	if (moduleKeys.has(entry.name)) return null
	const fields: ModuleClass["fields"]  = []
	for (const f of entry.fields ?? []) {
		if (f.extends?.type === "function") continue
		const optional = f.view?.endsWith("?") ?? false
		fields.push({
			name: f.name,
			view: optional ? f.view!.slice(0, -1) : (f.view ?? "unknown"),
			desc: f.rawdesc ?? f.desc ?? "",
			optional,
		})
	}
	if (!fields.length) return null
	return { name: entry.name, fields }
}

function buildModules(entries: DocEntry[]): Map<string, ModulePage> {
	const modules = new Map<string, ModulePage>()

	// Collect all module-level class keys (classes that represent the module table itself)
	// These are top-level @class annotations whose name matches a file stem
	const moduleKeys = new Set<string>()
	for (const entry of entries) {
		if (entry.type === "type") {
			const file = sourceFile(entry)
			if (file) {
				const key = file.replace(/\.lua$/, "")
				if (entry.name === key) moduleKeys.add(key)
			}
		}
	}

	function getModule(key: string): ModulePage {
		let mod = modules.get(key)
		if (!mod) {
			mod = { key, functions: [], classes: [] }
			modules.set(key, mod)
		}
		return mod
	}

	for (const entry of entries) {
		if (entry.type === "luals.config") continue
		const file = sourceFile(entry)
		if (!file) continue
		const key = file.replace(/\.lua$/, "")

		if (entry.type === "variable") {
			// Global function (e.g., rootbeer.file)
			const def = entry.defines?.[0]
			if (!def?.extends || def.extends.type !== "function") continue
			getModule(key).functions.push({
				name: entry.name.split(".").pop()!,
				desc: cleanDesc(def.rawdesc ?? ""),
				args: (def.extends.args ?? []).filter((a) => a.name !== "self"),
				returns: def.extends.returns ?? [],
			})
		} else if (entry.type === "type") {
			const mod = getModule(key)
			// Extract functions defined as fields on the class (module pattern)
			mod.functions.push(...extractFunctionsFromType(entry))
			// Extract class docs (parameter/return types)
			const cls = extractClass(entry, moduleKeys)
			if (cls) mod.classes.push(cls)
		}
	}

	return modules
}

// ── Rendering ──────────────────────────────────────────────────────

function escapeCell(s: string): string {
	return s.replace(/\|/g, "\\|")
}

/** Map from module key to display info */
const MODULE_META: Record<string, { title: string; requirePath: string; varName: string }> = {
	core: { title: "Core API", requirePath: "@rootbeer", varName: "rb" },
}

function moduleMeta(key: string) {
	return MODULE_META[key] ?? {
		title: key,
		requirePath: `@rootbeer/${key.replace(/\./g, "/")}`,
		varName: key.split(".").pop()!,
	}
}

function renderPage(mod: ModulePage): string {
	const meta = moduleMeta(mod.key)
	const out: string[] = []

	out.push("---")
	out.push("outline: deep")
	out.push("---")
	out.push("")
	out.push(`# ${meta.title}`)
	out.push("")
	out.push("```lua")
	out.push(`local ${meta.varName} = require("${meta.requirePath}")`)
	out.push("```")
	out.push("")

	for (const func of mod.functions) {
		const paramList = func.args.map((a) => a.name).join(", ")
		out.push(`## \`${func.name}(${paramList})\``)
		out.push("")

		if (func.desc) {
			out.push(func.desc)
			out.push("")
		}

		// Check if any arg references a known class
		const classArgs = func.args.filter((a) =>
			mod.classes.some((c) => c.name === a.view)
		)
		const plainArgs = func.args.filter((a) =>
			!mod.classes.some((c) => c.name === a.view)
		)

		if (plainArgs.length) {
			out.push("**Parameters:**")
			out.push("")
			out.push("| Name | Type | Description |")
			out.push("|------|------|-------------|")
			for (const a of plainArgs) {
				out.push(`| \`${a.name}\` | \`${escapeCell(a.view)}\` | ${escapeCell(a.desc ?? "")} |`)
			}
			out.push("")
		}

		for (const a of classArgs) {
			const cls = mod.classes.find((c) => c.name === a.view)!
			out.push(`**\`${a.name}\`** \`${escapeCell(a.view)}\` — ${escapeCell(a.desc ?? "")}`)
			out.push("")
			out.push("| Name | Type | Description |")
			out.push("|------|------|-------------|")
			for (const f of cls.fields) {
				const badge = f.optional ? " *(optional)*" : ""
				out.push(`| \`${f.name}\`${badge} | \`${escapeCell(f.view)}\` | ${escapeCell(f.desc)} |`)
			}
			out.push("")
		}

		if (func.returns.length) {
			out.push("**Returns:**")
			out.push("")
			for (const r of func.returns) {
				const cls = mod.classes.find((c) => c.name === r.view)
				if (cls) {
					const desc = r.desc ? ` — ${r.desc}` : ""
					out.push(`\`${escapeCell(r.view)}\`${desc}`)
					out.push("")
					out.push("| Name | Type | Description |")
					out.push("|------|------|-------------|")
					for (const f of cls.fields) {
						const badge = f.optional ? " *(optional)*" : ""
						out.push(`| \`${f.name}\`${badge} | \`${escapeCell(f.view)}\` | ${escapeCell(f.desc)} |`)
					}
				} else {
					const desc = r.desc ? ` — ${r.desc}` : ""
					out.push(`- \`${escapeCell(r.view)}\`${desc}`)
				}
			}
			out.push("")
		}
	}

	return out.join("\n") + "\n"
}

// ── Main ───────────────────────────────────────────────────────────

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

	const absRoot = resolve(root)
	const tmpDir = execSync("mktemp -d", { encoding: "utf8" }).trim()

	try {
		execSync(`lua-language-server --doc=${absRoot} --doc_out_path=${tmpDir}`, {
			stdio: ["pipe", "pipe", "pipe"],
		})
	} catch (e: any) {
		console.error("Failed to run lua-language-server:", e.message)
		process.exit(1)
	}

	const jsonPath = join(tmpDir, "doc.json")
	if (!existsSync(jsonPath)) {
		console.error("lua-language-server did not produce doc.json")
		process.exit(1)
	}

	const entries: DocEntry[] = JSON.parse(readFileSync(jsonPath, "utf8"))
	const modules = buildModules(entries)

	if (!modules.size) {
		console.error("No documented modules found.")
		process.exit(1)
	}

	if (!outDir) {
		for (const mod of modules.values()) {
			process.stdout.write(renderPage(mod))
			process.stdout.write("\n---\n\n")
		}
		return
	}

	mkdirSync(outDir, { recursive: true })
	for (const mod of modules.values()) {
		const file = `${mod.key}.md`
		writeFileSync(join(outDir, file), renderPage(mod))
		console.error(`  wrote ${join(outDir, file)}`)
	}
}

main()
