import assert from "node:assert/strict";
import { generateKeyPairSync, createHash, sign } from "node:crypto";
import { test } from "node:test";
import {
  availableVersions,
  compareVersions,
  defaultVersion,
  loadCatalog,
  matchesPackage,
  packageCommand,
  preferredVersion,
  primaryCommand,
  searchPackages,
  type CatalogPackage,
} from "./catalog";

const pkg: CatalogPackage = {
  name: "test-tool",
  aliases: ["test-alias"],
  description: "Search local files",
  homepage: "https://example.org/project",
  default_version: "2.0",
  default_versions: { "aarch64-linux": "1.0" },
  versions: {
    "2.0": { systems: ["aarch64-macos"], bins: ["test-tool", "test-scan"], revision: 1 },
    "1.0": { systems: ["aarch64-linux"], bins: ["test-tool"], revision: 2 },
  },
};

function fixture(
  packages: CatalogPackage[] = [pkg],
  artifacts: Record<string, Record<string, unknown>> = {
    "test-tool@2.0": { "aarch64-macos": {} },
    "test-tool@1.0": { "aarch64-linux": {} },
  },
  schema = 1,
) {
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  const snapshot = JSON.stringify({
    schema,
    catalog: {
      schema: 1,
      packages: Object.fromEntries(packages.map((entry) => [entry.name, entry])),
    },
    artifacts,
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
    assert.equal(defaultVersion(result.packages[0], "aarch64-linux"), "1.0");
    assert.equal(defaultVersion(result.packages[0], "aarch64-macos"), "2.0");
  });
});

test("accepts signed schemas 2 through 5 and rejects unsupported schemas", async () => {
  for (const schema of [2, 3, 4, 5, 6]) {
    const data = fixture(undefined, undefined, schema);
    await withResponses(data, async () => {
      if (schema < 6) {
        assert.equal((await loadCatalog(data.source)).packages[0].name, pkg.name);
        return;
      }
      await assert.rejects(loadCatalog(data.source), /unavailable/);
    });
  }
});

test("rejects a changed manifest signature", async () => {
  const data = fixture();
  data.manifest.sequence = 2;
  await withResponses(data, async () => {
    await assert.rejects(loadCatalog(data.source), /signature could not be verified/);
  });
});

test("accepts library-only recipes in schema 4 and rejects unsafe archive paths", async () => {
  for (const library of ["lib/libtest.a", "lib/../escape.a", "/lib/test.a", "lib/test.so"]) {
    const entry = structuredClone(pkg);
    for (const recipe of Object.values(entry.versions)) {
      recipe.bins = [];
      recipe.build = { url: "https://example.org/source.tar.gz", libraries: [library] };
    }
    const data = fixture([entry], undefined, 4);
    await withResponses(data, async () => {
      if (library === "lib/libtest.a") {
        assert.equal((await loadCatalog(data.source)).packages[0].versions["1.0"].bins.length, 0);
      } else {
        await assert.rejects(loadCatalog(data.source));
      }
    });
  }
});

test("rejects changed snapshot bytes", async () => {
  const data = fixture();
  data.snapshot += " ";
  await withResponses(data, async () => {
    await assert.rejects(loadCatalog(data.source), /contents could not be verified/);
  });
});

