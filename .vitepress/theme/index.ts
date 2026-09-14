import DefaultTheme from "vitepress/theme-without-fonts";
import DocsLayout from "./DocsLayout.vue";
import "./custom.css";

export default {
  extends: DefaultTheme,
  Layout: DocsLayout,
};
