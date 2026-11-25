import { llms, loader } from 'fumadocs-core/source';
import { docsRoute, siteUrl } from './shared';
import { defineDocs } from 'fumadocs-mdx/macro';
import { metaSchema, pageSchema } from 'fumadocs-core/source/schema';

const docs = defineDocs({
  dir: 'content/docs',
  docs: {
    schema: pageSchema,
    postprocess: {
      includeProcessedMarkdown: true,
    },
  },
  meta: {
    schema: metaSchema,
  },
});

// See https://fumadocs.dev/docs/headless/source-api for more info
export const source = loader({
  baseUrl: docsRoute,
  source: docs.toFumadocsSource(),
  plugins: [],
});

// Links in the Markdown start with "/", but AI crawlers read these files on their own, so they need
// the full address of the site.
function absoluteLinks(markdown: string) {
  return markdown.replace(/\]\(\//g, `](${siteUrl}/`).replace(/src="\//g, `src="${siteUrl}/`);
}

export const docsLlms = llms(source, {
  renderPage: async (page) => `# ${page.data.title} (${siteUrl}${page.url}/)

${absoluteLinks(await page.data.getText('processed'))}`,
  // full addresses, because AI crawlers read llms.txt on its own
  renderUrl: (item) =>
    /^\/.*\.(md|txt)$/.test(item.url) ? `${siteUrl}/llms.mdx${item.url}` : item.url.startsWith('/') ? `${siteUrl}${item.url}` : item.url,
});
