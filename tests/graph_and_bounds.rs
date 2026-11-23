mod support;
use std::{fs, process::Command};

#[test]
fn imports_canonical_diamond_once() {
    let work = support::Scratch::new();
    fs::create_dir(work.join("nested")).unwrap();
    fs::write(work.join("shared.lume"), "fn seed() -> i64 { return 19; }").unwrap();
    fs::write(
        work.join("nested/side.lume"),
        "import \"../shared.lume\"; fn next() -> i64 {return seed()+2;}",
    )
    .unwrap();
    let root = work.join("main.lume");
    fs::write(
        &root,
        "import \"nested/side.lume\"; import \"./shared.lume\"; fn main() {print(seed()+next());}",
    )
    .unwrap();
    let wasm = work.join("output.wasm");
    fs::write(&wasm, lumec::compile_path(&root, 2).ok().unwrap().wasm).unwrap();
    assert_eq!(support::node("tools/host.cjs", &[&wasm]), "40\n");
}
#[test]
fn imports_detect_cycle() {
    let work = support::Scratch::new();
    let a = work.join("a.lume");
    fs::write(&a, "import \"b.lume\"; fn main() {}\n").unwrap();
    fs::write(work.join("b.lume"), "import \"a.lume\";\n").unwrap();
    let d = lumec::compile_path(&a, 2).err().unwrap();
    assert!(d[0].to_string().contains("ciclo de imports"));
    assert!(d[0].to_string().contains("b.lume:1:1"));
}
#[test]
fn imported_body_reports_its_own_path() {
    let work = support::Scratch::new();
    let a = work.join("a.lume");
    fs::write(&a, "import \"b.lume\"; fn main() {f();}\n").unwrap();
    fs::write(work.join("b.lume"), "fn f() {\n    print(false);\n}\n").unwrap();
    let d = lumec::compile_path(&a, 0).err().unwrap();
    assert!(d[0].to_string().contains("b.lume:2:11"));
}
fn traps(code: &str) {
    let work = support::Scratch::new();
    for level in [0, 2] {
        let file = work.join("output.wasm");
        let source = format!("fn main() {{ {code} }}");
        fs::write(
            &file,
            lumec::compile_text("trap.lume", &source, level)
                .ok()
                .unwrap()
                .wasm,
        )
        .unwrap();
        let output = Command::new("node")
            .current_dir(support::root())
            .arg("tools/host.cjs")
            .arg(file)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("unreachable"));
    }
}
#[test]
fn array_rejects_negative_index() {
    traps("let a=[1,2]; print(a[-1]);");
}
#[test]
fn array_rejects_index_equal_to_length() {
    traps("let a=[1,2]; a[2]=3;");
}
#[test]
fn slice_rejects_reversed_interval() {
    traps("let a=[1,2]; let s=a[2..1]; print(len(s));");
}
#[test]
fn slice_rejects_end_after_length() {
    traps("let a=[1,2]; let s=a[0..3]; print(len(s));");
}
#[test]
fn utf8_string_rejects_index_after_bytes() {
    traps("let a=\"λ\"; print(a[2]);");
}
