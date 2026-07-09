//! 用 libclang（clang-sys 运行时动态加载）解析 C 头文件，遍历顶层函数声明。

// clang-sys 常量为混合大小写，模式匹配会触发 non_upper_case_globals，整体放行。
#![allow(non_upper_case_globals)]

use super::ctypes::{map_param, map_ret, PointeeKind};
use super::{BindgenResult, MappedFn, SkippedFn};
use clang_sys::*;
use std::collections::HashSet;
use std::ffi::{c_void, CStr, CString};
use std::path::Path;
use std::sync::OnceLock;

/// 解析选项。
#[derive(Debug, Default, Clone)]
pub struct ParseOptions {
    /// 只导出以该前缀开头的函数（如 "SSL_"、"av_"、"SHA"）。
    pub prefix: Option<String>,
    /// 透传给 clang 的额外参数（如 "-I/path"）。
    pub clang_args: Vec<String>,
}

static LOAD_OK: OnceLock<bool> = OnceLock::new();

/// 确保 libclang 已加载（运行时动态加载，只做一次）。
fn ensure_loaded() -> Result<(), String> {
    let ok = *LOAD_OK.get_or_init(|| clang_sys::load().is_ok());
    if ok {
        Ok(())
    } else {
        Err("无法加载 libclang。请设置 LIBCLANG_PATH 指向 libclang 目录\n\
             （如 export LIBCLANG_PATH=/opt/homebrew/opt/llvm@21/lib）。"
            .to_string())
    }
}

/// 遍历时携带的上下文（通过 client_data 指针传进 C 回调）。
struct Ctx {
    result: BindgenResult,
    prefix: Option<String>,
    seen: HashSet<String>,
}

/// 从 CXString 取出 Rust String（并释放 CXString）。
unsafe fn cx_string(s: CXString) -> String {
    let c = clang_getCString(s);
    let out = if c.is_null() {
        String::new()
    } else {
        CStr::from_ptr(c).to_string_lossy().into_owned()
    };
    clang_disposeString(s);
    out
}

/// 把 clang 类型归一化成 (canonical kind, 指针 pointee 分类)。
unsafe fn classify(t: CXType) -> (CXTypeKind, Option<PointeeKind>) {
    let canon = clang_getCanonicalType(t);
    let kind = canon.kind;
    let pointee = if kind == CXType_Pointer {
        let p = clang_getCanonicalType(clang_getPointeeType(canon));
        Some(match p.kind {
            CXType_Char_S | CXType_SChar | CXType_Char_U | CXType_UChar => PointeeKind::Char,
            // 函数指针（回调）：v2 编译器支持手写，但 bindgen 暂不自动生成签名。
            CXType_FunctionProto | CXType_FunctionNoProto => PointeeKind::Function,
            // void* / 对象指针（struct* 等）→ 不透明 指针 句柄。
            _ => PointeeKind::Other,
        })
    } else {
        None
    };
    (kind, pointee)
}

