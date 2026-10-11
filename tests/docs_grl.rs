//! Checks the documentation's examples: every ```grl block in the README and the guide
//! (`docs/src`) must parse. Rust blocks run as doc tests (see `src/lib.rs`).

use rust_runes::KnowledgeBase;
use std::fs;
use std::path::{Path, PathBuf};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn documents() -> Vec<PathBuf> {
    let mut paths = vec![Path::new(ROOT).join("README.md")];
    for entry in fs::read_dir(Path::new(ROOT).join("docs/src")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|ext| ext == "md") {
            paths.push(path);
        }
    }
    paths.sort();
    paths
}

/// Fenced code blocks as (info string, first line number, contents).
fn code_blocks(text: &str) -> Vec<(String, usize, String)> {
    let mut blocks = Vec::new();
    let mut current: Option<(String, usize, String)> = None;
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        match current.take() {
            None => {
                if let Some(info) = trimmed.strip_prefix("```") {
                    current = Some((info.trim().to_string(), i + 2, String::new()));
                }
            }
            Some(block) if trimmed.starts_with("```") => blocks.push(block),
            Some((info, start, mut body)) => {
                body.push_str(line);
                body.push('\n');
                current = Some((info, start, body));
            }
        }
    }
    assert!(current.is_none(), "unclosed code block");
    blocks
}

#[test]
fn grl_examples_parse() {
    let mut checked = 0;
    for path in documents() {
        let text = fs::read_to_string(&path).unwrap();
        for (info, line, body) in code_blocks(&text) {
            let location = format!("{}:{}", path.display(), line);
            // An untagged block would run as a Rust doc test
            assert!(
                !info.is_empty(),
                "{location}: code block without a language"
            );
            if info != "grl" {
                continue;
            }
            match KnowledgeBase::new().add_rules_from_str(&body) {
                Ok(0) => panic!("{location}: GRL example has no rules"),
                Ok(_) => checked += 1,
                Err(e) => panic!("{location}: GRL example doesn't parse: {e}"),
            }
        }
    }
    assert!(checked > 0);
}

#[test]
fn every_chapter_runs_as_a_doc_test() {
    let lib = fs::read_to_string(Path::new(ROOT).join("src/lib.rs")).unwrap();
    for path in documents() {
        let name = path.file_name().unwrap().to_str().unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let has_rust = code_blocks(&text)
            .iter()
            .any(|(info, _, _)| info.split(',').next() == Some("rust"));
        if has_rust {
            assert!(
                lib.contains(&format!("{name}\")]")),
                "{name} has Rust examples but isn't included in src/lib.rs's `guide` module"
            );
        }
    }
}
