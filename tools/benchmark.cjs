'use strict';
const { spawnSync } = require('node:child_process');
const { mkdtempSync, readFileSync, rmSync } = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { run } = require('./host.cjs');
const bin = process.env.LUMEC || path.join(process.env.CARGO_TARGET_DIR || 'target', 'release', 'lumec');
const directory = mkdtempSync(path.join(os.tmpdir(), 'lume-bench-'));
const cases = [['fib', '9227465\n'], ['sieve', '664579\n'], ['sort', '1074203578281370\n']];
const rounds = Number(process.env.LUME_BENCH_ROUNDS || 11);
if (!Number.isInteger(rounds) || rounds < 3) throw new Error('Use três rodadas ou mais.');
function median(values) {
  const sorted = [...values].sort((a,b) => a-b);
  const center = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[center] : (sorted[center - 1] + sorted[center]) / 2;
}
try {
  console.log(`Node ${process.version} | ${os.arch()} | ${os.cpus()[0].model} | ${rounds} amostras por nível`);
  let improved = 0;
  for (const [name, expected] of cases) {
    const modules = [0, 2].map((level) => {
      const wasm = path.join(directory, `${name}-${level}.wasm`);
      const proc = spawnSync(bin, ['build', `benchmarks/${name}.lume`, `-O${level}`, '-o', wasm], {encoding: 'utf8'});
      if (proc.status !== 0) throw new Error(proc.error || proc.stderr);
      return new WebAssembly.Module(readFileSync(wasm));
    });
    const samples = [[], []];
    function sample(i, timed) {
      const start = process.hrtime.bigint();
      const {stdout} = run(modules[i]);
      const elapsed = Number(process.hrtime.bigint() - start) / 1e6;
      if (stdout !== expected) throw new Error(`${name}: saída ${JSON.stringify(stdout)} vs ${JSON.stringify(expected)}`);
      if (timed) samples[i].push(elapsed);
    }
    for (let i = 0; i < 3; i++) { sample(0, false); sample(1, false); }
    for (let i = 0; i < rounds; i++) { sample(i % 2, true); sample(1 - i % 2, true); }
    const [base, opt] = samples.map(median);
    improved += opt < base;
    console.log(`${name}: O0 ${base.toFixed(2)} ms | O2 ${opt.toFixed(2)} ms | ${(base / opt).toFixed(3)}x | ${(100 * (base - opt) / base).toFixed(1)}%`);
    console.log(`  amostras O0 [${samples[0].map((x) => x.toFixed(2)).join(', ')}] O2 [${samples[1].map((x) => x.toFixed(2)).join(', ')}]`);
  }
  console.log(`O2 mais rápido em ${improved}/3`);
  if (improved < 2) process.exitCode = 1;
} finally { rmSync(directory, {recursive: true, force: true}); }
