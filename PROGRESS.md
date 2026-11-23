# Progresso do Lume

Início registrado em 2026-11-23 15:21 (America/Sao_Paulo). Os marcos abaixo registram as verificações efetivamente executadas.

- M1 em implementação. Lexer e parser escritos, com spans, recuperação léxica e sincronização entre declarações. Verificação pendente da primeira compilação Rust.
- M2–M6 em implementação. Resolução, verificação bidirecional, IR estruturada, emissor WASM, tipos compostos, monomorfização, traits, closures e arena foram escritos na versão Rust. Validação e correções em andamento.
- M1–M6. `cargo build --release` passou. Primeiro módulo executado via Node com trait, monomorfização, closure, função de primeira classe, `Option?`, match, slice, `f64` e Unicode. As suítes ainda estão pendentes.
- M10 (primeiro ponto fixo). Bootstrap real em Node.js, sem front-end no host. `cmp` dos estágios 2 e 3 passou. SHA-256 inicial `6a998c949dd69b0ce3d63bdd7926a78785afde6bd7a845173ae525982e79544a`. Ainda falta medir a paridade da linguagem.
- M7. Biblioteca em Lume com `Vec`, `HashMap`, `String`, formatação, iteradores, quicksort e mergesort genéricos. Programa de fumaça compilado e executado com módulos importados.
- M8. Propagação/dobra, eliminação de código morto, inlining, CSE e expansão de primitivas de memória habilitadas na IR. Uma medição de 11 amostras por nível confirmou ganhos em fib e crivo. A ordenação foi 1,3% mais lenta nessa rodada.
- M9. 106/106 programas Rust em Node real, cada um em O0, O1 e O2, e 50/50 programas inválidos confirmados. Fuzz determinístico com 5.000 programas, zero divergências e saídas também conferidas com um oráculo inteiro. A suíte geral identificou inicialmente 94/106 programas aceitos pelo bootstrap.
- M10. O bootstrap em Lume ganhou tipos compostos, padrões exaustivos, fatias, lambdas com captura, funções de primeira classe, generics, traits e imports relativos. A mesma suíte completa passou 106/106 no estágio 3 e os bytes de estágio 2 e 3 coincidiram. O hash desta rodada foi `bb171d8b9a28cbe818e83906cada5b2a071c6ed52bf980b7a59d922b35e02acd`.
- Validação final em andamento. A primeira medição de fib tinha binários idênticos em O0 e O2. A causa foi corrigida com inlining recursivo limitado a uma expansão. A rodada seguinte verificou binários distintos e melhoria de 25,0% em fib e 13,9% no crivo.
- Conferência adicional do bootstrap. As 50 fontes inválidas são rejeitadas com diagnóstico e destaque, além da comparação de saída de todos os 106 programas. A validação WASM Rust agora verifica tipos da pilha nos corpos emitidos.
- Aceite final em 23/11/2026. Build release, 333 testes Cargo e Clippy estrito passaram. Os 106 programas tiveram a mesma saída esperada em Rust O0/O1/O2 e no estágio 3. Rust conferiu os 50 diagnósticos esperados. O estágio 3 rejeitou as 50 fontes. Fuzz de 5.000 programas sem divergências. Benchmark final com 15 amostras por nível registrou duas vitórias para O2.
- Ponto fixo final confirmado byte a byte com SHA-256 `2da2c332c6cf779a5fff6fee3eb078c47b5dfba76501739e83898d4aafb7ffd1`. A especificação, arquitetura, instruções de uso, relatório e evidências foram incluídos no projeto.
- Pacote ZIP conferido por extração. Os hashes, a suíte do estágio 3 e a compilação de `closure_chain.lume` voltaram a passar a partir da cópia extraída.
