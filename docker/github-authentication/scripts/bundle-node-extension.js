#!/usr/bin/env node
const fs = require('fs');
const path = require('path');

const sourceDir = process.argv[2];
const outDir = process.argv[3];

if (!sourceDir || !outDir) {
  console.error('usage: bundle-node-extension.js <source-dir> <out-dir>');
  process.exit(1);
}

const esbuild = require(path.join(sourceDir, 'node_modules', 'esbuild'));

const packageJsonPath = path.join(sourceDir, 'package.json');
const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
const entryPoint = path.join(sourceDir, 'src', 'extension.ts');

if (!fs.existsSync(entryPoint)) {
  console.error(`missing entrypoint: ${entryPoint}`);
  process.exit(1);
}

fs.mkdirSync(path.join(outDir, 'dist'), { recursive: true });

esbuild.buildSync({
  entryPoints: [entryPoint],
  outfile: path.join(outDir, 'dist', 'extension.js'),
  bundle: true,
  platform: 'node',
  target: ['node18'],
  format: 'cjs',
  external: ['vscode', 'electron'],
  sourcemap: false,
  minify: false,
  logLevel: 'info',
});

packageJson.main = './dist/extension.js';
delete packageJson.browser;
delete packageJson.scripts;
delete packageJson.devDependencies;

fs.writeFileSync(path.join(outDir, 'package.json'), `${JSON.stringify(packageJson, null, 2)}\n`);

for (const name of ['README.md', 'package.nls.json']) {
  const source = path.join(sourceDir, name);
  if (fs.existsSync(source)) {
    fs.copyFileSync(source, path.join(outDir, name));
  }
}

for (const name of ['images', 'media']) {
  const source = path.join(sourceDir, name);
  if (fs.existsSync(source)) {
    fs.cpSync(source, path.join(outDir, name), { recursive: true });
  }
}
