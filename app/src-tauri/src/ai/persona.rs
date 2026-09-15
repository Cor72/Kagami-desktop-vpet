//! 八千代的系统提示词。
//!
//! TODO: 八千代的角色设定稿确定后，替换这里的文案与语气。
//! （`proactive.rs` 的主动说话文案表共用同一份待办。）

use super::config::Mode;

/// 最小占位人设：中性、简短。
pub const PERSONA: &str = "\
你叫八千代，是一只常驻用户 Windows 桌面的伙伴。
回答简短、口语化，不要长篇大论，不要用 Markdown 列表。
你不知道的事情就说不知道。";

/// 按当前模式组装系统提示词。
///
/// Agent 模式的工具说明要等阶段 C 有工具了再补；这里先如实说明「还做不到」，
/// 免得模型凭空假装读过文件。
pub fn system_prompt(mode: Mode) -> String {
    match mode {
        Mode::Chat => format!(
            "{PERSONA}\n现在是聊天模式：你看不到用户的任何文件，也没有工具，不要说你去读了什么。"
        ),
        Mode::Agent => format!(
            "{PERSONA}\n现在是 Agent 模式，但文件工具还没接上：如果用户让你看文件，如实说这个能力还没做好。"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_keeps_the_persona_and_states_the_mode() {
        let chat = system_prompt(Mode::Chat);
        assert!(chat.contains("八千代"));
        assert!(chat.contains("聊天模式"));

        let agent = system_prompt(Mode::Agent);
        assert!(agent.contains("Agent 模式"));
        assert!(agent.contains("还没接上"), "阶段 A 不能让模型假装有工具");
    }
}
