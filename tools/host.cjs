'use strict';
const fs = require('node:fs');
const path = require('node:path');
const { TextDecoder } = require('node:util');

function run(module, input = Buffer.alloc(0), {capture = true, filename} = {}) {
  let instance;
  const stdout = [];
  const output = [];
  const decoder = new TextDecoder('utf-8', {fatal: true});
  const entry = filename && fs.realpathSync(filename);
  const files = entry ? [{name: entry, bytes: input}] : [];
  const ids = new Map(entry ? [[entry, 0]] : []);
  const textAt = (p) => {
    const address = Number(p);
    const memory = instance.exports.memory.buffer;
    const length = Number(new DataView(memory).getBigUint64(address, true));
    return decoder.decode(new Uint8Array(memory, address + 8, length));
  };
  const print = (text) => {
    if (capture) stdout.push(text);
    else process.stdout.write(text);
    return 0n;
  };
  const lume = {
    print: (n) => print(`${n}\n`),
    print_float: (n) => {
      const view = new DataView(new ArrayBuffer(8));
      view.setBigInt64(0, n, true);
      return print(`${view.getFloat64(0, true)}\n`);
    },
    print_char: (n) => print(String.fromCodePoint(Number(n))),
    write: (n) => print(textAt(n)),
    file_open: (base, relative) => {
      const from = files[Number(base)];
      if (!from) return -1n;
      try {
        const name = fs.realpathSync(path.resolve(path.dirname(from.name), textAt(relative)));
        if (!ids.has(name)) {
          const bytes = fs.readFileSync(name);
          ids.set(name, files.length);
          files.push({name, bytes});
        }
        return BigInt(ids.get(name));
      } catch { return -1n; }
    },
    file_byte: (index, at) => {
      const file = files[Number(index)];
      if (!file) return -1n;
      if (at === -1n) return BigInt(file.bytes.length);
      if (at < 0n || at >= file.bytes.length) return -1n;
      return BigInt(file.bytes[Number(at)]);
    },
    input_len: () => BigInt(input.length),
    input_byte: (n) => n < 0n || n >= input.length ? -1n : BigInt(input[Number(n)]),
    output_byte: (n) => { output.push(Number(n & 255n)); return 0n; },
    abort: (n) => { throw new Error(`Lume abortou (${n})${stdout.length ? `: ${stdout.join('')}` : ''}`); },
  };
  instance = new WebAssembly.Instance(module instanceof WebAssembly.Module ? module : new WebAssembly.Module(module), {lume});
  const result = instance.exports.main();
  return {result, stdout: stdout.join(''), output: Buffer.from(output)};
}

if (require.main === module) {
  try {
    const [wasm, source, destination] = process.argv.slice(2);
    if (!wasm || (source === undefined) !== (destination === undefined)) throw new Error('uso: node tools/host.cjs programa.wasm [entrada.lume saída.wasm]');
    const {output} = run(fs.readFileSync(wasm), source ? fs.readFileSync(source) : undefined, {capture: false, filename: source});
    if (destination) {
      if (!WebAssembly.validate(output)) throw new Error('o compilador WASM produziu bytes inválidos');
      fs.writeFileSync(destination, output);
    }
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
module.exports = {run};
