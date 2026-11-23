# Lume

Lume é uma linguagem de programação estaticamente tipada. `lumec` compila fontes Lume para binários WebAssembly. O compilador inicial usa Rust e a biblioteca `std`. O compilador de bootstrap é escrito em Lume e recompila a própria fonte dentro de um runtime WebAssembly.

## Começar

Para reproduzir, use Rust 1.85 ou mais recente, Cargo, Node.js 20 ou mais recente e Bash. Para o relatório de verificação foram usados Rust 1.100.0 e Node.js 24.22.3 em Linux x86-64.

```sh
cargo build --release
./target/release/lumec run tests/programs/fibonacci.lume
./target/release/lumec build tests/programs/quicksort_generic.lume -O2 -o quicksort.wasm
node tools/host.cjs quicksort.wasm
```

`build` usa a extensão `.wasm` da entrada quando `-o` é omitido. `run` executa o módulo no Node.js. `check` percorre as fases de compilação e validação sem escrever saída. `emit-ir` mostra a IR após as otimizações escolhidas. `-O0` é o padrão. `-O1` habilita propagação, dobra, simplificação e eliminação de código morto. `-O2` acrescenta inlining, CSE e expansão de primitivas de memória.

```sh
./target/release/lumec check tests/programs/result_option.lume
./target/release/lumec emit-ir tests/programs/closure_chain.lume -O2
```

O WebAssembly usa a interface de host `lume`. `tools/host.cjs` implementa impressão, entrada e saída binária e leitura de imports para o compilador em Lume. O Node carrega binários WebAssembly diretamente. Um host alternativo precisa fornecer a mesma ABI, descrita no [DESIGN.md](DESIGN.md).

## Bootstrap

```sh
./tools/bootstrap.sh
node tools/suite.cjs
node tools/host.cjs build/stage3.wasm tests/programs/result_option.lume exemplo.wasm
node tools/host.cjs exemplo.wasm
```

1. `lumec` gera `build/stage1.wasm` a partir de `bootstrap/compiler.lume`.
2. `stage1.wasm` lê os bytes de `bootstrap/compiler.lume` e gera `build/stage2.wasm`.
3. `stage2.wasm` lê a mesma fonte e gera `build/stage3.wasm`.
4. `cmp` verifica a igualdade dos estágios 2 e 3. `sha256sum` registra o resultado.

O compilador em Lume usa emissão canônica. O nível `-O2` na primeira etapa otimiza o próprio compilador executado no estágio 1. As etapas seguintes executam o compilador em Lume para produzir seus binários.

## Verificação

```sh
./tools/verify.sh
```

A verificação compila o projeto, executa os testes do Cargo e o Clippy, repete o bootstrap, compara a suíte no estágio 3 e roda os benchmarks. `LUME_BENCH_ROUNDS` ajusta o número de amostras por nível. `cargo test --test fuzz -- --nocapture` imprime o resultado e a semente do fuzz.

| Caminho | Conteúdo |
| --- | --- |
| `src/syntax.rs` | Lexer com spans, diagnóstico, AST e parser Pratt |
| `src/lower.rs` | Resolução, tipos, especialização, closures e lowering |
| `src/ir.rs` | IR estruturada, passes de otimização e impressão |
| `src/wasm.rs` | Emissão WebAssembly e validação interna |
| `bootstrap/compiler.lume` | Compilador em Lume, incluindo resolução de módulos e emissão |
| `std/` | Coleções, strings, iteração e algoritmos de ordenação |
| `tests/programs/` e `tests/errors/` | Fontes e saídas esperadas |
| `benchmarks/` | Fibonacci recursivo, crivo e quicksort |

A descrição da linguagem está no [SPEC.md](SPEC.md). O [DESIGN.md](DESIGN.md) descreve os layouts e escolhas de implementação. Os resultados medidos e os limites observados estão no [RELATORIO.md](RELATORIO.md). O diário de marcos está no [PROGRESS.md](PROGRESS.md).
