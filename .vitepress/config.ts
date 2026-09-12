import { defineConfig } from "vitepress";
import { modulesSection, referenceSection, sidebarFromSection } from "./nav";

// https://vitepress.dev/reference/site-config
export default defineConfig({
  srcDir: "docs",
  cleanUrls: true,
  title: "Rootbeer",
  description: "Declare your packages and system configuration in Lua.",
  themeConfig: {
    catalog: {
      url: process.env.ROOTBEER_INDEX_URL || "https://tale.github.io/rootbeer-index/latest.json",
      publicKey:
        process.env.ROOTBEER_INDEX_PUBLIC_KEY ||
        "028c5b185fb63ea61128a0bf6fb0decc8b700020561db08d82a998c7d0493bc0",
    },
    search: { provider: "local" },
    nav: [
      { text: "Guide", link: "/guide/getting-started" },
      { text: "Packages", link: "/packages/" },
      { text: "Modules", link: modulesSection.root },
      { text: "Reference", link: referenceSection.root },
    ],

    sidebar: [
      {
        text: "Introduction",
        collapsed: false,
        items: [
          { text: "What is Rootbeer?", link: "/guide/what-is-rootbeer" },
          { text: "Getting Started", link: "/guide/getting-started" },
          { text: "Profiles", link: "/guide/profiles" },
        ],
      },
      {
        text: "Packages",
        collapsed: false,
        items: [
          { text: "Browse the Catalog", link: "/packages/" },
          { text: "Declare and Install", link: "/guide/packages" },
          { text: "Updates and Offline Use", link: "/guide/package-locks" },
          { text: "Catalogs and Backends", link: "/guide/package-sources" },
        ],
      },
      {
        text: "Modules",
        items: sidebarFromSection(modulesSection),
      },
      {
        text: "Reference",
        collapsed: true,
        items: sidebarFromSection(referenceSection),
      },
      {
        text: "Contributing",
        collapsed: true,
        items: [
          { text: "Dev Setup", link: "/contributing/setup" },
          { text: "Architecture", link: "/contributing/architecture" },
          { text: "Testing", link: "/contributing/testing" },
          { text: "Contribute Packages", link: "/contributing/packaging" },
          { text: "Index Hosting and Trust", link: "/contributing/package-hosting" },
          { text: "Distributing Rootbeer", link: "/contributing/distribution" },
        ],
      },
    ],

    outline: {
      level: "deep",
    },

    socialLinks: [
      { icon: "github", link: "https://github.com/tale/rootbeer" },
      { icon: "githubsponsors", link: "https://github.com/sponsors/tale" },
    ],
  },
});
