<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  availableVersions,
  defaultVersion,
  packageCommand,
  packageDependencies,
  platforms,
  preferredVersion,
  primaryCommand,
  type CatalogPackage,
} from "./catalog";

const props = defineProps<{ pkg: CatalogPackage; system: string; repositoryUrl?: string }>();
const version = defineModel<string>("version", { default: "" });
const mode = ref<"bootstrap" | "run" | "use" | "config">(
  props.pkg.name === "rootbeer" && !version.value ? "bootstrap" : "use",
);
const modes = computed(() => [
  ...(props.pkg.name === "rootbeer" ? [["bootstrap", "Install Rootbeer"]] : []),
  ["use", props.pkg.name === "rootbeer" ? "Install with rb" : "Install"],
  ["run", "Run once"],
  ["config", "Lua config"],
]);
const selectedBin = ref("");
const copied = ref(false);
const copyError = ref("");
const versions = computed(() => availableVersions(props.pkg, props.system));
const selectedVersion = computed(() =>
  versions.value.includes(version.value)
    ? version.value
    : preferredVersion(props.pkg, props.system),
);
const isInvalidVersion = computed(
  () => Boolean(version.value) && !versions.value.includes(version.value),
);
const recipe = computed(() => props.pkg.versions[selectedVersion.value]);
const dependencies = computed(() => packageDependencies(recipe.value, props.system));
const dependencyGroups = computed(() => {
  const descriptions: Record<string, string> = {
    "Build / link": "Tools and libraries used to build this package.",
    Build: "Tools used during compilation.",
    Link: "Libraries linked during compilation.",
    Runtime: "Packages required when running this package.",
    "Link / runtime": "Libraries needed during compilation and at runtime.",
  };
  return [...new Set(dependencies.value.map((dependency) => dependency.kind))].map((kind) => ({
    kind,
    description: descriptions[kind],
    entries: dependencies.value.filter((dependency) => dependency.kind === kind),
  }));
});
const isLibrary = computed(() => recipe.value.bins.length === 0);
const isDefaultAvailable = computed(() =>
  versions.value.includes(defaultVersion(props.pkg, props.system)),
);
const isPinned = computed(() => Boolean(version.value) || !isDefaultAvailable.value);
const bin = computed(() =>
  recipe.value?.bins.includes(selectedBin.value)
    ? selectedBin.value
    : primaryCommand(props.pkg, selectedVersion.value) || recipe.value?.bins[0] || "",
);
const command = computed(() =>
  packageCommand(
    props.pkg,
    mode.value === "bootstrap" ? "use" : mode.value,
    isPinned.value ? selectedVersion.value : "",
    mode.value === "run" ? bin.value : "",
  ),
);
const snippet = computed(() =>
  mode.value === "bootstrap"
    ? 'sh -c "$(curl -fsSL https://rootbeer.tale.me/rb.sh)"\nexport PATH="$HOME/.rootbeer/bin:$PATH"'
    : isLibrary.value
      ? `dependencies = { "${props.pkg.name}@${selectedVersion.value}" }`
      : mode.value === "use"
        ? `${command.value}\neval "$(rb env)"`
        : command.value,
);
const recipeUrl = computed(() =>
  props.repositoryUrl ? `${props.repositoryUrl}/blob/main/packages/${props.pkg.name}.lua` : "",
);
const permalink = computed(() => {
  const params = new URLSearchParams({ show: props.pkg.name });
  if (props.system) params.set("platform", props.system);
  if (isPinned.value) params.set("version", selectedVersion.value);
  return `/packages/?${params}`;
});
const sourceUrl = computed(() => {
  const source = recipe.value?.source?.match(/^github:([^@]+)@(.+)$/);
  if (source)
    return `https://github.com/${source[1]}/releases/tag/${encodeURIComponent(source[2])}`;
  const url = recipe.value?.build?.url;
  if (!url) return "";
  try {
    return new URL(url).protocol === "https:" ? url : "";
  } catch {
    return "";
  }
});

watch(snippet, () => {
  copied.value = false;
  copyError.value = "";
});
watch(version, () => {
  if (mode.value === "bootstrap") mode.value = "use";
});
watch(selectedVersion, () => {
  selectedBin.value = "";
});

async function copy() {
  try {
    await navigator.clipboard.writeText(snippet.value);
    copied.value = true;
    copyError.value = "";
  } catch {
    copyError.value = "Could not copy. Select and copy the command below.";
  }
}

function chooseCommand(command: string) {
  selectedBin.value =
    command === (primaryCommand(props.pkg, selectedVersion.value) || recipe.value.bins[0])
      ? ""
      : command;
  mode.value = "run";
}
</script>

