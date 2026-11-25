import { source } from '@/lib/source';
import {
  DocsBody,
  DocsDescription,
  DocsPage,
  DocsTitle,
  MarkdownCopyButton,
  ViewOptionsPopover,
} from 'fumadocs-ui/layouts/docs/page';
import { notFound } from 'next/navigation';
import { getMDXComponents } from '@/components/mdx';
import type { Metadata } from 'next';
import { createRelativeLink } from 'fumadocs-ui/mdx';
import { JsonLd } from '@/lib/json-ld';
import { lastModified } from '@/lib/last-modified';
import { absoluteUrl, appName, getPageImageUrl, gitConfig, openGraphBase, withBase } from '@/lib/shared';

export default async function Page(props: PageProps<'/docs/[[...slug]]'>) {
  const params = await props.params;
  const page = source.getPage(params.slug);
  if (!page) notFound();

  const MDX = page.data.body;
  const markdownUrl = withBase(`/llms.mdx${page.url}.md`);

  return (
    <DocsPage toc={page.data.toc} full={page.data.full}>
      <JsonLd data={structuredData(page)} />
      <DocsTitle>{page.data.title}</DocsTitle>
      <DocsDescription className="mb-0">{page.data.description}</DocsDescription>
      <div className="flex flex-row gap-2 items-center border-b pb-6">
        <MarkdownCopyButton markdownUrl={markdownUrl} />
        <ViewOptionsPopover
          markdownUrl={markdownUrl}
          githubUrl={`https://github.com/${gitConfig.user}/${gitConfig.repo}/blob/${gitConfig.branch}/website/content/docs/${encodeURI(page.path)}`}
        />
      </div>
      <DocsBody>
        <MDX
          components={getMDXComponents({
            // this allows you to link to other pages with relative file paths
            a: createRelativeLink(source, page),
          })}
        />
      </DocsBody>
    </DocsPage>
  );
}

export async function generateStaticParams() {
  return source.generateParams();
}

export async function generateMetadata(props: PageProps<'/docs/[[...slug]]'>): Promise<Metadata> {
  const params = await props.params;
  const page = source.getPage(params.slug);
  if (!page) notFound();

  return {
    // "Install Xilo", not "Install Xilo | Xilo".
    title: page.data.title.includes(appName) ? { absolute: page.data.title } : page.data.title,
    description: page.data.description,
    alternates: { canonical: `${page.url}/` },
    openGraph: {
      ...openGraphBase,
      type: 'article',
      url: `${page.url}/`,
      images: { url: getPageImageUrl(page).url, width: 1200, height: 630 },
    },
    twitter: { card: 'summary_large_image' },
  };
}

type DocsPageData = NonNullable<ReturnType<typeof source.getPage>>;

/** An article about Xilo, and the path to it from the docs home. */
function structuredData(page: DocsPageData) {
  const trail = [{ name: 'Xilo docs', url: '/docs' }];
  if (page.url.startsWith('/docs/devices/')) trail.push({ name: 'Supported devices', url: '/docs/devices' });
  if (page.url !== '/docs') trail.push({ name: page.data.title, url: page.url });
  return {
    '@graph': [
      {
        '@type': 'TechArticle',
        headline: page.data.title,
        description: page.data.description,
        url: absoluteUrl(page.url),
        image: absoluteUrl(getPageImageUrl(page).url),
        dateModified: lastModified(`content/docs/${page.path}`)?.toISOString(),
        inLanguage: 'en',
        about: { '@type': 'SoftwareApplication', name: appName, applicationCategory: 'UtilitiesApplication' },
      },
      {
        '@type': 'BreadcrumbList',
        itemListElement: trail.map((step, i) => ({
          '@type': 'ListItem',
          position: i + 1,
          name: step.name,
          item: absoluteUrl(step.url),
        })),
      },
    ],
  };
}
