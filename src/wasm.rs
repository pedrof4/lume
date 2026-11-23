use crate::ir::{self, Expr, Func, Module, Op, Stmt, Un};

pub fn uleb(out: &mut Vec<u8>, mut x: u32) {
    loop {
        let byte = (x & 127) as u8;
        x >>= 7;
        out.push(byte | if x != 0 { 128 } else { 0 });
        if x == 0 {
            break;
        }
    }
}
pub fn sleb(out: &mut Vec<u8>, mut x: i64) {
    loop {
        let byte = (x & 127) as u8;
        x >>= 7;
        let done = (x == 0 && byte & 64 == 0) || (x == -1 && byte & 64 != 0);
        out.push(byte | if done { 0 } else { 128 });
        if done {
            break;
        }
    }
}
fn name(out: &mut Vec<u8>, s: &str) {
    uleb(out, s.len() as u32);
    out.extend(s.bytes());
}
fn section(out: &mut Vec<u8>, id: u8, data: Vec<u8>) {
    out.push(id);
    uleb(out, data.len() as u32);
    out.extend(data);
}
fn instruction(out: &mut Vec<u8>, op: u8, value: u32) {
    out.push(op);
    uleb(out, value);
}
fn iconst(out: &mut Vec<u8>, val: i64) {
    out.push(0x42);
    sleb(out, val);
}
fn check_ir(module: &Module, arities: &[u32]) -> Result<(), String> {
    fn expr(e: &Expr, f: &Func, arities: &[u32], depth: u32) -> bool {
        match e {
            Expr::Local(id) => *id < f.locals,
            Expr::Bin(_, a, b) => expr(a, f, arities, depth) && expr(b, f, arities, depth),
            Expr::Unary(_, e) => expr(e, f, arities, depth),
            Expr::Call(id, args) | Expr::Memory(id, args) => {
                arities.get(*id as usize) == Some(&(args.len() as u32))
                    && args.iter().all(|e| expr(e, f, arities, depth))
            }
            Expr::Indirect(c, args) => {
                expr(c, f, arities, depth) && args.iter().all(|e| expr(e, f, arities, depth))
            }
            Expr::If(c, a, b) => [c, a, b].iter().all(|e| expr(e, f, arities, depth)),
            Expr::Block(ss, e) => block(ss, f, arities, depth) && expr(e, f, arities, depth),
            Expr::Const(_) | Expr::Trap => true,
        }
    }
    fn block(ss: &[Stmt], f: &Func, arities: &[u32], depth: u32) -> bool {
        ss.iter().all(|s| match s {
            Stmt::Set(id, e) => *id < f.locals && expr(e, f, arities, depth),
            Stmt::Eval(e) | Stmt::Return(e) => expr(e, f, arities, depth),
            Stmt::If(c, a, b) => {
                expr(c, f, arities, depth)
                    && block(a, f, arities, depth)
                    && block(b, f, arities, depth)
            }
            Stmt::Loop(c, b) => expr(c, f, arities, depth) && block(b, f, arities, depth + 1),
            Stmt::Break | Stmt::Continue => depth > 0,
        })
    }
    if arities.get(module.entry as usize) != Some(&0)
        || module
            .table
            .iter()
            .any(|id| arities.get(*id as usize).is_none_or(|a| *a == 0))
    {
        return Err("export ou tabela inválida".into());
    }
    for f in &module.functions {
        if f.arity > f.locals || !block(&f.body, f, arities, 0) {
            return Err(format!("IR inválida em {}", f.name));
        }
    }
    Ok(())
}

