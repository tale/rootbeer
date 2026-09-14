<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useData } from "vitepress";
import { defaultVersion, loadCatalog, matchesPackage, type CatalogPackage } from "./catalog";

const { theme } = useData();
const packages = ref<CatalogPackage[]>([]);
const query = ref("");
const system = ref("");
const loading = ref(true);
const error = ref("");
const mode = ref("run");
const copied = ref("");
const copyError = ref("");
const platforms = [
  ["aarch64-macos", "macOS · Apple silicon"],
  ["x86_64-macos", "macOS · Intel"],
  ["aarch64-linux", "Linux · ARM64"],
  ["x86_64-linux", "Linux · x86-64"],
];
const results = computed(() =>
  packages.value.filter((pkg) => matchesPackage(pkg, query.value, system.value)),
);

async function refresh() {
  loading.value = true;
  error.value = "";
  try {
    const catalog = await loadCatalog(theme.value.catalog);
    packages.value = catalog.packages;
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : "The catalog could not be loaded.";
  } finally {
    loading.value = false;
  }
}

function command(pkg: CatalogPackage): string {
  if (mode.value === "config") return `rb.package(${JSON.stringify(pkg.name)})`;
  return `rb ${mode.value} ${pkg.name}`;
}

async function copy(pkg: CatalogPackage) {
  copyError.value = "";
  try {
    await navigator.clipboard.writeText(command(pkg));
    copied.value = pkg.name;
  } catch {
    copyError.value = "Could not copy. Select and copy the command.";
  }
}

watch(mode, () => {
  copied.value = "";
  copyError.value = "";
});

onMounted(() => {
  const url = new URL(window.location.href);
  query.value = url.searchParams.get("q") ?? "";
  const selected = url.searchParams.get("platform") ?? "";
  system.value = platforms.some(([id]) => id === selected) ? selected : "";
  watch([query, system], () => {
    const url = new URL(window.location.href);
    for (const [key, value] of [
      ["q", query.value],
      ["platform", system.value],
    ]) {
      if (value) url.searchParams.set(key, value);
      else url.searchParams.delete(key);
    }
    window.history.replaceState(null, "", url);
  });
  refresh();
});
</script>

<template>
  <div class="package-search">
    <div class="search-controls">
      <label class="query-control"
        >Find a package
        <input
          v-model="query"
          type="search"
          placeholder="Search by name, alias, or description"
          autocomplete="off"
        />
      </label>
      <label
        >Platform
        <select v-model="system">
          <option value="">All platforms</option>
          <option v-for="[id, label] in platforms" :key="id" :value="id">{{ label }}</option>
        </select>
      </label>
    </div>
    <div class="usage-controls">
      <fieldset>
        <legend>Use a package</legend>
        <label
          v-for="[id, label] in [
            ['run', 'Run once'],
            ['use', 'Install for your user'],
            ['config', 'Add to config'],
          ]"
          :key="id"
          :class="{ selected: mode === id }"
        >
          <input v-model="mode" type="radio" name="package-usage" :value="id" />
          {{ label }}
        </label>
      </fieldset>
      <p v-if="mode === 'run'">Run a tool immediately. Pass arguments after <code>--</code>.</p>
      <p v-else-if="mode === 'use'">
        Install the package for your user. <a href="/guide/packages">Set up your PATH</a> to use it.
      </p>
      <p v-else>Add the declaration to <code>init.lua</code>, then run <code>rb apply</code>.</p>
    </div>
    <p v-if="loading" class="catalog-status" role="status">Loading packages…</p>
    <div v-else-if="error" class="catalog-error" role="alert">
      <p>{{ error }}</p>
      <button type="button" @click="refresh">Try again</button>
    </div>
    <template v-else>
      <div class="result-summary">
        <p role="status">
          {{ results.length }} {{ results.length === 1 ? "package" : "packages"
          }}<span v-if="query || system"> of {{ packages.length }}</span>
        </p>
        <button
          v-if="query || system"
          type="button"
          @click="
            query = '';
            system = '';
          "
        >
          Clear filters
        </button>
      </div>
      <p v-if="!results.length" class="catalog-status">
        No matching packages. Try another name or platform.
      </p>
      <p v-if="copyError" role="status">{{ copyError }}</p>
      <div class="package-grid">
        <article v-for="pkg in results" :key="pkg.name" class="package-result">
          <div class="result-heading">
            <h2>{{ pkg.name }}</h2>
            <a :href="pkg.homepage" target="_blank" rel="noopener noreferrer">Website ↗</a>
          </div>
          <p>{{ pkg.description }}</p>
          <p v-if="pkg.aliases.length" class="aliases">
            Also known as {{ pkg.aliases.join(", ") }}
          </p>
          <div class="declaration">
            <code>{{ command(pkg) }}</code>
            <button type="button" :aria-label="`Copy command for ${pkg.name}`" @click="copy(pkg)">
              {{ copied === pkg.name ? "Copied" : "Copy" }}
            </button>
          </div>
          <ul class="platform-list">
            <template v-for="[id, label] in platforms" :key="id">
              <li
                v-if="
                  (!system || system === id) &&
                  pkg.versions[defaultVersion(pkg, id)]?.systems.includes(id)
                "
              >
                {{ label }} <strong>{{ defaultVersion(pkg, id) }}</strong>
              </li>
            </template>
          </ul>
          <details>
            <summary>Available versions</summary>
            <ul>
              <li v-for="(recipe, version) in pkg.versions" :key="version">
                <code>{{ version }}</code> ·
                {{
                  recipe.systems
                    .map((id) => platforms.find(([key]) => key === id)?.[1] ?? id)
                    .join(", ") || "Not published"
                }}
              </li>
            </ul>
          </details>
        </article>
      </div>
    </template>
    <noscript
      >Enable JavaScript to search packages, or read the
      <a href="/guide/packages">package guide</a> to get started.</noscript
    >
  </div>
