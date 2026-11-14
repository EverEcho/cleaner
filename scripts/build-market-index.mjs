import { readFile, writeFile } from 'node:fs/promises';
import { parseMarketIndex } from '../src/market-index.ts';

const [mode, output, ...extra] = process.argv.slice(2);
if (extra.length || (mode === '--check' ? output : mode !== '--output' || !output)) {
  throw new Error('Usage: node scripts/build-market-index.mjs --check | --output PATH');
}

const catalog = await readFile(new URL('../market/catalog.json', import.meta.url), 'utf8');
const apps = parseMarketIndex(catalog);

if (mode === '--check') {
  console.log(`Validated ${apps.length} market entries`);
} else {
  const publishedApps = await Promise.all(apps.map(async ({ release_repo, upstream_version, ...app }) => {
    if (!release_repo) return app;

    const response = await fetch(`https://api.github.com/repos/${release_repo}/releases/latest`, {
      headers: { Accept: 'application/vnd.github+json', 'User-Agent': 'AppClear-market-index' },
      redirect: 'error',
      signal: AbortSignal.timeout(10_000),
    }).catch(error => { throw new Error(`Could not query ${release_repo}`, { cause: error }); });
    if (!response.ok) throw new Error(`Could not query ${release_repo} (HTTP ${response.status})`);
    const release = await response.json();
    if (release.draft !== false || release.prerelease !== false || typeof release.tag_name !== 'string') {
      throw new Error(`Invalid stable release for ${release_repo}`);
    }
    return { ...app, upstream_version: release.tag_name };
  }));

  const index = JSON.stringify({ schema_version: 1, apps: publishedApps }, null, 2) + '\n';
  parseMarketIndex(index);
  await writeFile(output, index);
  console.log(`Built market index with ${publishedApps.length} apps`);
}
