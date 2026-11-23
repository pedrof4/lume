use lumec::syntax::{self, Ex, Item, Source, St, TokenKind};
fn lex(text: &str) -> Vec<TokenKind> {
    let (tokens, errors) = syntax::lex(&Source::new("lexer.lume", text));
    assert!(errors.is_empty(), "{errors:?}");
    assert!(matches!(tokens.last().unwrap().kind, TokenKind::Eof));
    tokens
        .into_iter()
        .take_while(|t| t.kind != TokenKind::Eof)
        .map(|t| t.kind)
        .collect()
}
macro_rules! lex_cases {
    ($($id:ident: $text:literal => $($expected:expr),*;)+) => { $( #[test] fn $id() { assert_eq!(lex($text), vec![$($expected),*]); } )+ }
}
macro_rules! symbols {
    ($($id:ident: $op:literal,)+) => { lex_cases! { $($id: $op => TokenKind::Sym($op);)+ } }
}
symbols! {
    sym_arrow: "->", sym_fat_arrow: "=>", sym_eq: "==", sym_ne: "!=", sym_le: "<=", sym_ge: ">=",
    sym_and: "&&", sym_or: "||", sym_colons: "::", sym_range: "..", sym_lparen: "(", sym_rparen: ")",
    sym_lbrace: "{", sym_rbrace: "}", sym_lbracket: "[", sym_rbracket: "]", sym_comma: ",", sym_semicolon: ";",
    sym_colon: ":", sym_dot: ".", sym_try: "?", sym_assignment: "=", sym_plus: "+", sym_minus: "-",
    sym_mul: "*", sym_div: "/", sym_rem: "%", sym_lt: "<", sym_gt: ">", sym_bang: "!", sym_pipe: "|",
}
use TokenKind::{Char, Float, Int, Name, Str, Sym};
lex_cases! {
    empty: "" => ;
    line_comment: "// não importa ? @" => ;
    white_space: " \t\n\r\u{a0}" => ;
    line_comment_end: "x // y\nz" => Name("x".into()),Name("z".into());
    ident_unicode: "λβ17" => Name("λβ17".into());
    ident_underscore: "_x" => Name("_x".into());
    ident_numbers: "a123_b" => Name("a123_b".into());
    wildcard: "_" => Name("_".into());
    adjacent: "a12+34b" => Name("a12".into()),Sym("+"),Int(34),Name("b".into());
    basic_int: "120" => Int(120);
    grouped_int: "1_000_000" => Int(1_000_000);
    max_int: "9223372036854775807" => Int(i64::MAX);
    zero_int: "0000" => Int(0);
    negative_tokens: "-17" => Sym("-"),Int(17);
    fractional_float: "1.75" => Float(1.75);
    grouped_float: "1_000.0_25" => Float(1000.025);
    trailing_dot: "1." => Int(1),Sym(".");
    literal_range: "1..3" => Int(1),Sym(".."),Int(3);
    raw_empty_string: "\"\"" => Str("".into());
    utf8_string: "\"á λ 🌱\"" => Str("á λ 🌱".into());
    text_comment: "\"// comentário?\"" => Str("// comentário?".into());
    str_slash: "\"a\\\\b\"" => Str("a\\b".into());
    str_quote: "\"\\\"\"" => Str("\"".into());
    str_newline: "\"\\n\"" => Str("\n".into());
    str_tab: "\"\\t\"" => Str("\t".into());
    str_carriage: "\"\\r\"" => Str("\r".into());
    str_null: "\"\\0\"" => Str("\0".into());
    char_ascii: "'a'" => Char('a');
    char_unicode: "'λ'" => Char('λ');
    char_emoji: "'🌱'" => Char('🌱');
    char_quote: "'\\''" => Char('\'');
    char_tab: "'\\t'" => Char('\t');
    longest_match_arrow: ";->-" => Sym(";"),Sym("->"),Sym("-");
    longest_match_eq: "===" => Sym("=="),Sym("=");
    nested_angle: "A<B<C>>" => Name("A".into()),Sym("<"),Name("B".into()),Sym("<"),Name("C".into()),Sym(">"),Sym(">");
}
fn parsed(code: &str) -> Vec<Item> {
    syntax::parse(&Source::new("parser.lume", code)).unwrap_or_else(|d| panic!("{}", d[0]))
}
fn statement_shape(code: &str) -> String {
    let items = parsed(&format!("fn main() {{ {code} }}"));
    let Item::Function(f) = &items[0] else {
        panic!("fn")
    };
    format!("{:?}", f.body[0].kind)
}
macro_rules! statements {
    ($($id:ident: $code:literal => $prefix:literal;)+)=> {$(#[test] fn $id(){let got=statement_shape($code); assert!(got.starts_with($prefix),"{got}");})+};
}
statements! {
    let_inferred: "let n = 4;" => "Let(\"n\", false, None";
    let_typed: "let n: i64 = 4;" => "Let(\"n\", false, Some(Named(\"i64\"";
    let_mut: "let mut n = 4;" => "Let(\"n\", true, None";
    let_function_type: "let f: fn(i64)->bool = p;" => "Let(\"f\", false, Some(Fun(";
    let_nested_type: "let n: Option<Array<Vec<i64>>> = x;" => "Let(\"n\", false, Some(Named(\"Option\"";
    assign_local: "n = 4;" => "Set(Expr { kind: Name(\"n\")";
    assign_field: "n.x = 4;" => "Set(Expr { kind: Field(";
    assign_index: "n[x] = 4;" => "Set(Expr { kind: Index(";
    return_unit: "return;" => "Return(None)";
    return_value: "return 7;" => "Return(Some(Expr { kind: Int(7)";
    branch_no_else: "if true { return; }" => "If(Expr { kind: Bool(true)";
    branch_else: "if true {} else {}" => "If(Expr { kind: Bool(true)";
    branch_else_if: "if true {} else if false {}" => "If(Expr { kind: Bool(true)";
    empty_loop: "while true {}" => "While(Expr { kind: Bool(true)";
    loop_exit: "break;" => "Break";
    loop_next: "continue;" => "Continue";
    expr_named_call: "a();" => "Eval(Expr { kind: Call(Expr { kind: Name(\"a\")";
    expr_turbofish: "a::<i64>(3);" => "Eval(Expr { kind: Call(Expr { kind: Name(\"a\")";
    expr_chained_call: "a()(3);" => "Eval(Expr { kind: Call(Expr { kind: Call(";
    expr_empty_array: "[];" => "Eval(Expr { kind: Array([])";
    expr_nonempty_array: "[1,2,];" => "Eval(Expr { kind: Array([Expr { kind: Int(1)";
    expr_slice: "a[0..2];" => "Eval(Expr { kind: Slice(";
    expr_index: "a[0];" => "Eval(Expr { kind: Index(";
    expr_field: "a.x;" => "Eval(Expr { kind: Field(";
    expr_try: "a()?;" => "Eval(Expr { kind: Try(";
    expr_case: "match x { Some(p) => p, None => 0 };" => "Eval(Expr { kind: Match(";
    expr_closure: "|x:i64| -> bool {return x>0;};" => "Eval(Expr { kind: Closure([(";
    expr_closure_zero: "|| {};" => "Eval(Expr { kind: Closure([]";
    expr_char: "'λ';" => "Eval(Expr { kind: Char('λ')";
    expr_float: "5.125;" => "Eval(Expr { kind: Float(5.125)";
    expr_string: "\"hello\";" => "Eval(Expr { kind: Str(\"hello\")";
    binary_or: "a || b && c;" => "Eval(Expr { kind: Binary(\"||\"";
    binary_and: "a && b == c;" => "Eval(Expr { kind: Binary(\"&&\"";
    binary_equal: "a == b + c;" => "Eval(Expr { kind: Binary(\"==\"";
    binary_ord: "a < b + c;" => "Eval(Expr { kind: Binary(\"<\"";
    binary_mod: "a + b % c;" => "Eval(Expr { kind: Binary(\"+\"";
    binary_grouped: "(a + b) * c;" => "Eval(Expr { kind: Binary(\"*\"";
    unary_bool: "!p;" => "Eval(Expr { kind: Unary(\"!\"";
    unary_numeric: "-p;" => "Eval(Expr { kind: Unary(\"-\"";
}
macro_rules! items {
    ($($id:ident: $code:literal => $pattern:pat,)+)=> {$(#[test] fn $id(){assert!(matches!(parsed($code).as_slice(), [$pattern]));})+};
}
items! {
    item_import: "import \"a.lume\";" => Item::Import(..),
    item_fn: "fn f(a:i64,b:str)->bool {}" => Item::Function(..),
    item_fn_generic: "fn f<T:Order,U>(a:T,b:U)->T {}" => Item::Function(..),
    item_struct: "struct P<T> { a:T,b:T, }" => Item::Struct(..),
    item_enum: "enum E<T> { C, A(T), B(T,i64), }" => Item::Enum(..),
    item_trait: "trait A { fn f(s: Self) -> bool; fn g(s: Self); }" => Item::Trait(..),
    item_impl: "impl A for i64 { fn f(s: i64) -> bool { return true; } }" => Item::Impl(..),
}
fn expr_shape(code: &str) -> Ex {
    let items = parsed(&format!("fn f() {{ {code}; }}"));
    let Item::Function(f) = &items[0] else {
        panic!("fn")
    };
    let St::Eval(e) = &f.body[0].kind else {
        panic!("eval")
    };
    e.kind.clone()
}
#[test]
fn mul_binds_more_tightly_than_add() {
    let Ex::Binary("+", _, rhs) = expr_shape("2 + 3 * 4") else {
        panic!("+")
    };
    assert!(matches!(rhs.kind, Ex::Binary("*", ..)));
}
#[test]
fn subtraction_is_left_associative() {
    let Ex::Binary("-", lhs, rhs) = expr_shape("8 - 3 - 2") else {
        panic!("-")
    };
    assert!(matches!(lhs.kind, Ex::Binary("-", ..)));
    assert!(matches!(rhs.kind, Ex::Int(2)));
}
#[test]
fn call_binds_more_tightly_than_negation() {
    let Ex::Unary("-", inner) = expr_shape("-f(4)") else {
        panic!("-")
    };
    assert!(matches!(inner.kind, Ex::Call(..)));
}
#[test]
fn index_binds_more_tightly_than_negation() {
    let Ex::Unary("-", inner) = expr_shape("-a[0]") else {
        panic!("-")
    };
    assert!(matches!(inner.kind, Ex::Index(..)));
}
#[test]
fn lexer_recovers_after_two_errors() {
    let src = Source::new("lexer.lume", "@ λ # x");
    let (tokens, errors) = syntax::lex(&src);
    assert_eq!(errors.len(), 2);
    assert_eq!(tokens[0].kind, Name("λ".into()));
    assert_eq!(tokens[1].kind, Name("x".into()));
    assert_eq!((tokens[0].span.start, tokens[0].span.end), (2, 4));
    assert!(errors[1].to_string().contains("lexer.lume:1:5"));
}
#[test]
fn parser_recovers_at_next_declaration() {
    let src = Source::new(
        "parser.lume",
        "fn x() { let a = ; }\nfn y() { let b = ; }\nfn main() {}\n",
    );
    let errors = syntax::parse(&src).unwrap_err();
    assert_eq!(errors.len(), 2);
    assert!(errors[0].to_string().contains("parser.lume:1:"));
    assert!(errors[1].to_string().contains("parser.lume:2:"));
}
