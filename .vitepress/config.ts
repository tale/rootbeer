import { defineConfig } from 'vitepress'

// https://vitepress.dev/reference/site-config
export default defineConfig({
  srcDir: "docs",

  title: "Rootbeer",
  description: "Deterministically manage your dotfiles",
  themeConfig: {
    nav: [
      { text: 'Guide', link: '/guide/getting-started' },
      { text: 'Modules', link: '/modules/zsh' },
    ],

    sidebar: [
      {
        text: 'Guide',
        items: [
          { text: 'Getting Started', link: '/guide/getting-started' },
          { text: 'Core API', link: '/guide/core-api' },
        ]
      },
      {
        text: 'Modules',
        items: [
          { text: 'Zsh', link: '/modules/zsh' },
        ]
      }
    ],

    socialLinks: [
      { icon: 'github', link: 'https://github.com/tale/rootbeer' }
    ]
  }
})