fn emit_expr(out: &mut Vec<u8>, e: &Expr, breaks: &[u32], scratch: u32) {
    match e {
        Expr::Const(n) => iconst(out, *n),
        Expr::Local(l) => instruction(out, 0x20, *l),
        Expr::Bin(op, a, b) => {
            let (opcode, float, comparison) = match op {
                Op::Add => (0x7c, false, false),
                Op::Sub => (0x7d, false, false),
                Op::Mul => (0x7e, false, false),
                Op::Div => (0x7f, false, false),
                Op::Rem => (0x81, false, false),
                Op::Eq => (0x51, false, true),
                Op::Ne => (0x52, false, true),
                Op::Lt => (0x53, false, true),
                Op::Gt => (0x55, false, true),
                Op::Le => (0x57, false, true),
                Op::Ge => (0x59, false, true),
                Op::Ult => (0x54, false, true),
                Op::Ule => (0x58, false, true),
                Op::FAdd => (0xa0, true, false),
                Op::FSub => (0xa1, true, false),
                Op::FMul => (0xa2, true, false),
                Op::FDiv => (0xa3, true, false),
                Op::FEq => (0x61, true, true),
                Op::FNe => (0x62, true, true),
                Op::FLt => (0x63, true, true),
                Op::FGt => (0x64, true, true),
                Op::FLe => (0x65, true, true),
                Op::FGe => (0x66, true, true),
            };
            emit_expr(out, a, breaks, scratch);
            if float {
                out.push(0xbf);
            }
            emit_expr(out, b, breaks, scratch);
            if float {
                out.push(0xbf);
            }
            out.push(opcode);
            if comparison {
                out.push(0xad);
            } else if float {
                out.push(0xbd);
            }
        }
        Expr::Unary(op, a) => {
            if *op == Un::Neg {
                iconst(out, 0);
            }
            emit_expr(out, a, breaks, scratch);
            let ops: &[u8] = match op {
                Un::Neg => &[0x7d],
                Un::Not => &[0x50, 0xad],
                Un::ToI32 => &[0xa7, 0xac],
                Un::FNeg => &[0xbf, 0x9a, 0xbd],
                Un::IToF => &[0xb9, 0xbd],
                Un::FToI => &[0xbf, 0xb0],
            };
            out.extend(ops);
        }
        Expr::Call(id, args) => {
            for a in args {
                emit_expr(out, a, breaks, scratch);
            }
            instruction(out, 0x10, *id);
        }
        Expr::Memory(id, args) => {
            let address = &args[0];
            let (base, offset) = match address {
                Expr::Bin(Op::Add, base, n) if matches!(**n, Expr::Const(_)) => {
                    let Expr::Const(n) = **n else { unreachable!() };
                    // At address overflow, i32.wrap and a memory offset differ. Keep
                    // the wrapping form unless the base is a proven arena pointer.
                    if matches!(**base, Expr::Call(ir::ALLOC, _)) && (0..=65535).contains(&n) {
                        (&**base, n as u32)
                    } else {
                        (address, 0)
                    }
                }
                _ => (address, 0),
            };
            emit_expr(out, base, breaks, scratch);
            out.push(0xa7);
            if let Some(value) = args.get(1) {
                emit_expr(out, value, breaks, scratch);
            }
            let (opcode, align) = match id - ir::LOAD {
                0 => (0x29, 3),
                1 => (0x37, 3),
                2 => (0x31, 0),
                3 => (0x3c, 0),
                _ => unreachable!(),
            };
            out.extend([opcode, align]);
            uleb(out, offset);
            if args.len() == 2 {
                iconst(out, 0);
            }
        }
        Expr::Indirect(closure, args) => {
            emit_expr(out, closure, breaks, scratch);
            instruction(out, 0x21, scratch);
            instruction(out, 0x20, scratch);
            for arg in args {
                emit_expr(out, arg, breaks, scratch + 1);
            }
            instruction(out, 0x20, scratch);
            out.extend([0xa7, 0x28, 2, 0]);
            instruction(out, 0x11, args.len() as u32 + 1);
            out.push(0);
        }
        Expr::If(c, a, b) => {
            emit_expr(out, c, breaks, scratch);
            out.extend([0xa7, 0x04, 0x7e]);
            let nested: Vec<_> = breaks.iter().map(|d| d + 1).collect();
            emit_expr(out, a, &nested, scratch);
            out.push(0x05);
            emit_expr(out, b, &nested, scratch);
            out.push(0x0b);
        }
        Expr::Block(stmts, e) => {
            emit_block(out, stmts, breaks, scratch);
            emit_expr(out, e, breaks, scratch);
        }
        Expr::Trap => out.push(0),
    }
}
fn emit_block(out: &mut Vec<u8>, ss: &[Stmt], labels: &[u32], scratch: u32) {
    for s in ss {
        match s {
            Stmt::Set(l, e) => {
                emit_expr(out, e, labels, scratch);
                instruction(out, 0x21, *l);
            }
            Stmt::Eval(e) => {
                emit_expr(out, e, labels, scratch);
                out.push(0x1a);
            }
            Stmt::Return(e) => {
                emit_expr(out, e, labels, scratch);
                out.push(0x0f);
            }
            Stmt::If(c, a, b) => {
                emit_expr(out, c, labels, scratch);
                out.extend([0xa7, 0x04, 0x40]);
                let nested: Vec<_> = labels.iter().map(|d| d + 1).collect();
                emit_block(out, a, &nested, scratch);
                if !b.is_empty() {
                    out.push(0x05);
                    emit_block(out, b, &nested, scratch);
                }
                out.push(0x0b);
            }
            Stmt::Loop(c, b) => {
                out.extend([0x02, 0x40, 0x03, 0x40]);
                let depth = [0];
                emit_expr(out, c, &depth, scratch);
                out.extend([0x50, 0x0d, 1]);
                emit_block(out, b, &depth, scratch);
                out.extend([0x0c, 0, 0x0b, 0x0b]);
            }
            Stmt::Break => instruction(out, 0x0c, labels[0] + 1),
            Stmt::Continue => instruction(out, 0x0c, labels[0]),
        }
    }
}

