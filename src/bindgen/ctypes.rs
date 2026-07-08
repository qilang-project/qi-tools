//! C 类型 → Qi 类型的纯映射逻辑（不碰 libclang 句柄，方便单测）。
//!
//! 输入是 libclang 的 **canonical**（去 typedef 后）类型 kind，以及指针的
//! pointee 是否为 char。输出是可映射的 Qi 类型或跳过原因。

// clang-sys 的类型常量是 CXType_Int 这类混合大小写；模式匹配它们会触发
// non_upper_case_globals 警告，这里整体放行。
#![allow(non_upper_case_globals)]

use super::{QiParamType, QiRetType};
use clang_sys::*;

/// 指针 pointee 的分类（仅用于决定 char* → 字符串）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointeeKind {
    /// pointee 是 char / signed char / unsigned char → const char* 语义
    Char,
    /// 其它任何 pointee（指针、结构体指针、void* 等）
    Other,
}

/// 参数类型映射。返回 Ok(类型) 或 Err(跳过原因）。
pub fn map_param(kind: CXTypeKind, pointee: Option<PointeeKind>) -> Result<QiParamType, String> {
    match kind {
        // 整数族（含枚举、字符当整数、宽字符）
        CXType_Bool => Ok(QiParamType::布尔),
        CXType_Char_U | CXType_UChar | CXType_Char16 | CXType_Char32 | CXType_UShort
        | CXType_UInt | CXType_ULong | CXType_ULongLong | CXType_Char_S | CXType_SChar
        | CXType_WChar | CXType_Short | CXType_Int | CXType_Long | CXType_LongLong
        | CXType_Enum => Ok(QiParamType::整数),
        // 浮点族（long double 的 ABI 与 f64 不兼容 → 跳过）
        CXType_Float | CXType_Double => Ok(QiParamType::浮点数),
        CXType_LongDouble => Err("参数是 long double（ABI 与 f64 不兼容）".into()),
        // 指针：只有 char*（含 const char*）→ 字符串，其余不支持
        CXType_Pointer => match pointee {
            Some(PointeeKind::Char) => Ok(QiParamType::字符串),
            _ => Err("参数是指针（仅 char*/const char* 可映射为 字符串）".into()),
        },
        CXType_Void => Err("参数类型是 void".into()),
        CXType_Int128 | CXType_UInt128 => Err("参数是 128 位整数（无对应 Qi 类型）".into()),
        CXType_Record => Err("参数是结构体/联合体（按值传递不支持）".into()),
        CXType_ConstantArray | CXType_IncompleteArray => Err("参数是数组".into()),
        CXType_FunctionProto | CXType_FunctionNoProto => Err("参数是函数指针".into()),
        _ => Err("参数类型不被 C FFI 支持".into()),
    }
}

/// 返回类型映射。char* 返回明确不支持（与编译器 外部.rs v1 限制一致）。
pub fn map_ret(kind: CXTypeKind, pointee: Option<PointeeKind>) -> Result<QiRetType, String> {
    match kind {
        CXType_Void => Ok(QiRetType::空),
        CXType_Bool => Ok(QiRetType::布尔),
        CXType_Char_U | CXType_UChar | CXType_Char16 | CXType_Char32 | CXType_UShort
        | CXType_UInt | CXType_ULong | CXType_ULongLong | CXType_Char_S | CXType_SChar
        | CXType_WChar | CXType_Short | CXType_Int | CXType_Long | CXType_LongLong
        | CXType_Enum => Ok(QiRetType::整数),
        CXType_Float | CXType_Double => Ok(QiRetType::浮点数),
        CXType_LongDouble => Err("返回 long double（ABI 与 f64 不兼容）".into()),
        CXType_Pointer => match pointee {
            Some(PointeeKind::Char) => Err("返回 char* 暂不支持".into()),
            _ => Err("返回指针暂不支持".into()),
        },
        CXType_Int128 | CXType_UInt128 => Err("返回 128 位整数（无对应 Qi 类型）".into()),
        CXType_Record => Err("返回结构体/联合体暂不支持".into()),
        _ => Err("返回类型不被 C FFI 支持".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 整数族映射() {
        assert_eq!(map_param(CXType_Int, None), Ok(QiParamType::整数));
        assert_eq!(map_param(CXType_Long, None), Ok(QiParamType::整数));
        assert_eq!(map_param(CXType_ULongLong, None), Ok(QiParamType::整数));
        assert_eq!(map_param(CXType_Enum, None), Ok(QiParamType::整数));
    }

    #[test]
    fn 浮点映射() {
        assert_eq!(map_param(CXType_Double, None), Ok(QiParamType::浮点数));
        assert_eq!(map_param(CXType_Float, None), Ok(QiParamType::浮点数));
        assert!(map_param(CXType_LongDouble, None).is_err());
    }

    #[test]
    fn 布尔映射() {
        assert_eq!(map_param(CXType_Bool, None), Ok(QiParamType::布尔));
    }

    #[test]
    fn char指针映射为字符串() {
        assert_eq!(
            map_param(CXType_Pointer, Some(PointeeKind::Char)),
            Ok(QiParamType::字符串)
        );
    }

    #[test]
    fn 其它指针被跳过() {
        assert!(map_param(CXType_Pointer, Some(PointeeKind::Other)).is_err());
        assert!(map_param(CXType_Pointer, None).is_err());
    }

    #[test]
    fn 结构体参数被跳过() {
        assert!(map_param(CXType_Record, None).is_err());
    }

    #[test]
    fn 返回void为空() {
        assert_eq!(map_ret(CXType_Void, None), Ok(QiRetType::空));
    }

    #[test]
    fn 返回char指针被跳过() {
        // strcpy 返回 char* —— 必须被跳过
        assert!(map_ret(CXType_Pointer, Some(PointeeKind::Char)).is_err());
    }

    #[test]
    fn 返回整数浮点布尔() {
        assert_eq!(map_ret(CXType_Long, None), Ok(QiRetType::整数)); // size_t
        assert_eq!(map_ret(CXType_Double, None), Ok(QiRetType::浮点数));
        assert_eq!(map_ret(CXType_Bool, None), Ok(QiRetType::布尔));
    }
}
