import type { BaseLayoutProps } from 'fumadocs-ui/layouts/shared';
import { appName, downloadUrl, repoUrl, withBase } from './shared';

export function baseOptions(): BaseLayoutProps {
  return {
    nav: {
      title: (
        <>
          <img src={withBase('/logo.png')} alt="" width={24} height={24} className="rounded-md" />
          <span className="font-semibold">{appName}</span>
        </>
      ),
    },
    links: [
      { text: 'Docs', url: '/docs', active: 'nested-url' },
      { text: 'Devices', url: '/docs/devices', active: 'nested-url' },
      { text: 'Download', url: downloadUrl, external: true },
    ],
    githubUrl: repoUrl,
  };
}
