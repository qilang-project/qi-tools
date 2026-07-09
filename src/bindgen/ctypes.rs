//! C 类型 → Qi 类型的纯映射逻辑（不碰 libclang 句柄，方便单测）。
//!
//! 输入是 libclang 的 **canonical**（去 typedef 后）类型 kind，以及指针的
//! pointee 分类。输出是可映射的 Qi 类型或跳过原因。
//!
//! v2 升级：`外部` 编译器现支持 指针（void*/对象指针）参数与返回、char* 返回。
//! 因此映射规则相应放宽：
//!   - `void*` / 对象指针（`SHA256_CTX*` 等）参数与返回 → 指针（不透明句柄）
//!   - `char*` / `const char*` 返回 → 字符串（调用点拷贝进 Qi 堆串）
//!   - 函数指针参数、结构体按值传返 仍跳过（编译器支持手写，bindgen 暂不自动生成）

// clang-sys 的类型常量是 CXType_Int 这类混合大小写；模式匹配它们会触发
// non_upper_case_globals 警告，这里整体放行。
#![allow(non_upper_case_globals)]

use super::{QiParamType, QiRetType};
use clang_sys::*;

/// 指针 pointee 的分类（决定 char* → 字符串、fn* → 跳过、其余 → 指针）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointeeKind {
    /// pointee 是 char / signed char / unsigned char → const char* 语义
    Char,
    /// pointee 是函数（回调）→ bindgen 暂不自动生成
    Function,
    /// 其它任何 pointee（void*、结构体指针如 SHA256_CTX* 等）→ 不透明 指针
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
        // 指针：char* → 字符串；函数指针 → 跳过；其余（void*/对象指针）→ 指针
        CXType_Pointer => match pointee {
            Some(PointeeKind::Char) => Ok(QiParamType::字符串),
            Some(PointeeKind::Function) => {
                Err("参数是函数指针（回调 bindgen 暂不自动生成，请手写 外部 声明）".into())
            }
            _ => Ok(QiParamType::指针),
        },
        CXType_Void => Err("参数类型是 void".into()),
        CXType_Int128 | CXType_UInt128 => Err("参数是 128 位整数（无对应 Qi 类型）".into()),
        CXType_Record => {
            Err("参数是结构体/联合体（小结构体按值 bindgen 暂不自动生成，请手写）".into())
        }
        CXType_ConstantArray | CXType_IncompleteArray => Err("参数是数组".into()),
        CXType_FunctionProto | CXType_FunctionNoProto => Err("参数是函数指针".into()),
        _ => Err("参数类型不被 C FFI 支持".into()),
    }
}

/// 返回类型映射。v2：char* → 字符串（调用点拷贝）；void*/对象指针 → 指针。
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
        // char* 返回 → 字符串（拷贝进 Qi 堆串）；函数指针 → 跳过；其余 → 指针
        CXType_Pointer => match pointee {
            Some(PointeeKind::Char) => Ok(QiRetType::字符串),
            Some(PointeeKind::Function) => {
                Err("返回函数指针（bindgen 暂不自动生成，请手写 外部 声明）".into())
            }
            _ => Ok(QiRetType::指针),
        },
        CXType_Int128 | CXType_UInt128 => Err("返回 128 位整数（无对应 Qi 类型）".into()),
        CXType_Record => {
            Err("返回结构体/联合体（小结构体按值 bindgen 暂不自动生成，请手写）".into())
        }
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
    fn v2_对象指针映射为指针() {
        // void* / SHA256_CTX* 等 → 指针（OpenSSL ctx API 解锁）
        assert_eq!(
            map_param(CXType_Pointer, Some(PointeeKind::Other)),
            Ok(QiParamType::指针)
        );
        assert_eq!(map_param(CXType_Pointer, None), Ok(QiParamType::指针));
    }

    #[test]
    fn v2_函数指针参数被跳过() {
        assert!(map_param(CXType_Pointer, Some(PointeeKind::Function)).is_err());
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
    fn v2_返回char指针映射为字符串() {
        // strdup / getenv 返回 char* —— v2 拷贝进 Qi 堆串
        assert_eq!(
            map_ret(CXType_Pointer, Some(PointeeKind::Char)),
            Ok(QiRetType::字符串)
        );
    }

    #[test]
    fn v2_返回对象指针映射为指针() {
        // SHA256_CTX* 之类 → 指针
        assert_eq!(
            map_ret(CXType_Pointer, Some(PointeeKind::Other)),
            Ok(QiRetType::指针)
        );
    }

    #[test]
    fn v2_返回函数指针被跳过() {
        assert!(map_ret(CXType_Pointer, Some(PointeeKind::Function)).is_err());
    }

    #[test]
    fn 返回整数浮点布尔() {
        assert_eq!(map_ret(CXType_Long, None), Ok(QiRetType::整数)); // size_t
        assert_eq!(map_ret(CXType_Double, None), Ok(QiRetType::浮点数));
        assert_eq!(map_ret(CXType_Bool, None), Ok(QiRetType::布尔));
    }
}
