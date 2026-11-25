import { createGetUrl } from 'fumadocs-core/source';

export const appName = 'Xilo';
export const docsRoute = '/docs';
export const docsImageRoute = '/og/docs';

/** The base path from next.config.mjs, for paths that Next.js doesn't prefix: fetch, img and metadata. */
export const basePath = process.env.NEXT_PUBLIC_BASE_PATH ?? '';
/** The full address of the site, without a trailing slash. */
export const siteUrl = `${process.env.NEXT_PUBLIC_SITE_URL ?? ''}${basePath}`;

export function withBase(path: string) {
  return `${basePath}${path}`;
}

/** A full address, for the sitemap, structured data and llms.txt. Pages end with a slash, like the site. */
export function absoluteUrl(path: string) {
  return /\.[a-z]+$/.test(path) || path.endsWith('/') ? `${siteUrl}${path}` : `${siteUrl}${path}/`;
}

export const gitConfig = {
  user: 'ftchd',
  repo: 'xilo',
  branch: 'main',
};

export const repoUrl = `https://github.com/${gitConfig.user}/${gitConfig.repo}`;
export const downloadUrl = `${repoUrl}/releases/latest`;
export const licenseUrl = `${repoUrl}/blob/${gitConfig.branch}/LICENSE`;
export const noticeUrl = `${repoUrl}/blob/${gitConfig.branch}/NOTICE`;

export const tagline = 'The desktop app for Anker SOLIX power stations';
/** The picture for links shared on social media, for pages without their own. */
export const ogImage = { url: '/og.png', width: 1200, height: 630, alt: `Xilo: ${tagline}` };

/**
 * The parts of the Open Graph tags that every page has. A page that sets `openGraph`
 * replaces the whole object from the layout, so each page spreads this in.
 */
export const openGraphBase = { siteName: appName, locale: 'en_US' } as const;
export const description =
  'Monitor and control Anker SOLIX power stations, Solarbanks and chargers from macOS, Windows and Linux. Over Bluetooth, with no account, no cloud and no internet.';

const getImageUrl = createGetUrl(docsImageRoute);

export function getPageImageUrl(page: { slugs: string[]; locale?: string }) {
  const segments = [...page.slugs, 'image.png'];

  return { segments, url: getImageUrl(segments, page.locale) };
}
