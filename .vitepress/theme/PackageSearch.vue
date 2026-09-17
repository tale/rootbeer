<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useData } from "vitepress";
import PackageDetails from "./PackageDetails.vue";
import {
  availableVersions,
  loadCatalog,
  matchesPackage,
  platforms,
  preferredVersion,
  searchPackages,
  type CatalogPackage,
} from "./catalog";

const { theme } = useData();
const packages = ref<CatalogPackage[]>([]);
const query = ref("");
const system = ref("");
const sort = ref("relevance");
const show = ref("");
const version = ref("");
const loading = ref(true);
const error = ref("");
const results = computed(() => {
  const result = searchPackages(packages.value, query.value, system.value);
  return sort.value === "name" ? result.sort((a, b) => a.name.localeCompare(b.name)) : result;
});
const platformCounts = computed(() =>
  Object.fromEntries(
    platforms.map(({ id }) => [
      id,
      packages.value.filter((pkg) => matchesPackage(pkg, query.value, id)).length,
    ]),
  ),
);
const queryCount = computed(
  () => packages.value.filter((pkg) => matchesPackage(pkg, query.value, "")).length,
);

async function refresh() {
  loading.value = true;
  error.value = "";
  try {
    packages.value = (await loadCatalog(theme.value.catalog)).packages;
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : "The catalog could not be loaded.";
  } finally {
    loading.value = false;
  }
}

function readUrl() {
  const params = new URL(window.location.href).searchParams;
  query.value = params.get("q") ?? "";
  system.value = platforms.some(({ id }) => id === params.get("platform"))
    ? params.get("platform")!
    : "";
  sort.value = params.get("sort") === "name" ? "name" : "relevance";
  show.value = params.get("show") ?? "";
  version.value = params.get("version") ?? "";
}

function updateUrl() {
  const url = new URL(window.location.href);
  for (const [key, value] of [
    ["q", query.value],
    ["platform", system.value],
    ["show", show.value],
    ["version", show.value ? version.value : ""],
    ["sort", sort.value === "name" ? "name" : ""],
  ]) {
    if (value) url.searchParams.set(key, value);
    else url.searchParams.delete(key);
  }
  window.history.replaceState(window.history.state, "", url);
}

function clearSelection() {
  show.value = "";
  version.value = "";
}
function toggle(pkg: CatalogPackage) {
  show.value = show.value === pkg.name ? "" : pkg.name;
  version.value = "";
}
function clearFilters() {
  query.value = "";
  system.value = "";
  clearSelection();
}
function summaryPlatforms(pkg: CatalogPackage) {
  return platforms
    .filter(({ id }) => availableVersions(pkg, id).length)
    .map(({ short }) => short)
    .join(" · ");
}

let stopWatching: (() => void) | undefined;
onMounted(() => {
  readUrl();
  stopWatching = watch([query, system, sort, show, version], updateUrl);
  window.addEventListener("popstate", readUrl);
  refresh();
});
onUnmounted(() => {
  stopWatching?.();
  window.removeEventListener("popstate", readUrl);
});
</script>

