# Xilo website

The website at https://ftchd.github.io/xilo, made with [Fumadocs](https://fumadocs.dev) on Next.js and exported as static files for GitHub Pages.

```sh
npm install
npm run dev        # http://localhost:3000/xilo
npm run build      # writes the site to out/
```

| Path | Content |
| --- | --- |
| `app/(home)/page.tsx` | The landing page |
| `app/(home)/footer.tsx` | The footer of the landing page, with a link to every guide |
| `content/docs` | The documentation, in MDX |
| `content/docs/devices` | One page for each model. Made by `cargo xtask site`, don't edit |
| `public/devices` | The product pictures, also made by `cargo xtask site` |
| `public/screenshots` | Screenshots of the app |
| `lib/shared.ts` | The name, the addresses and the texts that every page uses |

The `pages` workflow reads the address and the base path from the GitHub Pages settings: `/xilo` on ftchd.github.io, and nothing on a custom domain. So a custom domain needs no change here. A local build uses https://ftchd.github.io/xilo, as set in `next.config.mjs`.

The `pages` workflow builds and publishes the site when `website/` changes on `main`.
