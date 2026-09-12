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
const copied = ref("");
const copyError = ref("");
const snapshotUrl = ref("");
const platforms = [
  ["aarch64-macos", "macOS · ARM64"],
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
    snapshotUrl.value = catalog.snapshotUrl;
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : "The catalog could not be loaded.";
  } finally {
    loading.value = false;
  }
}

async function copy(pkg: CatalogPackage) {
  copyError.value = "";
  try {
    await navigator.clipboard.writeText(`rb.package(${JSON.stringify(pkg.name)})`);
    copied.value = pkg.name;
  } catch {
    copyError.value = "Copy was unavailable. Select the declaration text to copy it.";
  }
}

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
          placeholder="Search names, aliases, and descriptions"
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
    <p v-if="loading" role="status">Loading the published catalog…</p>
    <div v-else-if="error" class="catalog-error" role="alert">
      <p>{{ error }}</p>
      <button type="button" @click="refresh">Try again</button>
    </div>
    <template v-else>
      <div class="result-summary">
        <p role="status">
          {{ results.length }} {{ results.length === 1 ? "package" : "packages" }}
        </p>
        <a :href="snapshotUrl">Published snapshot</a>
      </div>
      <p v-if="!results.length">No matching packages. Try another name or platform.</p>
      <p v-if="copyError" role="status">{{ copyError }}</p>
      <article v-for="pkg in results" :key="pkg.name" class="package-result">
        <div class="result-heading">
          <h2>{{ pkg.name }}</h2>
          <a :href="pkg.homepage" target="_blank" rel="noopener noreferrer">Project ↗</a>
        </div>
        <p>{{ pkg.description }}</p>
        <p v-if="pkg.aliases.length" class="aliases">Also known as {{ pkg.aliases.join(", ") }}</p>
        <div class="declaration">
          <code>rb.package("{{ pkg.name }}")</code>
          <button type="button" :aria-label="`Copy declaration for ${pkg.name}`" @click="copy(pkg)">
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
    </template>
    <noscript
      >Package search requires JavaScript to load the current catalog. You can inspect the published
      index directly.</noscript
    >
  </div>
</template>

<style scoped>
.search-controls {
  display: flex;
  gap: 1rem;
  flex-wrap: wrap;
  margin: 2rem 0 1rem;
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
  margin: 1rem 0;
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
</style>