<template>
  <div class="catalog-workspace">
    <aside class="catalog-sidebar">
      <fieldset class="platform-filter">
        <legend>Platform</legend>
        <label :class="{ active: !system }"
          ><input
            v-model="system"
            type="radio"
            name="platform"
            value=""
            @change="clearSelection"
          /><span>All platforms</span
          ><span class="filter-count">{{ loading ? "—" : queryCount }}</span></label
        >
        <label
          v-for="platform in platforms"
          :key="platform.id"
          :class="{ active: system === platform.id }"
          ><input
            v-model="system"
            type="radio"
            name="platform"
            :value="platform.id"
            @change="clearSelection"
          /><span>{{ platform.label }}</span
          ><span class="filter-count">{{
            loading ? "—" : platformCounts[platform.id]
          }}</span></label
        >
      </fieldset>
      <nav class="catalog-nav" aria-label="Package guides">
        <h2>Using packages</h2>
        <a href="/guide/packages#run-a-tool">Run a tool</a>
        <a href="/guide/packages#keep-tools-installed">Install for your user</a>
        <a href="/guide/packages#declare-tools-in-your-configuration">Manage with Lua</a>
        <a href="/guide/package-locks">Updates and offline use</a>
        <a href="/guide/package-sources">Other package sources</a>
      </nav>
      <nav class="catalog-nav contribute-nav" aria-label="Package contributions">
        <h2>The collection</h2>
        <a href="/contributing/packaging">Contribute a package ↗</a>
        <a
          v-if="theme.catalog.repositoryUrl"
          :href="theme.catalog.repositoryUrl"
          target="_blank"
          rel="noopener noreferrer"
          >Browse recipes ↗</a
        >
      </nav>
    </aside>

    <div class="catalog-results">
      <label class="search-label" for="package-query">Search packages</label>
      <div class="search-input">
        <svg
          width="19"
          height="19"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.6"
          aria-hidden="true"
        >
          <circle cx="10.5" cy="10.5" r="6.5" />
          <path d="m16 16 5 5" />
        </svg>
        <input
          id="package-query"
          v-model="query"
          type="search"
          placeholder="Name, command, or description…"
          autocomplete="off"
          @input="clearSelection"
        />
      </div>
      <div class="results-toolbar">
        <p role="status" aria-live="polite">
          {{
            loading
              ? "Loading packages…"
              : error
                ? "Catalog unavailable"
                : `${results.length} ${results.length === 1 ? "package" : "packages"}`
          }}<span v-if="!loading && !error && system">
            · {{ platforms.find(({ id }) => id === system)?.label }}</span
          >
        </p>
        <label
          >Sort
          <select v-model="sort" aria-label="Sort packages">
            <option value="relevance">Relevance</option>
            <option value="name">Name A–Z</option>
          </select></label
        >
      </div>
      <div v-if="query || system" class="active-filters">
        <span v-if="query">“{{ query }}”</span
        ><span v-if="system">{{ platforms.find(({ id }) => id === system)?.label }}</span
        ><button type="button" @click="clearFilters">Clear filters</button>
      </div>

      <div v-if="error" class="catalog-message" role="alert">
        <h2>Could not load packages</h2>
        <p>{{ error }}</p>
        <button type="button" @click="refresh">Try again</button
        ><a href="/guide/packages">Read the package guide →</a>
      </div>
      <p v-else-if="loading" class="catalog-message">Fetching the published package collection.</p>
      <div v-else-if="!results.length" class="catalog-message">
        <h2>No matching packages</h2>
        <p>Try another command name or description, or clear the platform filter.</p>
        <button v-if="query || system" type="button" @click="clearFilters">Show all packages</button
        ><a href="/guide/package-sources">Install from another source →</a>
      </div>
      <template v-else>
        <p v-if="show && !packages.some((pkg) => pkg.name === show)" class="catalog-message">
          “{{ show }}” is not in the published collection.
        </p>
        <div class="package-list">
          <article
            v-for="pkg in results"
            :key="pkg.name"
            class="package-result"
            :class="{ expanded: show === pkg.name }"
          >
            <h2 class="result-title">
              <button
                type="button"
                :aria-expanded="show === pkg.name"
                :aria-controls="`details-${pkg.name}`"
                @click="toggle(pkg)"
              >
                <span class="package-name">{{ pkg.name }}</span
                ><span class="package-version">{{ preferredVersion(pkg, system) }}</span
                ><span class="expand-indicator" aria-hidden="true">{{
                  show === pkg.name ? "−" : "+"
                }}</span>
              </button>
            </h2>
            <p class="description">{{ pkg.description }}</p>
            <div class="result-facts">
              <span
                >{{
                  pkg.versions[preferredVersion(pkg, system)].bins.length ? "Commands" : "Library"
                }}
                <code v-for="bin in pkg.versions[preferredVersion(pkg, system)].bins" :key="bin">{{
                  bin
                }}</code></span
              ><span>{{ summaryPlatforms(pkg) }}</span>
            </div>
            <div :id="`details-${pkg.name}`" :hidden="show !== pkg.name">
              <PackageDetails
                v-if="show === pkg.name"
                :key="`${pkg.name}:${system}`"
                v-model:version="version"
                :pkg="pkg"
                :system="system"
                :repository-url="theme.catalog.repositoryUrl"
              />
            </div>
          </article>
        </div>
        <p class="collection-note">
          <a href="/contributing/packaging">Missing a tool? Contribute a package →</a>
        </p>
      </template>
      <noscript
        >Enable JavaScript to search packages, or read the
        <a href="/guide/packages">package guide</a>.</noscript
      >
    </div>
  </div>
</template>

