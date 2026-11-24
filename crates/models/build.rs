use std::fmt::Write as _;
use std::path::Path;
use std::{env, fs};

fn main() {
    let root = Path::new("src/families");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    collect(root, &mut files);
    files.sort();
    let mut code = String::from("pub const FILES: &[(&str, &str)] = &[\n");
    for file in files {
        let rel = file.strip_prefix(root).expect("inside families");
        let abs = fs::canonicalize(&file).expect("canonical path");
        writeln!(
            code,
            "    ({:?}, include_str!({:?})),",
            rel.to_string_lossy(),
            abs.to_string_lossy()
        )
        .expect("write");
    }
    code.push_str("];\n");
    let out = Path::new(&env::var("OUT_DIR").expect("OUT_DIR")).join("builtin.rs");
    fs::write(out, code).expect("write builtin.rs");
}

fn collect(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        println!("cargo:rerun-if-changed={}", path.display());
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "toml")
            && !path.to_string_lossy().contains("fixtures")
        {
            out.push(path);
        }
    }
}