fn scratch_count(ss: &[Stmt]) -> u32 {
    fn expr(e: &Expr) -> u32 {
        match e {
            Expr::Bin(_, a, b) => expr(a).max(expr(b)),
            Expr::Unary(_, a) => expr(a),
            Expr::Call(_, args) | Expr::Memory(_, args) => args.iter().map(expr).max().unwrap_or(0),
            Expr::Indirect(c, args) => expr(c).max(1 + args.iter().map(expr).max().unwrap_or(0)),
            Expr::If(c, a, b) => expr(c).max(expr(a)).max(expr(b)),
            Expr::Block(ss, e) => scratch_count(ss).max(expr(e)),
            _ => 0,
        }
    }
    ss.iter()
        .map(|s| match s {
            Stmt::Set(_, e) | Stmt::Eval(e) | Stmt::Return(e) => expr(e),
            Stmt::If(c, a, b) => expr(c).max(scratch_count(a)).max(scratch_count(b)),
            Stmt::Loop(c, b) => expr(c).max(scratch_count(b)),
            _ => 0,
        })
        .max()
        .unwrap_or(0)
}
fn append_body(section: &mut Vec<u8>, body: &[u8]) {
    uleb(section, body.len() as u32);
    section.extend(body);
}

// End-exclusive bump pointer. Reject negative sizes and wrap before growing memory.
pub fn runtime_bodies() -> [&'static [u8]; 5] {
    [
        &[
            1, 2, 0x7e, 0x23, 0, 0x21, 1, 0x20, 0, 0x42, 0, 0x53, 0x20, 0, 0x42, 0x80, 0x80, 0x80,
            0x80, 0x08, 0x54, 0x45, 0x72, 0x04, 0x40, 0, 0x0b, 0x20, 1, 0x20, 0, 0x7c, 0x42, 7,
            0x7c, 0x42, 0x78, 0x83, 0x22, 2, 0x42, 0x80, 0x80, 0x80, 0x80, 0x08, 0x54, 0x45, 0x04,
            0x40, 0, 0x0b, 0x20, 2, 0x42, 0xff, 0xff, 3, 0x7c, 0x42, 16, 0x88, 0xa7, 0x3f, 0, 0x6b,
            0xac, 0x22, 0, 0x42, 0, 0x55, 0x04, 0x40, 0x20, 0, 0xa7, 0x40, 0, 0x41, 0x7f, 0x46,
            0x04, 0x40, 0, 0x0b, 0x0b, 0x20, 2, 0x24, 0, 0x20, 1, 0x0b,
        ],
        &[0, 0x20, 0, 0xa7, 0x29, 3, 0, 0x0b],
        &[0, 0x20, 0, 0xa7, 0x20, 1, 0x37, 3, 0, 0x42, 0, 0x0b],
        &[0, 0x20, 0, 0xa7, 0x31, 0, 0, 0x0b],
        &[0, 0x20, 0, 0xa7, 0x20, 1, 0x3c, 0, 0, 0x42, 0, 0x0b],
    ]
}