<style scoped>
.catalog-workspace {
  display: grid;
  grid-template-columns: 225px minmax(0, 1fr);
  gap: 40px;
  align-items: start;
}
.catalog-sidebar {
  position: sticky;
  top: calc(var(--vp-nav-height) + 24px);
  font-size: 13px;
}
fieldset {
  border: 0;
  padding: 0;
  margin: 0;
  min-width: 0;
}
legend,
.catalog-nav h2 {
  font-size: 12px;
  text-transform: uppercase;
  letter-spacing: 0.07em;
  font-weight: 600;
  margin: 0 0 12px;
}
.platform-filter label {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 9px 8px;
  margin: 0 -8px;
  cursor: pointer;
  border-left: 2px solid transparent;
}
.platform-filter label.active {
  border-left-color: var(--vp-c-brand-1);
  background: var(--vp-c-brand-soft);
  color: var(--vp-c-brand-1);
}
.platform-filter input {
  accent-color: var(--vp-c-brand-1);
}
.filter-count {
  margin-left: auto;
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  color: var(--vp-c-text-2);
}
.catalog-nav {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-top: 28px;
  padding-top: 24px;
  border-top: 1px solid var(--vp-c-divider);
}
.catalog-nav h2 {
  margin-bottom: 2px;
}
.catalog-nav a {
  color: var(--vp-c-text-2);
}
a:hover {
  color: var(--vp-c-brand-1);
  text-decoration: underline;
  text-underline-offset: 3px;
}
.catalog-results {
  min-width: 0;
}
.search-label {
  display: block;
  font-size: 13px;
  font-weight: 500;
  margin-bottom: 8px;
}
.search-input {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 0 14px;
  border: 1px solid var(--vp-c-text-2);
  background: var(--vp-c-bg);
}
.search-input:focus-within {
  outline: 2px solid var(--vp-c-brand-1);
  outline-offset: 2px;
}
.search-input svg {
  flex-shrink: 0;
  color: var(--vp-c-text-2);
}
.search-input input {
  width: 100%;
  min-width: 0;
  padding: 13px 0;
  background: transparent;
  font: inherit;
  font-size: 15px;
  outline: none;
}
.results-toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  min-height: 54px;
  font-size: 12px;
}
.results-toolbar p {
  margin: 0;
}
.results-toolbar p span {
  color: var(--vp-c-text-2);
}
.results-toolbar label {
  white-space: nowrap;
  color: var(--vp-c-text-2);
}
select {
  font: inherit;
  color: var(--vp-c-text-1);
  background: var(--vp-c-bg);
  padding: 4px;
  margin-left: 6px;
  border: 1px solid var(--vp-c-divider);
}
button {
  font: inherit;
  cursor: pointer;
}
button:focus-visible,
select:focus-visible,
a:focus-visible,
input[type="radio"]:focus-visible {
  outline: 2px solid var(--vp-c-brand-1);
  outline-offset: 3px;
}
.active-filters {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  margin-bottom: 16px;
}
.active-filters span {
  padding: 4px 8px;
  background: var(--vp-c-bg-soft);
}
.active-filters button {
  color: var(--vp-c-brand-1);
  padding: 4px 8px;
}
.package-list {
  border: 1px solid var(--vp-c-border);
}
.package-result + .package-result {
  border-top: 1px solid var(--vp-c-border);
}
.package-result.expanded {
  background: var(--vp-c-bg-elv);
}
.result-title {
  margin: 0;
  padding: 0;
  font-size: 16px;
  font-weight: 600;
}
.result-title > button {
  display: flex;
  align-items: baseline;
  gap: 10px;
  text-align: left;
  width: 100%;
  padding: 18px 24px 10px;
  flex-wrap: wrap;
}
.result-title > button:focus-visible {
  outline-offset: -4px;
}
.result-title > button:hover .package-name {
  text-decoration: underline;
  text-underline-offset: 4px;
}
.package-name {
  color: var(--vp-c-brand-1);
  overflow-wrap: anywhere;
}
.package-version {
  font-family: var(--vp-font-family-mono);
  overflow-wrap: anywhere;
  min-width: 0;
  font-size: 12px;
  font-weight: 400;
}
.expand-indicator {
  margin-left: auto;
  color: var(--vp-c-text-2);
  font-size: 20px;
  line-height: 1;
}
.description {
  margin: 0;
  padding: 0 24px;
  font-size: 14px;
  line-height: 1.65;
}
.result-facts {
  display: flex;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 6px 16px;
  font-size: 11px;
  color: var(--vp-c-text-2);
  padding: 10px 24px 18px;
  line-height: 1.8;
}
.result-facts code {
  font-family: var(--vp-font-family-mono);
  font-size: 11px;
  color: var(--vp-c-text-1);
  margin-left: 6px;
}
.catalog-message {
  border: 1px solid var(--vp-c-border);
  padding: 28px;
  font-size: 14px;
}
.catalog-message h2 {
  font-size: 17px;
  margin: 0 0 8px;
}
.catalog-message p {
  margin-bottom: 16px;
}
.catalog-message button {
  padding: 6px 12px;
  border: 1px solid var(--vp-c-border);
  margin-right: 16px;
}
.catalog-message a,
.collection-note a {
  color: var(--vp-c-brand-1);
}
.collection-note {
  font-size: 12px;
  color: var(--vp-c-text-2);
  margin-top: 24px;
}
@media (max-width: 960px) {
  .catalog-workspace {
    grid-template-columns: 200px minmax(0, 1fr);
    gap: 24px;
  }
}
@media (max-width: 767px) {
  .catalog-workspace {
    display: flex;
    flex-direction: column;
    gap: 24px;
  }
  .catalog-sidebar {
    position: static;
    width: 100%;
  }
  .platform-filter {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .platform-filter label {
    border: 1px solid var(--vp-c-border);
    padding: 6px 8px;
    margin: 0;
    font-size: 11px;
  }
  .filter-count {
    display: none;
  }
  .contribute-nav {
    display: none;
  }
  .catalog-nav {
    flex-direction: row;
    flex-wrap: wrap;
    gap: 8px 16px;
    margin-top: 16px;
    padding-top: 16px;
    font-size: 12px;
  }
  .catalog-nav h2 {
    display: none;
  }
  .catalog-results {
    width: 100%;
  }
  .result-title > button {
    padding: 16px 16px 10px;
    flex-wrap: wrap;
    gap: 6px;
  }
  .description {
    padding: 0 16px;
  }
  .result-facts {
    padding: 8px 16px 16px;
  }
}
</style>
