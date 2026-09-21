<script setup lang="ts">
import DefaultTheme from "vitepress/theme-without-fonts";
import { useRoute } from "vitepress";
import { computed } from "vue";
import { sectionForPath } from "../nav";

const route = useRoute();
const section = computed(() => sectionForPath(route.path));
</script>

<template>
  <DefaultTheme.Layout>
    <template #doc-before>
      <p v-if="section" class="doc-section">{{ section }}</p>
    </template>
  </DefaultTheme.Layout>
</template>
