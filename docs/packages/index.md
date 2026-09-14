---
layout: page
sidebar: false
aside: false
outline: false
title: Packages
description: Find tools to run on their own, install for your user, or add to your Rootbeer configuration.
---

<script setup>
import PackageSearch from '../../.vitepress/theme/PackageSearch.vue'
</script>

<div class="catalog-page">
  <nav class="catalog-nav" aria-label="Package navigation">
    <a class="catalog-home" href="/packages/" aria-current="page">Packages</a>
    <a href="/guide/packages">Getting started</a>
    <a href="/guide/package-locks">Updates and offline use</a>
    <a href="/guide/package-sources">Package sources</a>
    <a href="/contributing/packaging">Contribute a package</a>
  </nav>
  <div class="catalog-content vp-doc">
    <header class="catalog-header">
      <h1>Packages</h1>
      <p>Run a package, install it for your user, or add it to your configuration.</p>
    </header>
    <PackageSearch />
  </div>
</div>

<style scoped>
.catalog-page {
  display: grid;
  grid-template-columns: 200px minmax(0, 1fr);
  gap: 40px;
  max-width: 1440px;
  margin: 0 auto;
  padding: 40px 32px 80px;
}
.catalog-nav {
  display: flex;
  flex-direction: column;
  gap: 4px;
  position: sticky;
  top: calc(var(--vp-nav-height) + 32px);
  align-self: start;
  font-size: 0.875rem;
}
.catalog-nav a {
  padding: 8px 12px;
  color: var(--vp-c-text-2);
}
.catalog-nav a:hover {
  color: var(--vp-c-brand-1);
}
.catalog-nav .catalog-home {
  border-left: 3px solid var(--vp-c-brand-1);
  background: var(--vp-c-brand-soft);
  color: var(--vp-c-text-1);
  font-weight: 600;
  margin-bottom: 12px;
}
.catalog-content {
  min-width: 0;
}
.catalog-header {
  margin-bottom: 24px;
}
.catalog-header h1 {
  font-size: 2rem;
  border: 0;
  padding: 0;
}
.catalog-header p {
  color: var(--vp-c-text-2);
  margin-bottom: 0;
}
@media (max-width: 767px) {
  .catalog-page {
    display: block;
    padding: 24px 20px 64px;
  }
  .catalog-nav {
    position: static;
    flex-direction: row;
    gap: 8px;
    overflow-x: auto;
    white-space: nowrap;
    margin-bottom: 24px;
    padding-bottom: 8px;
  }
  .catalog-nav .catalog-home {
    margin-bottom: 0;
  }
}
</style>
