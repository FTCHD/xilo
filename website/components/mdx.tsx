import defaultMdxComponents from 'fumadocs-ui/mdx';
import { Tab, Tabs } from 'fumadocs-ui/components/tabs';
import type { MDXComponents } from 'mdx/types';
import type { ReactNode } from 'react';
import { withBase } from '@/lib/shared';

/** A product picture from public/devices, on the device pages. */
function DeviceImage({ src, alt }: { src: string; alt: string }) {
  return (
    <img
      src={withBase(src)}
      alt={alt}
      className="mx-auto my-6 max-h-56 w-auto drop-shadow-xl"
      loading="lazy"
    />
  );
}

/** A screenshot of Xilo from public/screenshots. */
function Screenshot({ src, alt }: { src: string; alt: string }) {
  return (
    <img
      src={withBase(src)}
      alt={alt}
      className="my-6 w-full rounded-xl border shadow-xl"
      loading="lazy"
    />
  );
}

/** A long list, in columns on wide screens. */
function Columns({ children }: { children: ReactNode }) {
  return <div className="my-5 sm:columns-2 lg:columns-3 [&>ul]:my-0 [&_li]:break-inside-avoid">{children}</div>;
}

export function getMDXComponents(components?: MDXComponents) {
  return {
    ...defaultMdxComponents,
    Columns,
    DeviceImage,
    Screenshot,
    Tab,
    Tabs,
    ...components,
  } satisfies MDXComponents;
}

export const useMDXComponents = getMDXComponents;

declare global {
  type MDXProvidedComponents = ReturnType<typeof getMDXComponents>;
}
