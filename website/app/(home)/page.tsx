import Link from 'next/link';
import type { Metadata } from 'next';
import type { ReactNode } from 'react';
import {
  ArrowRight,
  BatteryMedium,
  Bell,
  Bluetooth,
  CalendarClock,
  ChartLine,
  Download,
  Gauge,
  HardDrive,
  PanelTop,
  ShieldCheck,
  SlidersHorizontal,
  SquareTerminal,
  UserX,
  WifiOff,
} from 'lucide-react';
import { source } from '@/lib/source';
import { appName, description, downloadUrl, ogImage, repoUrl, siteUrl, tagline, withBase } from '@/lib/shared';

export const metadata: Metadata = {
  alternates: { canonical: '/' },
  openGraph: { url: '/', images: [ogImage] },
};

const features = [
  {
    icon: Gauge,
    title: 'Live dashboard',
    text: 'Battery, temperature, time left, and the power on every port, updated as it changes.',
  },
  {
    icon: SlidersHorizontal,
    title: 'Every setting',
    text: 'AC charging power, timeouts, the screen and the light, with the options of the Anker app. Xilo checks each value before it sends it.',
  },
  {
    icon: CalendarClock,
    title: 'Schedules',
    text: 'Switch outputs, the light or the screen at set times, on the days you choose.',
  },
  {
    icon: BatteryMedium,
    title: 'Battery reserve',
    text: 'Keep charge for later. Xilo turns the outputs you choose off at the level you set.',
  },
  {
    icon: Bell,
    title: 'Alerts',
    text: 'Errors, low battery, charged to 80, 90 or 100%, AC power lost and back, as system notifications.',
  },
  {
    icon: ChartLine,
    title: 'History',
    text: 'Battery and power every 10 seconds, kept on your computer, with energy per day and CSV export.',
  },
  {
    icon: PanelTop,
    title: 'Menu bar',
    text: 'The battery level at a glance and quick output switches, without opening the window.',
  },
  {
    icon: SquareTerminal,
    title: 'Command line',
    text: 'The xilo tool reads and changes the same things from a terminal or a script.',
  },
];

const faq = [
  {
    question: 'Is Xilo free?',
    answer: 'Yes. Xilo is free to download and use on macOS, Windows and Linux.',
  },
  {
    question: 'Does Xilo need an Anker account or the internet?',
    answer:
      'No. Xilo talks to your devices over Bluetooth only. There is no account to create, nothing to sign in to, and it works without an internet connection, for example during a power cut.',
  },
  {
    question: 'Which Anker devices does Xilo support?',
    answer:
      'Xilo supports 38 Anker models: SOLIX power stations such as the C300X, C1000 and F3800, Solarbanks, the Home Power Panel, Prime chargers and EverFrost coolers. The devices page lists them all.',
  },
  {
    question: 'Can I use Xilo and the Anker app at the same time?',
    answer:
      'A device accepts one Bluetooth connection at a time. When you want to use the Anker app on your phone, choose "Release to phone" in Xilo, and Xilo lets go of the device.',
  },
  {
    question: 'Does Xilo update the firmware?',
    answer: 'No. Xilo shows the firmware version that is installed, but it never changes it.',
  },
  {
    question: 'Is Xilo made by Anker?',
    answer: 'No. Xilo is an independent app. It is not made by or affiliated with Anker.',
  },
];

function JsonLd() {
  const data = [
    {
      '@context': 'https://schema.org',
      '@type': 'SoftwareApplication',
      name: appName,
      description,
      url: `${siteUrl}/`,
      applicationCategory: 'UtilitiesApplication',
      operatingSystem: 'macOS, Windows, Linux',
      image: `${siteUrl}/og.png`,
      screenshot: `${siteUrl}/screenshots/wall.webp`,
      downloadUrl,
      offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
    },
    {
      '@context': 'https://schema.org',
      '@type': 'FAQPage',
      mainEntity: faq.map((item) => ({
        '@type': 'Question',
        name: item.question,
        acceptedAnswer: { '@type': 'Answer', text: item.answer },
      })),
    },
  ];
  return (
    <script
      type="application/ld+json"
      dangerouslySetInnerHTML={{ __html: JSON.stringify(data).replace(/</g, '\\u003c') }}
    />
  );
}

