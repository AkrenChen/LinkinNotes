use std::{cell::RefCell, ptr, rc::Rc, sync::OnceLock};
use windows_sys::Win32::{Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM}, Graphics::Gdi::{CreateCompatibleDC, CreateDIBSection, CreateRoundRectRgn, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject, SetWindowRgn, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, AC_SRC_ALPHA, AC_SRC_OVER}, System::LibraryLoader::GetModuleHandleW, UI::{HiDpi::GetDpiForWindow, Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass}, WindowsAndMessaging::{CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowRect, IsIconic, IsWindow, IsWindowVisible, IsZoomed, RegisterClassW, SetWindowPos, ShowWindow, UpdateLayeredWindow, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, ULW_ALPHA, WM_DPICHANGED, WM_NCDESTROY, WM_SHOWWINDOW, WM_SIZE, WM_WINDOWPOSCHANGED, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP}}};
use crate::shadow_geometry::ShadowLayout;

const SUBCLASS_ID: usize=0x4c4e5348;
const SHADOW_STYLES: u32=WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;

struct Appearance {
    shadow: HWND,
    shape: Option<(i32,i32,u32,bool)>,
    bitmap: Option<ShadowLayout>,
    position: Option<(i32,i32)>,
    visible: bool,
}

impl Drop for Appearance {
    fn drop(&mut self) { unsafe { if IsWindow(self.shadow)!=0 { DestroyWindow(self.shadow); } } }
}

fn class_name() -> Vec<u16> { "LinkinNotes.InputTransparentShadow\0".encode_utf16().collect() }
fn last_error() -> String { std::io::Error::last_os_error().to_string() }

fn create_shadow(owner: HWND) -> Result<HWND,String> {
    static CLASS: OnceLock<Result<u16,String>>=OnceLock::new();
    CLASS.get_or_init(|| unsafe {
        let name=class_name();
        let class=WNDCLASSW { lpfnWndProc: Some(DefWindowProcW), hInstance: GetModuleHandleW(ptr::null()), lpszClassName: name.as_ptr(), ..std::mem::zeroed() };
        let atom=RegisterClassW(&class);
        if atom==0 { Err(last_error()) } else { Ok(atom) }
    }).as_ref().map_err(Clone::clone)?;
    let window=unsafe { CreateWindowExW(SHADOW_STYLES,class_name().as_ptr(),ptr::null(),WS_POPUP,0,0,0,0,owner,ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null()) };
    if window.is_null() { Err(last_error()) } else { Ok(window) }
}

fn paint(shadow: HWND, layout: ShadowLayout, position: (i32,i32)) -> Result<(),String> {
    let pixels=layout.pixels();
    unsafe {
        let screen=GetDC(ptr::null_mut());
        if screen.is_null() { return Err(last_error()); }
        let dc=CreateCompatibleDC(screen);
        if dc.is_null() { ReleaseDC(ptr::null_mut(),screen); return Err(last_error()); }
        let info=BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: layout.width, biHeight: -layout.height, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB, ..std::mem::zeroed() }, ..std::mem::zeroed() };
        let mut bits=ptr::null_mut();
        let bitmap=CreateDIBSection(screen,&info,DIB_RGB_COLORS,&mut bits,ptr::null_mut(),0);
        if bitmap.is_null() || bits.is_null() {
            let error=last_error();
            if !bitmap.is_null() { DeleteObject(bitmap); }
            DeleteDC(dc); ReleaseDC(ptr::null_mut(),screen);
            return Err(error);
        }
        ptr::copy_nonoverlapping(pixels.as_ptr(),bits.cast::<u8>(),pixels.len());
        let previous=SelectObject(dc,bitmap);
        let blend=BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
        let result=UpdateLayeredWindow(shadow,screen,&POINT{x:position.0,y:position.1},&SIZE{cx:layout.width,cy:layout.height},dc,&POINT{x:0,y:0},0,&blend,ULW_ALPHA);
        let error=if result==0 { Some(last_error()) } else { None };
        SelectObject(dc,previous); DeleteObject(bitmap); DeleteDC(dc); ReleaseDC(ptr::null_mut(),screen);
        if let Some(error)=error { return Err(error); }
    }
    Ok(())
}

