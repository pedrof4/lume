'use strict';
const fs = require('node:fs');
const {run} = require('./host.cjs');
const [file0, file1, file2, expected] = process.argv.slice(2);
const reference = fs.readFileSync(expected, 'utf8');
for (const file of [file0, file1, file2]) {
  const actual = run(fs.readFileSync(file)).stdout;
  if (actual !== reference) throw new Error(`${file}: esperado ${JSON.stringify(reference)}, recebido ${JSON.stringify(actual)}`);
}
