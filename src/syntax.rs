use std::{fmt, rc::Rc};

#[derive(Clone, Debug)]
pub struct Source(Rc<(String, String)>);
impl Source {
    pub fn new(name: &str, text: &str) -> Self {
        Self(Rc::new((name.into(), text.into())))
    }
    pub fn span(&self, start: usize, end: usize) -> Span {
        Span {
            source: self.clone(),
            start,
            end,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Span {
    pub source: Source,
    pub start: usize,
    pub end: usize,
}
impl Span {
    pub fn error(&self, message: &str) -> Diagnostic {
        Diagnostic {
            span: self.clone(),
            message: message.into(),
            hint: None,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub span: Span,
    pub message: String,
    pub hint: Option<String>,
}
impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (name, code) = &*self.span.source.0;
        let prefix = &code[..self.span.start];
        let start = prefix.rfind('\n').map_or(0, |at| at + 1);
        let end = code[self.span.start..]
            .find('\n')
            .map_or(code.len(), |at| self.span.start + at);
        let line = prefix.bytes().filter(|&b| b == b'\n').count() + 1;
        let column = code[start..self.span.start].chars().count() + 1;
        let width = code[self.span.start..end.min(self.span.end)]
            .chars()
            .count()
            .max(1);
        write!(
            f,
            "{name}:{line}:{column}: erro: {}\n  |\n{line:>2} | {}\n  | {}{}",
            self.message,
            &code[start..end],
            " ".repeat(column - 1),
            "^".repeat(width)
        )?;
        if let Some(hint) = &self.hint {
            write!(f, "\n  = sugestão: {hint}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Name(String),
    Int(i64),
    Float(f64),
    Char(char),
    Str(String),
    Sym(&'static str),
    Eof,
}
#[derive(Clone, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

pub fn lex(source: &Source) -> (Vec<Token>, Vec<Diagnostic>) {
    let text = &source.0 .1;
    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let start = at;
        let ch = text[at..].chars().next().unwrap();
        at += ch.len_utf8();
        let kind = if ch.is_whitespace() {
            continue;
        } else if text[start..].starts_with("//") {
            at = text[start..].find('\n').map_or(text.len(), |n| start + n);
            continue;
        } else if ch == '_' || ch.is_alphabetic() {
            while let Some(c) = text[at..]
                .chars()
                .next()
                .filter(|c| *c == '_' || c.is_alphanumeric())
            {
                at += c.len_utf8();
            }
            TokenKind::Name(text[start..at].into())
        } else if ch.is_ascii_digit() {
            while matches!(text.as_bytes().get(at), Some(b'0'..=b'9' | b'_')) {
                at += 1;
            }
            let float = text.as_bytes().get(at) == Some(&b'.')
                && matches!(text.as_bytes().get(at + 1), Some(b'0'..=b'9'));
            if float {
                at += 1;
                while matches!(text.as_bytes().get(at), Some(b'0'..=b'9' | b'_')) {
                    at += 1;
                }
            }
            let value = text[start..at].replace('_', "");
            let result = if float {
                value.parse().map(TokenKind::Float).map_err(|_| ())
            } else {
                value.parse().map(TokenKind::Int).map_err(|_| ())
            };
            match result {
                Ok(kind) => kind,
                Err(()) => {
                    errors.push(
                        source
                            .span(start, at)
                            .error("literal numérico fora do intervalo"),
                    );
                    continue;
                }
            }
        } else if ch == '"' || ch == '\'' {
            let mut value = String::new();
            let mut closed = false;
            while let Some(c) = text[at..].chars().next() {
                if c == '\n' || c == '\r' {
                    break;
                }
                at += c.len_utf8();
                if c == ch {
                    closed = true;
                    break;
                }
                if c != '\\' {
                    value.push(c);
                    continue;
                }
                let Some(escaped) = text[at..].chars().next() else {
                    break;
                };
                if escaped == '\n' || escaped == '\r' {
                    break;
                }
                at += escaped.len_utf8();
                match escaped {
                    'n' => value.push('\n'),
                    'r' => value.push('\r'),
                    't' => value.push('\t'),
                    '0' => value.push('\0'),
                    '\\' | '\'' | '"' => value.push(escaped),
                    _ => errors.push(
                        source
                            .span(at - escaped.len_utf8() - 1, at)
                            .error("escape desconhecido"),
                    ),
                }
            }
            if !closed {
                errors.push(source.span(start, at).error("literal sem fechamento"));
                continue;
            }
            if ch == '"' {
                TokenKind::Str(value)
            } else if value.chars().count() == 1 {
                TokenKind::Char(value.chars().next().unwrap())
            } else {
                errors.push(
                    source
                        .span(start, at)
                        .error("um char contém um único escalar Unicode"),
                );
                continue;
            }
        } else {
            const SYMBOLS: &[&str] = &[
                "->", "=>", "==", "!=", "<=", ">=", "&&", "||", "::", "..", "(", ")", "{", "}",
                "[", "]", ",", ";", ":", ".", "?", "=", "+", "-", "*", "/", "%", "<", ">", "!",
                "|",
            ];
            let Some(sym) = SYMBOLS
                .iter()
                .copied()
                .find(|s| text[start..].starts_with(s))
            else {
                errors.push(source.span(start, at).error("caractere inesperado"));
                continue;
            };
            at = start + sym.len();
            TokenKind::Sym(sym)
        };
        tokens.push(Token {
            kind,
            span: source.span(start, at),
        });
    }
    tokens.push(Token {
        kind: TokenKind::Eof,
        span: source.span(at, at),
    });
    (tokens, errors)
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    Named(String, Vec<Ty>),
    Fun(Vec<Ty>, Box<Ty>),
}
impl Ty {
    pub fn name(name: &str) -> Self {
        Self::Named(name.into(), vec![])
    }
    pub fn is(&self, name: &str) -> bool {
        matches!(self, Self::Named(n, args) if n == name && args.is_empty())
    }
}
impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn list(xs: &[Ty]) -> String {
            xs.iter().map(Ty::to_string).collect::<Vec<_>>().join(", ")
        }
        match self {
            Self::Named(n, args) if args.is_empty() => f.write_str(n),
            Self::Named(n, args) => write!(f, "{n}<{}>", list(args)),
            Self::Fun(args, ret) => write!(f, "fn({}) -> {ret}", list(args)),
        }
    }
}

pub type Block = Vec<Stmt>;
pub type Params = Vec<(String, Ty)>;
pub type Generics = Vec<(String, Option<String>)>;
#[derive(Clone, Debug)]
pub struct Function {
    pub name: String,
    pub generics: Generics,
    pub args: Params,
    pub ret: Ty,
    pub body: Block,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub enum Item {
    Import(String, Span),
    Function(Function),
    Struct(String, Generics, Params, Span),
    Enum(String, Generics, Vec<(String, Vec<Ty>)>, Span),
    Trait(String, Vec<Function>, Span),
    Impl(String, Ty, Vec<Function>, Span),
}
#[derive(Clone, Debug)]
pub struct Stmt {
    pub kind: St,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub enum St {
    Let(String, bool, Option<Ty>, Expr),
    Set(Expr, Expr),
    Eval(Expr),
    Return(Option<Expr>),
    If(Expr, Block, Block),
    While(Expr, Block),
    Break,
    Continue,
}
#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: Ex,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub enum Ex {
    Int(i64),
    Float(f64),
    Char(char),
    Str(String),
    Bool(bool),
    Name(String),
    Array(Vec<Expr>),
    Unary(&'static str, Box<Expr>),
    Binary(&'static str, Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Ty>, Vec<Expr>),
    Index(Box<Expr>, Box<Expr>),
    Slice(Box<Expr>, Box<Expr>, Box<Expr>),
    Field(Box<Expr>, String),
    Try(Box<Expr>),
    Closure(Params, Ty, Block),
    Match(Box<Expr>, Vec<(Pat, Expr)>),
}
#[derive(Clone, Debug)]
pub enum Pat {
    Any,
    Bool(bool),
    Int(i64),
    Variant(String, Vec<String>),
}

pub fn precedence(op: &str) -> u8 {
    match op {
        "||" => 1,
        "&&" => 2,
        "==" | "!=" => 3,
        "<" | ">" | "<=" | ">=" => 4,
        "+" | "-" => 5,
        "*" | "/" | "%" => 6,
        _ => 0,
    }
}

struct Parser {
    ts: Vec<Token>,
    at: usize,
}
type P<T> = Result<T, Diagnostic>;
impl Parser {
    fn here(&self) -> &Token {
        &self.ts[self.at]
    }
    fn bump(&mut self) -> Token {
        let token = self.here().clone();
        if token.kind != TokenKind::Eof {
            self.at += 1;
        }
        token
    }
    fn is(&self, s: &str) -> bool {
        match &self.here().kind {
            TokenKind::Name(n) => n == s,
            TokenKind::Sym(n) => *n == s,
            _ => false,
        }
    }
    fn eat(&mut self, s: &str) -> bool {
        if self.is(s) {
            self.bump();
            true
        } else {
            false
        }
    }
    fn need(&mut self, s: &str) -> P<()> {
        if self.eat(s) {
            Ok(())
        } else {
            Err(self.here().span.error(&format!("esperado `{s}`")))
        }
    }
    fn name(&mut self) -> P<String> {
        let t = self.bump();
        if let TokenKind::Name(n) = t.kind {
            Ok(n)
        } else {
            Err(t.span.error("esperado um identificador"))
        }
    }
    fn list<T>(&mut self, end: &str, f: impl Fn(&mut Self) -> P<T>) -> P<Vec<T>> {
        let mut xs = vec![];
        while !self.eat(end) {
            xs.push(f(self)?);
            if self.eat(end) {
                break;
            }
            self.need(",")?;
        }
        Ok(xs)
    }
    fn ty(&mut self) -> P<Ty> {
        if self.eat("fn") {
            self.need("(")?;
            let args = self.list(")", Self::ty)?;
            self.need("->")?;
            return Ok(Ty::Fun(args, Box::new(self.ty()?)));
        }
        let name = self.name()?;
        let args = if self.eat("<") {
            self.list(">", Self::ty)?
        } else {
            vec![]
        };
        Ok(Ty::Named(name, args))
    }
    fn param(&mut self) -> P<(String, Ty)> {
        let n = self.name()?;
        self.need(":")?;
        Ok((n, self.ty()?))
    }
    fn generics(&mut self) -> P<Generics> {
        if self.eat("<") {
            self.list(">", |p| {
                let n = p.name()?;
                let b = if p.eat(":") { Some(p.name()?) } else { None };
                Ok((n, b))
            })
        } else {
            Ok(vec![])
        }
    }
    fn function(&mut self, signature: bool) -> P<Function> {
        let span = self.here().span.clone();
        self.need("fn")?;
        let name = self.name()?;
        let generics = self.generics()?;
        self.need("(")?;
        let args = self.list(")", Self::param)?;
        let ret = if self.eat("->") {
            self.ty()?
        } else {
            Ty::name("unit")
        };
        let body = if signature {
            self.need(";")?;
            vec![]
        } else {
            self.block()?
        };
        Ok(Function {
            name,
            generics,
            args,
            ret,
            body,
            span,
        })
    }
    fn item(&mut self) -> P<Item> {
        let span = self.here().span.clone();
        if self.is("fn") {
            return Ok(Item::Function(self.function(false)?));
        }
        if self.eat("import") {
            let token = self.bump();
            let TokenKind::Str(path) = token.kind else {
                return Err(token.span.error("esperado caminho entre aspas"));
            };
            self.need(";")?;
            return Ok(Item::Import(path, span));
        }
        if self.eat("struct") {
            let name = self.name()?;
            let gs = self.generics()?;
            self.need("{")?;
            return Ok(Item::Struct(name, gs, self.list("}", Self::param)?, span));
        }
        if self.eat("enum") {
            let name = self.name()?;
            let gs = self.generics()?;
            self.need("{")?;
            let variants = self.list("}", |p| {
                let n = p.name()?;
                let args = if p.eat("(") {
                    p.list(")", Self::ty)?
                } else {
                    vec![]
                };
                Ok((n, args))
            })?;
            return Ok(Item::Enum(name, gs, variants, span));
        }
        if self.eat("trait") {
            let name = self.name()?;
            self.need("{")?;
            let mut fs = vec![];
            while !self.eat("}") {
                fs.push(self.function(true)?);
            }
            return Ok(Item::Trait(name, fs, span));
        }
        if self.eat("impl") {
            let name = self.name()?;
            self.need("for")?;
            let ty = self.ty()?;
            self.need("{")?;
            let mut fs = vec![];
            while !self.eat("}") {
                fs.push(self.function(false)?);
            }
            return Ok(Item::Impl(name, ty, fs, span));
        }
        Err(span.error("esperado `fn`, `import`, `struct`, `enum`, `trait` ou `impl`"))
    }
    fn block(&mut self) -> P<Block> {
        self.need("{")?;
        let mut stmts = Vec::new();
        while !self.eat("}") {
            stmts.push(self.stmt()?);
        }
        Ok(stmts)
    }
    fn stmt(&mut self) -> P<Stmt> {
        let span = self.here().span.clone();
        let kind = if self.eat("let") {
            let mutable = self.eat("mut");
            let n = self.name()?;
            let ty = if self.eat(":") {
                Some(self.ty()?)
            } else {
                None
            };
            self.need("=")?;
            let e = self.expr(1)?;
            self.need(";")?;
            St::Let(n, mutable, ty, e)
        } else if self.eat("return") {
            St::Return(if self.eat(";") {
                None
            } else {
                let e = self.expr(1)?;
                self.need(";")?;
                Some(e)
            })
        } else if self.eat("if") {
            let e = self.expr(1)?;
            let a = self.block()?;
            let b = if self.eat("else") {
                if self.is("if") {
                    vec![self.stmt()?]
                } else {
                    self.block()?
                }
            } else {
                vec![]
            };
            St::If(e, a, b)
        } else if self.eat("while") {
            let e = self.expr(1)?;
            St::While(e, self.block()?)
        } else if self.eat("break") {
            self.need(";")?;
            St::Break
        } else if self.eat("continue") {
            self.need(";")?;
            St::Continue
        } else {
            let lhs = self.expr(1)?;
            let s = if self.eat("=") {
                St::Set(lhs, self.expr(1)?)
            } else {
                St::Eval(lhs)
            };
            self.need(";")?;
            s
        };
        Ok(Stmt { kind, span })
    }
    fn pattern(&mut self) -> P<Pat> {
        let t = self.bump();
        match t.kind {
            TokenKind::Int(x) => Ok(Pat::Int(x)),
            TokenKind::Name(n) if n == "_" => Ok(Pat::Any),
            TokenKind::Name(n) if n == "true" || n == "false" => Ok(Pat::Bool(n == "true")),
            TokenKind::Name(n) => {
                let args = if self.eat("(") {
                    self.list(")", Self::name)?
                } else {
                    vec![]
                };
                Ok(Pat::Variant(n, args))
            }
            _ => Err(t.span.error("esperado um padrão")),
        }
    }
    fn expr(&mut self, min: u8) -> P<Expr> {
        let t = self.bump();
        let span = t.span;
        let kind = match t.kind {
            TokenKind::Int(v) => Ex::Int(v),
            TokenKind::Float(v) => Ex::Float(v),
            TokenKind::Char(v) => Ex::Char(v),
            TokenKind::Str(v) => Ex::Str(v),
            TokenKind::Name(n) if n == "true" || n == "false" => Ex::Bool(n == "true"),
            TokenKind::Name(n) if n == "match" => {
                let value = self.expr(1)?;
                self.need("{")?;
                let arms = self.list("}", |p| {
                    let pat = p.pattern()?;
                    p.need("=>")?;
                    Ok((pat, p.expr(1)?))
                })?;
                Ex::Match(Box::new(value), arms)
            }
            TokenKind::Name(n) => Ex::Name(n),
            TokenKind::Sym("(") => {
                let e = self.expr(1)?;
                self.need(")")?;
                e.kind
            }
            TokenKind::Sym("[") => Ex::Array(self.list("]", |p| p.expr(1))?),
            TokenKind::Sym(op @ ("-" | "!")) => Ex::Unary(op, Box::new(self.expr(7)?)),
            TokenKind::Sym(op @ ("|" | "||")) => {
                let args = if op == "|" {
                    self.list("|", Self::param)?
                } else {
                    vec![]
                };
                let ret = if self.eat("->") {
                    self.ty()?
                } else {
                    Ty::name("unit")
                };
                Ex::Closure(args, ret, self.block()?)
            }
            _ => return Err(span.error("esperada uma expressão")),
        };
        let mut e = Expr {
            kind,
            span: span.clone(),
        };
        loop {
            let kind = if self.is("(") || self.is("::") {
                let types = if self.eat("::") {
                    self.need("<")?;
                    self.list(">", Self::ty)?
                } else {
                    vec![]
                };
                self.need("(")?;
                Ex::Call(Box::new(e), types, self.list(")", |p| p.expr(1))?)
            } else if self.eat("[") {
                let first = self.expr(1)?;
                let indexed = if self.eat("..") {
                    Ex::Slice(Box::new(e), Box::new(first), Box::new(self.expr(1)?))
                } else {
                    Ex::Index(Box::new(e), Box::new(first))
                };
                self.need("]")?;
                indexed
            } else if self.eat(".") {
                Ex::Field(Box::new(e), self.name()?)
            } else if self.eat("?") {
                Ex::Try(Box::new(e))
            } else {
                let TokenKind::Sym(op) = self.here().kind else {
                    break;
                };
                let bp = precedence(op);
                if bp < min {
                    break;
                }
                self.bump();
                Ex::Binary(op, Box::new(e), Box::new(self.expr(bp + 1)?))
            };
            e = Expr {
                kind,
                span: span.clone(),
            };
        }
        Ok(e)
    }
}

pub fn parse(source: &Source) -> Result<Vec<Item>, Vec<Diagnostic>> {
    let (ts, mut errors) = lex(source);
    let mut p = Parser { ts, at: 0 };
    let mut items = vec![];
    while p.here().kind != TokenKind::Eof {
        let before = p.at;
        match p.item() {
            Ok(item) => items.push(item),
            Err(error) => {
                errors.push(error);
                if p.at == before {
                    p.bump();
                }
                while p.here().kind != TokenKind::Eof
                    && !["fn", "struct", "enum", "trait", "impl", "import"]
                        .iter()
                        .any(|s| p.is(s))
                {
                    p.bump();
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(items)
    } else {
        Err(errors)
    }
}
