import assert from "node:assert/strict";
import { generateKeyPairSync, createHash, sign } from "node:crypto";
import { test } from "node:test";
import { defaultVersion, loadCatalog, matchesPackage, type CatalogPackage } from "./catalog";

const pkg: CatalogPackage = {
  name: "test-tool",
  aliases: ["test-alias"],
  description: "Search local files",
  homepage: "https://example.org/project",
  default_version: "2.0",
  default_versions: { "x86_64-macos": "1.0" },
  versions: { "2.0": { systems: ["aarch64-macos"] }, "1.0": { systems: ["x86_64-macos"] } },
};

function fixture() {
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  const snapshot = JSON.stringify({
    schema: 1,
    catalog: { schema: 1, packages: { "test-tool": pkg } },
    artifacts: {
      "test-tool@2.0": { "aarch64-macos": {} },
      "test-tool@1.0": { "x86_64-macos": {} },
    },
  });
  const index = {
    url: "https://example.org/snapshot.json",
    sha256: createHash("sha256").update(snapshot).digest("hex"),
  };
  const payload = JSON.stringify(["rootbeer-index-v1", 1, index.url, index.sha256]);
  const manifest = {
    schema: 1,
    sequence: 1,
    index,
    signature: sign(null, Buffer.from(payload), privateKey).toString("hex"),
  };
  return {
    source: {
      url: "https://example.org/latest.json",
      publicKey: publicKey.export({ type: "spki", format: "der" }).subarray(-32).toString("hex"),
    },
    manifest,
    snapshot,
  };
}

async function withResponses(data: ReturnType<typeof fixture>, action: () => Promise<void>) {
  const original = globalThis.fetch;
  globalThis.fetch = async (url) =>
    new Response(
      String(url).endsWith("latest.json") ? JSON.stringify(data.manifest) : data.snapshot,
    );
  try {
    await action();
  } finally {
    globalThis.fetch = original;
  }
}

test("loads a signed published snapshot and preserves platform defaults", async () => {
  const data = fixture();
  await withResponses(data, async () => {
    const result = await loadCatalog(data.source);
    assert.equal(result.packages.length, 1);
    assert.equal(defaultVersion(result.packages[0], "x86_64-macos"), "1.0");
    assert.equal(defaultVersion(result.packages[0], "aarch64-macos"), "2.0");
  });
});

test("rejects a changed manifest signature", async () => {
  const data = fixture();
  data.manifest.sequence = 2;
  await withResponses(data, async () => {
    await assert.rejects(loadCatalog(data.source), /signature could not be verified/);
  });
});

test("rejects changed snapshot bytes", async () => {
  const data = fixture();
  data.snapshot += " ";
  await withResponses(data, async () => {
    await assert.rejects(loadCatalog(data.source), /contents could not be verified/);
  });
});

test("searches aliases and descriptions while respecting platform availability", () => {
  assert(matchesPackage(pkg, "TEST-ALIAS", "x86_64-macos"));
  assert(matchesPackage(pkg, "local search", "aarch64-macos"));
  assert(!matchesPackage(pkg, "missing", ""));
  assert(!matchesPackage(pkg, "", "x86_64-linux"));
});

test("reports unavailable catalog responses", async () => {
  const original = globalThis.fetch;
  globalThis.fetch = async () => new Response("", { status: 503 });
  try {
    await assert.rejects(loadCatalog(fixture().source), /unavailable/);
  } finally {
    globalThis.fetch = original;
  }
});
