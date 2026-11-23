use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
};
const HOST: &str = include_str!("../tools/host.cjs");
const USAGE: &str =
    "uso: lumec <build|run|check|emit-ir> arquivo.lume [-O0|-O1|-O2] [-o saída.wasm]";
fn main() -> ExitCode {
    match execute() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            if !message.is_empty() {
                eprintln!("{message}");
            }
            ExitCode::FAILURE
        }
    }
}
fn execute() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let command = args.next().ok_or(USAGE)?;
    if command == "--help" || command == "-h" {
        println!("{USAGE}");
        return Ok(());
    }
    let mut source = None;
    let mut output = None;
    let mut level = 0;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("-O0") => level = 0,
            Some("-O1") => level = 1,
            Some("-O2") => level = 2,
            Some("-o") if output.is_none() => {
                output = Some(PathBuf::from(args.next().ok_or(USAGE)?))
            }
            Some(s) if s.starts_with('-') => {
                return Err(format!("opção desconhecida `{s}`\n{USAGE}"))
            }
            _ if source.is_none() => source = Some(PathBuf::from(arg)),
            _ => return Err(USAGE.into()),
        }
    }
    let path = source.ok_or(USAGE)?;
    if !matches!(
        command.to_str(),
        Some("build" | "run" | "check" | "emit-ir")
    ) || output.is_some() && command != "build"
    {
        return Err(USAGE.into());
    }
    let result = if path == Path::new("/dev/stdin") {
        let text = std::io::read_to_string(std::io::stdin()).map_err(|e| e.to_string())?;
        lumec::compile_text("<stdin>", &text, level)
    } else {
        lumec::compile_path(&path, level)
    };
    let result = result.map_err(|ds| {
        ds.into_iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    match command.to_str().unwrap() {
        "build" => fs::write(
            output.unwrap_or_else(|| path.with_extension("wasm")),
            result.wasm,
        )
        .map_err(|e| e.to_string()),
        "emit-ir" => {
            print!("{}", result.ir);
            Ok(())
        }
        "run" => {
            // Node evaluates the embedded host. The module bytes stay on stdin.
            let launcher=format!("{HOST}\ntry {{ run(fs.readFileSync(0), undefined, {{capture:false}}); }} catch (err) {{ console.error(err.message); process.exitCode=1; }}");
            let mut child = Command::new("node")
                .arg("-e")
                .arg(launcher)
                .stdin(Stdio::piped())
                .spawn()
                .map_err(|e| format!("Node.js: {e}"))?;
            let write = child
                .stdin
                .take()
                .expect("stdin disponível")
                .write_all(&result.wasm);
            let status = child.wait().map_err(|e| e.to_string())?;
            if !status.success() {
                return Err(String::new());
            }
            write.map_err(|e| e.to_string())
        }
        _ => Ok(()),
    }
}
