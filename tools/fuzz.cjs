'use strict';
const { readFileSync } = require('node:fs');
const { run } = require('./host.cjs');
const bytes = readFileSync(process.argv[2]);
let pos = 0;
let count = 0;
function item() {
  const len = bytes.readUInt32LE(pos);
  pos += 4;
  if (pos + len > bytes.length) throw new Error('corpus truncado');
  const result = bytes.subarray(pos, pos + len);
  pos += len;
  return result;
}
while (pos < bytes.length) {
  const source = item().toString('utf8');
  const oracle = item().toString('utf8');
  const O0 = item();
  const O2 = item();
  try {
    const first = run(O0).stdout;
    const second = run(O2).stdout;
    if (first !== second || first !== oracle) {
      throw new Error(`O0 ${JSON.stringify(first)}, O2 ${JSON.stringify(second)}, oráculo ${JSON.stringify(oracle)}`);
    }
  } catch (error) {
    throw new Error(`caso ${count}\n${source}\n${error.message}`);
  }
  count++;
}
console.log(`${count} casos, 0 divergências`);
