import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const project = dirname(fileURLToPath(import.meta.url));
const repository = resolve(project, '../..');
const args = process.argv.slice(2);
if (args.length > 1 || (args.length === 1 && args[0] !== '--dry-run')) {
  throw new Error('Usage: npm run deploy -- [--dry-run]');
}
mkdirSync(join(repository, 'target'), { recursive: true });
const artifact = mkdtempSync(join(repository, 'target/docs-deployment-'));
const site = join(artifact, 'docs');
execFileSync('npm', ['run', 'build'], {
  cwd: project,
  stdio: 'inherit',
  env: { ...process.env, DOCS_SITE: 'https://parcela.davafons.com', DOCS_BASE: '/docs/', DOCS_OUTPUT: site },
});
copyFileSync(join(site, '404.html'), join(artifact, '404.html'));
writeFileSync(join(artifact, '_redirects'), '/ /docs/ 302\n');
console.log(`Deployment assets: ${artifact}`);
execFileSync(join(project, 'node_modules/.bin/wrangler'), [
  'deploy', '--config', join(project, 'wrangler.jsonc'), '--assets', artifact, '--strict', ...args,
], { cwd: project, stdio: 'inherit' });
