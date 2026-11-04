use std::{collections::BTreeMap, fs, sync::Mutex};
use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub shortcut: String,
    pub names: BTreeMap<String, String>,
    #[serde(rename = "followClient")]
    pub follow_client: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            shortcut: "Ctrl+Alt+Q".into(),
            follow_client: false,
            names: [("ui", "重要且紧急"), ("nui", "重要不紧急"), ("uni", "紧急不重要"), ("nuni", "不重要不紧急")]
                .into_iter().map(|(key,value)| (key.into(),value.into())).collect(),
        }
    }
}

impl Preferences {
    pub fn validate(&self) -> Result<(), String> {
        for key in ["ui","nui","uni","nuni"] {
            let name = self.names.get(key).ok_or("缺少象限名称")?;
            if name.trim().is_empty() || name.chars().count() > 24 { return Err("象限名称需为 1–24 字".into()); }
        }
        if !self.shortcut.is_empty() { self.shortcut.parse::<Shortcut>().map_err(|_| "快捷键格式无效")?; }
        Ok(())
    }
}

pub struct PreferenceState(pub Mutex<Preferences>, pub Mutex<Option<String>>, pub Mutex<Option<String>>);

pub fn initialize(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let path = app.path().app_config_dir()?.join("preferences.json");
    let settings = if path.exists() { serde_json::from_slice::<Preferences>(&fs::read(&path)?)? } else { Preferences::default() };
    settings.validate()?;
    let error = if settings.shortcut.is_empty() { None } else {
        app.global_shortcut().register(settings.shortcut.as_str()).err().map(|e| format!("唤起快捷键注册失败，请在设置中更换：{e}"))
    };
    let startup_error = if settings.follow_client { crate::client_startup::configure(true, path.parent().unwrap()).err() } else { None };
    app.manage(PreferenceState(Mutex::new(settings), Mutex::new(error), Mutex::new(startup_error)));
    Ok(())
}

#[tauri::command]
pub fn get_settings(state: tauri::State<PreferenceState>) -> Result<serde_json::Value, String> {
    let settings = state.0.lock().map_err(|_| "设置锁定失败")?.clone();
    let error = state.1.lock().map_err(|_| "设置锁定失败")?.clone();
    let startup_error = state.2.lock().map_err(|_| "设置锁定失败")?.clone();
    Ok(serde_json::json!({"settings":settings,"shortcutError":error,"startupError":startup_error}))
}

#[tauri::command]
pub fn save_settings(app: tauri::AppHandle, state: tauri::State<PreferenceState>, mut settings: Preferences) -> Result<Preferences, String> {
    for name in settings.names.values_mut() { *name = name.trim().to_owned(); }
    settings.validate()?;
    let mut current = state.0.lock().map_err(|_| "设置锁定失败")?;
    let had_error = state.1.lock().map_err(|_| "设置锁定失败")?.is_some();
    let changed = current.shortcut != settings.shortcut || had_error;
    if changed && !settings.shortcut.is_empty() {
        app.global_shortcut().register(settings.shortcut.as_str()).map_err(|e| format!("快捷键不可用，可能已被占用：{e}"))?;
    }
    let persist = (|| -> Result<(),String> {
        let folder = app.path().app_config_dir().map_err(|e| e.to_string())?;
        fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let path = folder.join("preferences.json");
        let previous = if path.exists() { Some(fs::read(&path).map_err(|e| e.to_string())?) } else { None };
        crate::client_startup::atomic_write(&path, &serde_json::to_vec_pretty(&settings).map_err(|e| e.to_string())?)?;
        if settings.follow_client || current.follow_client {
            if let Err(error) = crate::client_startup::configure(settings.follow_client, &folder) {
                let restore = if let Some(data) = previous { crate::client_startup::atomic_write(&path, &data) } else { fs::remove_file(&path).map_err(|e| e.to_string()) };
                return Err(format!("{error}{}", restore.err().map(|e| format!("；原设置恢复失败：{e}")).unwrap_or_default()));
            }
        }
        Ok(())
    })();
    if let Err(error) = persist {
        if changed && !settings.shortcut.is_empty() { let _ = app.global_shortcut().unregister(settings.shortcut.as_str()); }
        return Err(format!("设置保存失败：{error}"));
    }
    if changed && !had_error && !current.shortcut.is_empty() { let _ = app.global_shortcut().unregister(current.shortcut.as_str()); }
    *current = settings.clone();
    *state.1.lock().map_err(|_| "设置锁定失败")? = None;
    *state.2.lock().map_err(|_| "设置锁定失败")? = None;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_roundtrip_and_validation() {
        let mut settings = Preferences::default();
        settings.names.insert("ui".into(), "今天必须完成".into());
        settings.validate().unwrap();
        let loaded: Preferences = serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(loaded.names["ui"], "今天必须完成");
        assert!(!loaded.follow_client);
        settings.follow_client = true;
        let loaded: Preferences = serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert!(loaded.follow_client);
        settings.names.insert("ui".into(), " ".into());
        assert!(settings.validate().is_err());
        settings = Preferences::default(); settings.shortcut = "Ctrl+NotAKey".into();
        assert!(settings.validate().is_err());
    }
    #[test]
    fn old_preferences_keep_shortcut_and_custom_names_without_enabling_startup() {
        let loaded: Preferences = serde_json::from_str(r#"{"shortcut":"Ctrl+Shift+Q","names":{"ui":"今日优先","nui":"计划","uni":"稍后","nuni":"其他"}}"#).unwrap();
        loaded.validate().unwrap();
        assert_eq!(loaded.shortcut, "Ctrl+Shift+Q");
        assert_eq!(loaded.names["ui"], "今日优先");
        assert!(!loaded.follow_client);
    }
}