pub fn emit(module: &Module) -> Result<Vec<u8>, String> {
    let arities: Vec<_> = ir::HOSTS
        .iter()
        .chain(ir::RUNTIME)
        .map(|(_, a)| *a)
        .chain(module.functions.iter().map(|f| f.arity))
        .collect();
    check_ir(module, &arities)?;
    fn indirect_arity(ss: &[Stmt]) -> u32 {
        fn e(x: &Expr) -> u32 {
            match x {
                Expr::Bin(_, a, b) => e(a).max(e(b)),
                Expr::Unary(_, a) => e(a),
                Expr::If(c, a, b) => e(c).max(e(a)).max(e(b)),
                Expr::Block(ss, x) => indirect_arity(ss).max(e(x)),
                Expr::Call(_, xs) | Expr::Memory(_, xs) => xs.iter().map(e).max().unwrap_or(0),
                Expr::Indirect(c, xs) => (xs.len() as u32 + 1)
                    .max(e(c))
                    .max(xs.iter().map(e).max().unwrap_or(0)),
                _ => 0,
            }
        }
        ss.iter()
            .map(|s| match s {
                Stmt::Set(_, x) | Stmt::Eval(x) | Stmt::Return(x) => e(x),
                Stmt::If(c, a, b) => e(c).max(indirect_arity(a)).max(indirect_arity(b)),
                Stmt::Loop(c, b) => e(c).max(indirect_arity(b)),
                _ => 0,
            })
            .max()
            .unwrap_or(0)
    }
    let max_arity = arities.iter().copied().max().unwrap_or(0).max(
        module
            .functions
            .iter()
            .map(|f| indirect_arity(&f.body))
            .max()
            .unwrap_or(0),
    );
    let mut out = b"\0asm\x01\0\0\0".to_vec();
    let mut types = Vec::new();
    uleb(&mut types, max_arity + 1);
    for arity in 0..=max_arity {
        types.push(0x60);
        uleb(&mut types, arity);
        types.extend(std::iter::repeat_n(0x7e, arity as usize));
        types.extend([1, 0x7e]);
    }
    section(&mut out, 1, types);
    let mut imports = vec![];
    uleb(&mut imports, ir::HOSTS.len() as u32);
    for (n, a) in ir::HOSTS {
        name(&mut imports, "lume");
        name(&mut imports, n);
        imports.push(0);
        uleb(&mut imports, *a);
    }
    section(&mut out, 2, imports);
    let mut funcs = vec![];
    uleb(&mut funcs, arities.len() as u32 - ir::HOSTS.len() as u32);
    for a in &arities[ir::HOSTS.len()..] {
        uleb(&mut funcs, *a);
    }
    section(&mut out, 3, funcs);
    let mut table = vec![1, 0x70, 0];
    uleb(&mut table, module.table.len() as u32);
    section(&mut out, 4, table);
    let heap = ((1024 + module.strings.len() as u32 + 7) & !7).max(4096);
    let mut memory = vec![1, 0];
    uleb(&mut memory, heap.div_ceil(65536));
    section(&mut out, 5, memory);
    let mut global = vec![1, 0x7e, 1];
    iconst(&mut global, heap.into());
    global.push(0x0b);
    section(&mut out, 6, global);
    let mut exports = vec![2];
    name(&mut exports, "memory");
    exports.extend([2, 0]);
    name(&mut exports, "main");
    exports.push(0);
    uleb(&mut exports, module.entry);
    section(&mut out, 7, exports);
    let mut elems = vec![1, 0, 0x41, 0, 0x0b];
    uleb(&mut elems, module.table.len() as u32);
    for id in &module.table {
        uleb(&mut elems, *id);
    }
    section(&mut out, 9, elems);
    let mut code = vec![];
    uleb(
        &mut code,
        module.functions.len() as u32 + ir::RUNTIME.len() as u32,
    );
    for body in runtime_bodies() {
        append_body(&mut code, body);
    }
    for f in &module.functions {
        let mut body = vec![];
        let locals = f.locals - f.arity + scratch_count(&f.body);
        if locals == 0 {
            body.push(0);
        } else {
            body.push(1);
            uleb(&mut body, locals);
            body.push(0x7e);
        }
        emit_block(&mut body, &f.body, &[], f.locals);
        body.extend([0, 0x0b]);
        append_body(&mut code, &body);
    }
    section(&mut out, 10, code);
    let mut data = vec![1, 0, 0x41];
    sleb(&mut data, 1024);
    data.push(0x0b);
    uleb(&mut data, module.strings.len() as u32);
    data.extend(&module.strings);
    section(&mut out, 11, data);
    validate(&out, &arities, max_arity)?;
    Ok(out)
}

