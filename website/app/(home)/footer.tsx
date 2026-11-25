import { Download } from 'lucide-react';
import Link from 'next/link';
import type { ReactNode } from 'react';
import { appName, downloadUrl, licenseUrl, noticeUrl, repoUrl, withBase } from '@/lib/shared';
import { source } from '@/lib/source';

// The groups on the devices page, as written by `cargo xtask site`.
const deviceGroups = ['Power stations', 'Solarbanks', 'Home power', 'Chargers and power banks', 'Coolers'];

function Column({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div>
      <h2 className="mb-3 text-sm font-semibold">{title}</h2>
      <ul className="flex flex-col gap-2 text-sm text-fd-muted-foreground">{children}</ul>
    </div>
  );
}

function Item({ href, children }: { href: string; children: ReactNode }) {
  const className = 'transition hover:text-fd-foreground';
  return (
    <li>
      {href.startsWith('http') ? (
        <a href={href} className={className}>
          {children}
        </a>
      ) : (
        <Link href={href} className={className}>
          {children}
        </Link>
      )}
    </li>
  );
}

// Links to every guide and group of devices, so that search engines find them from the home page.
export function Footer() {
  const guides = source.pageTree.children.flatMap((node) => (node.type === 'page' ? [node] : []));

  return (
    <footer className="border-t border-fd-border bg-fd-card/40">
      <div className="mx-auto grid w-full max-w-6xl grid-cols-2 gap-x-6 gap-y-10 px-6 py-14 lg:grid-cols-[minmax(0,1.4fr)_repeat(3,minmax(0,1fr))]">
        <div className="col-span-2 flex flex-col gap-3 lg:col-span-1">
          <Link href="/" className="flex items-center gap-2.5 text-lg font-semibold">
            <img src={withBase('/logo.png')} alt="" width={28} height={28} className="rounded-md" />
            {appName}
          </Link>
          <p className="max-w-xs text-sm text-fd-muted-foreground">
            The desktop app for Anker SOLIX power stations, Solarbanks and chargers. Over Bluetooth, with no account.
          </p>
          <a
            href={downloadUrl}
            className="mt-2 inline-flex items-center gap-2 self-start rounded-full bg-fd-primary px-4 py-2 text-sm font-semibold text-fd-primary-foreground transition hover:opacity-90"
          >
            <Download className="size-4" />
            Download {appName}
          </a>
          <p className="mt-auto max-w-xs pt-4 text-xs text-fd-muted-foreground">
            Free and open source under the{' '}
            <a href={licenseUrl} className="underline-offset-4 hover:text-fd-foreground hover:underline">
              MIT License
            </a>
            . {appName} is not made by or affiliated with Anker. Anker and SOLIX are trademarks of Anker Innovations.
          </p>
        </div>
        <Column title="Guides">
          {guides.map((page) => (
            <Item key={page.url} href={page.url}>
              {page.name}
            </Item>
          ))}
        </Column>
        <Column title="Devices">
          <Item href="/docs/devices">All supported devices</Item>
          {deviceGroups.map((group) => (
            <Item key={group} href={`/docs/devices#${group.toLowerCase().replaceAll(' ', '-')}`}>
              {group}
            </Item>
          ))}
        </Column>
        <Column title="Project">
          <Item href={repoUrl}>GitHub</Item>
          <Item href={downloadUrl}>Releases</Item>
          <Item href={licenseUrl}>License</Item>
          <Item href={noticeUrl}>Notices</Item>
          <Item href={`${repoUrl}/issues`}>Report a problem</Item>
        </Column>
      </div>
    </footer>
  );
}
