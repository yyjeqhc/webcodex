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

#[cfg(target_os = "macos")]
use crate::{
    clipboard_read_result, ensure_correlated_fingerprint, is_secure_text_fingerprint,
    run_macos_clipboard_write_effect_steps, select_exact_ax_window_index,
    validate_element_state_target, validate_key_modifiers, validate_text_input_preflight,
    validate_text_input_target, AxObservationDeadline,
};

#[cfg(target_os = "macos")]
use block2::RcBlock;
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSPasteboard, NSPasteboardTypeString, NSRunningApplication, NSWorkspace,
    NSWorkspaceOpenConfiguration,
};
#[cfg(target_os = "macos")]
use objc2_application_services::{AXError, AXIsProcessTrusted, AXUIElement, AXValue, AXValueType};
#[cfg(target_os = "macos")]
use objc2_core_foundation::{
    CFArray, CFBoolean, CFIndex, CFRetained, CFString, CFType, CGPoint, CGRect, CGSize, CFUUID,
};
#[cfg(all(test, target_os = "macos"))]
use objc2_core_graphics::CGBitmapContextCreateImage;
#[cfg(target_os = "macos")]
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGDirectDisplayID, CGDisplayBounds,
    CGDisplayCopyDisplayMode, CGDisplayIsBuiltin, CGDisplayIsMain, CGDisplayMode,
    CGDisplayModelNumber, CGDisplayRotation, CGDisplaySerialNumber, CGDisplayUnitNumber,
    CGDisplayVendorNumber, CGError, CGEvent, CGEventFlags, CGEventSource, CGEventSourceStateID,
    CGEventTapLocation, CGEventType, CGGetActiveDisplayList, CGImage, CGImageAlphaInfo,
    CGImageByteOrderInfo, CGKeyCode, CGMouseButton, CGPreflightPostEventAccess,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{
    NSArray, NSBundle, NSDictionary, NSError, NSFileManager, NSSearchPathDirectory,
    NSSearchPathDomainMask, NSString, NSURL,
};
#[cfg(target_os = "macos")]
use serde::{Deserialize, Serialize};
#[cfg(target_os = "macos")]
use std::fs;
#[cfg(target_os = "macos")]
use std::os::unix::fs::MetadataExt;
#[cfg(target_os = "macos")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::sync::mpsc::{self, Receiver};

mod accessibility;
mod applications;
mod capture;
mod clipboard;
mod display;
mod input;

pub(crate) use accessibility::{
    accessibility_status, accessibility_tree, activate_window, control, element_state,
    scroll_to_element,
};
use accessibility::{
    ax_attribute_settable, checked_surface_pid, exact_ax_window, optional_ax_bool,
    optional_ax_string, prepare_ax_call, resolve_correlated_element, validate_key_input_target,
};
#[cfg(test)]
pub(crate) use applications::{
    application_identity_revalidates_for_test,
    macos_application_identity_revalidates_in_roots_for_test,
    macos_application_launch_completion_for_test, macos_application_launch_configuration_for_test,
    macos_application_launch_lost_completion_for_test,
    macos_application_launch_preparation_race_for_test, macos_applications_in_roots_for_test,
};
pub(crate) use applications::{launch_application, list_applications};
pub(crate) use capture::capture_display;
#[cfg(test)]
use capture::{capture_revalidated_macos_display, macos_cg_image_to_rgba};
pub(super) use capture::{ensure_capture_permission, ensure_platform_capture_bound, focus_state};
pub(crate) use clipboard::{read_clipboard, write_clipboard};
use display::find_exact_macos_display;
pub(crate) use display::list_displays;
#[cfg(test)]
pub(crate) use display::macos_display_identity_revalidates_for_test;
pub(crate) use input::{dispatch_pointer, input_text, key_input, prepare_pointer};
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use input::{macos_pointer_read_only_probe_for_test, MacPointerReadOnlyProbe};
