mod support;
use std::io::{BufWriter, Write};

const CASES: usize = 5_000;
const SEED: u64 = 0x8e3b_e2ab_69c4_71af;
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn choose(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}
fn integer(r: &mut Rng, depth: usize, args: (i64, i64)) -> (String, i64) {
    if depth == 0 || r.choose(5) == 0 {
        return match r.choose(6) {
            0 => ("a".into(), args.0),
            1 => ("b".into(), args.1),
            2 => ("9223372036854775807".into(), i64::MAX),
            _ => {
                let x = r.choose(2001) as i64 - 1000;
                (format!("({x})"), x)
            }
        };
    }
    let (lhs, a) = integer(r, depth - 1, args);
    let (rhs, b) = integer(r, depth - 1, args);
    match r.choose(7) {
        0 => (format!("({lhs} + {rhs})"), a.wrapping_add(b)),
        1 => (format!("({lhs} - {rhs})"), a.wrapping_sub(b)),
        2 => (format!("({lhs} * {rhs})"), a.wrapping_mul(b)),
        3 => (format!("(-{lhs})"), a.wrapping_neg()),
        4 => {
            let d = (b as u64 % 61 + 1) as i64;
            (format!("({lhs} / {d})"), a / d)
        }
        5 => {
            let d = (b as u64 % 61 + 1) as i64;
            (format!("({lhs} % {d})"), a % d)
        }
        _ => (
            format!("mix({lhs}, {rhs})"),
            a.wrapping_mul(3).wrapping_add(b),
        ),
    }
}
fn boolean(r: &mut Rng, depth: usize, args: (i64, i64)) -> (String, bool) {
    if depth > 0 && r.choose(3) == 0 {
        let (a, x) = boolean(r, depth - 1, args);
        let (b, y) = boolean(r, depth - 1, args);
        return if r.choose(2) == 0 {
            (format!("({a} && {b})"), x && y)
        } else {
            (format!("({a} || {b})"), x || y)
        };
    }
    let (a, x) = integer(r, 2, args);
    let (b, y) = integer(r, 2, args);
    match r.choose(5) {
        0 => (format!("({a} < {b})"), x < y),
        1 => (format!("({a} == {b})"), x == y),
        2 => (format!("!({a} != {b})"), x == y),
        3 => (format!("({a} <= {b})"), x <= y),
        _ => (format!("({a} > {b})"), x > y),
    }
}
fn generate(r: &mut Rng) -> (String, String) {
    let a = r.choose(20001) as i64 - 10000;
    let b = r.choose(20001) as i64 - 10000;
    let args = (a, b);
    let (test, condition) = boolean(r, 3, args);
    let (yes, x) = integer(r, 4, args);
    let (no, y) = integer(r, 4, args);
    let (same, z) = integer(r, 3, args);
    let loops = r.choose(10) as i64;
    let mut result = if condition { x } else { y };
    for i in 0..loops {
        result = result.wrapping_add(z).wrapping_add(i);
    }
    let final_result = result.wrapping_add(z).wrapping_add(z);
    let (decor, final_stdout) = match r.choose(6) {
        0 => (
            "let f = |x: i64| -> i64 { return x + final; }; print(f(7));",
            format!("{}\n", final_result.wrapping_add(7)),
        ),
        1 => (
            "let a = array(2, final); let view = a[0..2]; view[1] = 7; print(a[0] + a[1]);",
            format!("{}\n", final_result.wrapping_add(7)),
        ),
        2 => (
            "let o = Some(final); let n = match o { Some(v) => v, None => 108 }; print(n);",
            format!("{final_result}\n"),
        ),
        3 => (
            "let a = as_i32(final); let b: i32 = a * 513; print(as_i64(b));",
            format!("{}\n", (final_result as i32).wrapping_mul(513)),
        ),
        4 => (
            "if false && side() { print(999); } else { print(final); }",
            format!("{final_result}\n"),
        ),
        _ => ("print(final);", format!("{final_result}\n")),
    };
    let source = format!(
        r#"
fn mix(a: i64, b: i64) -> i64 {{ return a * 3 + b; }}
fn side() -> bool {{ print(999); return true; }}
fn calculate(a: i64, b: i64) {{
    let mut result = 0;
    if {test} {{ result = {yes}; }} else {{ result = {no}; }}
    let same = {same};
    let duplicate = {same};
    let mut i = 0;
    while i < {loops} {{ result = result + same + i; i = i + 1; }}
    print(result);
    let final = result + same + duplicate;
    {decor}
}}
fn main() {{ calculate({a}, {b}); }}
"#
    );
    (source, format!("{result}\n{final_stdout}"))
}

#[test]
fn fuzz_5000_programs() {
    let scratch = support::Scratch::new();
    let corpus = scratch.join("corpus.bin");
    let mut file = BufWriter::new(std::fs::File::create(&corpus).unwrap());
    let mut r = Rng(SEED);
    for case in 0..CASES {
        let (source, output) = generate(&mut r);
        let label = format!("fuzz-{case}.lume");
        let binaries: Vec<_> = [0, 2]
            .into_iter()
            .map(|optimization| {
                lumec::compile_text(&label, &source, optimization)
                    .unwrap_or_else(|e| panic!("caso {case}, -O{optimization}\n{source}\n{}", e[0]))
                    .wasm
            })
            .collect();
        for bytes in [
            source.as_bytes(),
            output.as_bytes(),
            &binaries[0],
            &binaries[1],
        ] {
            file.write_all(&(bytes.len() as u32).to_le_bytes()).unwrap();
            file.write_all(bytes).unwrap();
        }
    }
    file.flush().unwrap();
    let result = support::node("tools/fuzz.cjs", &[&corpus]);
    assert_eq!(result.trim(), format!("{CASES} casos, 0 divergências"));
    println!("{result}seed 0x{SEED:x}");
}
