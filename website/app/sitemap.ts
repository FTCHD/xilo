import type { MetadataRoute } from 'next';
import { lastModified } from '@/lib/last-modified';
import { absoluteUrl } from '@/lib/shared';
import { source } from '@/lib/source';

export const dynamic = 'force-static';

export default function sitemap(): MetadataRoute.Sitemap {
  return [
    { url: absoluteUrl('/'), lastModified: lastModified('app/(home)') },
    ...source.getPages().map((page) => ({
      url: absoluteUrl(page.url),
      lastModified: lastModified(`content/docs/${page.path}`),
    })),
  ];
}
