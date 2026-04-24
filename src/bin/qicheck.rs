//! qicheck - Qi 语法与语义检查工具
//! Syntax and semantic checker for the Qi programming language.

use std::path::{Path, PathBuf};
use std::process;

use clap::Parser;
use qi_tools::checker::{self, FileReport};

#[derive(Parser)]
#[command(name = "qicheck")]
#[command(about = "Qi 语言语法/语义检查工具", long_about = None)]
#[command(version)]
struct Cli {
    /// 要检查的文件或目录
    #[arg(value_name = "PATH", required = true)]
    paths: Vec<PathBuf>,

    /// 递归处理目录
    #[arg(short, long)]
    recursive: bool,

    /// 以 JSON 格式输出结果
    #[arg(long)]
    json: bool,

    /// 静默模式：只有错误时才输出
    #[arg(short, long)]
    quiet: bool,
}

fn main() {
    env_logger::init();
    let cli = Cli::parse();

    let mut reports = Vec::new();
    let mut collect_err: Option<String> = None;

    for path in &cli.paths {
        if path.is_dir() {
            if cli.recursive {
                collect_dir(path, &mut reports);
            } else {
                eprintln!("警告: {:?} 是目录，请使用 -r 选项递归处理", path);
            }
        } else if path.is_file() {
            reports.push(checker::check_file(path));
        } else {
            collect_err = Some(format!("路径不存在: {:?}", path));
        }
    }

    let total_errors: usize = reports.iter().map(FileReport::error_count).sum();
    let total_warnings: usize = reports.iter().map(FileReport::warning_count).sum();

    if cli.json {
        let payload = serde_json::json!({
            "files": reports,
            "summary": {
                "files": reports.len(),
                "errors": total_errors,
                "warnings": total_warnings,
            }
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        for report in &reports {
            if report.diagnostics.is_empty() && cli.quiet {
                continue;
            }
            print!("{}", checker::format_human(report));
        }

        if !cli.quiet {
            println!();
            println!("检查完成:");
            println!("  文件数: {}", reports.len());
            println!("  错误数: {}", total_errors);
            println!("  警告数: {}", total_warnings);
        }
    }

    if let Some(e) = collect_err {
        eprintln!("错误: {}", e);
        process::exit(1);
    }

    if total_errors > 0 {
        process::exit(1);
    }
}

fn collect_dir(dir: &Path, reports: &mut Vec<FileReport>) {
    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file()
            && entry.path().extension().and_then(|s| s.to_str()) == Some("qi")
        {
            reports.push(checker::check_file(entry.path()));
        }
    }
}
