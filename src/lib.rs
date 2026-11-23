pub mod ir;
mod lower;
pub mod syntax;
mod wasm;

use std::{collections::HashSet, fs, path::Path};
use syntax::{Diagnostic, Item, Source, Span};

pub struct Compilation {
    pub wasm: Vec<u8>,
    pub ir: ir::Module,
}

pub fn compile_text(name: &str, text: &str, level: u8) -> Result<Compilation, Vec<Diagnostic>> {
    let source = Source::new(name, text);
    let mut items = syntax::parse(&source)?;
    if let Some(Item::Import(_, span)) = items.iter().find(|item| matches!(item, Item::Import(..)))
    {
        return Err(vec![span.error("imports precisam de um caminho de arquivo")]);
    }
    compile_items(&mut items, level)
}

pub fn compile_path(path: &Path, level: u8) -> Result<Compilation, Vec<Diagnostic>> {
    fn visit(
        path: &Path,
        active: &mut HashSet<std::path::PathBuf>,
        done: &mut HashSet<std::path::PathBuf>,
        items: &mut Vec<Item>,
        origin: Option<&Span>,
    ) -> Result<(), Vec<Diagnostic>> {
        let io = |err: std::io::Error| {
            let msg = format!("{}: {err}", path.display());
            vec![origin.map_or_else(
                || {
                    Source::new(&path.display().to_string(), "")
                        .span(0, 0)
                        .error(&msg)
                },
                |s| s.error(&msg),
            )]
        };
        let path = fs::canonicalize(path).map_err(io)?;
        if done.contains(&path) {
            return Ok(());
        }
        if !active.insert(path.clone()) {
            return Err(vec![origin
                .expect("import recursivo")
                .error("ciclo de imports")]);
        }
        let source = Source::new(
            &path.display().to_string(),
            &fs::read_to_string(&path).map_err(io)?,
        );
        for item in syntax::parse(&source)? {
            if let Item::Import(name, span) = item {
                visit(
                    &path.parent().unwrap().join(name),
                    active,
                    done,
                    items,
                    Some(&span),
                )?;
            } else {
                items.push(item);
            }
        }
        active.remove(&path);
        done.insert(path);
        Ok(())
    }
    let mut items = Vec::new();
    visit(
        path,
        &mut HashSet::new(),
        &mut HashSet::new(),
        &mut items,
        None,
    )?;
    compile_items(&mut items, level)
}

fn compile_items(items: &mut Vec<Item>, level: u8) -> Result<Compilation, Vec<Diagnostic>> {
    let prelude = Source::new(
        "<prelude>",
        "enum Option<T> { None, Some(T) } enum Result<T,E> { Ok(T), Err(E) }",
    );
    let mut base = syntax::parse(&prelude)?;
    base.append(items);
    let mut ir = lower::lower(&base).map_err(|d| vec![d])?;
    ir::optimize(&mut ir, level);
    let wasm = wasm::emit(&ir).map_err(|message| vec![prelude.span(0, 0).error(&message)])?;
    Ok(Compilation { wasm, ir })
}
