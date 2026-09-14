<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  availableVersions,
  defaultVersion,
  packageCommand,
  platforms,
  preferredVersion,
  primaryCommand,
  type CatalogPackage,
} from "./catalog";

const props = defineProps<{ pkg: CatalogPackage; system: string; repositoryUrl?: string }>();
const version = defineModel<string>("version", { default: "" });
const mode = ref<"run" | "use" | "config">("run");
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
    mode.value,
    isPinned.value ? selectedVersion.value : "",
    mode.value === "run" ? bin.value : "",
  ),
);
const snippet = computed(() =>
  mode.value === "use" ? `${command.value}\neval "$(rb env)"` : command.value,
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
        <fieldset class="usage-modes">
          <legend>Use this package</legend>
          <label
            v-for="[id, label] in [
              ['run', 'Run once'],
              ['use', 'Install'],
              ['config', 'Lua config'],
            ]"
            :key="id"
            :class="{ selected: mode === id }"
          >
            <input v-model="mode" type="radio" :name="`usage-${pkg.name}`" :value="id" />
            {{ label }}
          </label>
        </fieldset>
        <div class="command-options">
          <label
            >Version
            <select v-model="version">
              <option v-if="isInvalidVersion" :value="version" disabled>
                {{ version }} · unavailable
              </option>
              <option v-if="isDefaultAvailable" value="">
                Default{{ system ? ` · ${defaultVersion(pkg, system)}` : " for your platform" }}
              </option>
              <option v-else value="">{{ selectedVersion }} · exact version</option>
              <option v-for="entry in versions" :key="entry" :value="entry">
                {{ entry }} · exact version
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
        <p v-if="mode === 'run'" class="usage-note">
          Download and run <code>{{ bin }}</code
          >. No configuration or shell setup required. Pass arguments after <code>--</code>.
        </p>
        <p v-else-if="mode === 'use'" class="usage-note">
          Keep this package installed for your user. The second line makes its commands available in
          this shell.
        </p>
        <p v-else class="usage-note">
          Add this to <code>init.lua</code>, then run <code>rb apply</code> and
          <code>eval "$(rb env)"</code>.
        </p>
        <p v-if="isInvalidVersion" class="usage-note" role="alert">
          Version {{ version }} is not available{{ system ? " on this platform" : "" }}. Choose an
          available version to see its install command.
        </p>
        <div v-else class="command-block">
          <div class="command-bar">
            <span>{{ mode === "config" ? "init.lua" : "Terminal" }}</span
            ><button
              type="button"
              :aria-label="`Copy ${mode} instructions for ${pkg.name}`"
              @click="copy"
            >
              {{ copied ? "Copied" : "Copy" }}
            </button>
          </div>
          <pre><code>{{ snippet }}</code></pre>
        </div>
        <p v-if="copyError" class="usage-note" role="status">{{ copyError }}</p>
        <p v-if="!isInvalidVersion && isPinned" class="version-note">
          Pinned to {{ selectedVersion }}. Updating keeps this version.
        </p>
        <p v-else-if="!isInvalidVersion" class="version-note">
          Uses your platform's default on first install. Repeat runs keep the cached version; use
          <code>--update</code> to refresh it.
        </p>
        <a
          class="guide-link"
          :href="
            mode === 'run'
              ? '/guide/packages#run-a-tool'
              : mode === 'use'
                ? '/guide/packages#set-up-your-shell'
                : '/guide/packages#declare-tools-in-your-configuration'
          "
          >{{
            mode === "run"
              ? "More run examples"
              : mode === "use"
                ? "Set up new terminals"
                : "Configuration guide"
          }}
          →</a
        >
      </section>

      <section class="metadata" :aria-label="`${pkg.name} package details`">
        <h3>
          Commands provided <span>{{ selectedVersion }}</span>
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
        <p v-if="!recipe.bins.includes(pkg.name)" class="metadata-note">
          Install as <code>{{ pkg.name }}</code
          >; run it with
          {{ recipe.bins.length === 1 ? "the command above" : "one of these commands" }}.
        </p>
        <p v-if="pkg.aliases.length" class="metadata-note">
          Package aliases: <code v-for="alias in pkg.aliases" :key="alias">{{ alias }}</code>
        </p>
        <template v-if="recipe.apps && Object.keys(recipe.apps).length">
          <h3>Apps provided</h3>
          <p class="metadata-note">
            <code v-for="name in Object.keys(recipe.apps)" :key="name">{{ name }}</code>
          </p>
          <p class="metadata-note">
            Installing with <code>rb use</code> or <code>rb apply</code> creates managed
            links in <code>~/Applications</code>. Existing apps are never overwritten.
          </p>
        </template>
        <h3>Platform defaults</h3>
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
        <p class="metadata-note">
          Older defaults can keep a platform supported. Rootbeer installs a prebuilt package; it
          does not compile on your machine.
        </p>
      </section>
    </div>

    <section class="versions" :aria-label="`${pkg.name} versions`">
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
      <p class="metadata-note">
        Only published versions are listed.
        <a href="/guide/packages#choose-a-version">How version selection works →</a>
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
.metadata-note,
.version-note {
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
  white-space: nowrap;
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
