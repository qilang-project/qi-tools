//! Qi 代码格式化器 - 基于 AST 的格式化实现
//! AST-based code formatter for Qi language.

mod config;
mod writer;

pub use config::FormatConfig;
pub use writer::CodeWriter;

use qi_compiler::parser::ast::{
    ArrayAccessExpression, ArrayLiteralExpression, AssignmentExpression, AstNode, AwaitExpression,
    BinaryExpression, BinaryOperator, BlockStatement, EnumDeclaration, ExpressionStatement,
    FieldAccessExpression, ForStatement, FunctionCallExpression, FunctionDeclaration,
    IdentifierExpression, IfStatement, ImportStatement, LiteralExpression, LiteralValue,
    MethodCallExpression, Parameter, Program, ReturnStatement, StringConcatExpression,
    StructDeclaration, StructField, StructFieldValue, StructLiteralExpression, TypeNode,
    UnaryExpression, UnaryOperator, VariableDeclaration, WhileStatement,
};
use qi_compiler::parser::Parser;

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

    /// 格式化 Qi 源文件。优先通过 AST 重新生成；解析失败时回退到文本格式化。
    pub fn format_file(&self, source: &str) -> Result<String, String> {
        match Parser::new().parse_source(source) {
            Ok(program) => Ok(self.format_program(&program)),
            Err(_) => Ok(self.simple_format(source)),
        }
    }

    fn format_program(&self, program: &Program) -> String {
        let mut out = String::new();

        if let Some(pkg) = &program.package_name {
            out.push_str(&format!("包 {};\n\n", pkg));
        }

        if !program.imports.is_empty() {
            for imp in &program.imports {
                out.push_str(&self.format_import(imp));
                out.push('\n');
            }
            out.push('\n');
        }

        for (idx, stmt) in program.statements.iter().enumerate() {
            if idx > 0 {
                if !out.ends_with("\n\n") {
                    out.push('\n');
                }
            }
            out.push_str(&self.format_node(stmt, 0));
            if !out.ends_with('\n') {
                out.push('\n');
            }
        }

        while out.ends_with("\n\n\n") {
            out.pop();
        }
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out
    }

    fn format_import(&self, imp: &ImportStatement) -> String {
        let path = imp.module_path.join(".");
        let prefix = if imp.is_public { "公开 导入" } else { "导入" };
        match &imp.alias {
            Some(alias) => format!("{} {} 作为 {};", prefix, path, alias),
            None => format!("{} {};", prefix, path),
        }
    }

    fn indent(&self, level: usize) -> String {
        self.config.indent_str().repeat(level)
    }

    fn format_node(&self, node: &AstNode, level: usize) -> String {
        match node {
            AstNode::函数声明(f) => self.format_function(f, level),
            AstNode::变量声明(v) => self.format_variable_decl(v, level),
            AstNode::结构体声明(s) => self.format_struct(s, level),
            AstNode::枚举声明(e) => self.format_enum(e, level),
            AstNode::如果语句(i) => self.format_if(i, level),
            AstNode::当语句(w) => self.format_while(w, level),
            AstNode::对于语句(f) => self.format_for(f, level),
            AstNode::返回语句(r) => self.format_return(r, level),
            AstNode::块语句(b) => self.format_block_stmt(b, level),
            AstNode::表达式语句(e) => self.format_expression_stmt(e, level),
            AstNode::跳出语句(_) => format!("{}跳出;", self.indent(level)),
            AstNode::继续语句(_) => format!("{}继续;", self.indent(level)),
            _ => {
                let mut s = self.indent(level);
                s.push_str(&self.format_expression(node));
                s.push(';');
                s
            }
        }
    }

    fn format_function(&self, f: &FunctionDeclaration, level: usize) -> String {
        let mut out = self.indent(level);
        out.push_str("函数 ");
        out.push_str(&f.name);
        out.push('(');
        out.push_str(&self.format_parameters(&f.parameters));
        out.push(')');
        if let Some(rt) = &f.return_type {
            out.push_str(": ");
            out.push_str(&self.format_type(rt));
        }
        out.push_str(" {\n");
        for stmt in &f.body {
            out.push_str(&self.format_node(stmt, level + 1));
            if !out.ends_with('\n') {
                out.push('\n');
            }
        }
        out.push_str(&self.indent(level));
        out.push('}');
        out.push('\n');
        out
    }

    fn format_parameters(&self, params: &[Parameter]) -> String {
        params
            .iter()
            .map(|p| match &p.type_annotation {
                Some(ty) => format!("{}: {}", p.name, self.format_type(ty)),
                None => p.name.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn format_variable_decl(&self, v: &VariableDeclaration, level: usize) -> String {
        let mut out = self.indent(level);
        out.push_str(if v.is_mutable { "变量 " } else { "常量 " });
        out.push_str(&v.name);
        if let Some(ty) = &v.type_annotation {
            out.push_str(": ");
            out.push_str(&self.format_type(ty));
        }
        if let Some(init) = &v.initializer {
            out.push_str(" = ");
            out.push_str(&self.format_expression(init));
        }
        out.push(';');
        out
    }

    fn format_struct(&self, s: &StructDeclaration, level: usize) -> String {
        let mut out = self.indent(level);
        out.push_str("结构体 ");
        out.push_str(&s.name);
        out.push_str(" {\n");
        for field in &s.fields {
            out.push_str(&self.format_struct_field(field, level + 1));
            out.push('\n');
        }
        out.push_str(&self.indent(level));
        out.push('}');
        out.push('\n');
        out
    }

    fn format_struct_field(&self, field: &StructField, level: usize) -> String {
        format!(
            "{}{}: {},",
            self.indent(level),
            field.name,
            self.format_type(&field.type_annotation)
        )
    }

    fn format_enum(&self, e: &EnumDeclaration, level: usize) -> String {
        let mut out = self.indent(level);
        out.push_str("枚举 ");
        out.push_str(&e.name);
        out.push_str(" {\n");
        for v in &e.variants {
            out.push_str(&self.indent(level + 1));
            out.push_str(&v.name);
            if let Some(val) = v.value {
                out.push_str(&format!(" = {}", val));
            }
            out.push_str(",\n");
        }
        out.push_str(&self.indent(level));
        out.push('}');
        out.push('\n');
        out
    }

    fn format_if(&self, i: &IfStatement, level: usize) -> String {
        let mut out = self.indent(level);
        out.push_str("如果 ");
        out.push_str(&self.format_expression(&i.condition));
        out.push_str(" {\n");
        for stmt in &i.then_branch {
            out.push_str(&self.format_node(stmt, level + 1));
            if !out.ends_with('\n') {
                out.push('\n');
            }
        }
        out.push_str(&self.indent(level));
        out.push('}');
        if let Some(else_branch) = &i.else_branch {
            out.push_str(" 否则 ");
            match else_branch.as_ref() {
                AstNode::如果语句(nested) => {
                    let inner = self.format_if(nested, level);
                    out.push_str(inner.trim_start());
                }
                AstNode::块语句(b) => {
                    out.push_str("{\n");
                    for stmt in &b.statements {
                        out.push_str(&self.format_node(stmt, level + 1));
                        if !out.ends_with('\n') {
                            out.push('\n');
                        }
                    }
                    out.push_str(&self.indent(level));
                    out.push('}');
                }
                other => {
                    out.push_str("{\n");
                    out.push_str(&self.format_node(other, level + 1));
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                    out.push_str(&self.indent(level));
                    out.push('}');
                }
            }
        }
        out
    }

    fn format_while(&self, w: &WhileStatement, level: usize) -> String {
        let mut out = self.indent(level);
        out.push_str("当 ");
        out.push_str(&self.format_expression(&w.condition));
        out.push_str(" {\n");
        for stmt in &w.body {
            out.push_str(&self.format_node(stmt, level + 1));
            if !out.ends_with('\n') {
                out.push('\n');
            }
        }
        out.push_str(&self.indent(level));
        out.push('}');
        out
    }

    fn format_for(&self, f: &ForStatement, level: usize) -> String {
        let mut out = self.indent(level);
        out.push_str("对于 ");
        out.push_str(&f.variable);
        out.push_str(" 在 ");
        out.push_str(&self.format_expression(&f.range));
        out.push_str(" {\n");
        for stmt in &f.body {
            out.push_str(&self.format_node(stmt, level + 1));
            if !out.ends_with('\n') {
                out.push('\n');
            }
        }
        out.push_str(&self.indent(level));
        out.push('}');
        out
    }

    fn format_return(&self, r: &ReturnStatement, level: usize) -> String {
        let mut out = self.indent(level);
        out.push_str("返回");
        if let Some(v) = &r.value {
            out.push(' ');
            out.push_str(&self.format_expression(v));
        }
        out.push(';');
        out
    }

    fn format_block_stmt(&self, b: &BlockStatement, level: usize) -> String {
        let mut out = self.indent(level);
        out.push_str("{\n");
        for stmt in &b.statements {
            out.push_str(&self.format_node(stmt, level + 1));
            if !out.ends_with('\n') {
                out.push('\n');
            }
        }
        out.push_str(&self.indent(level));
        out.push('}');
        out
    }

    fn format_expression_stmt(&self, e: &ExpressionStatement, level: usize) -> String {
        let mut out = self.indent(level);
        out.push_str(&self.format_expression(&e.expression));
        out.push(';');
        out
    }

    fn format_expression(&self, node: &AstNode) -> String {
        match node {
            AstNode::字面量表达式(lit) => self.format_literal(lit),
            AstNode::标识符表达式(id) => self.format_identifier(id),
            AstNode::二元操作表达式(b) => self.format_binary(b),
            AstNode::一元操作表达式(u) => self.format_unary(u),
            AstNode::函数调用表达式(c) => self.format_call(c),
            AstNode::赋值表达式(a) => self.format_assignment(a),
            AstNode::数组访问表达式(a) => self.format_array_access(a),
            AstNode::数组字面量表达式(a) => self.format_array_literal(a),
            AstNode::字符串连接表达式(c) => self.format_string_concat(c),
            AstNode::字段访问表达式(f) => self.format_field_access(f),
            AstNode::方法调用表达式(m) => self.format_method_call(m),
            AstNode::结构体实例化表达式(s) => self.format_struct_literal(s),
            AstNode::等待表达式(a) => self.format_await(a),
            _ => format!("/* {:?} */", std::any::type_name::<AstNode>()),
        }
    }

    fn format_literal(&self, lit: &LiteralExpression) -> String {
        match &lit.value {
            LiteralValue::整数(n) => n.to_string(),
            LiteralValue::浮点数(f) => {
                if f.fract() == 0.0 && f.is_finite() {
                    format!("{:.1}", f)
                } else {
                    f.to_string()
                }
            }
            LiteralValue::字符串(s) => format!("\"{}\"", escape_string(s)),
            LiteralValue::布尔(b) => {
                if *b {
                    "真".to_string()
                } else {
                    "假".to_string()
                }
            }
            LiteralValue::字符(c) => format!("'{}'", c),
        }
    }

    fn format_identifier(&self, id: &IdentifierExpression) -> String {
        id.name.clone()
    }

    fn format_binary(&self, b: &BinaryExpression) -> String {
        format!(
            "{} {} {}",
            self.format_expression(&b.left),
            format_binary_op(b.operator),
            self.format_expression(&b.right)
        )
    }

    fn format_unary(&self, u: &UnaryExpression) -> String {
        let op = match u.operator {
            UnaryOperator::非 => "!",
            UnaryOperator::负 => "-",
            UnaryOperator::正 => "+",
        };
        format!("{}{}", op, self.format_expression(&u.operand))
    }

    fn format_call(&self, c: &FunctionCallExpression) -> String {
        let args = c
            .arguments
            .iter()
            .map(|a| self.format_expression(a))
            .collect::<Vec<_>>()
            .join(", ");
        match &c.module_qualifier {
            Some(q) => format!("{}.{}({})", q, c.callee, args),
            None => format!("{}({})", c.callee, args),
        }
    }

    fn format_assignment(&self, a: &AssignmentExpression) -> String {
        format!(
            "{} = {}",
            self.format_expression(&a.target),
            self.format_expression(&a.value)
        )
    }

    fn format_array_access(&self, a: &ArrayAccessExpression) -> String {
        format!(
            "{}[{}]",
            self.format_expression(&a.array),
            self.format_expression(&a.index)
        )
    }

    fn format_array_literal(&self, a: &ArrayLiteralExpression) -> String {
        let items = a
            .elements
            .iter()
            .map(|e| self.format_expression(e))
            .collect::<Vec<_>>()
            .join(", ");
        format!("[{}]", items)
    }

    fn format_string_concat(&self, c: &StringConcatExpression) -> String {
        format!(
            "{} + {}",
            self.format_expression(&c.left),
            self.format_expression(&c.right)
        )
    }

    fn format_field_access(&self, f: &FieldAccessExpression) -> String {
        format!("{}.{}", self.format_expression(&f.object), f.field)
    }

    fn format_method_call(&self, m: &MethodCallExpression) -> String {
        let args = m
            .arguments
            .iter()
            .map(|a| self.format_expression(a))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "{}.{}({})",
            self.format_expression(&m.object),
            m.method_name,
            args
        )
    }

    fn format_struct_literal(&self, s: &StructLiteralExpression) -> String {
        let fields = s
            .fields
            .iter()
            .map(|f: &StructFieldValue| {
                format!("{}: {}", f.name, self.format_expression(&f.value))
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!("{} {{ {} }}", s.struct_name, fields)
    }

    fn format_await(&self, a: &AwaitExpression) -> String {
        format!("等待 {}", self.format_expression(&a.expression))
    }

    fn format_type(&self, ty: &TypeNode) -> String {
        match ty {
            TypeNode::基础类型(b) => format!("{:?}", b),
            TypeNode::自定义类型(name) => name.clone(),
            TypeNode::数组类型(a) => match a.size {
                Some(n) => format!("[{}; {}]", self.format_type(&a.element_type), n),
                None => format!("[{}]", self.format_type(&a.element_type)),
            },
            TypeNode::列表类型(l) => format!("列表<{}>", self.format_type(&l.element_type)),
            TypeNode::集合类型(s) => format!("集合<{}>", self.format_type(&s.element_type)),
            TypeNode::字典类型(d) => format!(
                "字典<{}, {}>",
                self.format_type(&d.key_type),
                self.format_type(&d.value_type)
            ),
            TypeNode::通道类型(c) => format!("通道<{}>", self.format_type(&c.element_type)),
            TypeNode::指针类型(p) => format!("*{}", self.format_type(&p.target_type)),
            TypeNode::引用类型(r) => {
                if r.is_mutable {
                    format!("&mut {}", self.format_type(&r.target_type))
                } else {
                    format!("&{}", self.format_type(&r.target_type))
                }
            }
            TypeNode::未来类型(inner) => format!("未来<{}>", self.format_type(inner)),
            TypeNode::结果类型(r) => format!(
                "结果<{}, {}>",
                self.format_type(&r.ok_type),
                self.format_type(&r.err_type)
            ),
            TypeNode::选项类型(o) => format!("选项<{}>", self.format_type(&o.inner_type)),
            TypeNode::泛型类型(g) => {
                let args = g
                    .type_arguments
                    .iter()
                    .map(|t| self.format_type(t))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}<{}>", g.base_type, args)
            }
            _ => format!("{:?}", ty),
        }
    }

    fn simple_format(&self, source: &str) -> String {
        let mut preprocessed = String::new();
        let chars: Vec<char> = source.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            let ch = chars[i];
            match ch {
                '{' | '【' => {
                    preprocessed.push(ch);
                    preprocessed.push('\n');
                }
                '}' | '】' => {
                    if !preprocessed.ends_with('\n') {
                        preprocessed.push('\n');
                    }
                    preprocessed.push(ch);
                    preprocessed.push('\n');
                }
                ';' | '；' => {
                    preprocessed.push(ch);
                    preprocessed.push('\n');
                }
                _ => preprocessed.push(ch),
            }
            i += 1;
        }

        let mut result = String::new();
        let mut indent_level: usize = 0;
        let mut prev_was_closing_brace = false;

        for line in preprocessed.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if prev_was_closing_brace && indent_level == 0 {
                if !trimmed.starts_with('}') && !trimmed.starts_with('】') {
                    result.push('\n');
                }
            }

            if trimmed.starts_with("包 ") || trimmed.starts_with("包\t") {
                let indent = self.config.indent_str().repeat(indent_level);
                result.push_str(&indent);
                result.push_str(trimmed);
                result.push('\n');
                result.push('\n');
                prev_was_closing_brace = false;
                continue;
            }

            if trimmed.starts_with('}') || trimmed.starts_with('】') {
                if indent_level > 0 {
                    indent_level -= 1;
                }
                prev_was_closing_brace = true;
            } else {
                prev_was_closing_brace = false;
            }

            let indent = self.config.indent_str().repeat(indent_level);
            result.push_str(&indent);
            result.push_str(trimmed);
            result.push('\n');

            if trimmed.ends_with('{') || trimmed.ends_with('【') {
                indent_level += 1;
            }
        }

        result
    }
}

