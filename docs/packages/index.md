---
layout: page
sidebar: false
aside: false
outline: false
title: Packages
description: Search Rootbeer packages by name or command. Explore versions, supported platforms, and instructions for running or installing each tool.
---

<script setup>
import PackageSearch from '../../.vitepress/theme/PackageSearch.vue'
</script>

<div class="catalog-page">
  <header class="catalog-header">
    <h1>Packages</h1>
    <p>Command-line tools for macOS and Linux. Find a tool, check its versions, and make it yours.</p>
  </header>
  <PackageSearch />
</div>

<style scoped>
.catalog-page {
  max-width: 1400px;
  margin: 0 auto;
  padding: 36px 40px 80px;
}
.catalog-header {
  margin-bottom: 32px;
  padding-bottom: 24px;
  border-bottom: 1px solid var(--vp-c-divider);
}
.catalog-header h1 {
  font-size: 30px;
  font-weight: 600;
  line-height: 1.3;
  letter-spacing: -0.025em;
  margin: 0 0 8px;
}
.catalog-header p {
  font-size: 14px;
  color: var(--vp-c-text-2);
  margin: 0;
  line-height: 1.7;
}
@media (max-width: 767px) {
  .catalog-page { padding: 24px 20px 56px; }
  .catalog-header { margin-bottom: 24px; padding-bottom: 20px; }
}
</style>
