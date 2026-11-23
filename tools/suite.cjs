'use strict';
const fs = require('node:fs');
const path = require('node:path');
const {run} = require('./host.cjs');
const stage = new WebAssembly.Module(fs.readFileSync(process.argv[2] || 'build/stage3.wasm'));
const dir = path.resolve('tests/programs');
const sources = fs.readdirSync(dir).filter((name) => name.endsWith('.lume')).sort();
let count = 0;
for (const file of sources) {
  const source = fs.readFileSync(path.join(dir, file));
  const expected = fs.readFileSync(path.join(dir, file.replace(/\.lume$/, '.stdout')), 'utf8');
  try {
    const {output} = run(stage, source, {filename: path.join(dir, file)});
    const {stdout} = run(output);
    if (stdout !== expected) throw new Error(`saída ${JSON.stringify(stdout)}; esperado ${JSON.stringify(expected)}`);
    count++;
  } catch (error) {
    console.error(`${file}: ${error.message.trim().replace(/\n/g, ' / ')}`);
  }
}
console.log(`Bootstrap: ${count}/${sources.length} programas com a saída esperada`);
if (count !== sources.length) process.exitCode = 1;

const errorsDir = path.resolve('tests/errors');
const invalid = fs.readdirSync(errorsDir).filter((name) => name.endsWith('.lume')).sort();
let rejected = 0;
for (const name of invalid) {
  const filename = path.join(errorsDir, name);
  try {
    run(stage, fs.readFileSync(filename), {filename});
    console.error(`${name}: fonte inválida aceita`);
  } catch (error) {
    if (/:[0-9]+:[0-9]+: erro:.*\n[^]*\^/.test(error.message)) rejected++;
    else console.error(`${name}: ${error.message}`);
  }
}
console.log(`Bootstrap: ${rejected}/${invalid.length} fontes inválidas rejeitadas com diagnóstico de origem`);
if (rejected !== invalid.length) process.exitCode = 1;