function Section({ id, children, className = '' }: { id?: string; children: ReactNode; className?: string }) {
  return (
    <section id={id} className={`mx-auto w-full max-w-6xl px-6 py-20 ${className}`}>
      {children}
    </section>
  );
}

function Heading({ eyebrow, title, text }: { eyebrow: string; title: string; text?: string }) {
  return (
    <div className="mx-auto mb-12 max-w-2xl text-center">
      <p className="mb-3 text-sm font-semibold uppercase tracking-wider text-fd-primary">{eyebrow}</p>
      <h2 className="text-3xl font-bold tracking-tight md:text-4xl">{title}</h2>
      {text && <p className="mt-4 text-lg text-fd-muted-foreground">{text}</p>}
    </div>
  );
}

function Shot({ src, alt, width, height, className = '' }: { src: string; alt: string; width: number; height: number; className?: string }) {
  return (
    <img
      src={withBase(src)}
      alt={alt}
      width={width}
      height={height}
      loading="lazy"
      className={`h-auto w-full rounded-xl border border-fd-border shadow-2xl shadow-black/30 ${className}`}
    />
  );
}

function Hero() {
  return (
    <div className="relative overflow-hidden">
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 -z-10 bg-[radial-gradient(60%_60%_at_15%_0%,rgba(0,203,235,0.28),transparent_70%),radial-gradient(50%_50%_at_90%_10%,rgba(3,92,143,0.35),transparent_70%)]"
      />
      <div className="mx-auto max-w-6xl px-6 pb-16 pt-20 text-center md:pt-28">
        <p className="mx-auto mb-6 inline-flex items-center gap-2 rounded-full border border-fd-border bg-fd-card px-4 py-1.5 text-sm text-fd-muted-foreground">
          <Bluetooth className="size-4 text-fd-primary" />
          Free for macOS, Windows and Linux
        </p>
        <h1 className="mx-auto max-w-4xl text-4xl font-bold tracking-tight md:text-6xl">
          Your Anker SOLIX power station,{' '}
          <span className="bg-gradient-to-r from-[#00cbeb] to-[#3b9fe0] bg-clip-text text-transparent">
            on your desktop
          </span>
        </h1>
        <p className="mx-auto mt-6 max-w-2xl text-lg text-fd-muted-foreground md:text-xl">
          {appName} monitors and controls Anker SOLIX power stations, Solarbanks and chargers over Bluetooth. No account,
          no cloud, no internet.
        </p>
        <div className="mt-10 flex flex-wrap items-center justify-center gap-3">
          <a
            href={downloadUrl}
            className="inline-flex items-center gap-2 rounded-full bg-fd-primary px-6 py-3 font-semibold text-fd-primary-foreground shadow-lg shadow-[#00cbeb]/20 transition hover:opacity-90"
          >
            <Download className="size-5" />
            Download {appName}
          </a>
          <Link
            href="/docs"
            className="inline-flex items-center gap-2 rounded-full border border-fd-border bg-fd-card px-6 py-3 font-semibold transition hover:bg-fd-accent"
          >
            Read the docs
            <ArrowRight className="size-4" />
          </Link>
        </div>
      </div>
      <div className="mx-auto max-w-6xl px-6">
        {/* The picture says "click to see every screen up close", so it links to the full size. */}
        <a href={withBase('/screenshots/wall-full.webp')} className="block">
          <img
            src={withBase('/screenshots/wall.webp')}
            alt="Xilo showing the dashboard of an Anker SOLIX C300X in front of a wall of its other screens"
            width={2400}
            height={1800}
            fetchPriority="high"
            className="h-auto w-full rounded-2xl border border-fd-border shadow-2xl shadow-[#00cbeb]/10"
          />
        </a>
      </div>
    </div>
  );
}

