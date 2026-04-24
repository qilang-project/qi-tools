//! qidoc - Qi 文档生成工具
//! Markdown documentation generator for Qi source files.

use std::path::{Path, PathBuf};
use std::process;

use clap::Parser;
use qi_tools::doc;

#[derive(Parser)]
#[command(name = "qidoc")]
#[command(about = "Qi 语言文档生成工具", long_about = None)]
#[command(version)]
struct Cli {
    /// 要生成文档的源文件或目录
    #[arg(value_name = "PATH", required = true)]
    paths: Vec<PathBuf>,

    /// 输出目录（不指定时写入标准输出）
    #[arg(short, long, value_name = "DIR")]
    output: Option<PathBuf>,

    /// 递归处理目录
    #[arg(short, long)]
    recursive: bool,

    /// 静默模式
    #[arg(short, long)]
    quiet: bool,
}

fn main() {
    env_logger::init();
    let cli = Cli::parse();

    let mut files = Vec::new();
    let mut errors = 0usize;

    for path in &cli.paths {
        if path.is_dir() {
            if cli.recursive {
                collect_dir(path, &mut files);
            } else {
                eprintln!("警告: {:?} 是目录，请使用 -r 选项递归处理", path);
            }
        } else if path.is_file() {
            files.push(path.clone());
        } else {
            eprintln!("错误: 路径不存在: {:?}", path);
            errors += 1;
        }
    }

    if let Some(ref out_dir) = cli.output {
        if let Err(e) = std::fs::create_dir_all(out_dir) {
            eprintln!("错误: 无法创建输出目录 {:?}: {}", out_dir, e);
            process::exit(1);
        }
    }

    for file in &files {
        match doc::generate_for_file(file) {
            Ok(markdown) => match &cli.output {
                Some(dir) => {
                    let out_path = output_path(dir, file);
                    if let Some(parent) = out_path.parent() {
                        if let Err(e) = std::fs::create_dir_all(parent) {
                            eprintln!("错误: 无法创建目录 {:?}: {}", parent, e);
                            errors += 1;
                            continue;
                        }
                    }
                    if let Err(e) = std::fs::write(&out_path, markdown) {
                        eprintln!("错误: 写入 {:?} 失败: {}", out_path, e);
                        errors += 1;
                    } else if !cli.quiet {
                        println!("已生成: {}", out_path.display());
                    }
                }
                None => {
                    print!("{}", markdown);
                }
            },
            Err(e) => {
                eprintln!("错误: {:?}: {}", file, e);
                errors += 1;
            }
        }
    }

    if !cli.quiet && cli.output.is_some() {
        println!("\n文档生成完成: {} 个文件", files.len() - errors);
    }

    if errors > 0 {
        process::exit(1);
    }
}

fn collect_dir(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file()
            && entry.path().extension().and_then(|s| s.to_str()) == Some("qi")
        {
            files.push(entry.path().to_path_buf());
        }
    }
}

fn output_path(out_dir: &Path, source: &Path) -> PathBuf {
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("module");
    out_dir.join(format!("{}.md", stem))
}
