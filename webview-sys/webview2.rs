use libc::{c_char, c_double, c_int, c_void};
use once_cell::unsync::OnceCell;
use std::mem;
use std::ptr;
use std::rc::Rc;
use webview2::Controller;
use winapi::{
    shared::minwindef::*, shared::windef::*, um::libloaderapi::GetModuleHandleW, um::winuser::*,
};
use std::ffi::CStr;
use winapi::um::wingdi::{CreateSolidBrush, RGB};
use winapi::um::winbase::MulDiv;
use winapi::shared::ntdef::LONG;


type ExternalInvokeCallback = extern "C" fn(webview: *mut WebView, arg: *const c_char);
type DispatchFn = extern "C" fn(webview: *mut WebView, arg: *mut c_void);


const WM_WEBVIEW_DISPATCH: UINT = WM_APP + 1;

#[repr(C)]
struct WebView {
    url: *const c_char,
    title: *const c_char,
    width: c_int,
    height: c_int,
    resizable: c_int,
    debug: c_int,
    frameless: c_int,
    visible: c_int,
    min_width: c_int,
    min_height: c_int,
    external_invoke_cb: ExternalInvokeCallback,
    is_fullscreen: BOOL,
    saved_style: LONG,
    saved_ex_style: LONG,
    saved_rect: RECT,
    hwnd: HWND,
    js_busy: c_int,
    controller: *mut Controller,
    userdata: *mut c_void,
}


