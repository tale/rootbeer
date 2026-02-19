import { defineConfig } from 'vitepress'

// https://vitepress.dev/reference/site-config
export default defineConfig({
  srcDir: "docs",

  title: "Rootbeer",
  description: "Deterministically manage your dotfiles",
  themeConfig: {
    // https://vitepress.dev/reference/default-theme-config
    nav: [
      { text: 'Home', link: '/' },
    ],

    sidebar: [
      {
        text: 'Modules',
        items: [
          { text: 'Zsh', link: '/modules/zsh' }
        ]
      }
    ],

    socialLinks: [
      { icon: 'github', link: 'https://github.com/tale/rootbeer' }
    ]
  }
})
