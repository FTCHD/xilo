import { docsLlms } from '@/lib/source';
import { absoluteUrl, description } from '@/lib/shared';

export const revalidate = false;

export async function GET() {
  // The index starts with its own title, which this file replaces.
  const docs = (await docsLlms.index()).replace(/^# .*\n+/, '');
  const text = `# Xilo

> ${description} Xilo is a free desktop app. It is not made by or affiliated with Anker.

Xilo shows the battery and the power on every port, changes every setting of the Anker app, and runs automations: schedules, a battery reserve, USB wake and alerts. It also keeps a history on the computer and has a command line tool, \`xilo\`. Each supported model has its own page under Devices.

## Docs

${docs}

## Optional

- [Everything in one file](${absoluteUrl('/llms-full.txt')}): all the docs and device pages.
`;
  return new Response(text, { headers: { 'Content-Type': 'text/plain; charset=utf-8' } });
}
