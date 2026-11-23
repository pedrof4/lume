use std::collections::{HashMap, HashSet};

use crate::{
    ir::{self, Expr as I, Func, Module, Op, Stmt as S, Un},
    syntax::{Block, Diagnostic, Ex, Expr, Function, Generics, Item, Pat, Span, St, Ty},
};

type R<T> = Result<T, Diagnostic>;
type Sub = HashMap<String, Ty>;
type Fields = Vec<(String, Ty)>;
type Variants = Vec<(String, Vec<Ty>)>;
#[derive(Clone)]
enum Data {
    Struct(Fields),
    Enum(Variants),
}
#[derive(Clone)]
struct Definition {
    generics: Generics,
    data: Data,
    span: Span,
}
#[derive(Clone)]
struct Binding {
    local: u32,
    ty: Ty,
    mutable: bool,
}
struct Context {
    scopes: Vec<HashMap<String, Binding>>,
    locals: u32,
    ret: Ty,
    sub: Sub,
    loops: u32,
}
impl Context {
    fn new(ret: Ty, sub: Sub) -> Self {
        Self {
            scopes: vec![HashMap::new()],
            locals: 0,
            ret,
            sub,
            loops: 0,
        }
    }
    fn temporary(&mut self) -> u32 {
        let id = self.locals;
        self.locals += 1;
        id
    }
    fn lookup(&self, n: &str) -> Option<&Binding> {
        self.scopes.iter().rev().find_map(|s| s.get(n))
    }
    fn bind(&mut self, n: &str, ty: Ty, mutable: bool, at: &Span) -> R<u32> {
        if self.scopes.last().unwrap().contains_key(n) {
            return Err(at.error(&format!("nome `{n}` já declarado neste escopo")));
        }
        let id = self.temporary();
        self.scopes.last_mut().unwrap().insert(
            n.into(),
            Binding {
                local: id,
                ty,
                mutable,
            },
        );
        Ok(id)
    }
}
struct Lower {
    module: Module,
    functions: Vec<Function>,
    names: HashMap<String, usize>,
    data: HashMap<String, Definition>,
    constructors: HashMap<String, (String, Option<usize>)>,
    traits: HashMap<String, Vec<Function>>,
    assignments: HashMap<(String, Ty), HashMap<String, usize>>,
    methods: HashMap<String, String>,
    instances: HashMap<(usize, Vec<Ty>), u32>,
    queue: Vec<(usize, Vec<Ty>, u32)>,
    strings: HashMap<String, i64>,
    thunks: HashMap<u32, u32>,
    str_eq: Option<u32>,
    signatures_ready: bool,
}
fn ty(n: &str) -> Ty {
    Ty::name(n)
}
fn subst(t: &Ty, s: &Sub) -> Ty {
    match t {
        Ty::Named(n, xs) if xs.is_empty() && s.contains_key(n) => s[n].clone(),
        Ty::Named(n, xs) => Ty::Named(n.clone(), xs.iter().map(|x| subst(x, s)).collect()),
        Ty::Fun(xs, r) => Ty::Fun(
            xs.iter().map(|x| subst(x, s)).collect(),
            Box::new(subst(r, s)),
        ),
    }
}
fn sub(g: &Generics, xs: &[Ty]) -> Sub {
    g.iter()
        .zip(xs)
        .map(|((n, _), x)| (n.clone(), x.clone()))
        .collect()
}
fn mismatch(at: &Span, expected: &Ty, actual: &Ty) -> Diagnostic {
    at.error(&format!(
        "tipo esperado `{expected}`, encontrado `{actual}`"
    ))
}
fn fits(at: &Span, a: &Ty, b: &Ty) -> R<()> {
    if a == b {
        Ok(())
    } else {
        Err(mismatch(at, a, b))
    }
}
fn infer(template: &Ty, actual: &Ty, generic: &Generics, s: &mut Sub, at: &Span) -> R<()> {
    match (template, actual) {
        (Ty::Named(n, args), _) if args.is_empty() && generic.iter().any(|(v, _)| v == n) => {
            if let Some(t) = s.get(n) {
                fits(at, t, actual)?;
            } else {
                s.insert(n.clone(), actual.clone());
            }
        }
        (Ty::Named(n, args), Ty::Named(m, values)) if n == m && args.len() == values.len() => {
            for (a, b) in args.iter().zip(values) {
                infer(a, b, generic, s, at)?;
            }
        }
        (Ty::Fun(a, r), Ty::Fun(b, sr)) if a.len() == b.len() => {
            for (a, b) in a.iter().zip(b) {
                infer(a, b, generic, s, at)?;
            }
            infer(r, sr, generic, s, at)?;
        }
        _ => fits(at, template, actual)?,
    }
    Ok(())
}
fn concrete(t: &Ty, gs: &Generics, s: &Sub) -> bool {
    match t {
        Ty::Named(n, xs) => {
            (!gs.iter().any(|(v, _)| v == n) || s.contains_key(n))
                && xs.iter().all(|x| concrete(x, gs, s))
        }
        Ty::Fun(xs, r) => xs.iter().all(|x| concrete(x, gs, s)) && concrete(r, gs, s),
    }
}
fn prepend(a: I, n: i64) -> I {
    I::bin(Op::Add, a, I::Const(n))
}
fn load(a: I, off: i64) -> I {
    I::call(ir::LOAD, vec![prepend(a, off)])
}
fn store(a: I, off: i64, v: I) -> S {
    S::Eval(I::call(ir::STORE, vec![prepend(a, off), v]))
}
fn trap_if(c: I) -> S {
    S::If(c, vec![S::Eval(I::Trap)], vec![])
}
fn numeric(t: &Ty) -> bool {
    t.is("i64") || t.is("i32") || t.is("f64")
}
fn wrap(t: &Ty, x: I) -> I {
    if t.is("i32") {
        I::unary(Un::ToI32, x)
    } else {
        x
    }
}

