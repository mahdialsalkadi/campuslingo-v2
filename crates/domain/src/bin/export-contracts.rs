use std::{fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = root.join("packages/contracts/schema");
    let check = std::env::args().any(|argument| argument == "--check");
    if !check {
        fs::create_dir_all(&directory)?;
    }
    for (name, schema) in campuslingo_domain::contracts::schemas() {
        let mut value = serde_json::to_value(schema)?;
        value["$id"] = format!("https://campuslingo.invalid/contracts/1.0.0/{name}.json").into();
        let content = serde_json::to_string_pretty(&value)? + "\n";
        let path = directory.join(format!("{name}.json"));
        if check {
            if fs::read_to_string(&path).unwrap_or_default() != content {
                return Err(format!("schema drift: {}", path.display()).into());
            }
        } else {
            fs::write(path, content)?;
        }
    }
    println!(
        "Rust contract schemas {}",
        if check { "verified" } else { "exported" }
    );
    Ok(())
}
