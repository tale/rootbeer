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
  versions: Record<string, { systems: string[] }>;
}

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
    index.schema !== 1 ||
    index.catalog?.schema !== 1 ||
    !index.catalog.packages ||
    !index.artifacts
  ) {
    throw new Error("This catalog format is not supported. Try updating the website.");
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
      const published = index.artifacts[`${pkg.name}@${version}`] ?? {};
      recipe.systems = recipe.systems.filter((system) => Object.hasOwn(published, system));
    }
  }
  return {
    packages: packages.sort((a, b) => a.name.localeCompare(b.name)),
    snapshotUrl: manifest.index.url,
  };
}

export function defaultVersion(pkg: CatalogPackage, system: string): string {
  return pkg.default_versions?.[system] ?? pkg.default_version;
}

export function matchesPackage(pkg: CatalogPackage, query: string, system: string): boolean {
  const terms = query.toLowerCase().trim().split(/\s+/).filter(Boolean);
  const text = [pkg.name, ...pkg.aliases, pkg.description].join(" ").toLowerCase();
  if (!terms.every((term) => text.includes(term))) return false;
  return !system || Boolean(pkg.versions[defaultVersion(pkg, system)]?.systems.includes(system));
}