pub fn lower(items: &[Item]) -> R<Module> {
    let mut l = Lower {
        module: Module::default(),
        functions: vec![],
        names: HashMap::new(),
        data: HashMap::new(),
        constructors: HashMap::new(),
        traits: HashMap::new(),
        assignments: HashMap::new(),
        methods: HashMap::new(),
        instances: HashMap::new(),
        queue: vec![],
        strings: HashMap::new(),
        thunks: HashMap::new(),
        str_eq: None,
        signatures_ready: false,
    };
    for item in items {
        match item {
            Item::Function(f) => {
                if l.names.insert(f.name.clone(), l.functions.len()).is_some()
                    || builtin(&f.name)
                    || ir::HOSTS
                        .iter()
                        .chain(ir::RUNTIME)
                        .any(|(n, _)| *n == f.name)
                {
                    return Err(f
                        .span
                        .error(&format!("função `{}` já declarada ou reservada", f.name)));
                }
                l.functions.push(f.clone());
            }
            Item::Struct(n, g, fields, span) => {
                l.add_data(n, g, Data::Struct(fields.clone()), span)?;
                l.constructor(n, n, None, span)?;
                unique(fields.iter().map(|(n, _)| n), span)?;
            }
            Item::Enum(n, g, vs, span) => {
                if vs.is_empty() {
                    return Err(span.error("enum sem variantes"));
                }
                l.add_data(n, g, Data::Enum(vs.clone()), span)?;
                for (i, (v, _)) in vs.iter().enumerate() {
                    l.constructor(v, n, Some(i), span)?;
                }
            }
            Item::Trait(n, fs, span) => {
                if l.traits.insert(n.clone(), fs.clone()).is_some() {
                    return Err(span.error("trait já declarado"));
                }
                for f in fs {
                    if !f.generics.is_empty() || !f.args.first().is_some_and(|(_, t)| t.is("Self"))
                    {
                        return Err(f.span.error("método de trait precisa de primeiro parâmetro `Self` e não declara generics"));
                    }
                    if l.methods.insert(f.name.clone(), n.clone()).is_some() {
                        return Err(f.span.error("nome de método de trait já declarado"));
                    }
                }
            }
            Item::Impl(..) | Item::Import(..) => {}
        }
    }
    for (n, (owner, _)) in &l.constructors {
        if l.names.contains_key(n)
            || builtin(n)
            || ir::HOSTS
                .iter()
                .chain(ir::RUNTIME)
                .any(|(name, _)| *name == n)
        {
            return Err(l.data[owner].span.error("nome de construtor em conflito"));
        }
    }
    for item in items {
        if let Item::Impl(n, t, fs, span) = item {
            l.valid_ty(t, &vec![], span)?;
            let contract = l
                .traits
                .get(n)
                .ok_or_else(|| span.error(&format!("trait `{n}` desconhecido")))?;
            let mapping = HashMap::from([("Self".into(), t.clone())]);
            let mut entries = HashMap::new();
            for f in fs {
                let sig = contract
                    .iter()
                    .find(|m| m.name == f.name)
                    .ok_or_else(|| f.span.error("método ausente do trait"))?;
                if !f.generics.is_empty() || f.args.len() != sig.args.len() {
                    return Err(f.span.error("assinatura incompatível com o trait"));
                }
                for ((_, got), (_, want)) in f.args.iter().zip(&sig.args) {
                    fits(&f.span, &subst(want, &mapping), got)?;
                }
                fits(&f.span, &subst(&sig.ret, &mapping), &f.ret)?;
                if entries.insert(f.name.clone(), l.functions.len()).is_some() {
                    return Err(f.span.error("método implementado duas vezes"));
                }
                let mut f = f.clone();
                f.name = format!("{n}::{t}::{}", f.name);
                l.functions.push(f);
            }
            if entries.len() != contract.len() {
                return Err(span.error("implementação de trait incompleta"));
            }
            if l.assignments
                .insert((n.clone(), t.clone()), entries)
                .is_some()
            {
                return Err(span.error("implementação de trait duplicada"));
            }
        }
    }
    l.signatures_ready = true;
    for (n, d) in &l.data {
        l.validate_generics(&d.generics, &d.span)?;
        let ts: Vec<_> = match &d.data {
            Data::Struct(xs) => xs.iter().map(|(_, t)| t).collect(),
            Data::Enum(vs) => vs.iter().flat_map(|(_, ts)| ts).collect(),
        };
        for t in ts {
            l.valid_ty(t, &d.generics, &d.span)?;
        }
        if l.names.contains_key(n) || l.methods.contains_key(n) {
            return Err(d.span.error("nome de tipo conflita com função ou método"));
        }
    }
    for fs in l.traits.values() {
        let self_ty = vec![("Self".into(), None)];
        for f in fs {
            for (_, t) in &f.args {
                l.valid_ty(t, &self_ty, &f.span)?;
            }
            l.valid_ty(&f.ret, &self_ty, &f.span)?;
            if l.names.contains_key(&f.name)
                || l.constructors.contains_key(&f.name)
                || builtin(&f.name)
                || ir::HOSTS
                    .iter()
                    .chain(ir::RUNTIME)
                    .any(|(n, _)| *n == f.name)
            {
                return Err(f
                    .span
                    .error("nome de método conflita com função ou construtor"));
            }
        }
    }
    for item in items {
        if let Item::Impl(_, t, _, span) = item {
            l.valid_ty(t, &vec![], span)?;
        }
    }
    let fallback = items
        .last()
        .map(|item| match item {
            Item::Function(f) => &f.span,
            Item::Import(_, s)
            | Item::Struct(_, _, _, s)
            | Item::Enum(_, _, _, s)
            | Item::Trait(_, _, s)
            | Item::Impl(_, _, _, s) => s,
        })
        .unwrap();
    let entry = *l
        .names
        .get("main")
        .ok_or_else(|| fallback.error("função `main` ausente"))?;
    let main = &l.functions[entry];
    if !main.generics.is_empty()
        || !main.args.is_empty()
        || !(main.ret.is("unit") || main.ret.is("i64"))
    {
        return Err(main
            .span
            .error("main deve ter assinatura `fn main()` ou `fn main() -> i64`"));
    }
    for i in 0..l.functions.len() {
        let f = &l.functions[i];
        l.validate_generics(&f.generics, &f.span)?;
        unique(f.args.iter().map(|(n, _)| n), &f.span)?;
        for (_, t) in &f.args {
            l.valid_ty(t, &f.generics, &f.span)?;
        }
        l.valid_ty(&f.ret, &f.generics, &f.span)?;
        if f.generics.is_empty() {
            let span = f.span.clone();
            l.instance(i, vec![], &span)?;
        }
    }
    l.module.entry = l.instances[&(entry, vec![])];
    let mut at = 0;
    while let Some((id, xs, output)) = l.queue.get(at).cloned() {
        let f = l.functions[id].clone();
        let s = sub(&f.generics, &xs);
        let mut c = Context::new(subst(&f.ret, &s), s);
        for (n, t) in &f.args {
            c.bind(n, subst(t, &c.sub), false, &f.span)?;
        }
        let body = l.function_body(&mut c, &f.body, &f.span)?;
        l.module.functions[(output - ir::BASE) as usize] = Func {
            name: if xs.is_empty() {
                f.name
            } else {
                format!(
                    "{}::<{}>",
                    f.name,
                    xs.iter().map(Ty::to_string).collect::<Vec<_>>().join(",")
                )
            },
            arity: f.args.len() as u32,
            locals: c.locals,
            body,
        };
        at += 1;
    }
    Ok(l.module)
}
fn unique<'a>(items: impl Iterator<Item = &'a String>, at: &Span) -> R<()> {
    let mut seen = HashSet::new();
    for n in items {
        if !seen.insert(n) {
            return Err(at.error(&format!("nome `{n}` declarado duas vezes")));
        }
    }
    Ok(())
}
fn builtin(n: &str) -> bool {
    matches!(
        n,
        "len" | "array" | "as_i32" | "as_i64" | "as_f64" | "codepoint" | "str_from_raw"
    )
}

