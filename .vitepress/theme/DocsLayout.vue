<script setup lang="ts">
import DefaultTheme from "vitepress/theme-without-fonts";
import { useData, useRoute } from "vitepress";
import { computed } from "vue";
import { sectionForPath } from "../nav";
import CatalogLayout from "./CatalogLayout.vue";

const route = useRoute();
const { frontmatter } = useData();
const section = computed(() => sectionForPath(route.path));
</script>

<template>
  <CatalogLayout v-if="frontmatter.catalog" />
  <DefaultTheme.Layout v-else>
    <template #doc-before>
      <p v-if="section" class="doc-section">{{ section }}</p>
    </template>
  </DefaultTheme.Layout>
</template>
