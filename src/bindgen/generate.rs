//! 把解析结果渲染成可被 Qi 编译器接受的 `.qi` 外部声明文本。

use super::{BindgenResult, MappedFn, SkippedFn};

/// 生成完整 `.qi` 文件内容。
///
/// - `header`：来源头文件路径（写进头部注释）
/// - `lib`：库名（`外部 "<lib>"`）
/// - `prefix`：可选的前缀过滤说明（写进注释；实际过滤在解析阶段做）
pub fn render(header: &str, lib: &str, prefix: Option<&str>, result: &BindgenResult) -> String {
    let mut out = String::new();
    out.push_str("// 由 qi-bindgen 自动生成，请勿手工编辑。\n");
    out.push_str(&format!("// 来源头文件: {}\n", header));
    out.push_str(&format!("// 库: {}\n", lib));
    if let Some(p) = prefix {
        out.push_str(&format!("// 前缀过滤: {}\n", p));
    }
    out.push_str("// 生成时间: <占位>\n");
    out.push_str(&format!(
        "// 统计: 可映射 {} 个，跳过 {} 个\n",
        result.mapped.len(),
        result.skipped.len()
    ));
    out.push('\n');

    out.push_str(&format!("外部 \"{}\" {{\n", lib));

    for f in &result.mapped {
        out.push_str(&render_mapped(f));
        out.push('\n');
    }

    if !result.skipped.is_empty() {
        if !result.mapped.is_empty() {
            out.push('\n');
        }
        out.push_str("    // ==== 以下函数被跳过（类型暂不支持），仅作记录 ====\n");
        for s in &result.skipped {
            out.push_str(&render_skipped(s));
            out.push('\n');
        }
    }

    out.push_str("}\n");
    out
}

fn render_mapped(f: &MappedFn) -> String {
    let params = f
        .params
        .iter()
        .map(|(name, ty)| format!("{}: {}", name, ty.名称()))
        .collect::<Vec<_>>()
        .join(", ");
    format!("    函数 {}({}): {};", f.name, params, f.ret.名称())
}

fn render_skipped(s: &SkippedFn) -> String {
    format!("    // 跳过 {}: {}", s.name, s.reason)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bindgen::{QiParamType, QiRetType};

    fn 样例() -> BindgenResult {
        BindgenResult {
            mapped: vec![
                MappedFn {
                    name: "cos".into(),
                    params: vec![("p0".into(), QiParamType::浮点数)],
                    ret: QiRetType::浮点数,
                },
                MappedFn {
                    name: "strlen".into(),
                    params: vec![("p0".into(), QiParamType::字符串)],
                    ret: QiRetType::整数,
                },
            ],
            skipped: vec![SkippedFn {
                name: "strcpy".into(),
                reason: "返回 char* 暂不支持".into(),
            }],
        }
    }

    #[test]
    fn 渲染包含外部块头() {
        let s = render("math.h", "m", None, &样例());
        assert!(s.contains("外部 \"m\" {"));
        assert!(s.contains("函数 cos(p0: 浮点数): 浮点数;"));
        assert!(s.contains("函数 strlen(p0: 字符串): 整数;"));
    }

    #[test]
    fn 跳过项是注释() {
        let s = render("string.h", "c", None, &样例());
        assert!(s.contains("// 跳过 strcpy: 返回 char* 暂不支持"));
        // 跳过项绝不能出现为可编译的函数声明
        assert!(!s.contains("函数 strcpy"));
    }

    #[test]
    fn 无参函数() {
        let r = BindgenResult {
            mapped: vec![MappedFn {
                name: "rand".into(),
                params: vec![],
                ret: QiRetType::整数,
            }],
            skipped: vec![],
        };
        let s = render("stdlib.h", "c", None, &r);
        assert!(s.contains("函数 rand(): 整数;"));
    }

    #[test]
    fn 头部有统计和前缀() {
        let s = render("openssl/sha.h", "crypto", Some("SHA"), &样例());
        assert!(s.contains("// 前缀过滤: SHA"));
        assert!(s.contains("// 统计: 可映射 2 个，跳过 1 个"));
        assert!(s.contains("// 来源头文件: openssl/sha.h"));
    }
}
