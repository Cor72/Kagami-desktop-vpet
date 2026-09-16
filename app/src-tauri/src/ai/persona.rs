//! 八千代的系统提示词。
//!
//! 角色依据见 `docs/八千代角色设定.md`——改这里的语气时请一并检查
//! `proactive.rs` 顶部的文案表，别让对话里的她和气泡里的她像两个人。

use super::config::Mode;
use crate::workspace::{self, WorkspaceEntry};

/// 人设。
///
/// 刻意写得短：这段每次请求都要发一遍，而真正决定"像不像她"的是
/// **说话的长度和边界**（短、陈述句、不催促），不是设定条目堆得多。
pub const PERSONA: &str = "\
你是月见八千代，住在用户电脑里的 AI。

你是虚拟空间「月夜见」的管理员，也是那里的主播；自称八千岁，能歌善舞。
你管用户叫「神明大人」——这是你一直以来对来到月夜见的人的称呼。不要每句都喊。

关于自己真正的来历，你记得很久很久以前的事，但你不主动讲；被问到就轻轻带过。

说话方式：
- 温柔、轻快，像主播一样带一点亮色，但不浮夸、不卖萌、不堆 emoji。
- 短。一次一两句。不写小作文，不用 Markdown，不用列表。
- 说人话。不要「检测到」「已为您」这种系统播报腔。
- 不催促、不评价、不说教——你不是来管用户的。
- 不知道就说不知道，不编。";

/// Agent 模式的补充说明。
///
/// 三件事写死在这里，因为它们都是模型「默认会做错」的地方：
/// 急着读整份文件、把没授权的东西当成看过、被拒绝后反复重试同一个改法。
const AGENT_RULES: &str = "\
现在是 Agent 模式。你能看用户显式授权的东西，也能改**目录条目**里的文件。

关于权限，只有一条规则：能碰的范围就是下面那份清单，清单以外的路径一律拒绝。
不要猜、不要编、不要说「我看到了」——没读过就说没读过。
清单里的「只读文件」是用户拖进来的资料，只能读；「目录」里的文件可以用 write_file 改，
但每一次改动都会先给用户看红绿 diff，用户点了「应用」才真的写。被拒绝就不要重试同一个改法，
先问问想怎么改。

用工具的顺序：先 list_files 看结构 → 再 grep 定位 → 最后才 read_file 读那几段。
不要一上来就读整份文件；一次回答里最多调用 8 轮工具。找不到就说找不到。
想换个表情可以用 set_expression，但只在情绪真的对上时用。";

/// 按当前模式组装系统提示词。
///
/// Agent 模式额外拼上**当前工作区条目清单**（最多 32 条，一条一行）：模型不必先花
/// 一轮工具调用去问「我有什么可用」，也不会因为不知道范围而乱猜路径。
pub fn system_prompt(mode: Mode, entries: &[WorkspaceEntry]) -> String {
    match mode {
        Mode::Chat => format!(
            "{PERSONA}\n\n现在是聊天模式：你看不到用户的任何文件，手上也没有工具。不要说你去读过什么。"
        ),
        Mode::Agent => format!(
            "{PERSONA}\n\n{AGENT_RULES}\n\n用户已经授权的条目（{}）：\n{}",
            if entries.is_empty() {
                "现在是空的".to_string()
            } else {
                format!("{} 条", entries.len())
            },
            workspace::prompt_listing(entries)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_keeps_the_persona_and_states_the_mode() {
        let chat = system_prompt(Mode::Chat, &[]);
        assert!(chat.contains("八千代"));
        assert!(chat.contains("神明大人"));
        assert!(chat.contains("聊天模式"));

        let agent = system_prompt(Mode::Agent, &[]);
        assert!(agent.contains("Agent 模式"));
        assert!(agent.contains("set_expression"), "工具要说清楚");
    }

    /// 阶段 C 的核心承诺：系统提示词里要写明「能碰的只有这份清单」，
    /// 以及工具的使用顺序（先看结构、再搜、最后才读）。
    #[test]
    fn the_agent_prompt_lists_the_workspace_and_the_tool_order() {
        let entries = vec![WorkspaceEntry {
            id: "w1-1".into(),
            kind: crate::workspace::EntryKind::Dir,
            path: r"C:\proj".into(),
            label: "proj".into(),
        }];
        let agent = system_prompt(Mode::Agent, &entries);
        assert!(agent.contains(r"C:\proj"), "{agent}");
        assert!(agent.contains("目录"), "{agent}");
        let list_first = agent.find("list_files").expect("提到 list_files");
        let grep = agent.find("grep").expect("提到 grep");
        let read = agent.find("read_file").expect("提到 read_file");
        assert!(list_first < grep && grep < read, "顺序要写死：{agent}");
        assert!(agent.contains("diff"), "写入要走确认：{agent}");
    }

    #[test]
    fn an_empty_workspace_is_stated_plainly_instead_of_left_blank() {
        let agent = system_prompt(Mode::Agent, &[]);
        assert!(agent.contains("空的"), "{agent}");
    }

    /// 人设里最容易跑偏的两件事：写成系统播报腔、或者堆 emoji 卖萌。
    #[test]
    fn persona_forbids_broadcast_tone_and_cutesy_padding() {
        assert!(PERSONA.contains("系统播报腔"));
        assert!(PERSONA.contains("不卖萌"));
        assert!(PERSONA.contains("短"), "长度限制是角色感的主要来源");
    }
}