impl Appearance {
    fn refresh(&mut self, owner: HWND) -> Result<(),String> {
        unsafe {
            let mut bounds: RECT=std::mem::zeroed();
            if GetWindowRect(owner,&mut bounds)==0 { return Err(last_error()); }
            let width=bounds.right-bounds.left;
            let height=bounds.bottom-bounds.top;
            if width<=0 || height<=0 { return Ok(()); }
            let dpi=GetDpiForWindow(owner).max(96);
            let maximized=IsZoomed(owner)!=0;
            let shape=(width,height,dpi,maximized);
            if self.shape!=Some(shape) {
                self.shape=Some(shape);
                let radius=if maximized { 0 } else { (9.0*dpi as f64/96.0) as i32 };
                let region=CreateRoundRectRgn(0,0,width+1,height+1,radius*2,radius*2);
                if region.is_null() { self.shape=None; return Err(last_error()); }
                // Cache the shape before SetWindowRgn sends nested position/size messages.
                if SetWindowRgn(owner,region,1)==0 { DeleteObject(region); self.shape=None; return Err(last_error()); }
            }
            let show=IsWindowVisible(owner)!=0 && IsIconic(owner)==0 && !maximized;
            if !show {
                if self.visible { self.visible=false; ShowWindow(self.shadow,SW_HIDE); }
                return Ok(());
            }
            let layout=ShadowLayout::new(width,height,dpi);
            let position=(bounds.left-layout.padding,bounds.top-layout.padding);
            if self.bitmap!=Some(layout) {
                paint(self.shadow,layout,position)?;
                self.bitmap=Some(layout);
                self.position=Some(position);
            } else if self.position!=Some(position) {
                if SetWindowPos(self.shadow,ptr::null_mut(),position.0,position.1,0,0,SWP_NOACTIVATE|SWP_NOZORDER|SWP_NOSIZE)==0 { return Err(last_error()); }
                self.position=Some(position);
            }
            if !self.visible { self.visible=true; ShowWindow(self.shadow,SW_SHOWNOACTIVATE); }
        }
        Ok(())
    }
}

fn refresh(owner: HWND, state: &RefCell<Appearance>) -> Result<(),String> {
    // Native window updates may reenter the callback before this borrow is released.
    let Ok(mut appearance)=state.try_borrow_mut() else { return Ok(()); };
    appearance.refresh(owner)
}

unsafe extern "system" fn subclass(owner: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, id: usize, data: usize) -> LRESULT {
    let pointer=data as *mut Rc<RefCell<Appearance>>;
    let state=Rc::clone(&*pointer);
    if message==WM_NCDESTROY {
        RemoveWindowSubclass(owner,Some(subclass),id);
        drop(Box::from_raw(pointer));
        return DefSubclassProc(owner,message,wparam,lparam);
    }
    let result=DefSubclassProc(owner,message,wparam,lparam);
    if IsWindow(owner)!=0 && matches!(message,WM_WINDOWPOSCHANGED|WM_SIZE|WM_SHOWWINDOW|WM_DPICHANGED) {
        if let Err(error)=refresh(owner,&state) { eprintln!("Window shadow update failed: {error}"); }
    }
    result
}

fn attach(owner: HWND) -> Result<HWND,String> {
    let shadow=create_shadow(owner)?;
    let state=Rc::new(RefCell::new(Appearance { shadow, shape: None, bitmap: None, position: None, visible: false }));
    let pointer=Box::into_raw(Box::new(Rc::clone(&state)));
    unsafe {
        if SetWindowSubclass(owner,Some(subclass),SUBCLASS_ID,pointer as usize)==0 {
            drop(Box::from_raw(pointer));
            return Err(last_error());
        }
        if let Err(error)=refresh(owner,&state) {
            RemoveWindowSubclass(owner,Some(subclass),SUBCLASS_ID);
            drop(Box::from_raw(pointer));
            return Err(error);
        }
    }
    Ok(shadow)
}

pub fn initialize(window: &tauri::Window) -> Result<(),String> {
    let hwnd=window.hwnd().map_err(|e|e.to_string())?;
    attach(hwnd.0 as _).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindow, GetWindowLongPtrW, GWL_EXSTYLE, GW_OWNER};
    #[test]
    fn hidden_shadow_uses_click_through_noactivate_styles_and_follows_owner_lifetime() {
        let owner=create_shadow(ptr::null_mut()).unwrap();
        unsafe { assert_ne!(SetWindowPos(owner,ptr::null_mut(),100,100,320,240,SWP_NOACTIVATE|SWP_NOZORDER),0); }
        let shadow=attach(owner).unwrap();
        unsafe {
            let styles=GetWindowLongPtrW(shadow,GWL_EXSTYLE) as u32;
            assert_eq!(styles & SHADOW_STYLES,SHADOW_STYLES);
            assert_eq!(GetWindow(shadow,GW_OWNER),owner);
            assert_eq!(IsWindowVisible(shadow),0);
            assert_ne!(SetWindowPos(owner,ptr::null_mut(),120,130,400,300,SWP_NOACTIVATE|SWP_NOZORDER),0);
            let mut bounds: RECT=std::mem::zeroed();
            assert_ne!(GetWindowRect(owner,&mut bounds),0);
            assert_eq!((bounds.right-bounds.left,bounds.bottom-bounds.top),(400,300));
            assert_eq!(IsWindowVisible(owner),0);
            assert_eq!(IsWindowVisible(shadow),0);
            let layout=ShadowLayout::new(100,80,96);
            paint(shadow,layout,(100,100)).unwrap();
            assert_eq!(IsWindowVisible(shadow),0);
            DestroyWindow(owner);
            assert_eq!(IsWindow(shadow),0);
        }
    }
}
