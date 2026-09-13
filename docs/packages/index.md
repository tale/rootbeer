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

Find tools to install with Rootbeer. Add a package to `init.lua`, then run
`rb apply`. Follow [the package guide](/guide/packages) to make the commands
available in your shell.

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