impl Lower {
    fn add_data(&mut self, n: &str, g: &Generics, data: Data, at: &Span) -> R<()> {
        if matches!(
            n,
            "Array" | "Slice" | "i32" | "i64" | "f64" | "unit" | "bool" | "char" | "str" | "Self"
        ) || self
            .data
            .insert(
                n.into(),
                Definition {
                    generics: g.clone(),
                    data,
                    span: at.clone(),
                },
            )
            .is_some()
        {
            return Err(at.error(&format!("tipo `{n}` duplicado ou reservado")));
        }
        Ok(())
    }
    fn constructor(&mut self, n: &str, t: &str, v: Option<usize>, at: &Span) -> R<()> {
        if self.constructors.insert(n.into(), (t.into(), v)).is_some() {
            return Err(at.error(&format!("construtor `{n}` já declarado")));
        }
        Ok(())
    }
    fn validate_generics(&self, gs: &Generics, at: &Span) -> R<()> {
        unique(gs.iter().map(|(n, _)| n), at)?;
        for (n, b) in gs {
            if matches!(
                n.as_str(),
                "Array"
                    | "Slice"
                    | "i32"
                    | "i64"
                    | "f64"
                    | "unit"
                    | "bool"
                    | "char"
                    | "str"
                    | "Self"
            ) {
                return Err(at.error(&format!("parâmetro de tipo `{n}` está reservado")));
            }
            if let Some(b) = b {
                if !self.traits.contains_key(b) {
                    return Err(at.error(&format!("trait `{b}` desconhecido")));
                }
            }
        }
        Ok(())
    }
    fn valid_ty(&self, t: &Ty, gs: &Generics, at: &Span) -> R<()> {
        match t {
            Ty::Fun(xs, r) => {
                for x in xs {
                    self.valid_ty(x, gs, at)?;
                }
                self.valid_ty(r, gs, at)?;
            }
            Ty::Named(n, xs) => {
                let a = match n.as_str() {
                    "i32" | "i64" | "f64" | "unit" | "bool" | "char" | "str" => 0,
                    "Array" | "Slice" => 1,
                    _ if gs.iter().any(|(g, _)| g == n) => 0,
                    _ => self
                        .data
                        .get(n)
                        .ok_or_else(|| at.error(&format!("tipo `{n}` desconhecido")))?
                        .generics
                        .len(),
                };
                if xs.len() != a {
                    return Err(at.error(&format!("tipo `{n}` espera {a} argumentos de tipo")));
                }
                for x in xs {
                    self.valid_ty(x, gs, at)?;
                }
                if let Some(d) = self.data.get(n).filter(|_| {
                    self.signatures_ready
                        && !gs.iter().any(|(g, _)| g == n)
                        && xs.iter().all(|x| concrete(x, gs, &Sub::new()))
                }) {
                    self.bounds(&d.generics, xs, at)?;
                }
            }
        }
        Ok(())
    }
    fn bounds(&self, g: &Generics, xs: &[Ty], at: &Span) -> R<()> {
        for ((_, b), x) in g.iter().zip(xs) {
            if let Some(b) = b {
                if !self.assignments.contains_key(&(b.clone(), x.clone())) {
                    return Err(at.error(&format!("`{x}` não implementa `{b}`")));
                }
            }
        }
        Ok(())
    }
    fn reserve(&mut self, name: String, arity: u32) -> u32 {
        let idx = ir::BASE + self.module.functions.len() as u32;
        self.module.functions.push(Func {
            name,
            arity,
            locals: arity,
            body: vec![],
        });
        idx
    }
    fn instance(&mut self, id: usize, xs: Vec<Ty>, at: &Span) -> R<u32> {
        let key = (id, xs);
        if let Some(idx) = self.instances.get(&key) {
            return Ok(*idx);
        }
        if self.queue.len() > 4096 || key.1.iter().any(|t| type_depth(t) > 64) {
            return Err(at.error("limite de monomorfizações excedido"));
        }
        let f = &self.functions[id];
        self.bounds(&f.generics, &key.1, at)?;
        let idx = self.reserve(f.name.clone(), f.args.len() as u32);
        self.queue.push((id, key.1.clone(), idx));
        self.instances.insert(key, idx);
        Ok(idx)
    }
    fn function_body(&mut self, c: &mut Context, b: &Block, at: &Span) -> R<Vec<S>> {
        let mut code = self.block(c, b)?;
        if !returns(b) {
            if !c.ret.is("unit") {
                return Err(at.error(&format!("nem todos os caminhos retornam `{}`", c.ret)));
            }
            code.push(S::Return(I::Const(0)));
        }
        Ok(code)
    }
    fn block(&mut self, c: &mut Context, b: &Block) -> R<Vec<S>> {
        c.scopes.push(HashMap::new());
        let mut out = vec![];
        for s in b {
            match &s.kind {
                St::Let(n, m, annotation, e) => {
                    let want = annotation.as_ref().map(|t| subst(t, &c.sub));
                    if let Some(t) = &want {
                        self.valid_ty(t, &vec![], &s.span)?;
                    }
                    let (value, t) = self.expr(c, e, want.as_ref())?;
                    let id = c.bind(n, t, *m, &s.span)?;
                    out.push(S::Set(id, value));
                }
                St::Set(target, value) => match &target.kind {
                    Ex::Name(n) => {
                        let b = c
                            .lookup(n)
                            .cloned()
                            .ok_or_else(|| self.unknown(n, c, &target.span))?;
                        if !b.mutable {
                            let mut d = target.span.error("atribuição a variável imutável");
                            d.hint = Some(format!("declare com `let mut {n}`"));
                            return Err(d);
                        }
                        let (v, _) = self.expr(c, value, Some(&b.ty))?;
                        out.push(S::Set(b.local, v));
                    }
                    Ex::Field(o, n) => {
                        let (base, t) = self.expr(c, o, None)?;
                        let (off, t) = self.field(&t, n, &target.span)?;
                        let (v, _) = self.expr(c, value, Some(&t))?;
                        out.push(store(base, off, v));
                    }
                    Ex::Index(o, n) => {
                        let (p, el, byte) = self.address(c, o, n)?;
                        if byte {
                            return Err(target.span.error("str é imutável"));
                        }
                        let (v, _) = self.expr(c, value, Some(&el))?;
                        out.push(store(p, 0, v));
                    }
                    _ => return Err(target.span.error("alvo de atribuição inválido")),
                },
                St::Eval(e) => {
                    let (v, _) = self.expr(c, e, None)?;
                    out.push(S::Eval(v));
                }
                St::Return(e) => {
                    let t = c.ret.clone();
                    let v = if let Some(e) = e {
                        self.expr(c, e, Some(&t))?.0
                    } else {
                        fits(&s.span, &t, &ty("unit"))?;
                        I::Const(0)
                    };
                    out.push(S::Return(v));
                }
                St::If(e, a, b) => {
                    let (v, _) = self.expr(c, e, Some(&ty("bool")))?;
                    out.push(S::If(v, self.block(c, a)?, self.block(c, b)?));
                }
                St::While(e, b) => {
                    let (v, _) = self.expr(c, e, Some(&ty("bool")))?;
                    c.loops += 1;
                    let b = self.block(c, b)?;
                    c.loops -= 1;
                    out.push(S::Loop(v, b));
                }
                St::Break | St::Continue => {
                    if c.loops == 0 {
                        return Err(s.span.error("controle de laço fora de `while`"));
                    }
                    out.push(if matches!(s.kind, St::Break) {
                        S::Break
                    } else {
                        S::Continue
                    });
                }
            }
        }
        c.scopes.pop();
        Ok(out)
    }
    fn unknown(&self, n: &str, c: &Context, at: &Span) -> Diagnostic {
        let mut diag = at.error(&format!("nome `{n}` desconhecido"));
        let mut names: Vec<_> = self
            .names
            .keys()
            .chain(c.scopes.iter().flat_map(|s| s.keys()))
            .collect();
        names.sort();
        if let Some(x) = names.into_iter().find(|x| near(n, x)) {
            diag.hint = Some(format!("você quis dizer `{x}`?"));
        }
        diag
    }
    fn string(&mut self, s: &str) -> i64 {
        if let Some(p) = self.strings.get(s) {
            return *p;
        }
        let ptr = 1024 + self.module.strings.len() as i64;
        self.module.strings.extend((s.len() as u64).to_le_bytes());
        self.module.strings.extend(s.as_bytes());
        let n = self.module.strings.len().next_multiple_of(8);
        self.module.strings.resize(n, 0);
        self.strings.insert(s.into(), ptr);
        ptr
    }
    fn field(&self, t: &Ty, n: &str, at: &Span) -> R<(i64, Ty)> {
        if let Ty::Named(name, xs) = t {
            if let Some(Definition {
                generics,
                data: Data::Struct(fs),
                ..
            }) = self.data.get(name)
            {
                if let Some((i, (_, ft))) = fs.iter().enumerate().find(|(_, (f, _))| f == n) {
                    return Ok((i as i64 * 8, subst(ft, &sub(generics, xs))));
                }
            }
        }
        Err(at.error(&format!("`{t}` não possui campo `{n}`")))
    }
    fn record(&self, c: &mut Context, values: Vec<I>) -> I {
        let p = c.temporary();
        let mut ss = vec![S::Set(
            p,
            I::call(ir::ALLOC, vec![I::Const((values.len() as i64 * 8).max(8))]),
        )];
        for (i, v) in values.into_iter().enumerate() {
            ss.push(store(I::Local(p), i as i64 * 8, v));
        }
        I::block(ss, I::Local(p))
    }
    fn address(&mut self, c: &mut Context, object: &Expr, index: &Expr) -> R<(I, Ty, bool)> {
        let (base, t) = self.expr(c, object, None)?;
        let (ix, _) = self.expr(c, index, Some(&ty("i64")))?;
        let (element, kind) = sequence(&t, &object.span)?;
        let b = c.temporary();
        let i = c.temporary();
        let offset = if kind == 2 {
            I::Local(i)
        } else {
            I::bin(Op::Mul, I::Local(i), I::Const(8))
        };
        let data = if kind == 1 {
            load(I::Local(b), 8)
        } else {
            prepend(I::Local(b), 8)
        };
        Ok((
            I::block(
                vec![
                    S::Set(b, base),
                    S::Set(i, ix),
                    trap_if(I::bin(Op::Ule, load(I::Local(b), 0), I::Local(i))),
                ],
                I::bin(Op::Add, data, offset),
            ),
            element,
            kind == 2,
        ))
    }
    fn expr(&mut self, c: &mut Context, e: &Expr, want: Option<&Ty>) -> R<(I, Ty)> {
        let at = &e.span;
        let (v, t) = match &e.kind {
            Ex::Int(n) => {
                if want.is_some_and(|t| t.is("i32")) {
                    if i32::try_from(*n).is_err() {
                        return Err(at.error("literal fora do intervalo de i32"));
                    }
                    (I::Const(*n), ty("i32"))
                } else {
                    (I::Const(*n), ty("i64"))
                }
            }
            Ex::Float(x) => (I::Const(x.to_bits() as i64), ty("f64")),
            Ex::Char(x) => (I::Const(i64::from(u32::from(*x))), ty("char")),
            Ex::Bool(x) => (I::Const((*x).into()), ty("bool")),
            Ex::Str(x) => (I::Const(self.string(x)), ty("str")),
            Ex::Name(n) => {
                if let Some(b) = c.lookup(n) {
                    (I::Local(b.local), b.ty.clone())
                } else if let Some(id) = self.names.get(n).copied() {
                    let f = self.functions[id].clone();
                    if !f.generics.is_empty() {
                        return Err(at.error("função genérica exige chamada para especialização"));
                    }
                    let params = f.args.into_iter().map(|(_, t)| t).collect::<Vec<_>>();
                    let target = self.instance(id, vec![], at)?;
                    let table = if let Some(index) = self.thunks.get(&target) {
                        *index
                    } else {
                        let arity = params.len() as u32 + 1;
                        let thunk = self.reserve(format!("<valor:{n}>"), arity);
                        self.module.functions[(thunk - ir::BASE) as usize].body = vec![S::Return(
                            I::call(target, (1..arity).map(I::Local).collect()),
                        )];
                        let index = self.module.table.len() as u32;
                        self.module.table.push(thunk);
                        self.thunks.insert(target, index);
                        index
                    };
                    (
                        self.record(c, vec![I::Const(i64::from(table))]),
                        Ty::Fun(params, Box::new(f.ret)),
                    )
                } else if self.constructors.contains_key(n) {
                    self.construct(c, n, &[], &[], want, at)?
                } else {
                    return Err(self.unknown(n, c, at));
                }
            }
            Ex::Array(xs) => {
                let expected = match want {
                    Some(Ty::Named(n, ts)) if n == "Array" => ts.first().cloned(),
                    _ => None,
                };
                let mut el = expected;
                let mut vs = vec![I::Const(xs.len() as i64)];
                for x in xs {
                    let (v, t) = self.expr(c, x, el.as_ref())?;
                    el = Some(t);
                    vs.push(v);
                }
                let el =
                    el.ok_or_else(|| at.error("literal de array vazio precisa de anotação"))?;
                (self.record(c, vs), Ty::Named("Array".into(), vec![el]))
            }
            Ex::Unary(op, x) => {
                if *op == "-"
                    && want.is_some_and(|t| t.is("i32"))
                    && matches!(x.kind, Ex::Int(2147483648))
                {
                    (I::Const(i64::from(i32::MIN)), ty("i32"))
                } else {
                    let (v, t) = self.expr(c, x, want.filter(|t| numeric(t)))?;
                    match *op {
                        "-" if numeric(&t) => (
                            wrap(
                                &t,
                                I::unary(if t.is("f64") { Un::FNeg } else { Un::Neg }, v),
                            ),
                            t,
                        ),
                        "!" if t.is("bool") => (I::unary(Un::Not, v), t),
                        _ => return Err(at.error(&format!("operador `{op}` não admite `{t}`"))),
                    }
                }
            }
            Ex::Binary(op, a, b) => self.binary(c, op, a, b, want)?,
            Ex::Call(f, types, args) => self.call(c, f, types, args, want, at)?,
            Ex::Field(e, n) => {
                let (p, t) = self.expr(c, e, None)?;
                let (off, t) = self.field(&t, n, at)?;
                (load(p, off), t)
            }
            Ex::Index(p, i) => {
                let (addr, t, byte) = self.address(c, p, i)?;
                (
                    I::call(if byte { ir::LOAD + 2 } else { ir::LOAD }, vec![addr]),
                    t,
                )
            }
            Ex::Slice(base, start, end) => {
                let (value, t) = self.expr(c, base, None)?;
                let (element, kind) = sequence(&t, at)?;
                if kind == 2 {
                    return Err(at.error("slice exige Array ou Slice"));
                }
                let b = c.temporary();
                let a = c.temporary();
                let z = c.temporary();
                let ia = self.expr(c, start, Some(&ty("i64")))?.0;
                let iz = self.expr(c, end, Some(&ty("i64")))?.0;
                let diff = I::bin(Op::Sub, I::Local(z), I::Local(a));
                let data = if kind == 1 {
                    load(I::Local(b), 8)
                } else {
                    prepend(I::Local(b), 8)
                };
                let addr = I::bin(Op::Add, data, I::bin(Op::Mul, I::Local(a), I::Const(8)));
                let slice = self.record(c, vec![diff, addr]);
                (
                    I::block(
                        vec![
                            S::Set(b, value),
                            S::Set(a, ia),
                            S::Set(z, iz),
                            trap_if(I::bin(Op::Ult, I::Local(z), I::Local(a))),
                            trap_if(I::bin(Op::Ult, load(I::Local(b), 0), I::Local(z))),
                        ],
                        slice,
                    ),
                    Ty::Named("Slice".into(), vec![element]),
                )
            }
            Ex::Closure(args, ret, body) => self.closure(c, args, ret, body, at)?,
            Ex::Match(e, arms) => self.match_expr(c, e, arms, want, at)?,
            Ex::Try(e) => {
                let (v, t) = self.expr(c, e, None)?;
                let (Ty::Named(n, xs), Ty::Named(r, ys)) = (&t, &c.ret) else {
                    return Err(at.error("`?` requer Option ou Result no operando e no retorno"));
                };
                if !(n == r && (n == "Option" || n == "Result" && xs[1] == ys[1])) {
                    return Err(
                        at.error("`?` requer Option ou Result com erro compatível no retorno")
                    );
                }
                let p = c.temporary();
                let fail = if n == "Option" { 0 } else { 1 };
                let test = I::bin(Op::Eq, load(I::Local(p), 0), I::Const(fail));
                (
                    I::block(
                        vec![
                            S::Set(p, v),
                            S::If(test, vec![S::Return(I::Local(p))], vec![]),
                        ],
                        load(I::Local(p), 8),
                    ),
                    xs[0].clone(),
                )
            }
        };
        if let Some(want) = want {
            fits(at, want, &t)?;
        }
        Ok((v, t))
    }
    fn binary(
        &mut self,
        c: &mut Context,
        op: &str,
        a: &Expr,
        b: &Expr,
        want: Option<&Ty>,
    ) -> R<(I, Ty)> {
        if op == "&&" || op == "||" {
            let a = self.expr(c, a, Some(&ty("bool")))?.0;
            let b = self.expr(c, b, Some(&ty("bool")))?.0;
            return Ok((
                if op == "&&" {
                    I::select(a, b, I::Const(0))
                } else {
                    I::select(a, I::Const(1), b)
                },
                ty("bool"),
            ));
        }
        let expected = want.filter(|t| numeric(t));
        let (lhs, rhs, t) = if matches!(a.kind, Ex::Int(_))
            && !matches!(b.kind, Ex::Int(_))
            && expected.is_none()
        {
            let (rhs, t) = self.expr(c, b, None)?;
            let (lhs, _) = self.expr(c, a, Some(&t))?;
            (lhs, rhs, t)
        } else {
            let (lhs, t) = self.expr(c, a, expected)?;
            let (rhs, _) = self.expr(c, b, Some(&t))?;
            (lhs, rhs, t)
        };
        if t.is("str") && matches!(op, "==" | "!=") {
            let id = self.string_eq();
            let v = I::call(id, vec![lhs, rhs]);
            return Ok((
                if op == "==" { v } else { I::unary(Un::Not, v) },
                ty("bool"),
            ));
        }
        let cmp = matches!(op, "==" | "!=" | "<" | ">" | "<=" | ">=");
        if !(numeric(&t) || cmp && t.is("char") || matches!(op, "==" | "!=") && t.is("bool"))
            || op == "%" && t.is("f64")
        {
            return Err(a.span.error(&format!("operador `{op}` não admite `{t}`")));
        }
        let ins = match (op, t.is("f64")) {
            ("+", false) => Op::Add,
            ("-", false) => Op::Sub,
            ("*", false) => Op::Mul,
            ("/", false) => Op::Div,
            ("%", false) => Op::Rem,
            ("==", false) => Op::Eq,
            ("!=", false) => Op::Ne,
            ("<", false) => Op::Lt,
            (">", false) => Op::Gt,
            ("<=", false) => Op::Le,
            (">=", false) => Op::Ge,
            ("+", true) => Op::FAdd,
            ("-", true) => Op::FSub,
            ("*", true) => Op::FMul,
            ("/", true) => Op::FDiv,
            ("==", true) => Op::FEq,
            ("!=", true) => Op::FNe,
            ("<", true) => Op::FLt,
            (">", true) => Op::FGt,
            ("<=", true) => Op::FLe,
            (">=", true) => Op::FGe,
            _ => unreachable!(),
        };
        Ok(if cmp {
            (I::bin(ins, lhs, rhs), ty("bool"))
        } else {
            (wrap(&t, I::bin(ins, lhs, rhs)), t)
        })
    }
    fn args(
        &mut self,
        c: &mut Context,
        gs: &Generics,
        formal: &[Ty],
        actual: &[Expr],
        s: &mut Sub,
        at: &Span,
    ) -> R<Vec<I>> {
        if formal.len() != actual.len() {
            return Err(at.error(&format!(
                "esperados {} argumentos, encontrados {}",
                formal.len(),
                actual.len()
            )));
        }
        let mut code = Vec::with_capacity(actual.len());
        for (t, e) in formal.iter().zip(actual) {
            let expected = concrete(t, gs, s).then(|| subst(t, s));
            let (v, got) = self.expr(c, e, expected.as_ref())?;
            infer(t, &got, gs, s, &e.span)?;
            code.push(v);
        }
        Ok(code)
    }
    fn type_args(&self, c: &Context, gs: &Generics, xs: &[Ty], at: &Span) -> R<Sub> {
        if xs.is_empty() {
            return Ok(Sub::new());
        }
        if xs.len() != gs.len() {
            return Err(at.error("quantidade incorreta de argumentos de tipo"));
        }
        let xs = xs.iter().map(|t| subst(t, &c.sub)).collect::<Vec<_>>();
        for t in &xs {
            self.valid_ty(t, &vec![], at)?;
        }
        Ok(sub(gs, &xs))
    }
    fn inferred(&self, gs: &Generics, s: &Sub, at: &Span) -> R<Vec<Ty>> {
        gs.iter()
            .map(|(n, _)| {
                s.get(n).cloned().ok_or_else(|| {
                    let mut d = at.error(&format!("não foi possível inferir `{n}`"));
                    d.hint = Some("adicione uma anotação ou use `::<Tipo>`".into());
                    d
                })
            })
            .collect()
    }
    fn construct(
        &mut self,
        c: &mut Context,
        n: &str,
        xs: &[Ty],
        es: &[Expr],
        want: Option<&Ty>,
        at: &Span,
    ) -> R<(I, Ty)> {
        let (owner, tag) = self.constructors[n].clone();
        let d = self.data[&owner].clone();
        let mut map = self.type_args(c, &d.generics, xs, at)?;
        let template = Ty::Named(
            owner.clone(),
            d.generics.iter().map(|(n, _)| ty(n)).collect(),
        );
        if let Some(want) = want {
            infer(&template, want, &d.generics, &mut map, at)?;
        }
        let f = match d.data {
            Data::Struct(fs) => fs.into_iter().map(|(_, t)| t).collect(),
            Data::Enum(v) => v[tag.unwrap()].1.clone(),
        };
        let mut args = self.args(c, &d.generics, &f, es, &mut map, at)?;
        let xs = self.inferred(&d.generics, &map, at)?;
        self.bounds(&d.generics, &xs, at)?;
        if let Some(tag) = tag {
            args.insert(0, I::Const(tag as i64));
        }
        Ok((self.record(c, args), Ty::Named(owner, xs)))
    }
    fn call(
        &mut self,
        c: &mut Context,
        f: &Expr,
        types: &[Ty],
        es: &[Expr],
        want: Option<&Ty>,
        at: &Span,
    ) -> R<(I, Ty)> {
        if let Ex::Name(n) = &f.kind {
            if c.lookup(n).is_none() {
                if let Some(id) = self.names.get(n).copied() {
                    let f = self.functions[id].clone();
                    let mut s = self.type_args(c, &f.generics, types, at)?;
                    if let Some(want) = want {
                        infer(&f.ret, want, &f.generics, &mut s, at)?;
                    }
                    let args = self.args(
                        c,
                        &f.generics,
                        &f.args.iter().map(|(_, t)| t.clone()).collect::<Vec<_>>(),
                        es,
                        &mut s,
                        at,
                    )?;
                    let xs = self.inferred(&f.generics, &s, at)?;
                    let idx = self.instance(id, xs, at)?;
                    return Ok((I::call(idx, args), subst(&f.ret, &s)));
                }
                if self.constructors.contains_key(n) {
                    return self.construct(c, n, types, es, want, at);
                }
                if builtin(n) {
                    return self.special(c, n, types, es, want, at);
                }
                if let Some((idx, _)) = ir::HOSTS
                    .iter()
                    .chain(ir::RUNTIME)
                    .enumerate()
                    .find(|(_, (name, _))| *name == n)
                {
                    let (params, ret): (&[&str], &str) = match n.as_str() {
                        "write" => (&["str"], "unit"),
                        "print_float" => (&["f64"], "unit"),
                        "print_char" => (&["char"], "unit"),
                        "print" | "output_byte" | "abort" => (&["i64"], "unit"),
                        "input_len" => (&[], "i64"),
                        "file_open" => (&["i64", "str"], "i64"),
                        "file_byte" => (&["i64", "i64"], "i64"),
                        "store" | "store8" => (&["i64", "i64"], "unit"),
                        _ => (&["i64"], "i64"),
                    };
                    if !types.is_empty() {
                        return Err(at.error("intrínseco não recebe argumentos de tipo"));
                    }
                    let params = params.iter().map(|n| ty(n)).collect::<Vec<_>>();
                    let empty = vec![];
                    return Ok((
                        I::call(
                            idx as u32,
                            self.args(c, &empty, &params, es, &mut Sub::new(), at)?,
                        ),
                        ty(ret),
                    ));
                }
                if let Some(tr) = self.methods.get(n).cloned() {
                    if es.is_empty() || !types.is_empty() {
                        return Err(at.error(
                            "chamada de trait precisa do receptor e não recebe argumentos de tipo",
                        ));
                    }
                    let (receiver, t) = self.expr(c, &es[0], None)?;
                    let choices = self
                        .assignments
                        .get(&(tr.clone(), t.clone()))
                        .ok_or_else(|| at.error(&format!("`{t}` não implementa `{tr}`")))?;
                    let id = choices[n];
                    let f = self.functions[id].clone();
                    let idx = self.instance(id, vec![], at)?;
                    let mut args = vec![receiver];
                    args.extend(
                        self.args(
                            c,
                            &f.generics,
                            &f.args[1..]
                                .iter()
                                .map(|(_, t)| t.clone())
                                .collect::<Vec<_>>(),
                            &es[1..],
                            &mut Sub::new(),
                            at,
                        )?,
                    );
                    return Ok((I::call(idx, args), f.ret));
                }
            }
        }
        let (value, t) = self.expr(c, f, None)?;
        let Ty::Fun(params, ret) = t else {
            return Err(at.error("expressão chamada não é uma função"));
        };
        if !types.is_empty() {
            return Err(at.error("valor função não recebe argumentos de tipo"));
        }
        let args = self.args(c, &vec![], &params, es, &mut Sub::new(), at)?;
        Ok((I::Indirect(Box::new(value), args), *ret))
    }
    fn special(
        &mut self,
        c: &mut Context,
        n: &str,
        types: &[Ty],
        es: &[Expr],
        want: Option<&Ty>,
        at: &Span,
    ) -> R<(I, Ty)> {
        let arity = if n == "array" { 2 } else { 1 };
        if es.len() != arity || (n != "array" && !types.is_empty()) || types.len() > 1 {
            return Err(at.error("argumentos inválidos para intrínseco"));
        }
        if n == "array" {
            let el = types
                .first()
                .map(|t| subst(t, &c.sub))
                .or_else(|| match want {
                    Some(Ty::Named(n, xs)) if n == "Array" => xs.first().cloned(),
                    _ => None,
                });
            if let Some(t) = &el {
                self.valid_ty(t, &vec![], at)?;
            }
            let count = self.expr(c, &es[0], Some(&ty("i64")))?.0;
            let (value, el) = self.expr(c, &es[1], el.as_ref())?;
            let len = c.temporary();
            let fill = c.temporary();
            let p = c.temporary();
            let i = c.temporary();
            let bytes = I::bin(Op::Mul, prepend(I::Local(len), 1), I::Const(8));
            let addr = I::bin(
                Op::Add,
                I::Local(p),
                I::bin(Op::Mul, prepend(I::Local(i), 1), I::Const(8)),
            );
            let init = I::block(
                vec![
                    S::Set(len, count),
                    S::Set(fill, value),
                    trap_if(I::bin(Op::Ult, I::Const(268435454), I::Local(len))),
                    S::Set(p, I::call(ir::ALLOC, vec![bytes])),
                    store(I::Local(p), 0, I::Local(len)),
                    S::Set(i, I::Const(0)),
                    S::Loop(
                        I::bin(Op::Lt, I::Local(i), I::Local(len)),
                        vec![
                            store(addr, 0, I::Local(fill)),
                            S::Set(i, prepend(I::Local(i), 1)),
                        ],
                    ),
                ],
                I::Local(p),
            );
            return Ok((init, Ty::Named("Array".into(), vec![el])));
        }
        let (v, t) = self.expr(c, &es[0], None)?;
        match n {
            "len" => {
                sequence(&t, at)?;
                Ok((load(v, 0), ty("i64")))
            }
            "codepoint" => {
                fits(at, &ty("char"), &t)?;
                Ok((v, ty("i64")))
            }
            "str_from_raw" => {
                fits(at, &ty("i64"), &t)?;
                Ok((v, ty("str")))
            }
            "as_i32" | "as_i64" | "as_f64" if numeric(&t) => {
                let v = match (n, t.is("f64")) {
                    ("as_i32", true) => I::unary(Un::ToI32, I::unary(Un::FToI, v)),
                    ("as_i32", false) => I::unary(Un::ToI32, v),
                    ("as_i64", true) => I::unary(Un::FToI, v),
                    ("as_f64", false) => I::unary(Un::IToF, v),
                    _ => v,
                };
                Ok((v, ty(&n[3..])))
            }
            _ => Err(at.error("tipo inválido para conversão")),
        }
    }
    fn closure(
        &mut self,
        c: &mut Context,
        args: &Fields,
        ret: &Ty,
        body: &Block,
        at: &Span,
    ) -> R<(I, Ty)> {
        let ret = subst(ret, &c.sub);
        self.valid_ty(&ret, &vec![], at)?;
        let mut inner = Context::new(ret.clone(), c.sub.clone());
        inner.temporary();
        let mut params = vec![];
        for (n, t) in args {
            let t = subst(t, &c.sub);
            self.valid_ty(&t, &vec![], at)?;
            inner.bind(n, t.clone(), false, at)?;
            params.push(t);
        }
        inner.scopes.insert(0, HashMap::new());
        let mut captured = HashMap::new();
        for scope in &c.scopes {
            captured.extend(scope.iter().map(|(n, b)| (n.clone(), b.clone())));
        }
        let mut captured = captured
            .into_iter()
            .filter(|(n, _)| inner.lookup(n).is_none())
            .collect::<Vec<_>>();
        captured.sort_by_key(|(_, b)| b.local);
        let mut values = vec![I::Const(0)];
        let mut code = vec![];
        for (i, (n, b)) in captured.into_iter().enumerate() {
            let slot = inner.temporary();
            inner.scopes[0].insert(
                n,
                Binding {
                    local: slot,
                    ty: b.ty,
                    mutable: false,
                },
            );
            code.push(S::Set(slot, load(I::Local(0), (i as i64 + 1) * 8)));
            values.push(I::Local(b.local));
        }
        code.extend(self.function_body(&mut inner, body, at)?);
        let arity = args.len() as u32 + 1;
        let id = self.reserve("<closure>".into(), arity);
        self.module.functions[(id - ir::BASE) as usize] = Func {
            name: "<closure>".into(),
            arity,
            locals: inner.locals,
            body: code,
        };
        values[0] = I::Const(self.module.table.len() as i64);
        self.module.table.push(id);
        Ok((self.record(c, values), Ty::Fun(params, Box::new(ret))))
    }
    fn match_expr(
        &mut self,
        c: &mut Context,
        e: &Expr,
        arms: &[(Pat, Expr)],
        want: Option<&Ty>,
        at: &Span,
    ) -> R<(I, Ty)> {
        let (value, t) = self.expr(c, e, None)?;
        let p = c.temporary();
        let data = if let Ty::Named(n, xs) = &t {
            self.data.get(n).and_then(|d| match &d.data {
                Data::Enum(v) => Some((v.clone(), sub(&d.generics, xs))),
                _ => None,
            })
        } else {
            None
        };
        let mut seen = HashSet::new();
        let mut default = false;
        let mut expected = want.cloned();
        let mut branches = vec![];
        for (pat, arm) in arms {
            if default {
                return Err(arm.span.error("braço de match inalcançável"));
            }
            c.scopes.push(HashMap::new());
            let mut setup = vec![];
            let test = match pat {
                Pat::Any => {
                    default = true;
                    None
                }
                Pat::Bool(b) if t.is("bool") => Some((I::Local(p), i64::from(*b))),
                Pat::Int(n) if t.is("i64") || t.is("i32") && i32::try_from(*n).is_ok() => {
                    Some((I::Local(p), *n))
                }
                Pat::Variant(n, xs) => {
                    let (vs, sub) = data
                        .as_ref()
                        .ok_or_else(|| arm.span.error("padrão de variante exige enum"))?;
                    let (i, (_, fields)) = vs
                        .iter()
                        .enumerate()
                        .find(|(_, (v, _))| v == n)
                        .ok_or_else(|| arm.span.error("variante não pertence ao enum"))?;
                    if fields.len() != xs.len() {
                        return Err(arm.span.error("número incorreto de campos no padrão"));
                    }
                    for (i, (n, t)) in xs.iter().zip(fields).enumerate() {
                        if n != "_" {
                            let id = c.bind(n, subst(t, sub), false, &arm.span)?;
                            setup.push(S::Set(id, load(I::Local(p), (i as i64 + 1) * 8)));
                        }
                    }
                    Some((load(I::Local(p), 0), i as i64))
                }
                _ => return Err(arm.span.error("padrão incompatível com a expressão")),
            };
            if let Some((_, n)) = &test {
                if !seen.insert(*n) {
                    return Err(arm.span.error("padrão duplicado"));
                }
            }
            let (v, rt) = self.expr(c, arm, expected.as_ref())?;
            expected = Some(rt);
            c.scopes.pop();
            branches.push((
                test,
                if setup.is_empty() {
                    v
                } else {
                    I::block(setup, v)
                },
            ));
        }
        let total = if let Some((vs, _)) = &data {
            vs.len()
        } else if t.is("bool") {
            2
        } else {
            usize::MAX
        };
        if !default && seen.len() != total {
            let mut d = at.error("match não exaustivo");
            d.hint = Some(if let Some((vs, _)) = data {
                format!(
                    "adicione as variantes {}",
                    vs.iter()
                        .enumerate()
                        .filter(|(i, _)| !seen.contains(&(*i as i64)))
                        .map(|(_, (n, _))| n.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            } else {
                "adicione um braço `_`".into()
            });
            return Err(d);
        }
        let mut tail = I::Trap;
        for (test, value) in branches.into_iter().rev() {
            tail = if let Some((lhs, n)) = test {
                I::select(I::bin(Op::Eq, lhs, I::Const(n)), value, tail)
            } else {
                value
            };
        }
        Ok((
            I::block(vec![S::Set(p, value)], tail),
            expected.expect("match exaustivo tem um braço"),
        ))
    }
    fn string_eq(&mut self) -> u32 {
        if let Some(id) = self.str_eq {
            return id;
        }
        let id = self.reserve("<str.eq>".into(), 2);
        self.str_eq = Some(id);
        let char_at = |p| {
            I::call(
                ir::LOAD + 2,
                vec![I::bin(Op::Add, prepend(I::Local(p), 8), I::Local(2))],
            )
        };
        self.module.functions[(id - ir::BASE) as usize] = Func {
            name: "<str.eq>".into(),
            arity: 2,
            locals: 4,
            body: vec![
                S::Set(3, load(I::Local(0), 0)),
                S::If(
                    I::bin(Op::Ne, I::Local(3), load(I::Local(1), 0)),
                    vec![S::Return(I::Const(0))],
                    vec![],
                ),
                S::Set(2, I::Const(0)),
                S::Loop(
                    I::bin(Op::Lt, I::Local(2), I::Local(3)),
                    vec![
                        S::If(
                            I::bin(Op::Ne, char_at(0), char_at(1)),
                            vec![S::Return(I::Const(0))],
                            vec![],
                        ),
                        S::Set(2, prepend(I::Local(2), 1)),
                    ],
                ),
                S::Return(I::Const(1)),
            ],
        };
        id
    }
}
fn type_depth(t: &Ty) -> usize {
    match t {
        Ty::Named(_, xs) => 1 + xs.iter().map(type_depth).max().unwrap_or(0),
        Ty::Fun(xs, r) => 1 + type_depth(r).max(xs.iter().map(type_depth).max().unwrap_or(0)),
    }
}
fn sequence(t: &Ty, at: &Span) -> R<(Ty, u8)> {
    match t {
        Ty::Named(n, ts) if n == "Array" || n == "Slice" => {
            Ok((ts[0].clone(), u8::from(n == "Slice")))
        }
        t if t.is("str") => Ok((ty("i64"), 2)),
        _ => Err(at.error("operação exige Array, Slice ou str")),
    }
}
fn near(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() == b.len() {
        return a.iter().zip(b).filter(|(x, y)| x != y).count() == 1;
    }
    let (x, y) = if a.len() < b.len() { (a, b) } else { (b, a) };
    if x.len() + 1 != y.len() {
        return false;
    }
    let i = x.iter().zip(y).position(|(a, b)| a != b).unwrap_or(x.len());
    x[i..] == y[i + 1..]
}
fn returns(b: &Block) -> bool {
    b.iter().any(|s| match &s.kind {
        St::Return(_) => true,
        St::If(_, a, b) => returns(a) && returns(b),
        _ => false,
    })
}
