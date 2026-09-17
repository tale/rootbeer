export interface CatalogSource {
  url: string;
  publicKey: string;
}

export interface CatalogPackage {
  name: string;
  aliases: string[];
  description: string;
  homepage: string;
  default_version: string;
  default_versions?: Record<string, string>;
  versions: Record<string, CatalogRecipe>;
}

export interface CatalogRecipe {
  systems: string[];
  bins: string[];
  apps?: Record<string, string>;
  revision: number;
  source?: string;
  build?: {
    url: string;
    libraries?: string[];
    dependencies?: (
      | string
      | { package: string; kind: "all" | "build" | "link" | "runtime" | "link_runtime" }
    )[];
  };
}

export const platforms = [
  { id: "aarch64-macos", label: "macOS · Apple silicon", short: "macOS ARM64" },
  { id: "aarch64-linux", label: "Linux · ARM64", short: "Linux ARM64" },
  { id: "x86_64-linux", label: "Linux · x86-64", short: "Linux x86-64" },
];

export interface Catalog {
  packages: CatalogPackage[];
  snapshotUrl: string;
}

function hex(value: string, length: number): Uint8Array {
  if (!new RegExp(`^[0-9a-f]{${length * 2}}$`).test(value)) {
    throw new Error("The catalog contains an invalid verification value.");
  }
  return Uint8Array.from(value.match(/../g)!, (byte) => parseInt(byte, 16));
}

function https(value: string): string {
  const url = new URL(value);
  if (url.protocol !== "https:" || url.username || url.password) {
    throw new Error("The catalog requires an HTTPS address.");
  }
  return url.href;
}

async function bytes(url: string, limit: number): Promise<Uint8Array> {
  const response = await fetch(https(url), { signal: AbortSignal.timeout(15000) });
  if (!response.ok || !response.body)
    throw new Error("The package catalog is unavailable. Try again shortly.");
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  try {
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      size += value.length;
      if (size > limit) throw new Error("The catalog response is too large.");
      chunks.push(value);
    }
  } finally {
    await reader.cancel();
  }
  const output = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    output.set(chunk, offset);
    offset += chunk.length;
  }
  return output;
}

