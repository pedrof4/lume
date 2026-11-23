# Arquitetura e decisões

## Dois compiladores

O compilador Rust usa uma AST com spans. Um carregador visita a árvore de imports, detecta ciclos e inclui cada caminho canônico uma vez. A fase de lowering coleta assinaturas e dados nominais antes de percorrer corpos. Contextos léxicos armazenam o tipo e a mutabilidade de cada local. A expressão recebe um tipo esperado quando o contexto fornece um. A monomorfização mantém uma fila de especializações e reserva o índice da função antes de baixar seu corpo, inclusive para recursão. Os diagnósticos no Rust incluem caminho, linha, coluna, trecho, destaque e sugestões quando há evidência local de correção.

O compilador em Lume registra unidades de fonte, lexifica com intervalos, pré-declara tipos e traits, coleta assinaturas, resolve tipos e baixa corpos em uma fita de instruções tipadas pelo contexto. Os números negativos representam parâmetros de tipos em assinaturas genéricas. A resolução produz tipos concretos internados e agenda suas especializações. As closures agendam corpos elevados com os locais capturados. A fita distingue opcodes de operandos LEB128 e permite reservar e preencher comprimentos antes da emissão. O host não contém lexer, parser, regras de tipos, transformação de IR ou lógica de geração de código.

A duplicação permite que cada estágio tenha uma implementação independente. A mesma suíte de saída cobre os dois. Os números de funções, a ordem dos imports resolvidos, a tabela de closures e a posição das strings são determinados por travessia da fonte. A igualdade do estágio 2 e 3 é um ponto fixo do gerador em Lume sob o mesmo texto de entrada e a mesma ABI. O compilador Rust usa sua própria IR e sua própria ordem de emissão.

## IR e otimizações Rust

A IR é um IR estruturado de locais numerados, valores, blocos, condições e laços. Um local pode ser atualizado; a representação preserva o fluxo da fonte e serve de entrada para os passes e para a seleção de instruções WASM. Os rótulos de `break` e `continue` são calculados no lowering de blocos. Comparações, casts, operações `f64`, chamadas diretas e indiretas e operações de memória aparecem explicitamente.

- `-O0` emite a IR tal como produzida pelo lowering.
- `-O1` propaga constantes entre atribuições no mesmo bloco, dobra operações, simplifica identidades, elimina partes inalcançáveis e remove atribuições puras a locais sem leitura.
- `-O2` acrescenta CSE local, inlining de funções pequenas e expansão das primitivas de leitura e escrita da arena. Uma forma restrita de função recursiva é expandida por um nível. Chamadas introduzidas pela expansão mantêm o destino original.

Os passes invalidam fatos quando um controle ou uma escrita pode alterá-los. Uma expressão que pode causar trap, acessar memória ou produzir efeitos não é tratada como eliminável. O CSE preserva a quantidade e a ordem de acessos à memória. O inlining exige argumentos pequenos, sem efeitos ou traps, quando a substituição pode duplicá-los ou descartá-los. A emissão para a fita em Lume é canônica. A suíte diferencial de otimizações exercita a IR Rust.

## ABI e representação WASM

Cada parâmetro, local e resultado ocupa um `i64` WASM. `i32` é normalizado por extensão de sinal. `bool` é 0 ou 1, `char` é seu valor escalar e `f64` usa seus bits IEEE 754 em um `i64` na passagem de parâmetros e resultados. Aritmética flutuante reinterpreta os bits e usa as instruções nativas `f64`. Ponteiros são endereços na memória linear do módulo, convertidos para `i32` em acesso. Campos e elementos são palavras de oito bytes.

| Objeto | Palavras iniciais e payload |
| --- | --- |
| Struct | Campos em ordem de declaração |
| Enum | Tag a partir de zero, seguida dos campos da variante |
| Array | Comprimento, seguido dos elementos |
| Slice | Comprimento e endereço do primeiro elemento |
| `str` | Comprimento em bytes, seguido de UTF-8 |
| Closure | Índice na tabela de funções, seguido dos valores capturados |

Cada função elevada de closure recebe o próprio endereço de ambiente como primeiro argumento. Funções nomeadas usadas como valores recebem um wrapper equivalente. As chamadas por valor usam `call_indirect` com o tipo da aridade esperada. Traits e generics são resolvidos de forma estática e emitem `call` direto.

O módulo exporta `main` e `memory`. Ele importa funções do módulo `lume` com os parâmetros e resultado abaixo. Na fronteira JavaScript, parâmetros e retorno são `BigInt`.

| Import | Parâmetros | Resultado ABI |
| --- | --- | --- |
| `print` | Inteiro | 0 |
| `write` | Endereço de `str` | 0 |
| `input_len` | Nenhum | Comprimento |
| `input_byte` | Índice | Byte ou -1 |
| `output_byte` | Palavra | 0 |
| `abort` | Código | Interrompe |
| `print_float` | Bits IEEE 754 | 0 |
| `print_char` | Escalar Unicode | 0 |
| `file_open` | ID do arquivo base e endereço do caminho | ID estável ou -1 |
| `file_byte` | ID e índice | Byte ou -1. Índice -1 solicita o tamanho |

O host Node habilita a resolução de arquivos quando recebe um caminho de entrada de compilação. Ele resolve caminhos canônicos e fornece bytes. O compilador em Lume decide quando importar, controla a ordenação e detecta ciclos. A execução comum de `lumec run` recebe entrada binária vazia.

`type`, `import`, `function`, `table`, `memory`, `global`, `export`, `element`, `code` e `data` são emitidas diretamente. As seções e os corpos têm seus tamanhos codificados em LEB128. O Rust valida índices e aridades na IR, estrutura da saída, contagens, tamanhos e os tipos da pilha e do controle de cada corpo segundo os opcodes emitidos. A implementação em Lume valida o cabeçalho, a ordem e a moldura das seções. A integração valida o módulo completo com a VM do Node.js.

## Arena e biblioteca

O bump pointer é um global WASM. Cada alocação arredonda o fim para oito bytes, verifica o limite de 2 GiB e solicita páginas com `memory.grow` quando necessário. A memória da instância inteira é liberada pelo host ao fim da execução. Estruturas não precisam de cabeçalhos de coleção ou contagem de referências. A contrapartida é o acúmulo de alocações em funções de longa duração. Crescimento de um `Vec`, `HashMap` ou `String` copia para uma área maior e mantém a anterior até a instância terminar. Um slice de um `Vec` continua apontando ao armazenamento no qual foi criado.

A biblioteca padrão é código Lume. `Vec` cresce geometricamente. `HashMap` usa endereçamento aberto com sondagem linear, redimensionamento e remoção com reparo do agrupamento. `String` é um construtor mutável de bytes com conversão a `str`. Iteradores armazenam fonte e cursor. `map` e `filter` materializam um `Vec` e `fold` acumula sem criar coleção intermediária. Quicksort usa uma partição em três regiões e recorre sobre a região menor. Mergesort usa um buffer, mantém a ordem de chaves iguais e evita unir metades já ordenadas. `Option` e `Result` vêm de um prelude de tipos definido em sintaxe Lume.

## Limites de engenharia

O pacote usa a ABI própria de Node.js, disponível sem instalar um SDK WASI. O código implementa e valida o subconjunto WebAssembly que emite. A inferência é local. A linguagem trabalha com padrões de um nível, imports sem namespaces explícitos e closures com parâmetros anotados. A checagem de corpos genéricos ocorre por especialização. Comparadores usados nos sorts precisam representar uma ordem adequada para o domínio, especialmente em presença de NaN.
