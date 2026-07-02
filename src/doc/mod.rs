//! Qi 文档生成器 - 从源码提取 Markdown 文档
//! Documentation generator for Qi source files.

use std::path::Path;

use qi_compiler::parser::ast::{
    AstNode, EnumDeclaration, FunctionDeclaration, ImportStatement, Parameter, Program,
    StructDeclaration, StructField, TypeNode, Visibility,
};
use qi_compiler::parser::Parser;

/// 生成某个 Qi 源文件的 Markdown 文档。
pub fn generate_markdown(source: &str, file_name: Option<&str>) -> Result<String, String> {
    let parser = Parser::new();
    let program = parser
        .parse_source(source)
        .map_err(|e| format!("解析失败: {}", e))?;
    Ok(render_program(&program, file_name))
}

/// 便捷函数：读取并生成文档。
pub fn generate_for_file(path: &Path) -> Result<String, String> {
    let source = std::fs::read_to_string(path).map_err(|e| format!("读取失败: {}", e))?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string());
    generate_markdown(&source, name.as_deref())
}

fn render_program(program: &Program, file_name: Option<&str>) -> String {
    let mut out = String::new();

    let title = file_name
        .map(|s| s.to_string())
        .or_else(|| program.package_name.clone())
        .unwrap_or_else(|| "Qi 模块".to_string());

    out.push_str(&format!("# {}\n\n", title));

    if let Some(pkg) = &program.package_name {
        out.push_str(&format!("**包**: `{}`\n\n", pkg));
    }

    if !program.imports.is_empty() {
        out.push_str("## 导入\n\n");
        for imp in &program.imports {
            out.push_str(&format!("- {}\n", render_import(imp)));
        }
        out.push('\n');
    }

    let functions: Vec<&FunctionDeclaration> = program
        .statements
        .iter()
        .filter_map(|s| match s {
            AstNode::函数声明(f) => Some(f),
            _ => None,
        })
        .collect();

    let structs: Vec<&StructDeclaration> = program
        .statements
        .iter()
        .filter_map(|s| match s {
            AstNode::结构体声明(s) => Some(s),
            _ => None,
        })
        .collect();

    let enums: Vec<&EnumDeclaration> = program
        .statements
        .iter()
        .filter_map(|s| match s {
            AstNode::枚举声明(e) => Some(e),
            _ => None,
        })
        .collect();

    if !functions.is_empty() {
        out.push_str("## 函数\n\n");
        for f in functions {
            out.push_str(&render_function(f));
            out.push('\n');
        }
    }

    if !structs.is_empty() {
        out.push_str("## 结构体\n\n");
        for s in structs {
            out.push_str(&render_struct(s));
            out.push('\n');
        }
    }

    if !enums.is_empty() {
        out.push_str("## 枚举\n\n");
        for e in enums {
            out.push_str(&render_enum(e));
            out.push('\n');
        }
    }

    out
}

fn is_public(visibility: &Visibility) -> bool {
    matches!(visibility, Visibility::公开)
}

fn render_import(imp: &ImportStatement) -> String {
    let path = imp.module_path.join(".");
    let prefix = if imp.is_public {
        "公开 导入"
    } else {
        "导入"
    };
    match &imp.alias {
        Some(alias) => format!("`{} {} 作为 {}`", prefix, path, alias),
        None => format!("`{} {}`", prefix, path),
    }
}

fn render_function(f: &FunctionDeclaration) -> String {
    let signature = format_function_signature(f);
    let mut s = String::new();
    s.push_str(&format!("### `{}`\n\n", f.name));
    s.push_str("```qi\n");
    s.push_str(&signature);
    s.push_str("\n```\n");
    s
}

fn render_struct(st: &StructDeclaration) -> String {
    let mut s = String::new();
    s.push_str(&format!("### `{}`\n\n", st.name));

    if !st.fields.is_empty() {
        s.push_str("| 字段 | 类型 |\n");
        s.push_str("|------|------|\n");
        for field in &st.fields {
            s.push_str(&format!(
                "| `{}` | `{}` |\n",
                field.name,
                format_type(&field.type_annotation)
            ));
        }
        s.push('\n');
    }

    if !st.methods.is_empty() {
        s.push_str("**方法:**\n\n");
        for m in &st.methods {
            if !is_public(&m.visibility) {
                continue;
            }
            let params: Vec<String> = m.parameters.iter().map(format_parameter).collect();
            let ret = m
                .return_type
                .as_ref()
                .map(|t| format!(" -> {}", format_type(t)))
                .unwrap_or_default();
            s.push_str(&format!(
                "- `方法 {}({}){}`\n",
                m.method_name,
                params.join(", "),
                ret
            ));
        }
        s.push('\n');
    }

    s
}

fn render_enum(e: &EnumDeclaration) -> String {
    let mut s = String::new();
    s.push_str(&format!("### `{}`\n\n", e.name));
    for v in &e.variants {
        match v.value {
            Some(val) => s.push_str(&format!("- `{}` = {}\n", v.name, val)),
            None => s.push_str(&format!("- `{}`\n", v.name)),
        }
    }
    s.push('\n');
    s
}

fn format_function_signature(f: &FunctionDeclaration) -> String {
    let params: Vec<String> = f.parameters.iter().map(format_parameter).collect();
    let ret = f
        .return_type
        .as_ref()
        .map(|t| format!(" -> {}", format_type(t)))
        .unwrap_or_default();
    format!("函数 {}({}){}", f.name, params.join(", "), ret)
}

fn format_parameter(p: &Parameter) -> String {
    match &p.type_annotation {
        Some(ty) => format!("{}: {}", p.name, format_type(ty)),
        None => p.name.clone(),
    }
}

fn format_type(ty: &TypeNode) -> String {
    match ty {
        TypeNode::基础类型(b) => format!("{:?}", b),
        TypeNode::自定义类型(name) => name.clone(),
        TypeNode::数组类型(a) => match a.size {
            Some(n) => format!("[{}; {}]", format_type(&a.element_type), n),
            None => format!("[{}]", format_type(&a.element_type)),
        },
        TypeNode::列表类型(l) => format!("列表<{}>", format_type(&l.element_type)),
        TypeNode::集合类型(s) => format!("集合<{}>", format_type(&s.element_type)),
        TypeNode::字典类型(d) => format!(
            "字典<{}, {}>",
            format_type(&d.key_type),
            format_type(&d.value_type)
        ),
        TypeNode::通道类型(c) => format!("通道<{}>", format_type(&c.element_type)),
        TypeNode::指针类型(p) => format!("*{}", format_type(&p.target_type)),
        TypeNode::引用类型(r) => {
            if r.is_mutable {
                format!("&mut {}", format_type(&r.target_type))
            } else {
                format!("&{}", format_type(&r.target_type))
            }
        }
        TypeNode::未来类型(inner) => format!("未来<{}>", format_type(inner)),
        TypeNode::结果类型(r) => format!(
            "结果<{}, {}>",
            format_type(&r.ok_type),
            format_type(&r.err_type)
        ),
        TypeNode::选项类型(o) => format!("选项<{}>", format_type(&o.inner_type)),
        TypeNode::泛型类型(g) => {
            let args = g
                .type_arguments
                .iter()
                .map(format_type)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}<{}>", g.base_type, args)
        }
        _ => format!("{:?}", ty),
    }
}

#[allow(dead_code)]
fn field_summary(fields: &[StructField]) -> Vec<String> {
    fields
        .iter()
        .map(|f| format!("{}: {}", f.name, format_type(&f.type_annotation)))
        .collect()
}
