import { createMDX } from 'fumadocs-mdx/next';

const withMDX = createMDX();

// The site is at https://ftchd.github.io/xilo. For a custom domain, set the
// site URL to it and the base path to ''.
const siteUrl = 'https://ftchd.github.io';
const basePath = '/xilo';

/** @type {import('next').NextConfig} */
const config = {
  output: 'export',
  basePath,
  // GitHub Pages serves folders, so /docs/ has to be /docs/index.html.
  trailingSlash: true,
  images: { unoptimized: true },
  reactStrictMode: true,
  env: {
    NEXT_PUBLIC_SITE_URL: siteUrl,
    NEXT_PUBLIC_BASE_PATH: basePath,
  },
};

export default withMDX(config);
