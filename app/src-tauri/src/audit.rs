//! 可选的 Release 验收入口。通过应用自己的窗口服务重现隐藏、还原和最小化。
//! 只在 perf-audit 特性 + YACHIYO_AUDIT_DIR 环境变量同时存在时运行。
use crate::{desktop, settings::SettingsChange};
use serde_json::{json, Value};
use std::{
    fs::{self, File},
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Listener, Manager};

fn idle_sample_is_valid(entry: &Value, settled: bool) -> bool {
    let native_running = entry["nativeVisible"] == true
        && entry["minimized"] == false
        && entry["settings"]["visible"] == true
        && entry["settings"]["maxFps"] == 30;
    let report_age = entry["timestampMs"]
        .as_u64()
        .zip(entry["renderer"]["reportedAt"].as_u64())
        .and_then(|(now, reported)| now.checked_sub(reported));
    native_running
        && (!settled
            || (entry["renderer"]["running"] == true
                && entry["renderer"]["pageVisible"] == true
                && entry["renderer"]["maxFps"] == 30
                && report_age.is_some_and(|age| age <= 10000)))
}

pub fn start(app: &AppHandle) {
    let Some(directory) = std::env::var_os("YACHIYO_AUDIT_DIR") else {
        return;
    };
    let directory = PathBuf::from(directory);
    let latest = Arc::new(Mutex::new(Value::Null));
    let receiver = Arc::clone(&latest);
    app.listen("pet-render-metrics", move |event| {
        if let Ok(value) = serde_json::from_str(event.payload()) {
            if let Ok(mut snapshot) = receiver.lock() {
                *snapshot = value;
            }
        }
    });
    let app = app.clone();
    thread::spawn(move || {
        if let Err(error) = run(&app, &directory, latest) {
            let _ = fs::write(directory.join("error.txt"), &error);
            eprintln!("[audit] {error}");
            app.exit(1);
            return;
        }
        app.exit(0);
    });
}

fn run(
    app: &AppHandle,
    directory: &std::path::Path,
    latest: Arc<Mutex<Value>>,
) -> Result<(), String> {
    fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let mut output = File::create(directory.join("renderer.jsonl")).map_err(|e| e.to_string())?;
    let started = Instant::now();
    let window = app.get_webview_window("main").ok_or("桌宠窗口不存在")?;
    window.show().map_err(|e| e.to_string())?;
    while latest.lock().map_err(|e| e.to_string())?.is_null() {
        if started.elapsed() > Duration::from_secs(90) {
            return Err("90 秒内没有收到模型采样；检查 VITE_PERF_AUDIT=1 和模型加载".into());
        }
        thread::sleep(Duration::from_millis(200));
    }

    let mut idle_pause_count = None;
    let mut sample = |phase: &str, settled: bool| -> Result<(), String> {
        let renderer = latest.lock().map_err(|e| e.to_string())?.clone();
        let entry = json!({
            "phase": phase,
            "elapsedSeconds": started.elapsed().as_secs_f64(),
            "timestampMs": SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_millis(),
            "settings": desktop::get_settings(app)?,
            "nativeVisible": window.is_visible().map_err(|e| e.to_string())?,
            "minimized": window.is_minimized().map_err(|e| e.to_string())?,
            "scaleFactor": window.scale_factor().map_err(|e| e.to_string())?,
            "renderer": renderer,
        });
        writeln!(output, "{entry}").map_err(|e| e.to_string())?;
        output.flush().map_err(|e| e.to_string())?;
        fs::write(directory.join("status.json"), entry.to_string()).map_err(|e| e.to_string())?;
        if phase == "idle-30-minutes" {
            // 等前一阶段的最后一次恢复上报后，再固定暂停计数基线。
            let pause_changed = if settled {
                let pauses = entry["renderer"]["pauses"].as_u64();
                pauses != *idle_pause_count.get_or_insert(pauses)
            } else {
                false
            };
            if !idle_sample_is_valid(&entry, settled) || pause_changed {
                return Err("连续可见待机被中断或采样失效；本次不能判定为 30 分钟验收通过".into());
            }
        }
        Ok(())
    };
    let mut measure = |phase: &str, seconds: u64| -> Result<(), String> {
        let phase_started = Instant::now();
        sample(phase, false)?;
        while phase_started.elapsed() < Duration::from_secs(seconds) {
            thread::sleep(
                Duration::from_secs(5)
                    .min(Duration::from_secs(seconds).saturating_sub(phase_started.elapsed())),
            );
            sample(phase, phase_started.elapsed() >= Duration::from_secs(10))?;
        }
        Ok(())
    };

    measure("warmup", 20)?;
    desktop::update_settings(app, SettingsChange::MaxFps(30))?;
    measure("visible-30fps", 120)?;
    desktop::update_settings(app, SettingsChange::MaxFps(15))?;
    measure("visible-15fps", 120)?;
    desktop::update_settings(app, SettingsChange::Visible(false))?;
    measure("hidden", 120)?;
    desktop::update_settings(app, SettingsChange::Visible(true))?;
    measure("restored", 15)?;
    window.minimize().map_err(|e| e.to_string())?;
    measure("minimized", 120)?;
    desktop::update_settings(app, SettingsChange::Visible(true))?;
    measure("unminimized", 15)?;
    for cycle in 1..=20 {
        desktop::update_settings(app, SettingsChange::Visible(false))?;
        measure(&format!("cycle-{cycle:02}-hidden"), 2)?;
        desktop::update_settings(app, SettingsChange::Visible(true))?;
        measure(&format!("cycle-{cycle:02}-visible"), 2)?;
    }
    desktop::update_settings(app, SettingsChange::MaxFps(30))?;
    measure("idle-30-minutes", 1800)?;
    fs::write(
        directory.join("complete.json"),
        json!({
            "elapsedSeconds": started.elapsed().as_secs_f64(), "completed": true,
        })
        .to_string(),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visible_sample() -> Value {
        json!({
            "nativeVisible": true, "minimized": false, "timestampMs": 10001,
            "settings": { "visible": true, "maxFps": 30 },
            "renderer": { "running": true, "pageVisible": true, "maxFps": 30, "reportedAt": 10000 }
        })
    }

    #[test]
    fn idle_rejects_hidden_minimized_and_changed_fps() {
        for pointer in ["/nativeVisible", "/settings/visible"] {
            let mut sample = visible_sample();
            *sample.pointer_mut(pointer).unwrap() = json!(false);
            assert!(!idle_sample_is_valid(&sample, false));
        }
        let mut sample = visible_sample();
        sample["minimized"] = json!(true);
        assert!(!idle_sample_is_valid(&sample, false));
        let mut sample = visible_sample();
        sample["settings"]["maxFps"] = json!(15);
        assert!(!idle_sample_is_valid(&sample, false));
    }

    #[test]
    fn idle_requires_fresh_running_renderer_after_transition() {
        assert!(idle_sample_is_valid(&visible_sample(), true));
        let mut sample = visible_sample();
        sample["renderer"]["running"] = json!(false);
        assert!(idle_sample_is_valid(&sample, false));
        assert!(!idle_sample_is_valid(&sample, true));
        let mut sample = visible_sample();
        sample["timestampMs"] = json!(25000);
        assert!(!idle_sample_is_valid(&sample, true));
    }
}