#[no_mangle]
unsafe extern "C" fn webview_new(
    title: *const c_char,
    url: *const c_char,
    width: c_int,
    height: c_int,
    resizable: c_int,
    debug: c_int,
    frameless: c_int,
    visible: c_int,
    min_width: c_int,
    min_height: c_int,
    external_invoke_cb: ExternalInvokeCallback,
    userdata: *mut c_void,
) -> *mut WebView {
    unsafe {
        // High DPI support.
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let controller = Rc::new(OnceCell::<Controller>::new());
    let controller_clone = controller.clone();

    // Window procedure.
    let wnd_proc = move |hwnd, msg, w_param, l_param| match msg {
        WM_SIZE => {
            if let Some(c) = controller.get() {
                let mut r = unsafe { mem::zeroed() };
                unsafe {
                    GetClientRect(hwnd, &mut r);
                }
                c.put_bounds(r).unwrap();
            }
            0
        }
        WM_WEBVIEW_DISPATCH => {
            let data: Box<DispatchData> = Box::from_raw(l_param as _);
            (data.func)(data.target, data.arg);
            1
        }
        WM_MOVE => {
            if let Some(c) = controller.get() {
                let _ = c.notify_parent_window_position_changed();
            }
            0
        }
        // Optimization: don't render the webview when the window is minimized.
        WM_SYSCOMMAND if w_param == SC_MINIMIZE => {
            if let Some(c) = controller.get() {
                c.put_is_visible(false).unwrap();
            }
            unsafe { DefWindowProcW(hwnd, msg, w_param, l_param) }
        }
        WM_SYSCOMMAND if w_param == SC_RESTORE => {
            if let Some(c) = controller.get() {
                c.put_is_visible(true).unwrap();
            }
            unsafe { DefWindowProcW(hwnd, msg, w_param, l_param) }
        }
        // High DPI support.
        WM_DPICHANGED => unsafe {
            let rect = *(l_param as *const RECT);
            SetWindowPos(
                hwnd,
                ptr::null_mut(),
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            0
        },
        _ => unsafe { DefWindowProcW(hwnd, msg, w_param, l_param) },
    };

    // Register window class. (Standard windows GUI boilerplate).
    let class_name = utf_16_null_terminiated("WebView");

    let h_instance = unsafe { GetModuleHandleW(ptr::null()) };

    let class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        hCursor: unsafe { LoadCursorW(ptr::null_mut(), IDC_ARROW) },
        lpfnWndProc: Some(unsafe { wnd_proc_helper::as_global_wnd_proc(wnd_proc) }),
        lpszClassName: class_name.as_ptr(),
        hInstance: h_instance,
        hbrBackground: (COLOR_WINDOW + 1) as HBRUSH,
        ..unsafe { mem::zeroed() }
    };
    unsafe {
        if RegisterClassW(&class) == 0 {
            return ptr::null_mut();
        }
    }

    let wv = Box::new(WebView {
        url,
        title,
        width,
        height,
        resizable,
        debug,
        frameless,
        visible,
        min_width,
        min_height,
        external_invoke_cb,
        is_fullscreen: 0,
        saved_style: 0,
        saved_ex_style: 0,
        saved_rect: Default::default(),
        hwnd: ptr::null_mut(),
        js_busy: 0,
        controller: ptr::null_mut(),
        userdata,
    });

    let wv = Box::into_raw(wv);

    // Create window. (Standard windows GUI boilerplate).
    let window_title = utf_16_null_terminiated(&CStr::from_ptr(title).to_string_lossy());

    let mut style = WS_OVERLAPPEDWINDOW;

    if resizable == 0 {
        style &= !(WS_SIZEBOX);
    }

    if frameless == 1 {
        style &= !(WS_SYSMENU | WS_CAPTION | WS_MINIMIZEBOX | WS_MAXIMIZEBOX);
    }

    let screen = GetDC(ptr::null_mut());

    let dpi = unsafe { GetDpiForSystem() } as i32;

    ReleaseDC(ptr::null_mut(), screen);

    let mut rect = RECT {
        left: 0,
        top: 0,
        right: MulDiv(width, dpi, 96),
        bottom: MulDiv(height, dpi, 96),
    };
    AdjustWindowRect(&mut rect, style, 0);

    let mut client_rect = Default::default();
    GetClientRect(GetDesktopWindow(), &mut client_rect);
    let left = (client_rect.right / 2) - ((rect.right - rect.left) / 2);
    let top = (client_rect.bottom / 2) - ((rect.bottom - rect.top) / 2);

    rect.right = rect.right - rect.left + left;
    rect.left = left;
    rect.bottom = rect.bottom - rect.top + top;
    rect.top = top;

    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            window_title.as_ptr(),
            style,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            HWND_DESKTOP,
            ptr::null_mut(),
            h_instance,
            ptr::null_mut(),
        )
    };

    SetWindowLongPtrW(hwnd, GWL_STYLE, style as _);

    if hwnd.is_null() {
        return ptr::null_mut();
    }

    (*wv).hwnd = hwnd;

    unsafe {
        ShowWindow((*wv).hwnd, if visible == 1 { SW_SHOWDEFAULT } else { SW_HIDE });
        UpdateWindow(hwnd);
    }

    // Create the webview.
    let r = webview2::EnvironmentBuilder::new().build(move |env| {
        env.unwrap().create_controller(hwnd, move |c| {
            let c = c.unwrap();

            let mut r = unsafe { mem::zeroed() };
            unsafe {
                GetClientRect(hwnd, &mut r);
            }
            c.put_bounds(r).unwrap();

            let w = c.get_webview().unwrap();
            // Communication.
            // Receive message from webpage.
            w.add_web_message_received(|w, msg| {
                let msg = msg.try_get_web_message_as_string()?;
                todo!("INVOKE EXTERNAL CALLBACK");
                if msg.starts_with("m") {} else if msg.starts_with("d") {
                    SendMessageW(hwnd, WM_NCLBUTTONDOWN, 2, 0);
                }
                w.post_web_message_as_string(&msg)
            }).unwrap();
            w.execute_script(&include_str!("webview_preload_edge.js"), |_| Ok(()));
            controller_clone.set(c).unwrap();
            Ok(())
        })
    });

    if let Err(e) = r {
        return ptr::null_mut();
    }

    todo!("GET CONTROLLER");


    wv
}