test("searches aliases and descriptions while respecting platform availability", () => {
  assert(matchesPackage(pkg, "TEST-ALIAS", "aarch64-linux"));
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

test("orders numeric releases, prereleases, and calendar tags newest first", () => {
  const versions = ["1.9.0", "1.10.0-rc.2", "1.10.0", "1.10.0-rc.10", "1.10.0-beta", "1.10.0-1"];
  assert.deepEqual(
    versions.sort((a, b) => compareVersions(b, a)),
    ["1.10.0", "1.10.0-rc.10", "1.10.0-rc.2", "1.10.0-beta", "1.10.0-1", "1.9.0"],
  );
  assert.deepEqual(
    ["2025-9-1", "2025-10-1", "2026-1-1"].sort((a, b) => compareVersions(b, a)),
    ["2026-1-1", "2025-10-1", "2025-9-1"],
  );
  assert.equal(compareVersions("v1.10.0+build.7", "1.10.0+build.8"), 0);
  assert(compareVersions("1.10.0-rc.1", "1.10.0-rc") > 0);
});

test("keeps a lower platform default while sorting available releases newest first", () => {
  const entry = structuredClone(pkg);
  entry.versions["2.0"].systems.push("aarch64-linux");
  entry.versions["3.0"] = { systems: ["aarch64-macos"], bins: ["test-tool"], revision: 1 };
  entry.versions["4.0"] = { systems: [], bins: ["future-tool"], revision: 1 };

  assert.deepEqual(availableVersions(entry), ["3.0", "2.0", "1.0"]);
  assert.deepEqual(availableVersions(entry, "aarch64-linux"), ["2.0", "1.0"]);
  assert.equal(preferredVersion(entry, "aarch64-linux"), "1.0");
  assert.equal(preferredVersion(entry), "2.0");
  assert.deepEqual(availableVersions(entry, "x86_64-linux"), []);
  assert.equal(preferredVersion(entry, "x86_64-linux"), "");
});

test("falls back to the newest published version when the default is unavailable", () => {
  const entry = structuredClone(pkg);
  entry.versions["2.0"].systems = [];
  assert.equal(preferredVersion(entry), "1.0");
  assert(matchesPackage(entry, "test-tool", "aarch64-linux"));
});

test("filters unpublished systems, versions, and packages from the signed catalog", async () => {
  const entry = structuredClone(pkg);
  entry.versions["2.0"].systems.push("x86_64-linux");
  entry.versions["3.0"] = { systems: ["aarch64-macos"], bins: ["future-tool"], revision: 1 };
  const unpublished = { ...structuredClone(pkg), name: "unpublished-tool" };
  const data = fixture([entry, unpublished]);

  await withResponses(data, async () => {
    const { packages } = await loadCatalog(data.source);
    assert.deepEqual(
      packages.map((entry) => entry.name),
      ["test-tool"],
    );
    assert.deepEqual(packages[0].versions["2.0"].systems, ["aarch64-macos"]);
    assert.deepEqual(availableVersions(packages[0]), ["2.0", "1.0"]);
    assert.deepEqual(availableVersions(packages[0], "x86_64-linux"), []);
    assert.equal(packages[0].versions["1.0"].revision, 2);
    assert.deepEqual(packages[0].versions["2.0"].bins, ["test-tool", "test-scan"]);
  });
});

test("ranks exact names before aliases, exported commands, prefixes, and descriptions", () => {
  const named = { ...structuredClone(pkg), name: "scan", aliases: [] };
  const alias = { ...structuredClone(pkg), name: "alias-package", aliases: ["scan"] };
  const exported = { ...structuredClone(pkg), name: "command-package", aliases: [] };
  exported.versions["2.0"].bins = ["scan"];
  const prefix = { ...structuredClone(pkg), name: "scan-utils", aliases: [] };
  const described = {
    ...structuredClone(pkg),
    name: "description-package",
    aliases: [],
    description: "Scan files",
  };
  const entries = [described, prefix, exported, alias, named];

  assert.deepEqual(
    searchPackages(entries, " SCAN ").map((entry) => entry.name),
    ["scan", "alias-package", "command-package", "scan-utils", "description-package"],
  );
  assert(matchesPackage(exported, "SCAN local", "aarch64-macos"));
  assert.deepEqual(searchPackages(entries, "scan", "x86_64-linux"), []);
});

test("does not advertise commands absent from published versions on the selected platform", () => {
  const entry = structuredClone(pkg);
  entry.versions["3.0"] = { systems: [], bins: ["future-tool"], revision: 1 };

  assert(!matchesPackage(entry, "future-tool", ""));
  assert(!matchesPackage(entry, "test-scan", "aarch64-linux"));
  assert(matchesPackage(entry, "test-scan", "aarch64-macos"));
});

test("selects canonical or sole commands and emits explicit overrides for multi-command packages", () => {
  assert.equal(primaryCommand(pkg, "2.0"), "test-tool");
  assert.equal(packageCommand(pkg, "run", "2.0", "test-tool"), "rb run test-tool@2.0");
  assert.equal(
    packageCommand(pkg, "run", "2.0", "test-scan"),
    "rb run --bin test-scan test-tool@2.0",
  );
  assert.equal(packageCommand(pkg, "use", "2.0", "test-scan"), "rb use test-tool@2.0");

  const entry = structuredClone(pkg);
  entry.versions["2.0"].bins = ["one", "two"];
  assert.equal(primaryCommand(entry, "2.0"), "");
  assert.equal(packageCommand(entry, "run", "2.0", "two"), "rb run --bin two test-tool@2.0");
  entry.versions["2.0"].bins = ["one"];
  assert.equal(primaryCommand(entry, "2.0"), "one");
  assert.equal(packageCommand(entry, "run", "2.0", "one"), "rb run test-tool@2.0");
});

test("quotes pinned shell requests and emits a complete Lua declaration", () => {
  assert.equal(packageCommand(pkg, "run"), "rb run test-tool");
  assert.equal(
    packageCommand(pkg, "use", "release candidate"),
    "rb use 'test-tool@release candidate'",
  );
  assert.equal(
    packageCommand(pkg, "run", "$(touch /tmp/oops)"),
    "rb run 'test-tool@$(touch /tmp/oops)'",
  );
  assert.equal(packageCommand(pkg, "use", "it's-ready"), "rb use 'test-tool@it'\\''s-ready'");
  assert.equal(
    packageCommand(pkg, "config", 'release"candidate'),
    'local rb = require("rootbeer")\n\nrb.package("test-tool@release\\"candidate")',
  );
});

test("validates schema 3 app exports before presenting package metadata", async () => {
  const entry = structuredClone(pkg);
  entry.versions["2.0"].apps = { "Tool.app": "Applications/Tool.app" };
  const valid = fixture([entry], undefined, 3);
  await withResponses(valid, async () => {
    assert.deepEqual((await loadCatalog(valid.source)).packages[0].versions["2.0"].apps, {
      "Tool.app": "Applications/Tool.app",
    });
  });
  for (const [schema, apps, systems] of [
    [2, { "Tool.app": "Tool.app" }, ["aarch64-macos"]],
    [3, { "Tool.app": "../Tool.app" }, ["aarch64-macos"]],
    [3, { "../Tool.app": "Tool.app" }, ["aarch64-macos"]],
    [3, { "Tool.app": "/Tool.app" }, ["aarch64-macos"]],
    [3, { "Tool.app": "Tool.app" }, ["aarch64-linux"]],
    [3, [], ["aarch64-macos"]],
  ] as const) {
    const invalid = structuredClone(entry);
    invalid.versions["2.0"].apps = apps as Record<string, string>;
    invalid.versions["2.0"].systems = [...systems];
    const data = fixture([invalid], undefined, schema);
    await withResponses(data, async () => {
      await assert.rejects(loadCatalog(data.source), /invalid app exports/);
    });
  }
});
