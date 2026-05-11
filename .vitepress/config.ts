import { defineConfig } from "vitepress";
import { modulesSection, referenceSection, sidebarFromSection } from "./nav";

// https://vitepress.dev/reference/site-config
export default defineConfig({
  srcDir: "docs",
  cleanUrls: true,
  title: "Rootbeer",
  description: "Deterministically manage your dotfiles",
  themeConfig: {
    nav: [
      { text: "Guide", link: "/guide/getting-started" },
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
          { text: "Packages", link: "/guide/packages" },
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
          { text: "Packaging", link: "/contributing/packaging" },
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
