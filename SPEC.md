# Especificação do Lume 0.1

## Arquivos e léxico

Um programa tem um arquivo raiz UTF-8. `import "caminho.lume";` inclui declarações de outro arquivo. O caminho é relativo ao arquivo que contém o import. Caminhos canônicos repetidos são carregados uma vez. Um ciclo é erro. Os arquivos formam um espaço de nomes global para funções, construtores, tipos e métodos. A busca local precede a busca global em expressões. O ponto de entrada é `fn main()` ou `fn main() -> i64`, sem parâmetros ou generics. O host recebe seu resultado.

Nomes começam com letra ou `_` e podem continuar com letras, números ou `_`. Palavras usadas como sintaxe incluem `fn`, `struct`, `enum`, `trait`, `impl`, `for`, `import`, `let`, `mut`, `if`, `else`, `while`, `match`, `break`, `continue`, `return`, `true` e `false`. O lexer Rust separa os espaços Unicode e reconhece letras Unicode. O bootstrap faz a varredura por bytes e usa espaços ASCII. `//` inicia um comentário até fim de linha. Tokens de operadores são reconhecidos pelo maior comprimento possível. Use um separador entre `>` e `=` quando ambos são tokens distintos.

Literais inteiros são decimais com `_` opcional entre ou após dígitos. O valor positivo máximo é `9223372036854775807`. Literais fracionários têm parte inteira, ponto e ao menos um dígito de fração. `str` é delimitado por `"` e `char` por `'`. Os escapes são `\\`, `\"`, `\'`, `\n`, `\r`, `\t` e `\0`. Um `char` contém um escalar Unicode. Uma `str` de superfície contém UTF-8 e sua medida e indexação são em bytes.

## Gramática

EBNF. `{x}` é repetição, `[x]` é opcional, `list(x)` é uma lista separada por vírgulas com vírgula final opcional. Em regras de tokens literais, os delimitadores estão entre aspas.

```ebnf
program       = { declaration } ;
declaration   = "import" STRING ";"
              | "fn" IDENT generics parameters [ "->" type ] block
              | "struct" IDENT generics "{" list(parameter) "}"
              | "enum" IDENT generics "{" list(variant) "}"
              | "trait" IDENT "{" { method_signature } "}"
              | "impl" IDENT "for" type "{" { method } "}" ;
generics      = [ "<" list(IDENT [ ":" IDENT ]) ">" ] ;
parameters    = "(" list(parameter) ")" ;
parameter     = IDENT ":" type ;
variant       = IDENT [ "(" list(type) ")" ] ;
method_signature = "fn" IDENT parameters [ "->" type ] ";" ;
method        = "fn" IDENT parameters [ "->" type ] block ;
type          = IDENT [ "<" list(type) ">" ]
              | "fn" "(" list(type) ")" "->" type ;
block         = "{" { statement } "}" ;
statement     = "let" ["mut"] IDENT [ ":" type ] "=" expression ";"
              | expression [ "=" expression ] ";"
              | "return" [expression] ";"
              | "if" expression block [ "else" (block | if_statement) ]
              | "while" expression block
              | ("break" | "continue") ";" ;
expression    = atom { postfix | binary_operator expression } ;
atom          = INTEGER | FLOAT | CHAR | STRING | "true" | "false" | IDENT
              | "(" expression ")" | "[" list(expression) "]"
              | ("!" | "-") expression
              | ("|" list(parameter) "|" | "||") [ "->" type ] block
              | "match" expression "{" list(pattern "=>" expression) "}" ;
postfix       = [ "::" "<" list(type) ">" ] "(" list(expression) ")"
              | "[" expression [ ".." expression ] "]"
              | "." IDENT | "?" ;
pattern       = "_" | INTEGER | "true" | "false"
              | IDENT [ "(" list(IDENT) ")" ] ;
```

`if_statement` é a produção de `statement` iniciada por `if`. Pós-fixos têm a maior precedência, seguidos por `!` e `-` prefixos. As precedências binárias, em ordem decrescente, são `* / %`, `+ -`, `< <= > >=`, `== !=`, `&&`, `||`. Operadores binários associam à esquerda. Condições e operadores lógicos exigem `bool`. `&&` e `||` avaliam a direita apenas quando o resultado ainda depende dela.

## Tipos e avaliação

| Tipo | Valores |
| --- | --- |
| `i32`, `i64` | Inteiros com sinal, em complemento de dois |
| `f64` | Número IEEE 754 de precisão dupla |
| `bool` | `false` ou `true` |
| `char` | Escalar Unicode |
| `str` | Sequência UTF-8 imutável |
| `unit` | Resultado sem valor útil |
| `Array<T>` | Sequência de tamanho fixo e elementos mutáveis |
| `Slice<T>` | Vista de comprimento fixo sobre um `Array<T>` ou outro `Slice<T>` |
| `fn(A, B) -> R` | Valor invocável com os tipos de parâmetros e retorno indicados |

Um literal inteiro tem tipo `i64` na ausência de contexto. Uma anotação `i32`, um parâmetro, ou um operando com tipo conhecido pode determinar `i32`. Não há coerção geral entre tipos numéricos. `as_i32`, `as_i64` e `as_f64` convertem explicitamente os tipos numéricos. A redução para `i32` retém 32 bits. A conversão de `f64` para inteiro primeiro trunca em direção a zero como `i64` e causa trap quando a primeira conversão excede o intervalo ou recebe NaN. `codepoint(c)` retorna um `i64`.

