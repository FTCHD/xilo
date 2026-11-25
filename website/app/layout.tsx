import type { Metadata, Viewport } from 'next';
import { Inter } from 'next/font/google';
import { Provider } from '@/components/provider';
import { appName, description, ogImage, openGraphBase, siteUrl, tagline } from '@/lib/shared';
import './global.css';

const inter = Inter({
  subsets: ['latin'],
});

export const metadata: Metadata = {
  metadataBase: new URL(`${siteUrl}/`),
  title: {
    default: `${appName}: ${tagline}`,
    template: `%s | ${appName}`,
  },
  description,
  applicationName: appName,
  keywords: [
    'Anker SOLIX',
    'Anker SOLIX app',
    'Anker app for Mac',
    'Anker SOLIX desktop',
    'power station app',
    'Solarbank',
    'C300X',
    'C1000',
    'F3800',
    'Bluetooth',
    'macOS',
    'Windows',
    'Linux',
  ],
  openGraph: {
    type: 'website',
    ...openGraphBase,
    images: [ogImage],
  },
  twitter: { card: 'summary_large_image' },
  robots: { index: true, follow: true },
  // Google Search Console, for the https://ftchd.github.io/xilo/ property
  verification: { google: 'I8RYFqXbU94T7UiVvAIEtCeS81CYvVXzYIuha9fBjfE' },
};

export const viewport: Viewport = {
  themeColor: [
    { media: '(prefers-color-scheme: dark)', color: '#051c33' },
    { media: '(prefers-color-scheme: light)', color: '#f4fbfd' },
  ],
};

export default function Layout({ children }: LayoutProps<'/'>) {
  return (
    <html lang="en" className={inter.className} suppressHydrationWarning>
      <body className="flex flex-col min-h-screen">
        <Provider>{children}</Provider>
      </body>
    </html>
  );
}
