import { defineConfig } from "vitepress";
import { docsSidebar } from "./nav.ts";

export default defineConfig({
  srcDir: "docs",
  cleanUrls: true,
  title: "Rootbeer",
  description: "Run tools, manage packages, and configure your system with Lua.",
  themeConfig: {
    search: { provider: "local" },
    nav: [
      { text: "Documentation", link: "/guide/getting-started", activeMatch: "^/(guide|modules)/" },
      { text: "Reference", link: "/reference/", activeMatch: "^/(reference|formats|scripts)/" },
      {
        text: "Package catalog ↗",
        link: "https://search.rbpkg.com",
        target: "_blank",
        rel: "noopener noreferrer",
      },
    ],
    sidebar: docsSidebar,
    sidebarMenuLabel: "Documentation",
    docFooter: { prev: "Previous", next: "Next" },
    editLink: {
      pattern: ({ filePath }) => {
        const source = ["modules/index.md", "reference/index.md"].includes(filePath)
          ? ".vitepress/nav.ts"
          : `docs/${filePath}`;
        return `https://github.com/rootbeer-org/rootbeer/edit/main/${source}`;
      },
      text: "Improve this page",
    },

    outline: {
      level: [2, 3],
      label: "On this page",
    },

    socialLinks: [
      { icon: "github", link: "https://github.com/rootbeer-org/rootbeer" },
      { icon: "githubsponsors", link: "https://github.com/sponsors/tale" },
    ],
  },
});