function Devices() {
  const devices = source
    .getPages()
    .filter((page) => page.url.startsWith('/docs/devices/'))
    .map((page) => ({
      url: page.url,
      name: page.data.title.replace(/^Anker (SOLIX )?/, '').replace(/ \(.*\)$/, ''),
    }))
    .sort((a, b) => a.name.localeCompare(b.name, 'en', { numeric: true }));
  return (
    <Section className="pb-8">
      <Heading
        eyebrow="Supported devices"
        title={`Works with ${devices.length} Anker models`}
        text="Power stations, Solarbanks, the Home Power Panel, Prime chargers and EverFrost coolers."
      />
      <ul className="flex flex-wrap justify-center gap-2">
        {devices.map((device) => (
          <li key={device.url}>
            <Link
              href={device.url}
              className="block rounded-full border border-fd-border bg-fd-card px-3 py-1.5 text-sm text-fd-muted-foreground transition hover:border-fd-primary hover:text-fd-foreground"
            >
              {device.name}
            </Link>
          </li>
        ))}
      </ul>
    </Section>
  );
}

function Features() {
  return (
    <Section id="features">
      <Heading
        eyebrow="Features"
        title="Everything the Anker app does, and more"
        text="The dashboards follow the Anker app, and Xilo adds what a computer is good at: automations, history and a command line."
      />
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {features.map(({ icon: Icon, title, text }) => (
          <div key={title} className="rounded-2xl border border-fd-border bg-fd-card p-6">
            <div className="mb-4 inline-flex size-11 items-center justify-center rounded-xl bg-[#00cbeb]/12 text-fd-primary">
              <Icon className="size-5" />
            </div>
            <h3 className="mb-2 font-semibold">{title}</h3>
            <p className="text-sm text-fd-muted-foreground">{text}</p>
          </div>
        ))}
      </div>
    </Section>
  );
}

function Automations() {
  const points = [
    ['Schedules', 'Turn the AC output, the car output or the screen on or off, or set the light, at a set time on the days you choose.'],
    ['Battery reserve', 'When the battery drops to 10, 20, 30 or 50% while it discharges, Xilo turns off the outputs you choose.'],
    ['USB wake', 'Some devices, like the C300X, turn their USB ports off at low load. Xilo keeps them on.'],
    ['Activity', 'Every automation and alert from the last 7 days, in one list.'],
  ];
  return (
    <Section id="automations">
      <div className="grid items-center gap-12 lg:grid-cols-2">
        <div>
          <p className="mb-3 text-sm font-semibold uppercase tracking-wider text-fd-primary">Automations</p>
          <h2 className="text-3xl font-bold tracking-tight md:text-4xl">Let Xilo look after your devices</h2>
          <p className="mt-4 text-lg text-fd-muted-foreground">
            Automations run while Xilo is open or in the menu bar, and the device is in Bluetooth range.
          </p>
          <dl className="mt-8 space-y-5">
            {points.map(([term, text]) => (
              <div key={term}>
                <dt className="font-semibold">{term}</dt>
                <dd className="text-fd-muted-foreground">{text}</dd>
              </div>
            ))}
          </dl>
          <Link href="/docs/automations" className="mt-8 inline-flex items-center gap-2 font-semibold text-fd-primary">
            How automations work
            <ArrowRight className="size-4" />
          </Link>
        </div>
        <Shot
          src="/screenshots/automations.webp"
          alt="The Automations tab of an Anker SOLIX C300X in Xilo: three schedules, a battery reserve at 20% and USB wake"
          width={1600}
          height={1415}
        />
      </div>
    </Section>
  );
}

function Gallery() {
  const shots = [
    { src: '/screenshots/dashboard.webp', alt: 'The dashboard of an Anker SOLIX C300X in Xilo', title: 'Dashboard', height: 1335 },
    { src: '/screenshots/history.webp', alt: 'Battery and power history charts in Xilo', title: 'History', height: 1095 },
    { src: '/screenshots/settings.webp', alt: 'The settings of an Anker SOLIX C300X in Xilo', title: 'Settings', height: 1095 },
    { src: '/screenshots/schedule.webp', alt: 'Adding a schedule in Xilo', title: 'Schedules', height: 1095 },
  ];
  return (
    <Section id="screens">
      <Heading eyebrow="Screens" title="Made for a big screen" />
      <div className="grid gap-8 md:grid-cols-2">
        {shots.map((shot) => (
          <figure key={shot.src}>
            <Shot src={shot.src} alt={shot.alt} width={1600} height={shot.height} />
            <figcaption className="mt-3 text-center text-sm text-fd-muted-foreground">{shot.title}</figcaption>
          </figure>
        ))}
      </div>
    </Section>
  );
}

