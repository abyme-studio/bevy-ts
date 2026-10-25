//! The host's native window as Bevy's: raw handles with no winit. The engine holds its own
//! reference to the window (on Android `ANativeWindow_acquire`) and releases it when Bevy's last
//! `RawHandleWrapper` and the render world's surface have dropped it.

use bevy::window::{RawHandleWrapper, WindowWrapper};
use raw_window_handle::{DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle, WindowHandle};
use std::ffi::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A host window the engine holds a reference to.
pub struct NativeWindow {
    window: RawWindowHandle,
    display: RawDisplayHandle,
    ptr: *mut c_void,
    released: Arc<AtomicBool>,
}

// SAFETY: the handles are plain pointers to the host's window; Bevy hands them only to wgpu's
// surface creation, which the platforms allow off the window's thread (`ANativeWindow` is
// thread-safe; a macOS test view makes its layer on the main thread before passing it).
unsafe impl Send for NativeWindow {}
unsafe impl Sync for NativeWindow {}

impl NativeWindow {
    /// Takes a reference to the platform's window at `ptr`: Android's `ANativeWindow *`, or on
    /// macOS an `NSView *` the host keeps alive until the engine lets go of it.
    ///
    /// # Safety
    /// `ptr` must be a live native window of this platform.
    pub unsafe fn from_ptr(ptr: *mut c_void) -> Option<NativeWindow> {
        let (window, display) = unsafe { platform::handles(ptr)? };
        Some(NativeWindow { window, display, ptr, released: Arc::new(AtomicBool::new(false)) })
    }

    /// The pointer the host gave, to tell the same window at a new size from a new window.
    pub fn ptr(&self) -> *mut c_void {
        self.ptr
    }

    /// Becomes true once the engine let go of the window.
    pub fn released(&self) -> Arc<AtomicBool> {
        self.released.clone()
    }

    /// The component Bevy's renderer makes a surface from; the window lives while any copy does.
    pub fn into_handle(self) -> Result<RawHandleWrapper, HandleError> {
        RawHandleWrapper::new(&WindowWrapper::new(self))
    }
}

impl Drop for NativeWindow {
    fn drop(&mut self) {
        unsafe { platform::release(self.ptr) };
        self.released.store(true, Ordering::SeqCst);
    }
}

impl HasWindowHandle for NativeWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        // SAFETY: the window outlives `self`, which holds a reference to it.
        Ok(unsafe { WindowHandle::borrow_raw(self.window) })
    }
}

impl HasDisplayHandle for NativeWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(unsafe { DisplayHandle::borrow_raw(self.display) })
    }
}

#[cfg(target_os = "android")]
mod platform {
    use raw_window_handle::{AndroidDisplayHandle, AndroidNdkWindowHandle, RawDisplayHandle, RawWindowHandle};
    use std::ffi::c_void;
    use std::ptr::NonNull;

    #[link(name = "android")]
    unsafe extern "C" {
        fn ANativeWindow_acquire(window: *mut c_void);
        fn ANativeWindow_release(window: *mut c_void);
    }

    pub unsafe fn handles(ptr: *mut c_void) -> Option<(RawWindowHandle, RawDisplayHandle)> {
        let window = NonNull::new(ptr)?;
        unsafe { ANativeWindow_acquire(ptr) };
        Some((AndroidNdkWindowHandle::new(window).into(), AndroidDisplayHandle::new().into()))
    }

    pub unsafe fn release(ptr: *mut c_void) {
        unsafe { ANativeWindow_release(ptr) };
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use raw_window_handle::{AppKitDisplayHandle, AppKitWindowHandle, RawDisplayHandle, RawWindowHandle};
    use std::ffi::c_void;
    use std::ptr::NonNull;

    pub unsafe fn handles(ptr: *mut c_void) -> Option<(RawWindowHandle, RawDisplayHandle)> {
        let view = NonNull::new(ptr)?;
        Some((AppKitWindowHandle::new(view).into(), AppKitDisplayHandle::new().into()))
    }

    pub unsafe fn release(_: *mut c_void) {}
}

#[cfg(not(any(target_os = "android", target_os = "macos")))]
mod platform {
    use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
    use std::ffi::c_void;

    /// No native window here: the engine runs headless only.
    pub unsafe fn handles(_: *mut c_void) -> Option<(RawWindowHandle, RawDisplayHandle)> {
        None
    }

    pub unsafe fn release(_: *mut c_void) {}
}