<template>
  <div class="package-details">
    <nav class="package-links" :aria-label="`${pkg.name} links`">
      <a :href="pkg.homepage" target="_blank" rel="noopener noreferrer">Homepage ↗</a>
      <a v-if="recipeUrl" :href="recipeUrl" target="_blank" rel="noopener noreferrer"
        >Package recipe ↗</a
      >
      <a v-if="sourceUrl" :href="sourceUrl" target="_blank" rel="noopener noreferrer"
        >{{ recipe.build ? "Source archive" : "Upstream release" }} ↗</a
      >
      <a :href="permalink">Link to this package</a>
    </nav>

    <div class="detail-columns">
      <section class="usage" :aria-label="`Use ${pkg.name}`">
        <fieldset v-if="!isLibrary" class="usage-modes">
          <legend class="sr-only">Use this package</legend>
          <label v-for="[id, label] in modes" :key="id" :class="{ selected: mode === id }">
            <input v-model="mode" type="radio" :name="`usage-${pkg.name}`" :value="id" />
            {{ label }}
          </label>
        </fieldset>
        <div v-if="mode !== 'bootstrap'" class="command-options">
          <label
            >Version
            <select v-model="version">
              <option v-if="isInvalidVersion" :value="version" disabled>
                {{ version }} · unavailable
              </option>
              <option v-if="isDefaultAvailable" value="">
                Latest ({{ defaultVersion(pkg, system) }})
              </option>
              <option v-else value="">{{ selectedVersion }}</option>
              <option v-for="entry in versions" :key="entry" :value="entry">
                {{ entry }}
              </option>
            </select>
          </label>
          <label v-if="mode === 'run' && recipe.bins.length > 1"
            >Command
            <select v-model="selectedBin">
              <option value="">{{ primaryCommand(pkg, selectedVersion) || recipe.bins[0] }}</option>
              <option
                v-for="name in recipe.bins.filter(
                  (name) => name !== (primaryCommand(pkg, selectedVersion) || recipe.bins[0]),
                )"
                :key="name"
                :value="name"
              >
                {{ name }}
              </option>
            </select>
          </label>
        </div>
        <p v-if="mode === 'bootstrap'" class="usage-note">
          Install the latest Rootbeer nightly. Requires <code>curl</code> and <code>unzip</code>.
        </p>
        <p v-else-if="isLibrary" class="usage-note">
          Add to your recipe's <code>build</code> table to make this library available during
          compilation.
        </p>
        <p v-else-if="mode === 'run'" class="usage-note">
          Run <code>{{ bin }}</code> without adding it to your shell. Append <code>--</code>
          followed by any arguments for the command.
        </p>
        <p v-else-if="mode === 'use'" class="usage-note">
          Install <code>{{ pkg.name }}</code> for your user. The second line makes its commands
          available in your current shell.
        </p>
        <p v-else-if="mode === 'config'" class="usage-note">
          Add to <code>init.lua</code>, then run <code>rb apply</code>.
        </p>
        <p v-if="isInvalidVersion && mode !== 'bootstrap'" class="usage-note" role="alert">
          Version {{ version }} is not available{{ system ? " on this platform" : "" }}. Choose an
          available version to see its install command.
        </p>
        <div v-else class="command-block">
          <div class="command-bar">
            <span>{{
              isLibrary ? "Package recipe" : mode === "config" ? "init.lua" : "Terminal"
            }}</span
            ><button
              type="button"
              :aria-label="`Copy ${isLibrary ? 'build dependency' : mode} instructions for ${pkg.name}`"
              @click="copy"
            >
              {{ copied ? "Copied" : "Copy" }}
            </button>
          </div>
          <pre><code>{{ snippet }}</code></pre>
        </div>
        <p v-if="copyError" class="usage-note" role="status">{{ copyError }}</p>
        <a
          class="guide-link"
          :href="
            mode === 'bootstrap'
              ? '/guide/getting-started'
              : isLibrary
                ? '/contributing/packaging#library-dependencies'
                : '/guide/packages'
          "
          >{{
            mode === "bootstrap"
              ? "Installation guide"
              : isLibrary
                ? "Building with dependencies"
                : "Usage and updates"
          }}
          →</a
        >
      </section>

      <section class="metadata" :aria-label="`${pkg.name} package details`">
        <h3>
          {{ isLibrary ? "Libraries provided" : "Commands provided" }}
          <span>{{ selectedVersion }}</span>
        </h3>
        <div class="command-list">
          <button
            v-for="name in recipe.bins"
            :key="name"
            type="button"
            :title="`Show how to run ${name}`"
            @click="chooseCommand(name)"
          >
            <code>{{ name }}</code>
          </button>
        </div>
        <template v-if="recipe.build?.libraries?.length">
          <h3 v-if="!isLibrary">Libraries provided</h3>
          <p class="metadata-note">
            <code v-for="library in recipe.build.libraries" :key="library">{{ library }}</code>
          </p>
        </template>
        <p v-if="pkg.aliases.length" class="metadata-note">
          Package aliases: <code v-for="alias in pkg.aliases" :key="alias">{{ alias }}</code>
        </p>
        <template v-if="recipe.apps && Object.keys(recipe.apps).length">
          <h3>Apps provided</h3>
          <p class="metadata-note">
            <code v-for="name in Object.keys(recipe.apps)" :key="name">{{ name }}</code>
          </p>
        </template>
        <h3>Dependencies</h3>
        <div v-for="group in dependencyGroups" :key="group.kind" class="dependency-group">
          <p class="metadata-note">{{ group.description }}</p>
          <ul class="dependency-list">
            <li v-for="dependency in group.entries" :key="dependency.request">
              <a :href="dependency.href"
                ><code>{{ dependency.request }}</code></a
              >
            </li>
          </ul>
        </div>
        <p v-if="!dependencies.length" class="metadata-note">No package dependencies declared.</p>
        <h3>Platform defaults</h3>
        <p class="metadata-note">
          The version installed on each platform unless you choose a specific version.
        </p>
        <dl class="platform-defaults">
          <div
            v-for="platform in platforms"
            :key="platform.id"
            :class="{ 'chosen-platform': system === platform.id }"
          >
            <dt>{{ platform.label }}</dt>
            <dd>
              {{
                pkg.versions[defaultVersion(pkg, platform.id)]?.systems.includes(platform.id)
                  ? defaultVersion(pkg, platform.id)
                  : "Not available"
              }}
            </dd>
          </div>
        </dl>
      </section>
    </div>

    <section v-if="versions.length > 1" class="versions" :aria-label="`${pkg.name} versions`">
      <h3>
        Available versions
        <span
          >{{ versions.length }} · newest first{{
            system ? ` · ${platforms.find((platform) => platform.id === system)?.label}` : ""
          }}</span
        >
      </h3>
      <div class="version-table-wrap">
        <table>
          <thead>
            <tr>
              <th scope="col">Version</th>
              <th scope="col">Available on</th>
              <th scope="col"><span class="sr-only">Select version</span></th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="entry in versions"
              :key="entry"
              :class="{ 'chosen-version': selectedVersion === entry }"
            >
              <th scope="row">
                <code>{{ entry }}</code
                ><span v-if="entry === defaultVersion(pkg, system)" class="default-badge"
                  >Default{{
                    !system && Object.keys(pkg.default_versions ?? {}).length ? "*" : ""
                  }}</span
                >
              </th>
              <td>
                {{
                  platforms
                    .filter((platform) => pkg.versions[entry].systems.includes(platform.id))
                    .map((platform) => platform.short)
                    .join(" · ")
                }}
              </td>
              <td>
                <button
                  type="button"
                  :aria-label="`Use ${pkg.name} version ${entry}`"
                  @click="version = entry"
                >
                  {{ version === entry ? "Selected" : "Use version" }}
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <p v-if="!system && Object.keys(pkg.default_versions ?? {}).length" class="metadata-note">
        * Some platforms use a different default; see the platform list above.
      </p>
    </section>
  </div>
