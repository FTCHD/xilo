import { execFileSync } from 'node:child_process';

// The date of the last commit that changed a file or folder, for the sitemap and the structured
// data. Paths are from the website folder. It needs the full git history. Files that are not
// committed have no date.
export function lastModified(path: string): Date | undefined {
  try {
    const date = execFileSync('git', ['log', '-1', '--format=%cI', '--', path], { encoding: 'utf8' }).trim();
    return date ? new Date(date) : undefined;
  } catch {
    return undefined;
  }
}
