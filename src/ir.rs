use std::{
    collections::{HashMap, HashSet},
    fmt,
};

pub const HOSTS: &[(&str, u32)] = &[
    ("print", 1),
    ("write", 1),
    ("input_len", 0),
    ("input_byte", 1),
    ("output_byte", 1),
    ("abort", 1),
    ("print_float", 1),
    ("print_char", 1),
    ("file_open", 2),
    ("file_byte", 2),
];
pub const RUNTIME: &[(&str, u32)] = &[
    ("alloc", 1),
    ("load", 1),
    ("store", 2),
    ("load8", 1),
    ("store8", 2),
];
pub const BASE: u32 = (HOSTS.len() + RUNTIME.len()) as u32;
pub const ALLOC: u32 = HOSTS.len() as u32;
pub const LOAD: u32 = ALLOC + 1;
pub const STORE: u32 = ALLOC + 2;

#[derive(Clone, Debug, Default)]
pub struct Module {
    pub functions: Vec<Func>,
    pub strings: Vec<u8>,
    pub entry: u32,
    pub table: Vec<u32>,
}
#[derive(Clone, Debug)]
pub struct Func {
    pub name: String,
    pub arity: u32,
    pub locals: u32,
    pub body: Vec<Stmt>,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Expr {
    Const(i64),
    Local(u32),
    Bin(Op, Box<Expr>, Box<Expr>),
    Unary(Un, Box<Expr>),
    Call(u32, Vec<Expr>),
    Memory(u32, Vec<Expr>),
    Indirect(Box<Expr>, Vec<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Block(Vec<Stmt>, Box<Expr>),
    Trap,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    Ult,
    Ule,
    FAdd,
    FSub,
    FMul,
    FDiv,
    FEq,
    FNe,
    FLt,
    FGt,
    FLe,
    FGe,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Un {
    Neg,
    Not,
    ToI32,
    FNeg,
    IToF,
    FToI,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Stmt {
    Set(u32, Expr),
    Eval(Expr),
    Return(Expr),
    If(Expr, Vec<Stmt>, Vec<Stmt>),
    Loop(Expr, Vec<Stmt>),
    Break,
    Continue,
}
impl Expr {
    pub fn bin(op: Op, a: Self, b: Self) -> Self {
        Self::Bin(op, Box::new(a), Box::new(b))
    }
    pub fn unary(op: Un, value: Self) -> Self {
        Self::Unary(op, Box::new(value))
    }
    pub fn select(test: Self, a: Self, b: Self) -> Self {
        Self::If(Box::new(test), Box::new(a), Box::new(b))
    }
    pub fn call(id: u32, args: Vec<Self>) -> Self {
        Self::Call(id, args)
    }
    pub fn block(stmts: Vec<Stmt>, value: Self) -> Self {
        Self::Block(stmts, Box::new(value))
    }
}
impl fmt::Display for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, fun) in self.functions.iter().enumerate() {
            writeln!(
                f,
                "fn @{} {}(params={}, locals={}) {{",
                i as u32 + BASE,
                fun.name,
                fun.arity,
                fun.locals
            )?;
            for stmt in &fun.body {
                writeln!(f, "    {stmt:?}")?;
            }
            writeln!(f, "}}")?;
        }
        Ok(())
    }
}
fn constant(op: Op, a: i64, b: i64) -> Option<i64> {
    let x = f64::from_bits(a as u64);
    let y = f64::from_bits(b as u64);
    let bits = |v: f64| v.to_bits() as i64;
    Some(match op {
        Op::Add => a.wrapping_add(b),
        Op::Sub => a.wrapping_sub(b),
        Op::Mul => a.wrapping_mul(b),
        Op::Div => a.checked_div(b)?,
        Op::Rem if b == 0 => return None,
        Op::Rem => a.wrapping_rem(b),
        Op::Eq => (a == b).into(),
        Op::Ne => (a != b).into(),
        Op::Lt => (a < b).into(),
        Op::Gt => (a > b).into(),
        Op::Le => (a <= b).into(),
        Op::Ge => (a >= b).into(),
        Op::Ult => ((a as u64) < (b as u64)).into(),
        Op::Ule => ((a as u64) <= (b as u64)).into(),
        Op::FAdd => bits(x + y),
        Op::FSub => bits(x - y),
        Op::FMul => bits(x * y),
        Op::FDiv => bits(x / y),
        Op::FEq => (x == y).into(),
        Op::FNe => (x != y).into(),
        Op::FLt => (x < y).into(),
        Op::FGt => (x > y).into(),
        Op::FLe => (x <= y).into(),
        Op::FGe => (x >= y).into(),
    })
}
fn safe(e: &Expr) -> bool {
    match e {
        Expr::Const(_) | Expr::Local(_) => true,
        Expr::Unary(op, e) => *op != Un::FToI && safe(e),
        Expr::Bin(op, a, b) => !matches!(op, Op::Div | Op::Rem) && safe(a) && safe(b),
        _ => false,
    }
}
fn size(e: &Expr) -> usize {
    match e {
        Expr::Unary(_, a) => 1 + size(a),
        Expr::Bin(_, a, b) => 1 + size(a) + size(b),
        Expr::If(c, a, b) => 1 + size(c) + size(a) + size(b),
        Expr::Call(_, args) => 1 + args.iter().map(size).sum::<usize>(),
        _ => 1,
    }
}
fn substitute(e: &Expr, args: &[Expr]) -> Expr {
    match e {
        Expr::Local(l) => args[*l as usize].clone(),
        Expr::Unary(u, a) => Expr::unary(*u, substitute(a, args)),
        Expr::Bin(o, a, b) => Expr::bin(*o, substitute(a, args), substitute(b, args)),
        Expr::If(c, a, b) => Expr::select(
            substitute(c, args),
            substitute(a, args),
            substitute(b, args),
        ),
        Expr::Call(id, xs) => Expr::call(*id, xs.iter().map(|x| substitute(x, args)).collect()),
        _ => e.clone(),
    }
}
fn locals(e: &Expr, ids: &mut HashSet<u32>) {
    match e {
        Expr::Local(id) => {
            ids.insert(*id);
        }
        Expr::Bin(_, a, b) => {
            locals(a, ids);
            locals(b, ids);
        }
        Expr::Unary(_, e) => locals(e, ids),
        Expr::Call(_, args) | Expr::Memory(_, args) => {
            for a in args {
                locals(a, ids);
            }
        }
        Expr::Indirect(e, args) => {
            locals(e, ids);
            for a in args {
                locals(a, ids);
            }
        }
        Expr::If(c, a, b) => {
            locals(c, ids);
            locals(a, ids);
            locals(b, ids);
        }
        Expr::Block(xs, e) => {
            reads(xs, ids);
            locals(e, ids);
        }
        Expr::Const(_) | Expr::Trap => {}
    }
}
fn reads(stmts: &[Stmt], ids: &mut HashSet<u32>) {
    for s in stmts {
        match s {
            Stmt::Set(_, e) | Stmt::Eval(e) | Stmt::Return(e) => locals(e, ids),
            Stmt::If(c, a, b) => {
                locals(c, ids);
                reads(a, ids);
                reads(b, ids);
            }
            Stmt::Loop(c, b) => {
                locals(c, ids);
                reads(b, ids);
            }
            Stmt::Break | Stmt::Continue => {}
        }
    }
}
struct Optimizer {
    inline: HashMap<u32, Expr>,
    recursive: HashMap<u32, Expr>,
    level: u8,
}
impl Optimizer {
    fn expr(&self, e: &mut Expr, env: &HashMap<u32, i64>) {
        match e {
            Expr::Const(_) | Expr::Trap => {}
            Expr::Local(id) => {
                if let Some(n) = env.get(id) {
                    *e = Expr::Const(*n);
                }
            }
            Expr::Unary(op, a) => {
                self.expr(a, env);
                if let Expr::Const(n) = **a {
                    let v = match op {
                        Un::Neg => n.wrapping_neg(),
                        Un::Not => (n == 0).into(),
                        Un::ToI32 => i64::from(n as i32),
                        Un::FNeg => n ^ i64::MIN,
                        Un::IToF => (n as f64).to_bits() as i64,
                        Un::FToI => return,
                    };
                    *e = Expr::Const(v);
                }
            }
            Expr::Bin(op, a, b) => {
                self.expr(a, env);
                self.expr(b, env);
                if let (Expr::Const(x), Expr::Const(y)) = (&**a, &**b) {
                    if let Some(n) = constant(*op, *x, *y) {
                        *e = Expr::Const(n);
                        return;
                    }
                }
                match (op, &**a, &**b) {
                    (Op::Add | Op::Sub, _, Expr::Const(0)) | (Op::Mul, _, Expr::Const(1)) => {
                        *e = (**a).clone()
                    }
                    (Op::Add, Expr::Const(0), _) | (Op::Mul, Expr::Const(1), _) => {
                        *e = (**b).clone()
                    }
                    _ => {}
                }
            }
            Expr::Call(id, args) => {
                for a in args.iter_mut() {
                    self.expr(a, env);
                }
                if self.level >= 2 && (LOAD..BASE).contains(id) {
                    *e = Expr::Memory(*id, std::mem::take(args));
                    return;
                }
                if let Some(body) = self
                    .inline
                    .get(id)
                    .filter(|_| args.iter().all(|arg| safe(arg) && size(arg) <= 3))
                {
                    *e = substitute(body, args);
                    self.expr(e, env);
                } else if let Some(body) = self
                    .recursive
                    .get(id)
                    .filter(|_| args.iter().all(|arg| safe(arg) && size(arg) <= 3))
                {
                    // Expand one layer. Calls introduced by this rewrite stay as calls.
                    *e = substitute(body, args);
                }
            }
            Expr::Memory(_, args) => {
                for a in args {
                    self.expr(a, env);
                }
            }
            Expr::Indirect(c, args) => {
                self.expr(c, env);
                for a in args {
                    self.expr(a, env);
                }
            }
            Expr::If(c, a, b) => {
                self.expr(c, env);
                self.expr(a, env);
                self.expr(b, env);
                if let Expr::Const(n) = **c {
                    *e = if n == 0 { (**b).clone() } else { (**a).clone() };
                }
            }
            Expr::Block(stmts, value) => {
                self.block(stmts);
                self.expr(value, &HashMap::new());
            }
        }
    }
    fn block(&self, stmts: &mut Vec<Stmt>) {
        let mut env = HashMap::new();
        let mut cache = HashMap::<Expr, u32>::new();
        let mut out = Vec::with_capacity(stmts.len());
        for mut s in std::mem::take(stmts) {
            match &mut s {
                Stmt::Set(id, e) => {
                    self.expr(e, &env);
                    cache.retain(|key, dst| {
                        let mut deps = HashSet::new();
                        locals(key, &mut deps);
                        dst != id && !deps.contains(id)
                    });
                    env.remove(id);
                    if let Expr::Const(n) = e {
                        env.insert(*id, *n);
                    } else if self.level >= 2 && safe(e) && size(e) > 1 {
                        if let Some(dst) = cache.get(e) {
                            *e = Expr::Local(*dst);
                        } else {
                            let mut deps = HashSet::new();
                            locals(e, &mut deps);
                            if !deps.contains(id) {
                                cache.insert(e.clone(), *id);
                            }
                        }
                    }
                }
                Stmt::Eval(e) => {
                    self.expr(e, &env);
                    if safe(e) {
                        continue;
                    }
                }
                Stmt::Return(e) => self.expr(e, &env),
                Stmt::If(c, a, b) => {
                    self.expr(c, &env);
                    self.block(a);
                    self.block(b);
                    if let Expr::Const(n) = c {
                        let chosen = if *n == 0 { b } else { a };
                        out.append(chosen);
                        env.clear();
                        cache.clear();
                        continue;
                    }
                    env.clear();
                    cache.clear();
                }
                Stmt::Loop(c, b) => {
                    self.expr(c, &HashMap::new());
                    self.block(b);
                    env.clear();
                    cache.clear();
                }
                Stmt::Break | Stmt::Continue => {}
            }
            let stop = matches!(s, Stmt::Return(_) | Stmt::Break | Stmt::Continue);
            out.push(s);
            if stop {
                break;
            }
        }
        *stmts = out;
    }
}
fn remove_unused(stmts: &mut Vec<Stmt>, used: &HashSet<u32>) {
    stmts.retain(|s| !matches!(s, Stmt::Set(id,e) if !used.contains(id) && safe(e)));
    for s in stmts {
        match s {
            Stmt::If(_, a, b) => {
                remove_unused(a, used);
                remove_unused(b, used);
            }
            Stmt::Loop(_, b) => remove_unused(b, used),
            _ => {}
        }
    }
}
fn inlineable_recursive(e: &Expr, id: u32, arity: u32) -> bool {
    match e {
        Expr::Const(_) => true,
        Expr::Local(l) => *l < arity,
        Expr::Unary(_, a) => inlineable_recursive(a, id, arity),
        Expr::Bin(_, a, b) => [a, b].iter().all(|x| inlineable_recursive(x, id, arity)),
        Expr::If(c, a, b) => [c, a, b].iter().all(|x| inlineable_recursive(x, id, arity)),
        Expr::Call(target, args) => {
            *target == id && args.iter().all(|x| inlineable_recursive(x, id, arity))
        }
        _ => false,
    }
}
pub fn optimize(module: &mut Module, level: u8) {
    if level == 0 {
        return;
    }
    let mut pass = Optimizer {
        inline: HashMap::new(),
        recursive: HashMap::new(),
        level,
    };
    if level >= 2 {
        for (i, f) in module.functions.iter().enumerate() {
            let id = i as u32 + BASE;
            match f.body.as_slice() {
                [Stmt::Return(e)] if safe(e) && size(e) <= 20 => {
                    let mut refs = HashSet::new();
                    locals(e, &mut refs);
                    if refs.iter().all(|l| *l < f.arity) {
                        pass.inline.insert(id, e.clone());
                    }
                }
                [Stmt::If(c, yes, no), Stmt::Return(b)] if no.is_empty() => {
                    if let [Stmt::Return(a)] = yes.as_slice() {
                        let branch = Expr::select(c.clone(), a.clone(), b.clone());
                        if size(&branch) <= 24 && inlineable_recursive(&branch, id, f.arity) {
                            pass.recursive.insert(id, branch);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    for f in &mut module.functions {
        pass.block(&mut f.body);
        let mut used = HashSet::new();
        reads(&f.body, &mut used);
        remove_unused(&mut f.body, &used);
    }
}
