//! Qi 语义/语法检查器
//! Syntax and semantic checker for Qi source files.

use std::path::{Path, PathBuf};

use qi_compiler::parser::Parser;
use qi_compiler::semantic::SemanticAnalyzer;

/// 检查级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Severity {
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "warning")]
    Warning,
}

/// 检查结果中的单个条目
#[derive(Debug, Clone, serde::Serialize)]
pub struct Diagnostic {
    pub file: PathBuf,
    pub line: usize,
    pub column: usize,
    pub severity: Severity,
    pub message: String,
}

/// 一个文件的检查报告
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileReport {
    pub file: PathBuf,
    pub diagnostics: Vec<Diagnostic>,
}

impl FileReport {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count()
    }

    pub fn warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count()
    }
}

/// 检查单个 Qi 源文件
pub fn check_file(path: &Path) -> FileReport {
    let mut diagnostics = Vec::new();

    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            diagnostics.push(Diagnostic {
                file: path.to_path_buf(),
                line: 0,
                column: 0,
                severity: Severity::Error,
                message: format!("无法读取文件: {}", e),
            });
            return FileReport {
                file: path.to_path_buf(),
                diagnostics,
            };
        }
    };

    check_source(&source, path, &mut diagnostics);

    FileReport {
        file: path.to_path_buf(),
        diagnostics,
    }
}

fn check_source(source: &str, path: &Path, diagnostics: &mut Vec<Diagnostic>) {
    let parser = Parser::new();
    let program = match parser.parse_source(source) {
        Ok(p) => p,
        Err(err) => {
            let (line, col) = extract_position(&err.to_string(), source);
            diagnostics.push(Diagnostic {
                file: path.to_path_buf(),
                line,
                column: col,
                severity: Severity::Error,
                message: format!("语法错误: {}", err),
            });
            return;
        }
    };

    let program_node = qi_compiler::parser::AstNode::程序(program);
    let mut analyzer = SemanticAnalyzer::new();
    let analyze_result = analyzer.analyze(&program_node);

    let diag_manager = analyzer.diagnostics();
    for diag in diag_manager.get_diagnostics() {
        let (line, col) = diag
            .span
            .map(|s| offset_to_line_col(source, s.start))
            .unwrap_or((0, 0));

        let severity = match diag.level {
            qi_compiler::utils::diagnostics::DiagnosticLevel::错误 => Severity::Error,
            qi_compiler::utils::diagnostics::DiagnosticLevel::警告 => Severity::Warning,
            qi_compiler::utils::diagnostics::DiagnosticLevel::信息 => Severity::Warning,
        };

        diagnostics.push(Diagnostic {
            file: path.to_path_buf(),
            line,
            column: col,
            severity,
            message: diag.message.clone(),
        });
    }

    if let Err(err) = analyze_result {
        let already_reported = !diagnostics.is_empty();
        if !already_reported {
            diagnostics.push(Diagnostic {
                file: path.to_path_buf(),
                line: 0,
                column: 0,
                severity: Severity::Error,
                message: format!("语义错误: {}", err),
            });
        }
    }
}

/// Convert a byte offset into (line, column) using character positions.
fn offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    let mut seen = 0;
    for ch in source.chars() {
        if seen >= offset {
            break;
        }
        let len = ch.len_utf8();
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
        seen += len;
    }
    (line, col)
}

fn extract_position(_error: &str, _source: &str) -> (usize, usize) {
    (1, 1)
}

/// 格式化诊断为人类可读文本
pub fn format_human(report: &FileReport) -> String {
    let mut out = String::new();
    if report.diagnostics.is_empty() {
        out.push_str(&format!("{}: 检查通过\n", report.file.display()));
        return out;
    }
    for d in &report.diagnostics {
        let severity = match d.severity {
            Severity::Error => "错误",
            Severity::Warning => "警告",
        };
        out.push_str(&format!(
            "{}:{}:{}: {}: {}\n",
            d.file.display(),
            d.line,
            d.column,
            severity,
            d.message
        ));
    }
    out
}