#[no_mangle]
unsafe extern "C" fn webview_loop(webview: *mut WebView, blocking: c_int) -> c_int {
    let mut msg: MSG = unsafe { mem::zeroed() };
    while unsafe { GetMessageW(&mut msg, ptr::null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    0
}

fn utf_16_null_terminiated(x: &str) -> Vec<u16> {
    x.encode_utf16().chain(std::iter::once(0)).collect()
}


pub(crate) struct DispatchData {
    pub(crate) target: *mut WebView,
    pub(crate) func: DispatchFn,
    pub(crate) arg: *mut c_void,
}

#[no_mangle]
unsafe extern "C" fn webview_dispatch(webview: *mut WebView, func: DispatchFn, arg: *mut c_void) {
    let data = Box::new(DispatchData {
        target: webview,
        func,
        arg,
    });
    PostMessageW((*webview).hwnd, WM_WEBVIEW_DISPATCH,
                 0,
                 Box::into_raw(data) as _);
}

#[no_mangle]
unsafe extern "C" fn webview_set_color(webview: *mut WebView, r: u8, g: u8, b: u8, a: u8) {
    let brush = CreateSolidBrush(RGB(r, g, b));
    SetClassLongPtrW((*webview).hwnd, GCLP_HBRBACKGROUND, mem::transmute(brush));
}

#[no_mangle]
unsafe extern "C" fn webview_set_zoom_level(webview: *mut WebView, percentage: c_double) {
    (*(*webview).controller).put_zoom_factor(percentage);
}

#[no_mangle]
unsafe extern "C" fn webview_set_html(webview: *mut WebView, html: *const c_char) {
    if let Ok(wv) = (*(*webview).controller).get_webview() {
        let cstr = CStr::from_ptr(html);
        wv.navigate_to_string(&cstr.to_string_lossy());
    }
}

#[no_mangle]
unsafe extern "C" fn webview_eval(webview: *mut WebView, js: *const c_char) -> c_int {
    todo!("JS BUSY");
    if let Ok(wv) = (*(*webview).controller).get_webview() {
        let cstr = CStr::from_ptr(js);
        wv.execute_script(&cstr.to_string_lossy(), |_| { Ok(()) });
    }
    0
}

#[no_mangle]
unsafe extern "C" fn webview_free(webview: *mut WebView) {
    let _ = Box::from_raw(webview);
}

#[no_mangle]
unsafe extern "C" fn webview_get_user_data(webview: *mut WebView) -> *mut c_void {
    (*webview).userdata
}

#[no_mangle]
unsafe extern "C" fn webview_exit(webview: *mut WebView) {
    DestroyWindow((*webview).hwnd);
}

#[no_mangle]
unsafe extern "C" fn webview_set_title(webview: *mut WebView, title: *const c_char) {
    let title = utf_16_null_terminiated(&CStr::from_ptr(title).to_string_lossy());
    unsafe {
        SetWindowTextW((*webview).hwnd, title.as_ptr());
    }
}

#[no_mangle]
unsafe extern "C" fn webview_set_maximized(webview: *mut WebView, maximize: c_int) {
    let is_maximized = IsZoomed((*webview).hwnd);
    if is_maximized == maximize {
        return;
    }
    if is_maximized == 0 {
        GetWindowRect((*webview).hwnd, &mut (*webview).saved_rect);
    }
    if maximize == 1 {
        let mut rect: RECT = mem::zeroed();

        SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut rect as *mut RECT) as *mut core::ffi::c_void, 0);

        ShowWindow((*webview).hwnd, SW_MAXIMIZE);
        SetWindowPos((*webview).hwnd, ptr::null_mut(), rect.left, rect.top, rect.right - rect.left,
                     rect.bottom - rect.top,
                     SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
    } else {
        ShowWindow((*webview).hwnd, SW_RESTORE);
        SetWindowPos((*webview).hwnd, ptr::null_mut(), (*webview).saved_rect.left,
                     (*webview).saved_rect.top,
                     (*webview).saved_rect.right - (*webview).saved_rect.left,
                     (*webview).saved_rect.bottom - (*webview).saved_rect.top,
                     SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
    }
}

#[no_mangle]
unsafe extern "C" fn webview_set_minimized(webview: *mut WebView, minimize: c_int) {
    let is_minimized = IsIconic((*webview).hwnd);
    if is_minimized == minimize {
        return;
    } else if minimize == 1 {
        ShowWindow((*webview).hwnd, SW_MINIMIZE);
    } else {
        ShowWindow((*webview).hwnd, SW_RESTORE);
    }
}

#[no_mangle]
unsafe extern "C" fn webview_set_visible(webview: *mut WebView, visible: c_int) {
    ShowWindow(
        (*webview).hwnd,
        if visible == 1 { SW_SHOW } else { SW_HIDE },
    );
}

#[no_mangle]
unsafe extern "C" fn webview_set_fullscreen(webview: *mut WebView, fullscreen: c_int) {
    if fullscreen == (*webview).is_fullscreen {
        return;
    }

    if (*webview).is_fullscreen == 0 {
        (*webview).saved_style = GetWindowLongW((*webview).hwnd, GWL_STYLE);
        (*webview).saved_ex_style = GetWindowLongW((*webview).hwnd, GWL_EXSTYLE);
        GetWindowRect((*webview).hwnd, &mut (*webview).saved_rect);
    }

    (*webview).is_fullscreen = fullscreen;

    if (*webview).is_fullscreen == 0 {
        SetWindowLongW((*webview).hwnd, GWL_STYLE, (*webview).saved_style);
        SetWindowLongW((*webview).hwnd, GWL_EXSTYLE, (*webview).saved_ex_style);
        let rect = &(*webview).saved_rect;
        SetWindowPos(
            (*webview).hwnd,
            ptr::null_mut(),
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        return;
    }

    unsafe {
        let mut monitor_info: MONITORINFO = Default::default();
        monitor_info.cbSize = std::mem::size_of::<MONITORINFO>() as _;
        GetMonitorInfoW(
            MonitorFromWindow((*webview).hwnd, MONITOR_DEFAULTTONEAREST),
            &mut monitor_info,
        );

        SetWindowLongW(
            (*webview).hwnd,
            GWL_STYLE,
            (*webview).saved_style & !(WS_CAPTION | WS_THICKFRAME) as LONG,
        );

        SetWindowLongW(
            (*webview).hwnd,
            GWL_EXSTYLE,
            (*webview).saved_ex_style
                & !(WS_EX_DLGMODALFRAME
                | WS_EX_WINDOWEDGE
                | WS_EX_CLIENTEDGE
                | WS_EX_STATICEDGE) as LONG,
        );

        let rect = &monitor_info.rcMonitor;
        SetWindowPos(
            (*webview).hwnd,
            ptr::null_mut(),
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}


#[no_mangle]
unsafe extern "C" fn webview_get_window_handle(webview: *mut WebView) -> *mut c_void {
    (*webview).hwnd.cast()
}

mod wnd_proc_helper {
    use super::*;
    use std::cell::UnsafeCell;

    struct UnsafeSyncCell<T> {
        inner: UnsafeCell<T>,
    }

    impl<T> UnsafeSyncCell<T> {
        const fn new(t: T) -> UnsafeSyncCell<T> {
            UnsafeSyncCell {
                inner: UnsafeCell::new(t),
            }
        }
    }

    impl<T: Copy> UnsafeSyncCell<T> {
        unsafe fn get(&self) -> T {
            self.inner.get().read()
        }

        unsafe fn set(&self, v: T) {
            self.inner.get().write(v)
        }
    }

    unsafe impl<T: Copy> Sync for UnsafeSyncCell<T> {}

    static GLOBAL_F: UnsafeSyncCell<usize> = UnsafeSyncCell::new(0);

    /// Use a closure as window procedure.
    ///
    /// The closure will be boxed and stored in a global variable. It will be
    /// released upon WM_DESTROY. (It doesn't get to handle WM_DESTROY.)
    pub unsafe fn as_global_wnd_proc<F: Fn(HWND, UINT, WPARAM, LPARAM) -> isize + 'static>(
        f: F,
    ) -> unsafe extern "system" fn(hwnd: HWND, msg: UINT, w_param: WPARAM, l_param: LPARAM) -> isize
    {
        let f_ptr = Box::into_raw(Box::new(f));
        GLOBAL_F.set(f_ptr as usize);

        unsafe extern "system" fn wnd_proc<F: Fn(HWND, UINT, WPARAM, LPARAM) -> isize + 'static>(
            hwnd: HWND,
            msg: UINT,
            w_param: WPARAM,
            l_param: LPARAM,
        ) -> isize {
            let f_ptr = GLOBAL_F.get() as *mut F;

            if msg == WM_DESTROY {
                Box::from_raw(f_ptr);
                GLOBAL_F.set(0);
                PostQuitMessage(0);
                return 0;
            }

            if !f_ptr.is_null() {
                let f = &*f_ptr;

                f(hwnd, msg, w_param, l_param)
            } else {
                DefWindowProcW(hwnd, msg, w_param, l_param)
            }
        }

        wnd_proc::<F>
    }
}