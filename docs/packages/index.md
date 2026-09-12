---
layout: page
sidebar: false
aside: false
outline: false
---

<script setup>
import PackageSearch from '../../.vitepress/theme/PackageSearch.vue'
</script>

<div class="catalog-page vp-doc">

# Package catalog

Find command-line tools for your configuration. Results come from the live
published index, with the default version available for each platform.

Copy a declaration into your Lua configuration, then run `rb apply`.
[Learn about packages](/guide/packages), [manage updates](/guide/package-locks),
or [contribute a recipe](/contributing/packaging).

<PackageSearch />

</div>

<style scoped>
.catalog-page {
  max-width: 1120px;
  margin: 0 auto;
  padding: 48px 24px 80px;
}

@media (min-width: 768px) {
  .catalog-page {
    padding: 64px 48px 96px;
  }
}
</style>
