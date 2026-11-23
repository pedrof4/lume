# Relatório de entrega do Lume

Verificação executada em 23 de novembro de 2026 no ambiente Linux x86-64, com Rust 1.100.0, Node.js 24.22.3 e CPU Intel Xeon Platinum 8573C.

## Critérios de aceite

| Critério | Resultado verificado |
| --- | --- |
| Compilação, testes e Clippy | `cargo build --release`, `cargo test` e `cargo clippy --all-targets -- -D warnings` concluídos com sucesso. 333 testes de Cargo passaram. |
| Programas ponta a ponta | 106/106 fontes executadas no Node e comparadas com snapshots. O compilador Rust passou em O0, O1 e O2. O compilador Lume do estágio 3 passou nas mesmas 106 fontes. |
| Diagnósticos | 50/50 programas inválidos foram rejeitados pelo Rust e corresponderam aos trechos de diagnóstico esperados. O estágio 3 rejeitou as mesmas 50 fontes com diagnóstico destacado. |
| Fuzz diferencial | 5.000 programas válidos, semente `0x8e3be2ab69c471af`, nenhuma divergência entre O0 e O2. Ambas as saídas foram comparadas a um oráculo independente do compilador. |
| Benchmarks | O2 venceu em dois dos três benchmarks na rodada final de 15 amostras por nível. |
| Bootstrap | `cmp` confirmou estágio 2 e estágio 3 idênticos. Ambos foram carregados e validados pelo Node.js. |
| Documentação | `SPEC.md`, `README.md`, `DESIGN.md`, `PROGRESS.md` e este relatório estão no projeto. |

O corpus inclui Fibonacci, crivo, n-rainhas, quicksort e mergesort genéricos, um interpretador aritmético, análise sintática de JSON com contagem de valores, árvore AVL com checagem de invariantes, hash map próprio e closures encadeadas. Casos específicos exercitam efeitos e short-circuit, preservação de traps, valores extremos, slices com alias, ordem de avaliação, propagação, monomorfização, validação WASM e imports.

## Ponto fixo

O executável Rust construiu `stage1.wasm` compilando `bootstrap/compiler.lume`. O host entregou a mesma fonte ao `stage1.wasm` e ao `stage2.wasm`. Cada estágio executou o lexer, parser, resolução, checagem de tipos, lowering e emissão em WebAssembly. O host forneceu bytes de entrada, chamadas de I/O e a VM.

SHA-256 comum de `build/stage2.wasm` e `build/stage3.wasm`.

```text
2da2c332c6cf779a5fff6fee3eb078c47b5dfba76501739e83898d4aafb7ffd1
```

O comando reprodutível é `./tools/bootstrap.sh`. Os hashes dos três módulos estão em [evidence/bootstrap.sha256](evidence/bootstrap.sha256).

## Desempenho

Medições em uma única execução de Node. Para cada programa, os módulos foram compilados antes do cronômetro, cada nível recebeu três aquecimentos, e as instâncias novas foram cronometradas em ordem alternada. A saída foi verificada em cada execução. Os valores são medianas de 15 amostras por nível e incluem a instanciação do WASM.

| Programa | -O0 | -O2 | Variação do tempo |
| --- | ---: | ---: | ---: |
| Fibonacci recursivo de 35 | 52,56 ms | 38,39 ms | −27,0% |
| Crivo até 10 milhões | 67,03 ms | 68,43 ms | +2,1% |
| Quicksort de 1 milhão de inteiros | 181,36 ms | 178,45 ms | −1,6% |

A primeira implementação emitia o mesmo módulo de Fibonacci nos dois níveis. Medi uma diferença entre execuções, conferi os binários e descartei esse resultado como evidência de otimização. O inlining recursivo limitado a um nível gerou binários distintos. Em outra rodada de 11 amostras após a correção, Fibonacci e crivo melhoraram, e quicksort ficou 1,2% mais lento. A melhora de Fibonacci apareceu nas duas rodadas. As diferenças menores em crivo e ordenação oscilaram entre rodadas e devem ser interpretadas com a variabilidade da máquina. Todas as amostras finais estão em [evidence/benchmarks.txt](evidence/benchmarks.txt).

## Falhas corrigidas

- Tipos genéricos do prelude podiam conflitar com tipos nominais do usuário. Dois testes de diagnóstico revelaram o erro. A resolução agora considera o parâmetro de tipo antes do nome global.
- O bootstrap inicialmente aceitou 94 das 106 fontes. A análise por programa identificou construções ausentes. A versão final implementa coleções, closures, `match`, especializações, traits e imports e foi testada contra o corpus completo.
- A saída esperada do teste integrado da biblioteca precisava incluir o elemento removido por `vec_pop`. O snapshot agora registra a sequência inteira de operações.

## Limitações observadas

- O WASM usa a ABI própria descrita no `DESIGN.md`. Node.js foi o runtime executado. O compilador em Lume usa emissão canônica e diagnósticos que encerram no primeiro erro. Os três níveis de otimização e a recuperação de erros são da CLI Rust.
- Há diferenças de borda entre as implementações. O bootstrap limita a entrada agregada a um milhão de bytes, usa espaços ASCII e limita a conversão de literais de ponto flutuante a uma fração decimal que cabe no seu acumulador. A propagação de contexto para um literal `i32` à esquerda de certos binários é mais restrita no bootstrap. A suíte comprova os casos que executa e a igualdade dos estágios, sem estabelecer equivalência para toda fonte possível.
- As duas implementações verificam os corpos genéricos quando são concretizados. Padrões têm um nível de desestruturação. A arena libera memória por instância, o que pode elevar o consumo em processos que alocam continuamente.
- O relatório de benchmark descreve a máquina e as rodadas medidas. Um ganho pequeno em um runtime com JIT pode mudar em outra máquina ou rodada.

## Marcos e tempo aproximado

Horários em America/Sao_Paulo. São janelas aproximadas de implementação e verificação, com etapas intercaladas. A duração total decorre do relógio e não da soma das durações abaixo.

| Marco | Janela aproximada | Duração |
| --- | --- | ---: |
| M1, lexer, spans e parser | 15:21–15:25 | 4 min |
| M2, módulos, tipos e diagnósticos | 15:25–15:32 | 7 min |
| M3, IR, CLI e WASM inicial | 15:25–15:32 | 7 min |
| M4, structs, enums e match | 15:27–15:32 | 5 min |
| M5, generics e traits | 15:27–15:32 | 5 min |
| M6, closures e arena | 15:27–15:32 | 5 min |
| M7, biblioteca padrão em Lume | 15:36–15:39 | 3 min |
| M8, otimização e ajuste de desempenho | 15:37–16:01 | 24 min |
| M9, corpus, fuzz e benchmarks | 15:39–16:07 | 28 min |
| M10, primeiro ponto fixo e paridade do corpus | 15:32–16:02 | 30 min |
| Documentação e empacotamento | 16:02–16:08 | 6 min |

O trabalho durou aproximadamente 47 minutos até o empacotamento. A trilha integral da rodada final está em [evidence/verification.txt](evidence/verification.txt).
