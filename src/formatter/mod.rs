//! Qi 代码格式化器 —— 基于词法扫描的“只动空白”实现
//!
//! # 设计原则
//!
//! 格式化器只调整空白（缩进、行尾空白、连续空行），**绝不改动任何非空白内容**：
//!
//! - 不基于 AST 重打印。编译器 AST 不携带注释（lexer 直接丢弃）、不区分
//!   `类型`/`结构体` 声明风格、不保留 `导入 模块::{项1, 项2}` 的选择性导入
//!   原文，且部分节点（如 Go 风格接收者方法 `函数 (自身 类型) 名()`）无法
//!   无损还原。基于 AST 重打印必然吞注释、改写声明风格，甚至丢代码。
//! - 逐行处理：不合并行、不拆分行，`;` 永远跟随其语句。
//! - 通过轻量词法扫描（识别字符串、字符字面量、行注释、可嵌套块注释）
//!   统计花括号/圆括号深度，仅据此重排行首缩进。
//! - 输出对自身幂等：格式化两遍与一遍结果完全一致。

mod config;
mod writer;

pub use config::FormatConfig;
pub use writer::CodeWriter;

pub struct Formatter {
    config: FormatConfig,
}

impl Formatter {
    pub fn new() -> Self {
        Self {
            config: FormatConfig::default(),
        }
    }

    pub fn with_config(config: FormatConfig) -> Self {
        Self { config }
    }

    /// 格式化 Qi 源文件。只调整空白（缩进/行尾空白/连续空行），
    /// 非空白内容与源文件逐字符一致。
    pub fn format_file(&self, source: &str) -> Result<String, String> {
        Ok(self.reindent(source))
    }

    fn reindent(&self, source: &str) -> String {
        let indent_unit = self.config.indent_str();
        let mut out = String::with_capacity(source.len() + 16);

        // 跨行状态
        let mut depth = DepthState::default();
        let mut pending_blank = false;
        let mut wrote_any = false;

        for raw_line in source.lines() {
            // 起始于块注释内部的行：原样保留（仅去行尾空白），不重排缩进，
            // 以免破坏注释内的对齐/示意图。仍需扫描以更新注释/括号状态
            // （`*/` 之后同一行可能还有代码）。
            if depth.comment > 0 {
                if pending_blank {
                    out.push('\n');
                    pending_blank = false;
                }
                out.push_str(raw_line.trim_end());
                out.push('\n');
                wrote_any = true;
                scan_line(raw_line, &mut depth);
                continue;
            }

            let trimmed = raw_line.trim();
            if trimmed.is_empty() {
                // 连续空行折叠为一个；文件首部空行丢弃
                if wrote_any && self.config.preserve_blank_lines {
                    pending_blank = true;
                }
                continue;
            }

            if pending_blank {
                out.push('\n');
                pending_blank = false;
            }

            // 行首的闭合符决定本行相对当前深度回退多少级
            let lead = leading_closers(trimmed);
            let mut level = depth.brace.saturating_sub(lead.braces);
            if depth.paren.saturating_sub(lead.parens) > 0 {
                // 未闭合圆括号/方括号内的续行统一多缩进一级
                level += 1;
            }

            for _ in 0..level {
                out.push_str(&indent_unit);
            }
            out.push_str(trimmed);
            out.push('\n');
            wrote_any = true;

            scan_line(trimmed, &mut depth);
        }

        out
    }
}

impl Default for Formatter {
    fn default() -> Self {
        Self::new()
    }
}

/// 跨行扫描状态：块注释嵌套深度与括号深度
#[derive(Default)]
struct DepthState {
    /// 块注释嵌套深度（Qi 的 `/* */` 支持嵌套）
    comment: usize,
    /// 花括号（`{}` / `【】`）深度，决定缩进级别
    brace: usize,
    /// 圆括号/方括号（`()` / `（）` / `[]`）深度，用于续行缩进
    paren: usize,
}