export async function loadCatalog(source: CatalogSource): Promise<Catalog> {
  const decoder = new TextDecoder();
  const manifest = JSON.parse(decoder.decode(await bytes(source.url, 65536)));
  if (manifest.schema !== 1 || !Number.isSafeInteger(manifest.sequence) || manifest.sequence <= 0) {
    throw new Error("The catalog manifest is not supported.");
  }
  https(manifest.index.url);
  hex(manifest.index.sha256, 32);
  const key = await crypto.subtle.importKey("raw", hex(source.publicKey, 32), "Ed25519", false, [
    "verify",
  ]);
  const payload = new TextEncoder().encode(
    JSON.stringify([
      "rootbeer-index-v1",
      manifest.sequence,
      manifest.index.url,
      manifest.index.sha256,
    ]),
  );
  if (!(await crypto.subtle.verify("Ed25519", key, hex(manifest.signature, 64), payload))) {
    throw new Error("The package catalog signature could not be verified.");
  }
  const snapshot = await bytes(manifest.index.url, 16 * 1024 * 1024);
  const digest = Array.from(
    new Uint8Array(await crypto.subtle.digest("SHA-256", snapshot)),
    (byte) => byte.toString(16).padStart(2, "0"),
  ).join("");
  if (digest !== manifest.index.sha256)
    throw new Error("The package catalog contents could not be verified.");
  const index = JSON.parse(decoder.decode(snapshot));
  if (
    ![1, 2, 3, 4, 5, 6, 7].includes(index.schema) ||
    index.catalog?.schema !== 1 ||
    !index.catalog.packages ||
    !index.artifacts
  ) {
    throw new Error("Package search is temporarily unavailable. Please try again later.");
  }
  const packages = Object.values(index.catalog.packages) as CatalogPackage[];
  for (const pkg of packages) {
    if (
      !/^[a-z0-9][a-z0-9+._-]*$/.test(pkg.name) ||
      typeof pkg.description !== "string" ||
      !Array.isArray(pkg.aliases) ||
      !pkg.versions?.[pkg.default_version]
    ) {
      throw new Error("The catalog contains an invalid package.");
    }
    pkg.homepage = https(pkg.homepage);
    for (const [version, recipe] of Object.entries(pkg.versions)) {
      const libraries = recipe.build?.libraries ?? [];
      if (
        !Array.isArray(libraries) ||
        !libraries.every(
          (path) =>
            typeof path === "string" &&
            /^(lib|lib64)\/.+\.a$/.test(path) &&
            !/[\\\0]/.test(path) &&
            path.split("/").every((part) => part && part !== "." && part !== ".."),
        ) ||
        (libraries.length > 0 && index.schema < 4)
      ) {
        throw new Error("The catalog contains invalid library exports.");
      }
      if (
        !Array.isArray(recipe.systems) ||
        !recipe.systems.every((system) => platforms.some(({ id }) => id === system)) ||
        !Array.isArray(recipe.bins) ||
        (!recipe.bins.length && !libraries.length) ||
        !recipe.bins.every((bin) => typeof bin === "string" && /^[a-z0-9][a-z0-9+._-]*$/.test(bin))
      ) {
        throw new Error("The catalog contains invalid package commands or platforms.");
      }
      if (recipe.apps !== undefined) {
        if (
          !recipe.apps ||
          typeof recipe.apps !== "object" ||
          Array.isArray(recipe.apps) ||
          (Object.keys(recipe.apps).length > 0 &&
            (index.schema < 3 || recipe.systems.some((system) => !system.endsWith("-macos")))) ||
          !Object.entries(recipe.apps).every(
            ([name, path]) =>
              name.endsWith(".app") &&
              name !== ".app" &&
              !/[\/\\\0]/.test(name) &&
              typeof path === "string" &&
              path.endsWith(".app") &&
              !path.includes("\0") &&
              path.split("/").every((part) => part && part !== "." && part !== ".."),
          )
        ) {
          throw new Error("The catalog contains invalid app exports.");
        }
      }
      const published = index.artifacts[`${pkg.name}@${version}`] ?? {};
      recipe.systems = recipe.systems.filter(
        (system) =>
          Object.hasOwn(published, system) || (index.schema >= 7 && recipe.build !== undefined),
      );
    }
  }
  return {
    packages: packages
      .filter((pkg) => availableVersions(pkg).length)
      .sort((a, b) => a.name.localeCompare(b.name)),
    snapshotUrl: manifest.index.url,
  };
}

export function defaultVersion(pkg: CatalogPackage, system: string): string {
  return pkg.default_versions?.[system] ?? pkg.default_version;
}

export function matchesPackage(pkg: CatalogPackage, query: string, system: string): boolean {
  const terms = query.toLowerCase().trim().split(/\s+/).filter(Boolean);
  const versions = availableVersions(pkg, system);
  const text = [
    pkg.name,
    ...pkg.aliases,
    pkg.description,
    ...versions.flatMap((version) => pkg.versions[version].bins),
  ]
    .join(" ")
    .toLowerCase();
  if (!terms.every((term) => text.includes(term))) return false;
  return versions.length > 0;
}

const natural = new Intl.Collator("en", { numeric: true, sensitivity: "base" });

