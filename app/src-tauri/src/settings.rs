use serde::Serialize;
use std::sync::Mutex;

pub type PetState = Mutex<PetSettings>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetSettings {
    pub revision: u64,
    pub visible: bool,
    pub max_fps: u32,
    pub always_on_top: bool,
}

impl Default for PetSettings {
    fn default() -> Self {
        Self {
            revision: 0,
            visible: true,
            max_fps: 30,
            always_on_top: true,
        }
    }
}

#[derive(Clone, Copy)]
pub enum SettingsChange {
    Visible(bool),
    MaxFps(u32),
    AlwaysOnTop(bool),
}

impl PetSettings {
    pub fn update(
        &mut self,
        change: SettingsChange,
        apply_window: impl FnOnce() -> Result<(), String>,
    ) -> Result<Self, String> {
        let mut next = self.clone();
        match change {
            SettingsChange::Visible(visible) => next.visible = visible,
            SettingsChange::AlwaysOnTop(enabled) => next.always_on_top = enabled,
            SettingsChange::MaxFps(fps) => {
                if fps != 15 && fps != 30 {
                    return Err("帧率只能是 15 或 30".into());
                }
                next.max_fps = fps;
            }
        }
        // 先完成系统操作，失败时保留原状态和版本号。
        apply_window()?;
        next.revision += 1;
        *self = next.clone();
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_window_change_does_not_publish_new_state() {
        let mut settings = PetSettings::default();
        let result = settings.update(
            SettingsChange::Visible(false),
            || Err("窗口隐藏失败".into()),
        );
        assert!(result.is_err());
        assert!(settings.visible);
        assert_eq!(settings.revision, 0);
    }

    #[test]
    fn unsupported_fps_is_rejected_before_applying_changes() {
        let mut settings = PetSettings::default();
        for fps in [0, 14, 16, 60] {
            assert!(settings
                .update(SettingsChange::MaxFps(fps), || panic!("不应操作窗口"))
                .is_err());
            assert_eq!(settings.max_fps, 30);
            assert_eq!(settings.revision, 0);
        }
    }

    #[test]
    fn successful_changes_preserve_other_settings_and_advance_revision() {
        let mut settings = PetSettings::default();
        settings
            .update(SettingsChange::MaxFps(15), || Ok(()))
            .unwrap();
        settings
            .update(SettingsChange::Visible(false), || Ok(()))
            .unwrap();
        let result = settings
            .update(SettingsChange::AlwaysOnTop(false), || Ok(()))
            .unwrap();
        assert_eq!(
            result,
            PetSettings {
                revision: 3,
                visible: false,
                max_fps: 15,
                always_on_top: false
            }
        );
        assert_eq!(settings, result);
    }
}
