//! API Key 的存放。
//!
//! 硬约束（计划 §10 第 5 条）：**绝不回显、绝不进日志、绝不进 json**。
//!
//! 存放位置是 Windows 凭据管理器（`keyring` crate），
//! service = `yachiyo-desktop`，account = 服务商名（`deepseek` / `openai` / `custom`）。
//! 前端只能拿到 [`mask`] 生成的掩码，永远拿不到全文。

use keyring::Entry;

use super::config::Provider;

/// 凭据的 service 名。同名的凭据会在「Windows 凭据管理器 → 普通凭据」里显示出来。
pub const SERVICE: &str = "yachiyo-desktop";

/// 掩码里保留的尾部字符数。
const TAIL_CHARS: usize = 4;
/// 短于这个长度的 Key 一律整只隐去，不显示任何片段。
const MIN_CHARS_FOR_PARTIAL_MASK: usize = 12;

fn entry(provider: Provider) -> Result<Entry, String> {
    Entry::new(SERVICE, provider.id()).map_err(|error| {
        let reason = describe(error);
        format!("打开凭据管理器失败：{reason}")
    })
}

/// 把 Key 写进凭据管理器。空 Key 直接拒绝——「保存空值」通常意味着前端出了错。
pub fn save(provider: Provider, key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("API Key 不能为空".into());
    }
    entry(provider)?.set_password(key).map_err(|error| {
        let reason = describe(error);
        format!("保存 API Key 失败：{reason}")
    })
}

/// 读 Key。没存过返回 `Ok(None)`——这不是错误。
pub fn load(provider: Provider) -> Result<Option<String>, String> {
    match entry(provider)?.get_password() {
        Ok(key) if key.trim().is_empty() => Ok(None),
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => {
            let reason = describe(error);
            Err(format!("读取 API Key 失败：{reason}"))
        }
    }
}

/// 删除 Key。本来就没有也算成功（幂等）。
pub fn clear(provider: Provider) -> Result<(), String> {
    match entry(provider)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => {
            let reason = describe(error);
            Err(format!("删除 API Key 失败：{reason}"))
        }
    }
}

/// 生成给用户看的掩码，例如 `sk-****1234`。
///
/// 规则：保留开头到第一个 `-`（最多 3 个字符）与最后 4 个字符，中间一律 `****`。
/// 太短的 Key 整只隐去——否则「掩码」会把整个 Key 显示出来。
pub fn mask(key: &str) -> String {
    let chars: Vec<char> = key.trim().chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    if chars.len() < MIN_CHARS_FOR_PARTIAL_MASK {
        return "******".into();
    }
    let head_len = chars
        .iter()
        .take(4)
        .position(|character| *character == '-')
        .map(|index| index + 1)
        // 没有 `-` 前缀时只露 3 个字符，够用户认得出是哪一把 key。
        .unwrap_or(3);
    let head: String = chars[..head_len].iter().collect();
    let tail: String = chars[chars.len() - TAIL_CHARS..].iter().collect();
    format!("{head}****{tail}")
}

/// keyring 的错误文本是英文的，这里补一句中文场景说明；
/// **不改写原因**，免得掩盖真实故障（比如系统策略禁止访问凭据管理器）。
fn describe(error: keyring::Error) -> String {
    match error {
        keyring::Error::NoEntry => "凭据管理器里没有这条记录".into(),
        keyring::Error::TooLong(name, limit) => {
            format!("「{name}」太长，凭据管理器最多接受 {limit} 个字符")
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_keeps_the_recognizable_prefix_and_tail() {
        assert_eq!(mask("sk-1234567890abcdef"), "sk-****cdef");
        assert_eq!(mask("  sk-abcdefghijklmnop  "), "sk-****mnop");
    }

    #[test]
    fn mask_hides_short_keys_completely() {
        // 短 Key 如果按「前 3 + 后 4」显示，等于把整把 Key 露出来。
        assert_eq!(mask("shortkey"), "******");
        assert_eq!(mask("abcdefghijk"), "******");
        assert_eq!(mask(""), "");
        assert_eq!(mask("   "), "");
    }

    #[test]
    fn mask_works_for_keys_without_a_dash() {
        assert_eq!(mask("abcdefghijklmnop"), "abc****mnop");
    }

    #[test]
    fn mask_never_contains_the_middle_of_the_key() {
        let key = "sk-0123456789SECRETPART0000";
        let masked = mask(key);
        assert!(!masked.contains("SECRET"), "掩码泄露了中段：{masked}");
        assert_eq!(masked, "sk-****0000");
    }
}