export function compareVersions(a: string, b: string): number {
  const calendar = /^\d{4}-\d{1,2}(?:-\d{1,2})?$/;
  if (calendar.test(a) && calendar.test(b)) return natural.compare(a, b);
  const pattern = /^v?(\d+(?:\.\d+)*)(?:-([\da-z.-]+))?(?:\+[\da-z.-]+)?$/i;
  const left = a.match(pattern);
  const right = b.match(pattern);
  if (!left || !right) return natural.compare(a, b);

  const coreA = left[1].split(".");
  const coreB = right[1].split(".");
  for (let i = 0; i < Math.max(coreA.length, coreB.length); i++) {
    const order = natural.compare(coreA[i] ?? "0", coreB[i] ?? "0");
    if (order) return order;
  }
  if (!left[2] || !right[2]) return Number(!left[2]) - Number(!right[2]);

  const preA = left[2].split(".");
  const preB = right[2].split(".");
  for (let i = 0; i < Math.max(preA.length, preB.length); i++) {
    if (preA[i] === undefined) return -1;
    if (preB[i] === undefined) return 1;
    const numericA = /^\d+$/.test(preA[i]);
    const numericB = /^\d+$/.test(preB[i]);
    if (numericA !== numericB) return numericA ? -1 : 1;
    const order = numericA
      ? natural.compare(preA[i], preB[i])
      : preA[i] < preB[i]
        ? -1
        : Number(preA[i] > preB[i]);
    if (order) return order;
  }
  return 0;
}

export function availableVersions(pkg: CatalogPackage, system = ""): string[] {
  return Object.keys(pkg.versions)
    .filter((version) =>
      system
        ? pkg.versions[version].systems.includes(system)
        : pkg.versions[version].systems.length > 0,
    )
    .sort((a, b) => compareVersions(b, a) || b.localeCompare(a));
}

export function preferredVersion(pkg: CatalogPackage, system = ""): string {
  const versions = availableVersions(pkg, system);
  const preferred = defaultVersion(pkg, system);
  return versions.includes(preferred) ? preferred : (versions[0] ?? "");
}

export function searchPackages(
  packages: CatalogPackage[],
  query: string,
  system = "",
): CatalogPackage[] {
  const term = query.trim().toLowerCase();
  const rank = (pkg: CatalogPackage) => {
    if (pkg.name === term) return 0;
    if (pkg.aliases.includes(term)) return 1;
    if (availableVersions(pkg, system).some((version) => pkg.versions[version].bins.includes(term)))
      return 2;
    if (pkg.name.startsWith(term)) return 3;
    return 4;
  };
  return packages
    .filter((pkg) => matchesPackage(pkg, query, system))
    .sort((a, b) => rank(a) - rank(b) || a.name.localeCompare(b.name));
}

export function primaryCommand(pkg: CatalogPackage, version: string): string {
  const bins = pkg.versions[version]?.bins ?? [];
  return bins.includes(pkg.name) ? pkg.name : bins.length === 1 ? bins[0] : "";
}

export function packageCommand(
  pkg: CatalogPackage,
  mode: "run" | "use" | "config",
  version = "",
  bin = "",
): string {
  const request = version ? `${pkg.name}@${version}` : pkg.name;
  if (mode === "config")
    return `local rb = require("rootbeer")\n\nrb.package(${JSON.stringify(request)})`;
  const quote = (value: string) =>
    /^[a-zA-Z0-9._+@:/-]+$/.test(value) ? value : `'${value.replace(/'/g, "'\\''")}'`;
  const selectedBin =
    mode === "run" && bin && bin !== primaryCommand(pkg, version || pkg.default_version)
      ? ` --bin ${quote(bin)}`
      : "";
  return `rb ${mode}${selectedBin} ${quote(request)}`;
}

export function packageDependencies(recipe: CatalogRecipe, system = "") {
  const labels = {
    all: "Build / link",
    build: "Build",
    link: "Link",
    runtime: "Runtime",
    link_runtime: "Link / runtime",
  };
  return (recipe.build?.dependencies ?? []).map((dependency) => {
    const request = typeof dependency === "string" ? dependency : dependency.package;
    const kind = typeof dependency === "string" ? "all" : dependency.kind;
    const separator = request.indexOf("@");
    const name = separator < 0 ? request : request.slice(0, separator);
    const version = separator < 0 ? "" : request.slice(separator + 1);
    const params = new URLSearchParams({ show: name });
    if (version) params.set("version", version);
    if (system) params.set("platform", system);
    return { request, kind: labels[kind], href: `/packages/?${params}` };
  });
}
