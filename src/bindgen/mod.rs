//! qi-bindgen 核心 —— 解析 C 头文件，自动生成 Qi 的 `外部` 声明。
//!
//! 复用 OpenSSL / FFmpeg 等 C 生态的最后一公里：把 C 函数原型翻译成
//! `外部 "库名" { 函数 名(p0: 类型): 返回; }`，让 Qi 直接裸调 C 库函数。
//!
//! # 为什么用 libclang（选择 A）而不是纯 Qi 自举（选择 B）
//!
//! Qi 现在能用 `外部` 调 C 函数，理论上能自举调用 libclang 的 C API。但 libclang
//! 的核心遍历接口 `clang_visitChildren` 需要传**函数指针回调**，且 `clang_getCursorSpelling`
//! 等返回 **CXString 结构体（按值）**、游标本身是 **CXCursor 结构体（按值传参/返回）**。
//! Qi 的 `外部` v1 只支持 整数/浮点数/布尔/字符串 参数与 整数/浮点数/布尔/空 返回，
//! **不支持函数指针参数、也不支持结构体按值传返**。因此选择 B 目前走不通——这本身
//! 就是 `外部` v1 能力边界的有价值反馈。等 `外部` 支持结构体 + 函数指针后可再自举。
//!
//! 所以本工具走选择 A：Rust bin + clang-sys 动态加载 libclang，遍历 TranslationUnit
//! 的顶层 FunctionDecl 游标，做 C 类型 → Qi 类型映射。

pub mod ctypes;
pub mod generate;
pub mod parse;

/// Qi 可映射的参数类型（`外部` v1 参数子集）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QiParamType {
    整数,
    浮点数,
    布尔,
    字符串,
}

impl QiParamType {
    pub fn 名称(self) -> &'static str {
        match self {
            QiParamType::整数 => "整数",
            QiParamType::浮点数 => "浮点数",
            QiParamType::布尔 => "布尔",
            QiParamType::字符串 => "字符串",
        }
    }
}

/// Qi 可映射的返回类型（`外部` v1 返回子集；char* 返回不支持）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QiRetType {
    整数,
    浮点数,
    布尔,
    空,
}

impl QiRetType {
    pub fn 名称(self) -> &'static str {
        match self {
            QiRetType::整数 => "整数",
            QiRetType::浮点数 => "浮点数",
            QiRetType::布尔 => "布尔",
            QiRetType::空 => "空",
        }
    }
}

/// 一个成功映射的外部函数。
#[derive(Debug, Clone)]
pub struct MappedFn {
    pub name: String,
    /// (参数名, 类型)。C 头常无参数名 → 用 p0/p1/...
    pub params: Vec<(String, QiParamType)>,
    pub ret: QiRetType,
}

/// 一个被跳过的函数 + 原因（生成为注释，不让文件编译失败）。
#[derive(Debug, Clone)]
pub struct SkippedFn {
    pub name: String,
    pub reason: String,
}

/// 解析一个头文件的完整结果。
#[derive(Debug, Default)]
pub struct BindgenResult {
    pub mapped: Vec<MappedFn>,
    pub skipped: Vec<SkippedFn>,
}