impl Default for Formatter {
    fn default() -> Self {
        Self::new()
    }
}

fn format_binary_op(op: BinaryOperator) -> &'static str {
    match op {
        BinaryOperator::加 => "+",
        BinaryOperator::减 => "-",
        BinaryOperator::乘 => "*",
        BinaryOperator::除 => "/",
        BinaryOperator::取余 => "%",
        BinaryOperator::等于 => "==",
        BinaryOperator::不等于 => "!=",
        BinaryOperator::大于 => ">",
        BinaryOperator::小于 => "<",
        BinaryOperator::大于等于 => ">=",
        BinaryOperator::小于等于 => "<=",
        BinaryOperator::与 => "&&",
        BinaryOperator::或 => "||",
    }
}

fn escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_format() {
        let source = "包 测试;\n函数 示例() {\n变量 x = 10;\n}";
        let formatter = Formatter::new();
        let result = formatter.format_file(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_indent() {
        let source = "函数 测试() { 如果 真 { 打印(1); } }";
        let formatter = Formatter::new();
        let result = formatter.format_file(source).unwrap();
        assert!(result.contains("    "));
    }

    #[test]
    fn test_fallback_on_parse_error() {
        let source = "不是有效的 Qi { 代码";
        let formatter = Formatter::new();
        let result = formatter.format_file(source);
        assert!(result.is_ok());
    }
}
