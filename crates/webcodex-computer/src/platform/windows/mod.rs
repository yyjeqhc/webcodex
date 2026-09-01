use super::{map_error, resolve_surface_window};
use crate::validate_key_input;
use crate::{
    bounded_text, ensure_raw_capture_bound, prepare_clipboard_write_text, validate_input_text,
    AccessibilityTreeResult, ApplicationRecord, ClipboardWriteEffectState, ComputerAction,
    DisplayRecord, ElementRecord, PlatformApplication, PlatformDisplay, PointerAction, PointerPlan,
    SurfaceRecord,
};
use crate::{is_supported_text_input_fingerprint, ElementFingerprint};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::ptr::NonNull;
use std::time::{Duration, Instant};
use uuid::Uuid;
use xcap::Window;

#[cfg(windows)]
use crate::{
    clipboard_read_result_from_utf16, finish_clipboard_read, run_clipboard_write_effect_steps,
    PreparedClipboardText,
};
#[cfg(windows)]
use xcap::Monitor;

#[cfg(windows)]
use windows::Win32::System::{
    Com::SAFEARRAY,
    Ole::{
        SafeArrayDestroy, SafeArrayGetDim, SafeArrayGetElement, SafeArrayGetElemsize,
        SafeArrayGetLBound, SafeArrayGetUBound,
    },
};
#[cfg(windows)]
use windows::{
    core::{w, IUnknown, Interface, PCWSTR},
    Win32::{
        Foundation::{
            GetLastError, GlobalFree, SetLastError, E_NOINTERFACE, E_POINTER, HANDLE, HGLOBAL,
            HWND as WinHwnd, POINT, RPC_E_CHANGED_MODE, WIN32_ERROR,
        },
        Graphics::Gdi::{EnumDisplayDevicesW, DISPLAY_DEVICEW},
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IBindCtx,
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
            COINIT_MULTITHREADED,
        },
        System::DataExchange::{
            CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
            OpenClipboard, SetClipboardData,
        },
        System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE},
        System::Ole::CF_UNICODETEXT,
        UI::Accessibility::{
            CUIAutomation8, IUIAutomation2, IUIAutomationElement, IUIAutomationInvokePattern,
            IUIAutomationScrollItemPattern, IUIAutomationTreeWalker, IUIAutomationValuePattern,
            UIA_ButtonControlTypeId, UIA_CheckBoxControlTypeId, UIA_ComboBoxControlTypeId,
            UIA_CustomControlTypeId, UIA_DataGridControlTypeId, UIA_DataItemControlTypeId,
            UIA_DocumentControlTypeId, UIA_EditControlTypeId, UIA_GroupControlTypeId,
            UIA_HeaderControlTypeId, UIA_HeaderItemControlTypeId, UIA_HyperlinkControlTypeId,
            UIA_InvokePatternId, UIA_ListControlTypeId, UIA_ListItemControlTypeId,
            UIA_MenuControlTypeId, UIA_MenuItemControlTypeId, UIA_PaneControlTypeId,
            UIA_ProgressBarControlTypeId, UIA_RadioButtonControlTypeId, UIA_ScrollBarControlTypeId,
            UIA_ScrollItemPatternId, UIA_SeparatorControlTypeId, UIA_SliderControlTypeId,
            UIA_SpinnerControlTypeId, UIA_StatusBarControlTypeId, UIA_TabControlTypeId,
            UIA_TabItemControlTypeId, UIA_TableControlTypeId, UIA_TextControlTypeId,
            UIA_ToolBarControlTypeId, UIA_ToolTipControlTypeId, UIA_TreeControlTypeId,
            UIA_TreeItemControlTypeId, UIA_ValuePatternId, UIA_WindowControlTypeId,
            UIA_CONTROLTYPE_ID, UIA_E_ELEMENTNOTAVAILABLE, UIA_E_NOTSUPPORTED, UIA_PATTERN_ID,
        },
        UI::HiDpi::{
            SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT,
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        },
        UI::Input::KeyboardAndMouse::{
            GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT,
            KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MOUSEEVENTF_ABSOLUTE,
            MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK,
            MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY, VK_CONTROL, VK_DOWN, VK_END, VK_ESCAPE,
            VK_HOME, VK_LBUTTON, VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MBUTTON,
            VK_MENU, VK_NEXT, VK_PRIOR, VK_RBUTTON, VK_RCONTROL, VK_RETURN, VK_RIGHT, VK_RMENU,
            VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_TAB, VK_UP, VK_XBUTTON1, VK_XBUTTON2,
        },
        UI::Shell::{
            Common::ITEMIDLIST, FOLDERID_AppsFolder, IEnumIDList, ILCombine, ILGetSize,
            IShellFolder, IShellItem, SHCreateItemFromIDList, SHGetDesktopFolder,
            SHGetKnownFolderIDList, ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_IDLIST,
            SHCONTF_FOLDERS, SHCONTF_NONFOLDERS, SHELLEXECUTEINFOW, SIGDN_NORMALDISPLAY,
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, GetCursorPos, GetForegroundWindow, GetSystemMetrics,
            IsIconic, ShowWindowAsync, EDD_GET_DEVICE_INTERFACE_NAME, HWND_MESSAGE,
            SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
            SW_RESTORE, SW_SHOWNOACTIVATE, WINDOW_EX_STYLE, WINDOW_STYLE,
        },
    },
};
#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::HWND as SysHwnd,
    Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
        GetCurrentObject, GetDIBits, GetObjectW, GetWindowDC, ReleaseDC, SelectObject, BITMAP,
        BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, OBJ_BITMAP, SRCCOPY,
    },
    Storage::Xps::PrintWindow,
};