</template>

<style scoped>
.package-details {
  padding: 0 24px 24px;
  border-top: 1px solid var(--vp-c-divider);
}
.package-links {
  display: flex;
  gap: 12px 24px;
  flex-wrap: wrap;
  padding: 16px 0 24px;
  font-size: 13px;
}
a {
  color: var(--vp-c-brand-1);
  text-decoration: underline;
  text-underline-offset: 3px;
}
.detail-columns {
  display: grid;
  grid-template-columns: minmax(0, 1.2fr) minmax(0, 1fr);
  gap: 32px;
}
fieldset {
  border: 0;
  margin: 0;
  padding: 0;
  min-width: 0;
}
.usage-modes {
  display: flex;
  flex-wrap: wrap;
}
legend,
h3 {
  font-size: 14px;
  font-weight: 600;
  margin: 0 0 12px;
}
.usage-modes legend {
  margin-bottom: 12px;
}
.usage-modes label {
  display: inline-flex;
  gap: 6px;
  align-items: center;
  padding: 7px 12px;
  font-size: 13px;
  border: 1px solid var(--vp-c-border);
  margin-right: -1px;
  cursor: pointer;
}
.usage-modes .selected {
  color: var(--vp-c-brand-1);
  background: var(--vp-c-brand-soft);
  border-color: var(--vp-c-brand-1);
  z-index: 1;
}
input {
  accent-color: var(--vp-c-brand-1);
}
.command-options {
  display: flex;
  gap: 12px;
  margin-top: 16px;
}
.command-options label {
  flex: 1;
  min-width: 0;
  font-size: 12px;
  font-weight: 500;
}
select {
  display: block;
  width: 100%;
  margin-top: 5px;
  padding: 8px;
  border: 1px solid var(--vp-c-border);
  background: var(--vp-c-bg);
  color: var(--vp-c-text-1);
  font: inherit;
  font-size: 13px;
}
.usage-note,
.metadata-note {
  font-size: 13px;
  line-height: 1.65;
  color: var(--vp-c-text-2);
  margin: 12px 0;
}
.command-block {
  border: 1px solid var(--vp-c-border);
  margin-top: 12px;
  background: var(--vp-c-bg-alt);
}
.command-bar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 5px 10px;
  border-bottom: 1px solid var(--vp-c-divider);
  font-size: 11px;
  color: var(--vp-c-text-2);
}
pre {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  margin: 0;
  padding: 14px;
  font-size: 12px;
  line-height: 1.9;
}
code {
  font-family: var(--vp-font-family-mono);
  font-size: 0.92em;
}
button {
  cursor: pointer;
  font: inherit;
}
.command-bar button,
td button {
  padding: 4px 8px;
  border: 1px solid var(--vp-c-border);
  background: var(--vp-c-bg);
  color: var(--vp-c-text-1);
}
button:hover {
  color: var(--vp-c-brand-1);
  border-color: var(--vp-c-brand-1);
}
button:focus-visible,
select:focus-visible,
input:focus-visible,
a:focus-visible {
  outline: 2px solid var(--vp-c-brand-1);
  outline-offset: 3px;
}
.guide-link {
  display: inline-block;
  margin-top: 12px;
  font-size: 13px;
}
h3 span {
  font-weight: 400;
  color: var(--vp-c-text-2);
  margin-left: 6px;
  font-size: 12px;
}
.command-list {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.command-list button {
  border: 1px solid var(--vp-c-border);
  padding: 4px 9px;
  background: var(--vp-c-bg);
}
.metadata-note code + code {
  margin-left: 8px;
}
.metadata h3:not(:first-child) {
  margin-top: 20px;
}
.dependency-list {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
  gap: 8px 16px;
  list-style: none;
  padding: 0;
  margin: 0;
  font-size: 13px;
}
.dependency-list li {
  min-width: 0;
}
.dependency-list a {
  overflow-wrap: anywhere;
}
.platform-defaults {
  margin: 0;
  font-size: 12px;
}
.platform-defaults > div {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  padding: 7px 0;
  border-bottom: 1px solid var(--vp-c-divider);
}
dd {
  margin: 0;
  font-family: var(--vp-font-family-mono);
  overflow-wrap: anywhere;
  text-align: right;
}
.chosen-platform {
  color: var(--vp-c-brand-1);
  font-weight: 500;
}
.versions {
  margin-top: 28px;
}
.version-table-wrap {
  max-height: 300px;
  overflow: auto;
  border: 1px solid var(--vp-c-border);
}
table {
  width: 100%;
  border-collapse: collapse;
  text-align: left;
  font-size: 12px;
}
thead {
  position: sticky;
  top: 0;
  background: var(--vp-c-bg-alt);
  z-index: 1;
}
th,
td {
  padding: 10px 12px;
  border-bottom: 1px solid var(--vp-c-divider);
}
th {
  font-weight: 500;
}
tbody th {
  white-space: nowrap;
}
td:last-child {
  text-align: right;
  white-space: nowrap;
}
.default-badge {
  display: inline-block;
  margin-left: 8px;
  padding: 1px 5px;
  background: var(--vp-c-brand-soft);
  color: var(--vp-c-brand-1);
  font: 10px var(--vp-font-family-base);
}
.chosen-version {
  background: var(--vp-c-bg-alt);
}
.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}
@media (max-width: 1100px) {
  .detail-columns {
    grid-template-columns: 1fr;
    gap: 24px;
  }
}
@media (max-width: 600px) {
  .package-details {
    padding: 0 16px 20px;
  }
  .command-options {
    flex-direction: column;
  }
  th,
  td {
    padding: 8px;
  }
}
</style>
