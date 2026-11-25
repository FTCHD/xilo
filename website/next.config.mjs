import { createMDX } from 'fumadocs-mdx/next';

const withMDX = createMDX();

// The pages workflow sets these from the GitHub Pages settings: on ftchd.github.io the
// base path is "/xilo", and with a custom domain it is empty. The defaults are for a
// local build, so that it matches the site. An empty base path is kept, an empty address is not.
const siteUrl = process.env.NEXT_PUBLIC_SITE_URL || 'https://ftchd.github.io';
const basePath = process.env.NEXT_PUBLIC_BASE_PATH ?? '/xilo';

/** @type {import('next').NextConfig} */
const config = {
  output: 'export',
  basePath: basePath || undefined,
  // GitHub Pages serves folders, so /docs/ has to be /docs/index.html.
  trailingSlash: true,
  images: { unoptimized: true },
  reactStrictMode: true,
  // Next.js writes an AGENTS.md when an AI agent runs `next dev`. This project doesn't keep one.
  agentRules: false,
  env: {
    NEXT_PUBLIC_SITE_URL: siteUrl,
    NEXT_PUBLIC_BASE_PATH: basePath,
  },
};

export default withMDX(config);