/// 行首连续闭合符统计（允许闭合符之间有空白），如 `};`、`} 否则 {`、`);`
struct LeadingClosers {
    braces: usize,
    parens: usize,
}

fn leading_closers(trimmed: &str) -> LeadingClosers {
    let mut braces = 0;
    let mut parens = 0;
    for c in trimmed.chars() {
        match c {
            '}' | '】' => braces += 1,
            ')' | '）' | ']' => parens += 1,
            c if c.is_whitespace() => continue,
            _ => break,
        }
    }
    LeadingClosers { braces, parens }
}

/// 扫描一行，更新括号/注释深度。
/// 跳过字符串字面量（含 f"..." 格式字符串）、字符字面量、行注释与可嵌套块注释，
/// 保证注释和字符串里的括号不影响缩进计算。
fn scan_line(line: &str, depth: &mut DepthState) {
    let chars: Vec<char> = line.chars().collect();
    let n = chars.len();
    let mut i = 0;

    while i < n {
        // 块注释内部：只找 `*/` 与嵌套的 `/*`
        if depth.comment > 0 {
            if chars[i] == '*' && i + 1 < n && chars[i + 1] == '/' {
                depth.comment -= 1;
                i += 2;
            } else if chars[i] == '/' && i + 1 < n && chars[i + 1] == '*' {
                depth.comment += 1;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }

        let c = chars[i];
        match c {
            // 行注释：本行剩余部分全部忽略
            '/' if i + 1 < n && chars[i + 1] == '/' => break,
            // 块注释开始
            '/' if i + 1 < n && chars[i + 1] == '*' => {
                depth.comment += 1;
                i += 2;
                continue;
            }
            // 字符串字面量（f"..." 的 f 前缀无需特判，引号处理一致）
            '"' => {
                i += 1;
                while i < n && chars[i] != '"' {
                    if chars[i] == '\\' {
                        i += 1; // 跳过转义字符
                    }
                    i += 1;
                }
                i += 1; // 跳过闭引号（或行尾）
                continue;
            }
            // 字符字面量：'x' 或 '\x'；不匹配该模式时按普通字符处理
            '\'' => {
                if i + 3 < n && chars[i + 1] == '\\' && chars[i + 3] == '\'' {
                    i += 4;
                    continue;
                }
                if i + 2 < n && chars[i + 2] == '\'' {
                    i += 3;
                    continue;
                }
            }
            '{' | '【' => depth.brace += 1,
            '}' | '】' => depth.brace = depth.brace.saturating_sub(1),
            '(' | '（' | '[' => depth.paren += 1,
            ')' | '）' | ']' => depth.paren = depth.paren.saturating_sub(1),
            _ => {}
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(source: &str) -> String {
        Formatter::new().format_file(source).unwrap()
    }

    /// 去掉全部空白后应与源文件逐字符一致（格式化器只动空白）
    fn assert_whitespace_only_change(source: &str) {
        let formatted = fmt(source);
        let strip = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        assert_eq!(strip(source), strip(&formatted), "非空白内容被改动");
    }

    /// 幂等：格式化两遍与一遍结果一致
    fn assert_idempotent(source: &str) {
        let once = fmt(source);
        let twice = fmt(&once);
        assert_eq!(once, twice, "格式化不幂等");
    }

    #[test]
    fn test_basic_format() {
        let source = "包 测试;\n函数 示例() {\n变量 x = 10;\n}\n";
        let result = fmt(source);
        assert!(result.contains("    变量 x = 10;"));
        assert_idempotent(source);
    }

    #[test]
    fn test_indent() {
        let source = "函数 测试() {\n如果 真 {\n打印(1);\n}\n}\n";
        let result = fmt(source);
        assert!(result.contains("    如果 真 {"));
        assert!(result.contains("        打印(1);"));
        assert!(result.contains("    }\n}"));
    }

    #[test]
    fn test_semicolon_stays_with_statement() {
        // 结构体字面量语句：`};` 不允许拆开，行结构原样保留
        let source = "函数 入口() {\n变量 矩 = 矩形 { 宽: 3.0, 高: 4.0 };\n}\n";
        let result = fmt(source);
        assert!(result.contains("    变量 矩 = 矩形 { 宽: 3.0, 高: 4.0 };"));
        assert!(!result.contains("\n;"));
        assert_whitespace_only_change(source);
        assert_idempotent(source);
    }

    #[test]
    fn test_comments_preserved() {
        let source = "// 整行注释\n包 主程序;\n\n函数 入口() {\n变量 x = 1; // 行尾注释\n/* 块注释\n   第二行 */\n打印行(x);\n}\n";
        let result = fmt(source);
        assert!(result.contains("// 整行注释"));
        assert!(result.contains("变量 x = 1; // 行尾注释"));
        assert!(result.contains("/* 块注释"));
        assert!(result.contains("第二行 */"));
        assert_whitespace_only_change(source);
        assert_idempotent(source);
    }

    #[test]
    fn test_selective_import_untouched() {
        let source = "包 主程序;\n\n导入 Web::{应用, 路由};\n";
        let result = fmt(source);
        assert!(result.contains("导入 Web::{应用, 路由};"));
        assert_idempotent(source);
    }

    #[test]
    fn test_type_declaration_style_untouched() {
        let source = "类型 矩形 {\n浮点数 宽;\n浮点数 高;\n}\n";
        let result = fmt(source);
        assert!(result.contains("类型 矩形 {"));
        assert!(result.contains("    浮点数 宽;"));
        assert!(!result.contains("结构体"));
        assert_whitespace_only_change(source);
    }

    #[test]
    fn test_receiver_method_untouched() {
        // Go 风格接收者方法：绝不允许被占位符替换
        let source = "函数 (自身 矩形) 面积(): 浮点数 {\n返回 自身.宽 * 自身.高;\n}\n";
        let result = fmt(source);
        assert!(result.contains("函数 (自身 矩形) 面积(): 浮点数 {"));
        assert!(result.contains("    返回 自身.宽 * 自身.高;"));
        assert!(!result.contains("/*"));
        assert_whitespace_only_change(source);
        assert_idempotent(source);
    }

    #[test]
    fn test_braces_in_strings_and_comments_ignored() {
        let source =
            "函数 入口() {\n变量 s = \"{{{\"; // 注释里的 } 也不算\n变量 c = '{';\n打印行(s, c);\n}\n";
        let result = fmt(source);
        assert!(result.contains("    打印行(s, c);"));
        assert!(result.ends_with("}\n"));
        assert_whitespace_only_change(source);
        assert_idempotent(source);
    }

    #[test]
    fn test_else_brace_line() {
        let source = "函数 入口() {\n如果 真 {\n打印(1);\n} 否则 {\n打印(2);\n}\n}\n";
        let result = fmt(source);
        assert!(result.contains("    } 否则 {"));
        assert_idempotent(source);
    }

    #[test]
    fn test_blank_lines_collapsed() {
        let source = "包 主程序;\n\n\n\n函数 入口() {\n}\n";
        let result = fmt(source);
        assert!(result.contains("包 主程序;\n\n函数 入口() {"));
        assert_idempotent(source);
    }

    #[test]
    fn test_continuation_line_in_parens() {
        let source = "函数 入口() {\n打印行(\"a\",\n\"b\");\n}\n";
        let result = fmt(source);
        assert!(result.contains("    打印行(\"a\",\n        \"b\");"));
        assert_idempotent(source);
    }

    #[test]
    fn test_fallback_on_parse_error() {
        // 语法不完整的输入也不应报错或丢内容
        let source = "不是有效的 Qi { 代码\n";
        let result = Formatter::new().format_file(source);
        assert!(result.is_ok());
        assert_whitespace_only_change(source);
    }
}