</template>

<style scoped>
.search-controls {
  display: flex;
  gap: 1rem;
  flex-wrap: wrap;
  margin: 0 0 1rem;
}
.search-controls label {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  font-weight: 600;
}
.query-control {
  flex: 1;
  min-width: min(100%, 18rem);
}
input,
select {
  background: var(--vp-c-bg);
  color: var(--vp-c-text-1);
  border: 1px solid var(--vp-c-border);
  padding: 0.7rem;
  font: inherit;
  width: 100%;
  border-radius: 0;
}
input:focus-visible,
select:focus-visible,
button:focus-visible,
summary:focus-visible {
  outline: 2px solid var(--vp-c-brand-1);
  outline-offset: 3px;
}
button {
  border: 1px solid var(--vp-c-border);
  padding: 0.3rem 0.75rem;
  font: inherit;
  cursor: pointer;
  background: var(--vp-c-bg);
}
button:hover {
  border-color: var(--vp-c-brand-1);
  color: var(--vp-c-brand-1);
}
.result-summary,
.result-heading,
.declaration {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
}
.result-summary {
  font-size: 0.875rem;
}
.package-result {
  border: 1px solid var(--vp-c-border);
  padding: 1.25rem;
  min-width: 0;
  background: var(--vp-c-bg);
}
.result-heading h2 {
  border: 0;
  padding: 0;
  margin: 0;
  font-size: 1.3rem;
}
.result-heading a,
.aliases {
  font-size: 0.875rem;
}
.package-result p {
  margin: 0.6rem 0;
}
.declaration {
  background: var(--vp-c-bg-soft);
  padding: 0.6rem 0.75rem;
  margin: 1rem 0;
  flex-wrap: wrap;
}
.declaration code {
  background: transparent;
  overflow-wrap: anywhere;
}
.platform-list {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem 1rem;
  padding: 0;
  list-style: none;
  font-size: 0.8rem;
}
.platform-list li {
  margin: 0;
}
.platform-list strong {
  margin-left: 0.3rem;
}
summary {
  cursor: pointer;
  font-size: 0.875rem;
}
details ul {
  font-size: 0.875rem;
}
.catalog-error {
  border-left: 3px solid var(--vp-c-brand-1);
  padding: 0.5rem 1rem;
}

.package-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 1rem;
  align-items: start;
}
.usage-controls {
  border-bottom: 1px solid var(--vp-c-border);
  padding-bottom: 1rem;
}
.usage-controls fieldset {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  border: 0;
  padding: 0;
  margin: 0;
}
.usage-controls legend {
  font-size: 0.875rem;
  font-weight: 600;
  margin-bottom: 0.5rem;
}
.usage-controls label {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  border: 1px solid var(--vp-c-border);
  padding: 0.35rem 0.7rem;
  cursor: pointer;
  font-size: 0.875rem;
}
.usage-controls label.selected {
  background: var(--vp-c-brand-soft);
  border-color: var(--vp-c-brand-1);
}
.usage-controls input {
  width: auto;
  accent-color: var(--vp-c-brand-1);
}
.usage-controls p {
  margin: 0.6rem 0 0;
  color: var(--vp-c-text-2);
  font-size: 0.875rem;
}
.catalog-status {
  padding: 2rem 0;
}
.result-summary {
  min-height: 4rem;
}
.result-heading a {
  flex-shrink: 0;
}
@media (max-width: 1100px) {
  .package-grid {
    grid-template-columns: 1fr;
  }
}
@media (max-width: 600px) {
  .search-controls > label {
    width: 100%;
  }
}
</style>
