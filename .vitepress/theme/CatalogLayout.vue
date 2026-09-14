<script setup lang="ts">
import { useData } from "vitepress";

const { isDark } = useData();
</script>

<template>
  <div class="catalog-layout">
    <a class="catalog-skip" href="#catalog-content">Skip to packages</a>
    <header class="catalog-app-header">
      <div class="catalog-app-nav">
        <a class="catalog-brand" href="/packages/">Rootbeer <span>Packages</span></a>
        <nav aria-label="Catalog navigation">
          <a href="/guide/getting-started">Documentation</a>
          <button type="button" @click="isDark = !isDark">
            {{ isDark ? "Light theme" : "Dark theme" }}
          </button>
        </nav>
      </div>
    </header>
    <main id="catalog-content" tabindex="-1"><Content /></main>
  </div>
</template>

<style scoped>
.catalog-layout {
  min-height: 100vh;
}
.catalog-app-header {
  position: sticky;
  top: 0;
  z-index: 20;
  background: var(--vp-c-bg);
  border-bottom: 1px solid var(--vp-c-border);
}
.catalog-app-nav {
  max-width: 1400px;
  min-height: var(--vp-nav-height);
  margin: 0 auto;
  padding: 0 40px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 20px;
}
.catalog-brand {
  display: flex;
  align-items: baseline;
  gap: 12px;
  font-size: 20px;
  font-weight: 600;
  letter-spacing: -0.025em;
}
.catalog-brand span {
  border-left: 1px solid var(--vp-c-border);
  padding-left: 12px;
  color: var(--vp-c-text-2);
  font-size: 15px;
  font-weight: 400;
  letter-spacing: 0;
}
nav {
  display: flex;
  align-items: center;
  gap: 24px;
  font-size: 13px;
}
nav a {
  color: var(--vp-c-text-2);
}
nav a:hover {
  color: var(--vp-c-brand-1);
}
button {
  cursor: pointer;
  border: 1px solid var(--vp-c-border);
  background: transparent;
  padding: 5px 9px;
  font: inherit;
  color: var(--vp-c-text-2);
}
button:hover {
  border-color: var(--vp-c-brand-1);
  color: var(--vp-c-brand-1);
}
.catalog-skip {
  position: fixed;
  top: 8px;
  left: 8px;
  z-index: 30;
  transform: translateY(-150%);
  background: var(--vp-c-bg);
  padding: 8px 12px;
  border: 1px solid var(--vp-c-brand-1);
}
.catalog-skip:focus {
  transform: none;
}
@media (max-width: 600px) {
  .catalog-app-nav {
    padding: 12px 20px;
    flex-wrap: wrap;
    gap: 10px 20px;
  }
  .catalog-brand {
    font-size: 18px;
  }
  nav {
    gap: 16px;
    font-size: 12px;
  }
}
</style>
