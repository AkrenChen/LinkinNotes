#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::{io::{BufRead, BufReader, Write}, process::{Child, ChildStdin, ChildStdout, Command, Stdio}, sync::Mutex};
use std::os::windows::process::CommandExt;
use tauri::{Manager, State, menu::{Menu, MenuItem}, tray::TrayIconBuilder};
use serde_json::{json, Value};
use tauri_plugin_global_shortcut::ShortcutState;
mod preferences;
mod appearance;

struct Bridge { child: Child, input: ChildStdin, output: BufReader<ChildStdout> }
impl Drop for Bridge { fn drop(&mut self) { let _ = self.child.kill(); } }
struct Backend(Mutex<Option<Bridge>>);

#[tauri::command]
fn toggle_pin(window: tauri::WebviewWindow) -> Result<bool,String> {
    let pinned = !window.is_always_on_top().map_err(|e|e.to_string())?;
    window.set_always_on_top(pinned).map_err(|e|e.to_string())?;
    Ok(pinned)
}

#[tauri::command]
fn hide_window(window: tauri::WebviewWindow) -> Result<(),String> { window.hide().map_err(|e|e.to_string()) }

#[tauri::command]
fn toggle_maximize(window: tauri::WebviewWindow) -> Result<bool,String> {
    if window.is_maximized().map_err(|e|e.to_string())? { window.unmaximize() } else { window.maximize() }.map_err(|e|e.to_string())?;
    window.is_maximized().map_err(|e|e.to_string())
}

#[tauri::command]
async fn rpc(app: tauri::AppHandle, backend: State<'_, Backend>, method: String, params: Option<Value>) -> Result<Value,String> {
    let handle = app.clone();
    let request = json!({"method":method,"params":params.unwrap_or(json!({}))});
    let _ = backend;
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<Backend>();
        let mut guard = state.0.lock().map_err(|_| "同步服务锁定失败")?;
        if guard.is_none() {
            let base = std::env::current_exe().map_err(|e|e.to_string())?.parent().unwrap().to_owned();
            let path = base.join("sidecar").join("linkin-backend.exe");
            let mut cmd = if path.exists() { Command::new(path) } else {
                let mut c = Command::new("python");
                c.arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../backend/service.py")); c
            };
            let mut child = cmd.creation_flags(0x08000000).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e|format!("启动同步服务失败：{e}"))?;
            let input = child.stdin.take().unwrap(); let output = BufReader::new(child.stdout.take().unwrap());
            *guard = Some(Bridge{child,input,output});
        }
        let bridge = guard.as_mut().unwrap();
        let exchange = (|| -> Result<Value,String> {
            writeln!(bridge.input,"{}",request).map_err(|e|e.to_string())?;
            bridge.input.flush().map_err(|e|e.to_string())?;
            let mut line=String::new();
            bridge.output.read_line(&mut line).map_err(|e|e.to_string())?;
            if line.is_empty() { return Err("同步服务已退出，请刷新重试".into()); }
            serde_json::from_str(&line).map_err(|e|e.to_string())
        })();
        match exchange { Ok(response) => {
            if let Some(error) = response.get("error").and_then(Value::as_str) { Err(error.to_owned()) }
            else { Ok(response["result"].clone()) }
        }, Err(error) => { *guard=None; Err(error) } }
    }).await.map_err(|e|e.to_string())?
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app,_,_| { if let Some(w)=app.get_webview_window("main") { let _=w.show(); let _=w.set_focus(); } }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().with_handler(|app,_,event| {
            if event.state == ShortcutState::Pressed {
                if let Some(window)=app.get_webview_window("main") {
                    let foreground = window.is_visible().unwrap_or(false)
                        && !window.is_minimized().unwrap_or(false)
                        && window.is_focused().unwrap_or(false);
                    if foreground { let _=window.hide(); }
                    else { let _=window.show(); let _=window.unminimize(); let _=window.set_focus(); }
                }
            }
        }).build())
        .manage(Backend(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![rpc,toggle_pin,hide_window,toggle_maximize,preferences::get_settings,preferences::save_settings])
        .setup(|app| {
            let window=app.get_webview_window("main").unwrap();
            appearance::refresh(&window.as_ref().window());
            preferences::initialize(app.handle())?;
            let show=MenuItem::with_id(app,"show","显示四象限",true,None::<&str>)?;
            let quit=MenuItem::with_id(app,"quit","退出",true,None::<&str>)?;
            let menu=Menu::with_items(app,&[&show,&quit])?;
            let mut pixels=vec![0u8;32*32*4];
            for y in 3..29 { for x in 3..29 { let i=(y*32+x)*4; let color=match (x<16,y<16) {(true,true)=>[226,103,114,255],(false,true)=>[99,156,220,255],(true,false)=>[224,177,74,255],_=>[143,151,167,255]}; pixels[i..i+4].copy_from_slice(&color); } }
            TrayIconBuilder::new().icon(tauri::image::Image::new_owned(pixels,32,32)).tooltip("四象限待办").menu(&menu).on_menu_event(|app,event| {
                match event.id.as_ref() { "quit"=>app.exit(0), "show"=>{if let Some(w)=app.get_webview_window("main") { let _=w.show(); let _=w.unminimize(); let _=w.set_focus(); }},_=>{} }
            }).build(app)?;
            Ok(())
        })
        .on_window_event(|window,event| {match event {
            tauri::WindowEvent::CloseRequested{api,..} => {api.prevent_close();let _=window.hide();},
            tauri::WindowEvent::Resized(_) | tauri::WindowEvent::ScaleFactorChanged{..} => appearance::refresh(window),
            _ => {}
        }})
        .build(tauri::generate_context!()).expect("启动四象限待办失败")
        .run(|app,event| { if let tauri::RunEvent::Exit = event {if let Ok(mut bridge)=app.state::<Backend>().0.lock(){ *bridge=None; }} });
}