Adição, subtração, multiplicação e negação inteiras são modulares. A divisão inteira trunca em direção a zero. Divisão e resto por zero causam trap. A divisão `i64::MIN / -1` causa trap pela regra de WASM. Para `i32`, a operação intermediária usa `i64` e é estreitada ao final. `%` não se aplica a `f64`. Números e caracteres permitem comparação ordinal. `bool` permite `==` e `!=`. `str` usa igualdade byte a byte. Comparações `f64` seguem a ordem parcial IEEE 754.

`let` infere o tipo do inicializador local. Uma anotação impõe o tipo esperado. Funções declaram tipos de parâmetro e de retorno, com `unit` implícito. Uma função de retorno diferente de `unit` exige retorno explícito em todos os caminhos reconhecidos pela análise estruturada. Um `while` pode executar zero vezes para essa análise. `break` e `continue` apontam para o laço mais interno. Argumentos e operandos binários estritos são avaliados da esquerda para a direita, uma vez. Um bloco cria escopo léxico. Atribuições a um nome exigem `let mut`. Escrita de campo ou elemento usa a mutabilidade do objeto referenciado.

```lume
fn main() {
    let mut n = 2;
    let transform = |x: i64| -> i64 { return x * n; };
    n = 10;
    print(transform(7)); // 14
}
```

Closures capturam por valor ao serem criadas. Um objeto capturado é um ponteiro compartilhado. O vínculo capturado é imutável no corpo da closure. Os tipos estruturados e coleções usam identidade de referência e, fora de `str`, não têm operador de igualdade implícito.

## Structs, enums e controle de erro

Um `struct` define um tipo nominal e um construtor posicional cujo argumento segue a ordem dos campos declarados. `p.campo` lê o campo. A atribuição em `p.campo` atualiza o objeto compartilhado por todos os aliases. Um `enum` define tipos soma nominais. Uma variante pode carregar vários valores. Uma variante de aridade zero pode ser escrita com ou sem parênteses.

`match` avalia o sujeito uma vez e retorna o valor do primeiro braço compatível. Tipos de resultados dos braços devem coincidir. Enums precisam cobrir todas as variantes, e `bool` precisa cobrir os dois valores. Um match inteiro exige `_`. O `_` final cobre todo o restante. Padrões de variantes vinculam campos diretamente ou ignoram um campo com `_`. Braços textualmente após um `_` e padrões repetidos são erros.

`Option<T>` contém `None` e `Some(T)`. `Result<T,E>` contém `Ok(T)` e `Err(E)`. `x?` extrai o valor de `Some` ou `Ok`. Em `None` ou `Err`, a função retorna imediatamente com o erro. O tipo de retorno usa o mesmo contêiner. Em `Result`, o tipo de erro precisa coincidir. O tipo de sucesso da função externa pode diferir.

```lume
enum Op { Add(i64, i64), Unknown }
fn apply(op: Op) -> Result<i64,str> {
    return match op { Add(x,y) => Ok(x + y), Unknown => Err("operação inválida") };
}
fn twice(op: Op) -> Result<i64,str> { return Ok(apply(op)? * 2); }
```

## Generics, traits e funções

`T` em `fn f<T>(x: T) -> T` é um parâmetro de tipo. Structs e enums também podem receber parâmetros de tipo. Cada chamada resolve argumentos por unificação estrutural com o contexto de retorno e os argumentos conhecidos. `f::<i64>(x)` pode fornecer tipos explicitamente. Um parâmetro sem evidência suficiente gera diagnóstico. Cada combinação de função e tipos concretos produz uma especialização. Implementações duplicadas, assinaturas incompatíveis e restrições insatisfeitas são erros.

`trait Order { fn less(a: Self, b: Self) -> bool; }` descreve a interface. `impl Order for i64 { ... }` deve fornecer os métodos declarados. O primeiro argumento de método é `Self`. `T:Order` obriga a existência de implementação para o tipo concreto. `less(x, y)` é resolvido pelo nome global do método e pelo tipo estático do primeiro argumento. A chamada emitida tem destino direto. Não há objetos de trait ou despacho dinâmico de trait. Funções não genéricas podem ser usadas como valores sem uma chamada.

## Arrays, slices e memória

`[x, y]` cria um `Array<T>`. O tipo de um array vazio vem da anotação; `array(n, v)` avalia `v` uma vez e inicializa todas as posições com o mesmo valor. `len(x)` aceita arrays, slices e `str`. `a[i]` verifica `0 <= i < len(a)`, com índice `i64`. `a[i..j]` cria uma vista semiaberta e verifica `0 <= i <= j <= len(a)`. Um índice ou intervalo inválido causa trap. Escrever em um slice modifica o array original.

A arena cresce em memória linear WASM. Objetos vivem até o fim da instância. Cada alocação está alinhada a oito bytes. `alloc`, `load`, `store`, `load8`, `store8` e `str_from_raw` são primitivas de baixo nível para runtime, compilador e bibliotecas. Seus chamadores são responsáveis pelo layout, limites dos objetos e UTF-8. A máquina WASM impõe os limites da memória linear.

## Intrínsecos de host

`print(i64)` e `print_float(f64)` imprimem com quebra de linha. `write(str)` e `print_char(char)` imprimem sem acrescentá-la. `input_len()` e `input_byte(i64)` leem a entrada binária, com `-1` fora do intervalo. `output_byte(i64)` entrega os oito bits inferiores ao host. `abort(i64)` encerra a execução com um erro. `file_open(i64,str)` e `file_byte(i64,i64)` são a interface de arquivos do bootstrap e dependem da configuração do host.

O [RELATORIO.md](RELATORIO.md) registra diferenças nas bordas do léxico e da inferência entre as duas implementações.