mod accessibility;
mod applications;
mod capture;
mod clipboard;
mod display;
mod input;

pub(crate) use accessibility::{
    accessibility_status, accessibility_tree, activate_window, control, element_state,
    scroll_to_element, uia_semantic_text_input_role, win_hwnd,
};
use accessibility::{
    exact_uia_window, resolve_uia_element, uia_element_has_exact_focus, uia_error,
    uia_text_pattern, uia_value_pattern_current_value, uia_value_pattern_writable,
    validate_windows_key_input_target, UiaContext,
};
#[cfg(test)]
pub(crate) use accessibility::{
    test_uia_is_offscreen, test_windows_focused_element_belongs_to_surface, uia_control_role,
    uia_semantic_focus_role, windows_control_attempt_error, windows_scroll_attempt_error,
    windows_window_activation_attempt_error,
};
#[cfg(test)]
pub(crate) use applications::{
    application_identity_revalidates_for_test, application_shell_execute_contract_for_test,
};
pub(crate) use applications::{launch_application, list_applications};
pub(crate) use capture::capture_display;
pub(super) use capture::{
    capture_window_gdi, ensure_capture_permission, ensure_platform_capture_bound, focus_state,
};
#[cfg(test)]
pub(crate) use clipboard::{
    clipboard_close_cleanup_armed_for_test, clipboard_owner_hwnd_contract_for_test,
};
pub(crate) use clipboard::{read_clipboard, write_clipboard};
pub(crate) use display::list_displays;
use display::{
    find_exact_display, windows_monitor_rect, windows_virtual_desktop_metrics,
    windows_xcap_virtual_bounds,
};
#[allow(unused_imports)]
pub(crate) use input::PointerCoordinateContext;
pub(crate) use input::{
    dispatch_pointer, enter_pointer_coordinate_context, input_text, key_input, prepare_pointer,
};
#[cfg(test)]
pub(crate) use input::{
    test_windows_key_input_plan, test_windows_keyboard_state_guard,
    test_windows_pointer_button_send_input_count, test_windows_pointer_coordinate_spaces,
    test_windows_pointer_dispatch_trace, test_windows_pointer_dpi_context_metrics,
    test_windows_pointer_input_flags, test_windows_pointer_map,
    test_windows_pointer_move_send_input_count, test_windows_pointer_postcondition,
    test_windows_pointer_state_guard, test_windows_send_input_count,
    windows_key_input_attempt_error, windows_text_input_attempt_error,
};
