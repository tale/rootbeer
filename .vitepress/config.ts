import { defineConfig } from 'vitepress'

// https://vitepress.dev/reference/site-config
export default defineConfig({
  srcDir: "docs",

  title: "Rootbeer",
  description: "Deterministically manage your dotfiles",
  themeConfig: {
    nav: [
      { text: 'Guide', link: '/guide/getting-started' },
      { text: 'API', link: '/api/core' },
    ],

    sidebar: [
      {
        text: 'Guide',
        items: [
          { text: 'Getting Started', link: '/guide/getting-started' },
        ]
      },
      {
        text: 'API Reference',
        items: [
          { text: 'Core', link: '/api/core' },
          { text: 'zsh', link: '/api/zsh' },
        ]
      }
    ],

    socialLinks: [
      { icon: 'github', link: 'https://github.com/tale/rootbeer' }
    ]
  }
})