fn read_u32(bytes: &[u8], at: &mut usize) -> Result<u32, String> {
    let mut value = 0;
    for shift in (0..35).step_by(7) {
        let b = *bytes.get(*at).ok_or("LEB128 truncado")?;
        *at += 1;
        if shift == 28 && b > 15 {
            return Err("LEB128 fora do intervalo".into());
        }
        value |= u32::from(b & 127) << shift;
        if b < 128 {
            return Ok(value);
        }
    }
    Err("LEB128 inválido".into())
}
fn validate(wasm: &[u8], arities: &[u32], max_arity: u32) -> Result<(), String> {
    let functions = (arities.len() - ir::HOSTS.len()) as u32;
    if !wasm.starts_with(b"\0asm\x01\0\0\0") {
        return Err("cabeçalho WASM inválido".into());
    }
    let mut at = 8;
    for expected in [1, 2, 3, 4, 5, 6, 7, 9, 10, 11] {
        if wasm.get(at) != Some(&expected) {
            return Err(format!("seção WASM {expected} ausente"));
        }
        at += 1;
        let len = read_u32(wasm, &mut at)? as usize;
        let end = at
            .checked_add(len)
            .filter(|n| *n <= wasm.len())
            .ok_or("seção WASM truncada")?;
        if expected == 3 || expected == 10 {
            let mut body = at;
            if read_u32(wasm, &mut body)? != functions {
                return Err("contagem de funções diverge do código".into());
            }
            for idx in ir::HOSTS.len()..arities.len() {
                let n = read_u32(wasm, &mut body)? as usize;
                if expected == 3 && n != arities[idx] as usize {
                    return Err(format!("assinatura de função {idx} inválida"));
                }
                if expected == 10 {
                    let start = body;
                    body = body
                        .checked_add(n)
                        .filter(|n| *n <= end)
                        .ok_or("corpo WASM truncado")?;
                    validate_body(&wasm[start..body], arities[idx], arities, max_arity)
                        .map_err(|e| format!("função WASM {idx}: {e}"))?;
                }
            }
            if body != end {
                return Err("seção de funções com bytes excedentes".into());
            }
        }
        at = end;
    }
    if at != wasm.len() {
        return Err("seções WASM excedentes".into());
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Value {
    I32,
    I64,
    F64,
}

struct Frame {
    opcode: u8,
    base: usize,
    result: Option<Value>,
    unreachable: bool,
    has_else: bool,
}

struct Validator<'a> {
    bytes: &'a [u8],
    at: usize,
    locals: Vec<Value>,
    stack: Vec<Value>,
    frames: Vec<Frame>,
}
impl Validator<'_> {
    fn byte(&mut self) -> Result<u8, &'static str> {
        let b = *self.bytes.get(self.at).ok_or("instrução truncada")?;
        self.at += 1;
        Ok(b)
    }

    fn index(&mut self) -> Result<usize, &'static str> {
        read_u32(self.bytes, &mut self.at)
            .map(|n| n as usize)
            .map_err(|_| "índice LEB128 inválido")
    }

    fn signed(&mut self, bits: u8) -> Result<(), &'static str> {
        let mut remaining = bits;
        loop {
            let b = self.byte()?;
            if remaining <= 7 {
                let mask = 127 ^ ((1 << (remaining - 1)) - 1);
                if b < 128 && (b & mask == 0 || b & mask == mask) {
                    return Ok(());
                }
                return Err("imediato assinado excede o tipo");
            }
            if b < 128 {
                return Ok(());
            }
            remaining -= 7;
        }
    }

    fn pop(&mut self, expected: Option<Value>) -> Result<(), &'static str> {
        let frame = self.frames.last().ok_or("pilha de rótulos vazia")?;
        if self.stack.len() == frame.base {
            return if frame.unreachable {
                Ok(())
            } else {
                Err("faltam operandos na pilha")
            };
        }
        let actual = self.stack.pop().ok_or("faltam operandos na pilha")?;
        if expected.is_some_and(|t| actual != t) {
            return Err("tipo incorreto na pilha");
        }
        Ok(())
    }

    fn finish_arm(&mut self) -> Result<(), &'static str> {
        let frame = self.frames.last().unwrap();
        let base = frame.base;
        if let Some(result) = frame.result {
            self.pop(Some(result))?;
        }
        if self.stack.len() != base {
            return Err("operandos excedentes no fim do bloco");
        }
        Ok(())
    }

    fn unreachable(&mut self) {
        let frame = self.frames.last_mut().unwrap();
        self.stack.truncate(frame.base);
        frame.unreachable = true;
    }

    fn unary(&mut self, from: Value, to: Value) -> Result<(), &'static str> {
        self.pop(Some(from))?;
        self.stack.push(to);
        Ok(())
    }

    fn binary(&mut self, from: Value, to: Value) -> Result<(), &'static str> {
        self.pop(Some(from))?;
        self.unary(from, to)
    }

    fn memory(
        &mut self,
        max_alignment: usize,
        value: Value,
        store: bool,
    ) -> Result<(), &'static str> {
        if self.index()? > max_alignment {
            return Err("alinhamento de memória inválido");
        }
        self.index()?; // Offset.
        if store {
            self.pop(Some(value))?;
            self.pop(Some(Value::I32))?;
        } else {
            self.unary(Value::I32, value)?;
        }
        Ok(())
    }

    fn call(&mut self, arity: u32) -> Result<(), &'static str> {
        for _ in 0..arity {
            self.pop(Some(Value::I64))?;
        }
        self.stack.push(Value::I64);
        Ok(())
    }
}

