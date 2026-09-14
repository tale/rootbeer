import { defineConfig } from "vitepress";
import { docsSidebar } from "./nav";

export default defineConfig({
  srcDir: "docs",
  cleanUrls: true,
  title: "Rootbeer",
  description: "Run tools, manage packages, and configure your system with Lua.",
  themeConfig: {
    catalog: {
      repositoryUrl: "https://github.com/tale/rootbeer-index",
      url: process.env.ROOTBEER_INDEX_URL || "https://tale.github.io/rootbeer-index/latest-v2.json",
      publicKey:
        process.env.ROOTBEER_INDEX_PUBLIC_KEY ||
        "028c5b185fb63ea61128a0bf6fb0decc8b700020561db08d82a998c7d0493bc0",
    },
    search: { provider: "local" },
    nav: [
      { text: "Documentation", link: "/guide/getting-started", activeMatch: "^/(guide|modules)/" },
      { text: "Reference", link: "/reference/", activeMatch: "^/(reference|formats|scripts)/" },
      {
        text: "Package catalog ↗",
        link: "/packages/",
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
        return `https://github.com/tale/rootbeer/edit/main/${source}`;
      },
      text: "Improve this page",
    },

    outline: {
      level: [2, 3],
      label: "On this page",
    },

    socialLinks: [
      { icon: "github", link: "https://github.com/tale/rootbeer" },
      { icon: "githubsponsors", link: "https://github.com/sponsors/tale" },
    ],
  },
});
