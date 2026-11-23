use lumec::ir::{Expr, Op, Stmt, Un};

fn optimized(level: u8, code: &str) -> lumec::Compilation {
    lumec::compile_text("ir.lume", code, level).unwrap_or_else(|ds| panic!("{}", ds[0]))
}
fn expression(level: u8, text: &str) -> Expr {
    let c = optimized(level, &format!("fn main() -> i64 {{ return {text}; }}"));
    let [Stmt::Return(e)] = c.ir.functions[0].body.as_slice() else {
        panic!("{:?}", c.ir.functions[0].body)
    };
    e.clone()
}
macro_rules! folds {
    ($($id:ident: $text:literal => $value:expr;)+)=> {$(#[test] fn $id(){assert_eq!(expression(1,$text),Expr::Const($value));})+};
}
folds! {
    fold_add: "1+2" => 3;
    fold_sub: "10-7-8" => -5;
    fold_mul: "-2*7" => -14;
    fold_div: "-13/5" => -2;
    fold_rem: "-13%5" => -3;
    fold_negs: "-(-8)" => 8;
    fold_nested: "2*(7-4)+11/3" => 9;
    fold_overflow_add: "9223372036854775807+1" => i64::MIN;
    fold_overflow_sub: "(-9223372036854775807-1)-1" => i64::MAX;
    fold_overflow_mul: "9223372036854775807*2" => -2;
    fold_negative_i32: "as_i64(as_i32(4294967295))" => -1;
    fold_i32_overflow: "as_i64(as_i32(2147483647)+1)" => i64::from(i32::MIN);
    fold_min_remainder: "(-9223372036854775807-1) % -1" => 0;
}
macro_rules! comparisons {
    ($($id:ident: $text:literal => $value:expr;)+)=> {$(#[test] fn $id(){
        let c=optimized(2,concat!("fn probe() -> bool { return ",$text,"; } fn main() {}"));
        assert_eq!(c.ir.functions[0].body,vec![Stmt::Return(Expr::Const($value))]);
    })+};
}
comparisons! {
    cmp_eq: "4==4" => 1; cmp_ne: "4!=4" => 0; cmp_lt: "4<5" => 1;
    cmp_gt: "-4>5" => 0; cmp_le: "4<=4" => 1; cmp_ge: "-4>=-4" => 1;
    cmp_float_eq: "-0.0 == 0.0" => 1; cmp_float_nan: "(0.0/0.0) == (0.0/0.0)" => 0;
    cmp_float_gt: "0.5 > 0.375" => 1; cmp_bool: "false || true && !false" => 1;
}
#[test]
fn o0_preserves_expression() {
    assert!(matches!(expression(0, "1+2*3"), Expr::Bin(Op::Add, ..)));
}
#[test]
fn division_by_zero_is_preserved() {
    assert!(matches!(expression(2, "9/0"), Expr::Bin(Op::Div, ..)));
}
#[test]
fn remainder_by_zero_is_preserved() {
    assert!(matches!(expression(2, "9%0"), Expr::Bin(Op::Rem, ..)));
}
#[test]
fn trapping_conversion_is_preserved() {
    assert!(matches!(
        expression(2, "as_i64(0.0/0.0)"),
        Expr::Unary(Un::FToI, ..)
    ));
}
#[test]
fn overflowing_division_is_preserved() {
    assert!(matches!(
        expression(2, "(-9223372036854775807-1)/-1"),
        Expr::Bin(Op::Div, ..)
    ));
}
#[test]
fn increments_fold_only_before_mutation() {
    let c = optimized(2, "fn main() -> i64 { let mut a=2; a=a+1; return a; }");
    assert_eq!(c.ir.functions[0].body, vec![Stmt::Return(Expr::Const(3))]);
}
#[test]
fn pure_calls_inline_in_o2() {
    let src = "fn f(n:i64) -> i64 {return n+4;} fn main() -> i64 {return f(8);}";
    assert!(matches!(
        optimized(1, src).ir.functions[1].body[0],
        Stmt::Return(Expr::Call(..))
    ));
    assert_eq!(
        optimized(2, src).ir.functions[1].body[0],
        Stmt::Return(Expr::Const(12))
    );
}
#[test]
fn effectful_arguments_keep_call() {
    let c=optimized(2,"fn f(n:i64) -> i64 {return n+n;} fn g() -> i64 {print(1); return 8;} fn main() -> i64 {return f(g());}");
    assert!(matches!(
        c.ir.functions[2].body[0],
        Stmt::Return(Expr::Call(..))
    ));
}
#[test]
fn unresolved_branch_kills_constant_facts() {
    let c = optimized(
        2,
        "fn p(b:bool) -> i64 {let mut a=1; if b {a=5;} return a;} fn main() {}",
    );
    assert!(matches!(
        c.ir.functions[0].body[2],
        Stmt::Return(Expr::Local(_))
    ));
}
#[test]
fn while_kills_constant_facts() {
    let c = optimized(
        2,
        "fn p(b:bool) -> i64 {let mut a=1; while b {a=5; break;} return a;} fn main() {}",
    );
    assert!(matches!(
        c.ir.functions[0].body[2],
        Stmt::Return(Expr::Local(_))
    ));
}
#[test]
fn dce_drops_unreachable_print() {
    let c = optimized(2, "fn main() { return; print(42); }");
    assert_eq!(c.ir.functions[0].body.len(), 1);
}
#[test]
fn dce_preserves_prints() {
    let c = optimized(2, "fn main() { let a=print(42); return; }");
    assert!(matches!(
        c.ir.functions[0].body[0],
        Stmt::Set(_, Expr::Call(0, _))
    ));
}
#[test]
fn dce_preserves_traps() {
    let c = optimized(2, "fn main() { let a=3/0; return; }");
    assert!(matches!(
        c.ir.functions[0].body[0],
        Stmt::Set(_, Expr::Bin(Op::Div, ..))
    ));
}
#[test]
fn cse_reuses_pure_expression() {
    let c = optimized(
        2,
        "fn f(a:i64,b:i64) -> i64 {let p=a*b;let q=a*b;return p+q;} fn main(){}",
    );
    assert!(matches!(
        c.ir.functions[0].body[1],
        Stmt::Set(3, Expr::Local(2))
    ));
}
#[test]
fn cse_respects_mutated_inputs() {
    let c = optimized(
        2,
        "fn f(a:i64,b:i64) -> i64 {let mut x=a;let p=x*b;x=x+1;let q=x*b;return p+q;} fn main(){}",
    );
    assert!(matches!(
        c.ir.functions[0].body[3],
        Stmt::Set(_, Expr::Bin(Op::Mul, ..))
    ));
}
#[test]
fn cse_never_eliminates_memory_reads() {
    let c = optimized(
        2,
        "fn f(a:i64) -> i64 {let x=load(a); store(a,7); return x+load(a);} fn main(){}",
    );
    assert!(
        c.ir.to_string()
            .matches(&format!("Memory({}", lumec::ir::LOAD))
            .count()
            == 2
    );
}
#[test]
fn deterministic_binary_output() {
    let s = "fn main() {let x = \"λ\"; let f=|n:i64| -> i64 {return n+len(x);}; print(f(3)); }";
    assert_eq!(optimized(2, s).wasm, optimized(2, s).wasm);
}
#[test]
fn wasm_signature() {
    assert_eq!(&optimized(2, "fn main(){}").wasm[..8], b"\0asm\x01\0\0\0");
}

#[test]
fn recursive_inlining_expands_one_layer() {
    let code = include_str!("../benchmarks/fib.lume");
    let base = optimized(0, code);
    let expanded = optimized(2, code);
    assert_ne!(base.wasm, expanded.wasm);
    assert!(
        matches!(expanded.ir.functions[0].body[1], Stmt::Return(Expr::Bin(Op::Add, ref a, ref b)) if matches!(**a, Expr::If(..)) && matches!(**b, Expr::If(..)))
    );
}
