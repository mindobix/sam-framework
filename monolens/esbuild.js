const esbuild = require('esbuild');

const watch = process.argv.includes('--watch');

const ctx = esbuild.context({
  entryPoints: ['src/extension.ts'],
  bundle: true,
  outfile: 'dist/extension.js',
  external: ['vscode'],          // VS Code API is provided by the runtime
  format: 'cjs',
  platform: 'node',
  target: 'node18',
  sourcemap: true,
  minify: false,
  logLevel: 'info',
});

ctx.then(async (c) => {
  if (watch) {
    await c.watch();
    console.log('esbuild: watching for changes...');
  } else {
    await c.rebuild();
    await c.dispose();
  }
}).catch(() => process.exit(1));