fn validate_body(
    bytes: &[u8],
    arity: u32,
    signatures: &[u32],
    max_arity: u32,
) -> Result<(), &'static str> {
    use Value::{F64, I32, I64};
    let mut v = Validator {
        bytes,
        at: 0,
        locals: vec![I64; arity as usize],
        stack: Vec::new(),
        frames: vec![Frame {
            opcode: 0xff,
            base: 0,
            result: Some(I64),
            unreachable: false,
            has_else: false,
        }],
    };
    let groups = v.index()?;
    for _ in 0..groups {
        let n = v.index()?;
        let t = match v.byte()? {
            0x7e => I64,
            _ => return Err("tipo de local inesperado"),
        };
        let total = v
            .locals
            .len()
            .checked_add(n)
            .filter(|n| *n <= 1_000_000)
            .ok_or("limite de locais excedido")?;
        v.locals.resize(total, t);
    }
    while !v.frames.is_empty() {
        let opcode = v.byte()?;
        match opcode {
            0x00 => v.unreachable(),
            0x02..=0x04 => {
                let result = match v.byte()? {
                    0x40 => None,
                    0x7e => Some(I64),
                    _ => return Err("tipo de bloco inesperado"),
                };
                if opcode == 0x04 {
                    v.pop(Some(I32))?;
                }
                v.frames.push(Frame {
                    opcode,
                    base: v.stack.len(),
                    result,
                    unreachable: false,
                    has_else: false,
                });
            }
            0x05 => {
                v.finish_arm()?;
                let frame = v.frames.last_mut().unwrap();
                if frame.opcode != 0x04 || frame.has_else {
                    return Err("else fora de if");
                }
                frame.has_else = true;
                frame.unreachable = false;
            }
            0x0b => {
                v.finish_arm()?;
                let frame = v.frames.pop().unwrap();
                if frame.opcode == 0x04 && frame.result.is_some() && !frame.has_else {
                    return Err("if com resultado sem else");
                }
                v.stack.extend(frame.result);
            }
            0x0c | 0x0d => {
                let depth = v.index()?;
                let target = v
                    .frames
                    .iter()
                    .rev()
                    .nth(depth)
                    .ok_or("rótulo fora do escopo")?;
                let result = target.result.filter(|_| target.opcode != 0x03);
                if opcode == 0x0d {
                    v.pop(Some(I32))?;
                }
                if let Some(t) = result {
                    v.pop(Some(t))?;
                }
                if opcode == 0x0c {
                    v.unreachable();
                } else {
                    v.stack.extend(result);
                }
            }
            0x0f => {
                v.pop(Some(I64))?;
                v.unreachable();
            }
            0x10 => {
                let index = v.index()?;
                let arity = *signatures.get(index).ok_or("índice de função inválido")?;
                v.call(arity)?;
            }
            0x11 => {
                let arity = v.index()?;
                if arity > max_arity as usize || v.index()? != 0 {
                    return Err("tipo ou tabela inválida em call_indirect");
                }
                v.pop(Some(I32))?;
                v.call(arity as u32)?;
            }
            0x1a => v.pop(None)?,
            0x20..=0x22 => {
                let index = v.index()?;
                let t = *v.locals.get(index).ok_or("índice de local inválido")?;
                if opcode != 0x20 {
                    v.pop(Some(t))?;
                }
                if opcode != 0x21 {
                    v.stack.push(t);
                }
            }
            0x23 | 0x24 => {
                if v.index()? != 0 {
                    return Err("índice global inválido");
                }
                if opcode == 0x23 {
                    v.stack.push(I64);
                } else {
                    v.pop(Some(I64))?;
                }
            }
            0x28 => v.memory(2, I32, false)?,
            0x29 => v.memory(3, I64, false)?,
            0x31 => v.memory(0, I64, false)?,
            0x37 => v.memory(3, I64, true)?,
            0x3c => v.memory(0, I64, true)?,
            0x3f | 0x40 => {
                if v.index()? != 0 {
                    return Err("índice de memória inválido");
                }
                if opcode == 0x40 {
                    v.pop(Some(I32))?;
                }
                v.stack.push(I32);
            }
            0x41 => {
                v.signed(32)?;
                v.stack.push(I32);
            }
            0x42 => {
                v.signed(64)?;
                v.stack.push(I64);
            }
            0x45 => v.unary(I32, I32)?,
            0x46 => v.binary(I32, I32)?,
            0x50 => v.unary(I64, I32)?,
            0x51..=0x5a => v.binary(I64, I32)?,
            0x61..=0x66 => v.binary(F64, I32)?,
            0x6b | 0x72 => v.binary(I32, I32)?,
            0x7c..=0x88 => v.binary(I64, I64)?,
            0x9a => v.unary(F64, F64)?,
            0xa0..=0xa3 => v.binary(F64, F64)?,
            0xa7 => v.unary(I64, I32)?,
            0xac | 0xad => v.unary(I32, I64)?,
            0xb0 | 0xbd => v.unary(F64, I64)?,
            0xb9 | 0xbf => v.unary(I64, F64)?,
            _ => return Err("opcode fora do subconjunto emitido"),
        }
    }
    if v.at != bytes.len() {
        return Err("bytes excedentes após end da função");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_body;

    #[test]
    fn verifies_body_with_i64_result() {
        assert!(validate_body(&[0, 0x42, 3, 0x0b], 0, &[], 0).is_ok());
    }
    #[test]
    fn rejects_body_without_result() {
        assert_eq!(
            validate_body(&[0, 0x0b], 0, &[], 0),
            Err("faltam operandos na pilha")
        );
    }
    #[test]
    fn rejects_result_of_wrong_wasm_type() {
        assert_eq!(
            validate_body(&[0, 0x41, 0, 0x0b], 0, &[], 0),
            Err("tipo incorreto na pilha")
        );
    }
    #[test]
    fn rejects_invalid_local_index() {
        assert_eq!(
            validate_body(&[0, 0x20, 1, 0x0b], 1, &[], 0),
            Err("índice de local inválido")
        );
    }
    #[test]
    fn rejects_jump_outside_function() {
        assert_eq!(
            validate_body(&[0, 0x0c, 1, 0x0b], 0, &[], 0),
            Err("rótulo fora do escopo")
        );
    }
    #[test]
    fn rejects_call_with_missing_arg() {
        assert_eq!(
            validate_body(&[0, 0x10, 0, 0x0b], 0, &[1], 1),
            Err("faltam operandos na pilha")
        );
    }
    #[test]
    fn validates_polymorphic_stack_after_unreachable() {
        assert!(validate_body(&[0, 0, 0x7c, 0x0b], 0, &[], 0).is_ok());
    }
    #[test]
    fn rejects_extra_bytes_after_body() {
        assert_eq!(
            validate_body(&[0, 0x42, 0, 0x0b, 0], 0, &[], 0),
            Err("bytes excedentes após end da função")
        );
    }
}
