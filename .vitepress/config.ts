import { defineConfig } from "vitepress";

// https://vitepress.dev/reference/site-config
export default defineConfig({
  srcDir: "docs",
  cleanUrls: true,
  title: "Rootbeer",
  description: "Deterministically manage your dotfiles",
  themeConfig: {
    nav: [
      { text: "Guide", link: "/guide/getting-started" },
      { text: "Modules", link: "/modules/zsh" },
      { text: "Reference", link: "/reference/core" },
    ],

    sidebar: [
      {
        text: "Guide",
        items: [
          { text: "Getting Started", link: "/guide/getting-started" },
          { text: "Core Concepts", link: "/guide/core-concepts" },
          { text: "Multi-Device Config", link: "/guide/multi-device" },
        ],
      },
      {
        text: "Modules",
        items: [
          {
            text: "Shell",
            collapsed: true,
            items: [{ text: "zsh", link: "/modules/zsh" }],
          },
          {
            text: "Developer Tools",
            collapsed: true,
            items: [
              { text: "git", link: "/modules/git" },
              { text: "ssh", link: "/modules/ssh" },
            ],
          },
          {
            text: "AI Coding",
            collapsed: true,
            items: [
              { text: "amp", link: "/modules/amp" },
              { text: "claude_code", link: "/modules/claude-code" },
            ],
          },
          {
            text: "Package Managers",
            collapsed: true,
            items: [{ text: "brew", link: "/modules/brew" }],
          },
          {
            text: "System",
            collapsed: true,
            items: [{ text: "mac", link: "/modules/mac" }],
          },
          {
            text: "Writers",
            collapsed: true,
            items: [
              { text: "json", link: "/modules/json" },
              { text: "toml", link: "/modules/toml" },
              { text: "ini", link: "/modules/ini" },
            ],
          },
        ],
      },
      {
        text: "Reference",
        collapsed: true,
        items: [
          { text: "Core API", link: "/reference/core" },
          { text: "Host", link: "/reference/host" },
        ],
      },
      {
        text: "Contributing",
        collapsed: true,
        items: [
          { text: "Dev Setup", link: "/contributing/setup" },
          { text: "Architecture", link: "/contributing/architecture" },
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