function Privacy() {
  const points = [
    { icon: UserX, title: 'No account', text: 'There is nothing to sign up for and nothing to sign in to.' },
    { icon: WifiOff, title: 'No internet', text: 'Xilo works over Bluetooth only, so it keeps working during a power cut.' },
    { icon: HardDrive, title: 'Your data stays here', text: 'Settings and history are kept on your computer, and nowhere else.' },
  ];
  return (
    <Section id="privacy">
      <div className="rounded-3xl border border-fd-border bg-gradient-to-br from-[#00cbeb]/10 via-transparent to-[#035c8f]/20 p-8 md:p-12">
        <div className="mb-10 flex items-center gap-3">
          <ShieldCheck className="size-7 text-fd-primary" />
          <h2 className="text-2xl font-bold md:text-3xl">Private by design</h2>
        </div>
        <div className="grid gap-8 md:grid-cols-3">
          {points.map(({ icon: Icon, title, text }) => (
            <div key={title}>
              <Icon className="mb-3 size-6 text-fd-primary" />
              <h3 className="mb-1 font-semibold">{title}</h3>
              <p className="text-fd-muted-foreground">{text}</p>
            </div>
          ))}
        </div>
      </div>
    </Section>
  );
}

function Faq() {
  return (
    <Section id="faq">
      <Heading eyebrow="FAQ" title="Questions" />
      <div className="mx-auto max-w-3xl divide-y divide-fd-border rounded-2xl border border-fd-border bg-fd-card">
        {faq.map((item) => (
          <details key={item.question} className="group p-6">
            <summary className="cursor-pointer list-none font-semibold marker:hidden">
              <span className="flex items-center justify-between gap-4">
                {item.question}
                <ArrowRight className="size-4 shrink-0 transition group-open:rotate-90" />
              </span>
            </summary>
            <p className="mt-3 text-fd-muted-foreground">{item.answer}</p>
          </details>
        ))}
      </div>
    </Section>
  );
}

function Closing() {
  return (
    <Section className="pb-12 text-center">
      <h2 className="text-3xl font-bold tracking-tight md:text-4xl">Get {appName}</h2>
      <p className="mx-auto mt-4 max-w-xl text-lg text-fd-muted-foreground">{tagline}, for macOS, Windows and Linux.</p>
      <div className="mt-8 flex flex-wrap justify-center gap-3">
        <a
          href={downloadUrl}
          className="inline-flex items-center gap-2 rounded-full bg-fd-primary px-6 py-3 font-semibold text-fd-primary-foreground transition hover:opacity-90"
        >
          <Download className="size-5" />
          Download
        </a>
        <Link
          href="/docs/install"
          className="inline-flex items-center gap-2 rounded-full border border-fd-border bg-fd-card px-6 py-3 font-semibold transition hover:bg-fd-accent"
        >
          How to install
        </Link>
      </div>
      <footer className="mt-20 border-t border-fd-border pt-8 text-sm text-fd-muted-foreground">
        <p>
          {appName} is not made by or affiliated with Anker. Anker and SOLIX are trademarks of Anker Innovations.
        </p>
        <p className="mt-2">
          <a href={repoUrl} className="hover:text-fd-foreground">
            GitHub
          </a>
          {' · '}
          <Link href="/docs" className="hover:text-fd-foreground">
            Documentation
          </Link>
          {' · '}
          <Link href="/docs/devices" className="hover:text-fd-foreground">
            Devices
          </Link>
        </p>
      </footer>
    </Section>
  );
}

export default function HomePage() {
  return (
    <main className="flex flex-1 flex-col">
      <JsonLd />
      <Hero />
      <Devices />
      <Features />
      <Automations />
      <Gallery />
      <Privacy />
      <Faq />
      <Closing />
    </main>
  );
}
