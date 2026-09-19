import { defineConfig } from 'vite';
import { execSync } from 'node:child_process';

// Every build names itself, inside the bundle and in dist/version.json. A running view compares the
// two to learn that a newer UI has been put in place, instead of a person guessing when to reload.
const git = cmd => { try { return execSync('git ' + cmd, { stdio: ['ignore', 'pipe', 'ignore'] }).toString().trim(); } catch { return ''; } };
const builtAt = new Date().toISOString().replace(/\.\d+Z$/, 'Z');
const commit = git('rev-parse --short=8 HEAD') || 'nogit';
const dirty = git('status --porcelain -- .') ? '+local' : '';
const version = { schema: 'dot.ui-version.v1', build: `${commit}${dirty}.${builtAt.replace(/[-:]/g, '').slice(0, 13)}`, commit, builtAt };

export default defineConfig({
  build: { target: 'es2022' },
  define: { __DOT_UI_VERSION__: JSON.stringify(version) },
  plugins: [{ name: 'dot-ui-version', generateBundle() { this.emitFile({ type: 'asset', fileName: 'version.json', source: JSON.stringify(version) + '\n' }); } }],
});