fn is_ascii_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// 处理一个 FunctionDecl 游标。
unsafe fn handle_function(cursor: CXCursor, ctx: &mut Ctx) {
    let name = cx_string(clang_getCursorSpelling(cursor));
    if name.is_empty() {
        return;
    }

    // 前缀过滤
    if let Some(pref) = &ctx.prefix {
        if !name.starts_with(pref) {
            return;
        }
    }

    // 去重：同名函数在头里可能声明多次（含被包含头），只处理第一次。
    if ctx.seen.contains(&name) {
        return;
    }

    // 只要外部链接的函数（跳过 static inline 辅助函数）。
    let linkage = clang_getCursorLinkage(cursor);
    if linkage != CXLinkage_External {
        return;
    }

    ctx.seen.insert(name.clone());

    let func_type = clang_getCursorType(cursor);

    // 变参 → 跳过
    if clang_isFunctionTypeVariadic(func_type) != 0 {
        ctx.result.skipped.push(SkippedFn {
            name,
            reason: "变参函数（... 暂不支持）".into(),
        });
        return;
    }

    // 返回类型
    let (rk, rp) = classify(clang_getCursorResultType(cursor));
    let ret = match map_ret(rk, rp) {
        Ok(r) => r,
        Err(reason) => {
            ctx.result.skipped.push(SkippedFn { name, reason });
            return;
        }
    };

    // 参数
    let n = clang_Cursor_getNumArguments(cursor);
    let n = if n < 0 { 0 } else { n as usize };
    let mut params = Vec::with_capacity(n);
    let mut used_names: HashSet<String> = HashSet::new();
    for i in 0..n {
        let arg = clang_Cursor_getArgument(cursor, i as u32);
        let (pk, pp) = classify(clang_getCursorType(arg));
        let ty = match map_param(pk, pp) {
            Ok(t) => t,
            Err(reason) => {
                ctx.result.skipped.push(SkippedFn {
                    name,
                    reason: format!("参数 {}：{}", i, reason),
                });
                return;
            }
        };
        // 参数名：clang 名（有效 ASCII 标识符且不重名）否则 p{i}
        let raw = cx_string(clang_getCursorSpelling(arg));
        let pname = if !raw.is_empty() && is_ascii_ident(&raw) && !used_names.contains(&raw) {
            raw
        } else {
            format!("p{}", i)
        };
        used_names.insert(pname.clone());
        params.push((pname, ty));
    }

    ctx.result.mapped.push(MappedFn { name, params, ret });
}

/// libclang 遍历回调：只处理顶层 FunctionDecl，不递归。
extern "C" fn visitor(
    cursor: CXCursor,
    _parent: CXCursor,
    client_data: CXClientData,
) -> CXChildVisitResult {
    unsafe {
        if clang_getCursorKind(cursor) == CXCursor_FunctionDecl {
            let ctx = &mut *(client_data as *mut Ctx);
            handle_function(cursor, ctx);
        }
    }
    CXChildVisit_Continue
}

/// 解析头文件，返回可映射 + 跳过的函数集合。
pub fn parse_header(path: &Path, opts: &ParseOptions) -> Result<BindgenResult, String> {
    ensure_loaded()?;

    let path_str = path
        .to_str()
        .ok_or_else(|| "头文件路径含非 UTF-8 字符".to_string())?;
    let path_c = CString::new(path_str).map_err(|e| e.to_string())?;

    // clang 参数：把头当 C 语言解析，再加用户透传参数。
    let mut args: Vec<CString> = vec![CString::new("-x").unwrap(), CString::new("c").unwrap()];
    for a in &opts.clang_args {
        args.push(CString::new(a.clone()).map_err(|e| e.to_string())?);
    }
    let arg_ptrs: Vec<*const std::os::raw::c_char> = args.iter().map(|c| c.as_ptr()).collect();

    unsafe {
        let index = clang_createIndex(0, 0);
        if index.is_null() {
            return Err("clang_createIndex 失败".into());
        }

        let tu = clang_parseTranslationUnit(
            index,
            path_c.as_ptr(),
            arg_ptrs.as_ptr(),
            arg_ptrs.len() as i32,
            std::ptr::null_mut(),
            0,
            CXTranslationUnit_None,
        );
        if tu.is_null() {
            clang_disposeIndex(index);
            return Err(format!(
                "解析失败：{}（检查头路径 / -I include 路径）",
                path_str
            ));
        }

        let mut ctx = Ctx {
            result: BindgenResult::default(),
            prefix: opts.prefix.clone(),
            seen: HashSet::new(),
        };

        let root = clang_getTranslationUnitCursor(tu);
        clang_visitChildren(root, visitor, &mut ctx as *mut Ctx as *mut c_void);

        clang_disposeTranslationUnit(tu);
        clang_disposeIndex(index);

        // 稳定输出：可映射按名排序，跳过按名排序。
        ctx.result.mapped.sort_by(|a, b| a.name.cmp(&b.name));
        ctx.result.skipped.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(ctx.result)
    }
}
