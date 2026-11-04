use windows_sys::Win32::Graphics::Gdi::{CreateRoundRectRgn, DeleteObject, SetWindowRgn};

pub fn refresh(window: &tauri::Window) {
    let (Ok(hwnd), Ok(size), Ok(scale)) = (window.hwnd(),window.outer_size(),window.scale_factor()) else { return; };
    let maximized = window.is_maximized().unwrap_or(false);
    let radius = if maximized { 0 } else { (24.0 * scale) as i32 };
    unsafe {
        let region = CreateRoundRectRgn(0,0,size.width as i32+1,size.height as i32+1,radius*2,radius*2);
        // Windows takes ownership of the region only when SetWindowRgn succeeds.
        if !region.is_null() && SetWindowRgn(hwnd.0 as _,region,1) == 0 { DeleteObject(region); }
    }
}
