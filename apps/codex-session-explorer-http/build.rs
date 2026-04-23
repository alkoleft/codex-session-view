use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let frontend_dist = workspace_root.join("apps/codex-session-explorer/dist");
    let output_dir = PathBuf::from(env::var("OUT_DIR").expect("out dir")).join("frontend-dist");

    println!("cargo:rerun-if-changed={}", frontend_dist.display());

    if output_dir.exists() {
        fs::remove_dir_all(&output_dir).expect("stale output dir should be removed");
    }
    fs::create_dir_all(&output_dir).expect("output dir should be created");

    if frontend_dist.is_dir() {
        copy_dir_all(&frontend_dist, &output_dir).expect("frontend dist should copy");
        return;
    }

    let placeholder = r#"<!doctype html>
<html lang="ru">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>codex-session-explorer</title>
  </head>
  <body>
    <main style="font-family: sans-serif; max-width: 42rem; margin: 3rem auto; line-height: 1.5;">
      <h1>Frontend assets не собраны</h1>
      <p>Сначала выполните <code>npm --prefix apps/codex-session-explorer run build</code>, затем пересоберите HTTP binary.</p>
    </main>
  </body>
</html>
"#;
    fs::write(output_dir.join("index.html"), placeholder).expect("placeholder index should write");
}

fn copy_dir_all(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
