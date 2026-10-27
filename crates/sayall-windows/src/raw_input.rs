use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::time::Duration;
use thiserror::Error;

pub const XIAOMI_REMOTE_VENDOR_ID: u16 = 0x2717;
pub const XIAOMI_REMOTE_PRODUCT_ID: u16 = 0x32B8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteButton {
    Back,
    Ok,
    Tv,
    Home,
    Right,
    Left,
    Down,
    Up,
    Menu,
    Power,
    VolumeMute,
    VolumeUp,
    VolumeDown,
}

/// 语义按键的稳定顺序：key_gate 的映射位掩码、UI 画布布局都依赖该表。
pub const ALL_BUTTONS: [RemoteButton; 13] = [
    RemoteButton::Back,
    RemoteButton::Ok,
    RemoteButton::Tv,
    RemoteButton::Home,
    RemoteButton::Right,
    RemoteButton::Left,
    RemoteButton::Down,
    RemoteButton::Up,
    RemoteButton::Menu,
    RemoteButton::Power,
    RemoteButton::VolumeMute,
    RemoteButton::VolumeUp,
    RemoteButton::VolumeDown,
];

/// Legacy passive helper buttons. TV has separate ownership/cancellation rules.
pub(crate) const RC003_TAP_BUTTONS: [RemoteButton; 3] = [
    RemoteButton::Back,
    RemoteButton::VolumeUp,
    RemoteButton::VolumeDown,
];
pub(crate) const RC003_TV_MASK: u16 = 8;
pub(crate) const RC003_OK_MASK: u16 = 16;
pub(crate) const RC003_ALL_MASK: u16 = 0x1fff;
pub(crate) const RC003_GESTURE_MASK: u16 = RC003_ALL_MASK & !(RC003_TV_MASK | RC003_OK_MASK);

/// Protocol bits are stable and intentionally independent of ALL_BUTTONS order.
pub(crate) const RC003_CAPTURE_BUTTONS: [RemoteButton; 13] = [
    RemoteButton::Back,
    RemoteButton::VolumeUp,
    RemoteButton::VolumeDown,
    RemoteButton::Tv,
    RemoteButton::Ok,
    RemoteButton::Up,
    RemoteButton::Down,
    RemoteButton::Left,
    RemoteButton::Right,
    RemoteButton::Home,
    RemoteButton::Menu,
    RemoteButton::Power,
    RemoteButton::VolumeMute,
];

pub(crate) fn rc003_button_mask(button: RemoteButton) -> u16 {
    1 << RC003_CAPTURE_BUTTONS
        .iter()
        .position(|candidate| *candidate == button)
        .expect("RC003 capture covers every ordinary remote button")
}

impl RemoteButton {
    /// 在 [`ALL_BUTTONS`] 中的序号（0..12），用于门控位掩码。
    pub fn ordinal(self) -> usize {
        ALL_BUTTONS
            .iter()
            .position(|button| *button == self)
            .expect("ALL_BUTTONS covers every RemoteButton variant")
    }

    /// 按住连发间隔（单击动作的自动重复），对齐 Mac 原版 HIDRemoteScheduler：
    /// 返回 50ms、方向键/音量± 100ms、其余按键不连发。仅当该键只配置了单击
    /// （未配置双击/长按）时由手势引擎启用。
    pub fn repeat_interval(self) -> Option<Duration> {
        match self {
            Self::Back => Some(Duration::from_millis(50)),
            Self::Up
            | Self::Down
            | Self::Left
            | Self::Right
            | Self::VolumeUp
            | Self::VolumeDown => Some(Duration::from_millis(100)),
            Self::Ok | Self::Tv | Self::Home | Self::Menu | Self::Power | Self::VolumeMute => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ButtonEdge {
    pub button: RemoteButton,
    pub is_pressed: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawInputPhase {
    #[default]
    Stopped,
    Starting,
    Ready,
    /// 监听器已运行，但当前系统里没有匹配遥控器的 HID 接口（多为 OS 侧 HOGP
    /// 接口缺失/链路僵死）。监听器已注册设备热插拔通知，待接口恢复
    /// （GIDC_ARRIVAL）会立即重新绑定，无需外层 10s 轮询重启。
    Awaiting,
    Failed,
    Unsupported,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawInputSnapshot {
    pub phase: RawInputPhase,
    pub matched_device_count: u32,
    pub raw_event_count: u64,
    pub semantic_edge_count: u64,
    pub last_button: Option<RemoteButton>,
    pub last_is_pressed: Option<bool>,
    /// 当前处于按下状态的语义按键（由映射引擎维护，用于 UI 高亮对账）。
    pub active_buttons: Vec<RemoteButton>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawKeyboardEvent {
    pub make_code: u16,
    pub flags: u16,
    pub virtual_key: u16,
    pub message: u32,
}

impl RawKeyboardEvent {
    pub fn is_pressed(self) -> bool {
        !matches!(self.message, 0x0101 | 0x0105)
    }

    pub fn button(self) -> Option<RemoteButton> {
        // RawKeyboardEvent enters the engine only after the listener matches the
        // selected remote. Keep shared Home/TV keys and filter aliases out of
        // the identity-free LL hook: only this attributed path may decode them.
        filter_alias_for_keyboard(self.virtual_key)
            .or_else(|| match self.virtual_key {
                0x24 => Some(RemoteButton::Home),
                0xC0 => Some(RemoteButton::Tv),
                _ => None,
            })
            .or_else(|| button_for_keyboard(self.virtual_key, self.make_code))
    }
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum RawInputDecodeError {
    #[error("unsupported Xiaomi voice remote HID report shape: {0} bytes")]
    UnsupportedReportShape(usize),
    #[error("invalid Xiaomi voice remote HID report prefix")]
    InvalidReportPrefix,
    #[error("RAWHID body is shorter than its 8-byte header")]
    RawHidBodyTooShort,
    #[error("RAWHID body declares an empty report")]
    EmptyRawHidReport,
    #[error("RAWHID body length overflow")]
    RawHidLengthOverflow,
    #[error("RAWHID body is truncated: expected {expected} bytes, got {actual}")]
    TruncatedRawHidBody { expected: usize, actual: usize },
}

pub fn device_path_matches_xiaomi_remote(path: &str) -> bool {
    let normalized = normalize_device_path(path);
    let classic = normalized.contains("vid_2717") && normalized.contains("pid_32b8");
    let ble = (normalized.contains("dev_vid&002717") || normalized.contains("dev_vid&012717"))
        && normalized.contains("pid&32b8");
    classic || ble
}

pub fn normalize_device_path(path: &str) -> String {
    path.trim().to_ascii_lowercase()
}

pub fn select_single_device_path(paths: &[String]) -> Result<String, DevicePathError> {
    let matches: Vec<_> = paths
        .iter()
        .filter(|path| device_path_matches_xiaomi_remote(path))
        .collect();
    match matches.as_slice() {
        [] => Err(DevicePathError::Missing),
        [path] => Ok((*path).clone()),
        _ => Err(DevicePathError::Ambiguous(matches.len())),
    }
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum DevicePathError {
    #[error("no RC001/RC003 Raw Input device path was found")]
    Missing,
    #[error("found {0} RC001/RC003 Raw Input device paths; refusing to choose one")]
    Ambiguous(usize),
}

pub fn decode_report_usages(report: &[u8]) -> Result<BTreeSet<u16>, RawInputDecodeError> {
    let payload = match report.len() {
        9 if report[..3] == [0x01, 0x00, 0x00] => &report[3..],
        9 => return Err(RawInputDecodeError::InvalidReportPrefix),
        7 if report[0] == 0x01 => &report[1..],
        7 => return Err(RawInputDecodeError::InvalidReportPrefix),
        6 => report,
        length => return Err(RawInputDecodeError::UnsupportedReportShape(length)),
    };

    let mut usages = BTreeSet::new();
    for slot in payload.chunks_exact(2) {
        let usage = u16::from_le_bytes([slot[0], slot[1]]);
        if usage != 0 {
            usages.insert(usage);
        }
    }
    Ok(usages)
}

pub fn parse_raw_hid_body(body: &[u8]) -> Result<Vec<&[u8]>, RawInputDecodeError> {
    if body.len() < 8 {
        return Err(RawInputDecodeError::RawHidBodyTooShort);
    }
    let report_size = u32::from_le_bytes(body[0..4].try_into().unwrap()) as usize;
    let report_count = u32::from_le_bytes(body[4..8].try_into().unwrap()) as usize;
    if report_size == 0 {
        return Err(RawInputDecodeError::EmptyRawHidReport);
    }
    let reports_length = report_size
        .checked_mul(report_count)
        .ok_or(RawInputDecodeError::RawHidLengthOverflow)?;
    let expected = 8usize
        .checked_add(reports_length)
        .ok_or(RawInputDecodeError::RawHidLengthOverflow)?;
    if body.len() < expected {
        return Err(RawInputDecodeError::TruncatedRawHidBody {
            expected,
            actual: body.len(),
        });
    }
    Ok(body[8..expected].chunks_exact(report_size).collect())
}

pub fn button_for_usage(usage: u16) -> Option<RemoteButton> {
    if let Some(button) = filter_alias_for_usage(usage) {
        return Some(button);
    }
    Some(match usage {
        0x00F1 => RemoteButton::Back,
        0x0028 => RemoteButton::Ok,
        0x0035 => RemoteButton::Tv,
        0x004A => RemoteButton::Home,
        0x004F => RemoteButton::Right,
        0x0050 => RemoteButton::Left,
        0x0051 => RemoteButton::Down,
        0x0052 => RemoteButton::Up,
        0x0065 => RemoteButton::Menu,
        0x0066 => RemoteButton::Power,
        0x007F => RemoteButton::VolumeMute,
        0x0080 => RemoteButton::VolumeUp,
        0x0081 => RemoteButton::VolumeDown,
        0x003E => return None,
        _ => return None,
    })
}

/// Optional RC003 HID filter aliases, decoded only after device attribution.
/// Original keyboard-page usages 0x80/0x81/0xF1 become F13/F14/F15 respectively.
pub(crate) fn filter_alias_for_usage(usage: u16) -> Option<RemoteButton> {
    match usage {
        0x0068 => Some(RemoteButton::VolumeUp),
        0x0069 => Some(RemoteButton::VolumeDown),
        0x006A => Some(RemoteButton::Back),
        _ => None,
    }
}

pub(crate) fn filter_alias_for_keyboard(virtual_key: u16) -> Option<RemoteButton> {
    match virtual_key {
        0x7C => Some(RemoteButton::VolumeUp),
        0x7D => Some(RemoteButton::VolumeDown),
        0x7E => Some(RemoteButton::Back),
        _ => None,
    }
}

/// Identity-free decoder shared with the LL hook. Home/OEM_3 and F13/F14/F15
/// must stay absent: an ordinary keyboard must never inherit their remote
/// attribution, even while a remote key is held or recently armed.
pub fn button_for_keyboard(virtual_key: u16, make_code: u16) -> Option<RemoteButton> {
    if virtual_key == 0xFF {
        return Some(match make_code {
            0x5E => RemoteButton::Power,
            0x6A => RemoteButton::Back,
            0x30 => RemoteButton::VolumeUp,
            0x2E => RemoteButton::VolumeDown,
            0x20 => RemoteButton::VolumeMute,
            _ => return None,
        });
    }
    Some(match virtual_key {
        0x27 => RemoteButton::Right,
        0x25 => RemoteButton::Left,
        0x28 => RemoteButton::Down,
        0x26 => RemoteButton::Up,
        0x0D => RemoteButton::Ok,
        0x5D => RemoteButton::Menu,
        0x5F => RemoteButton::Power,
        0xAD => RemoteButton::VolumeMute,
        0xAF => RemoteButton::VolumeUp,
        0xAE => RemoteButton::VolumeDown,
        0x74 => return None,
        _ => return None,
    })
}

#[derive(Debug, Default)]
pub struct ButtonStateMerger {
    keyboard: BTreeSet<RemoteButton>,
    hid: BTreeSet<RemoteButton>,
    rc003_tap: BTreeSet<RemoteButton>,
    rc003_tv_enabled: bool,
    rc003_tv_participated: bool,
    // A native gesture that predates the TV lease keeps its own release.
    tv_native_preexisting: bool,
    // After source loss, drain uncertain native edges through a physical release.
    tv_native_drain: bool,
    rc003_ok_enabled: bool,
    rc003_ok_participated: bool,
    ok_native_preexisting: bool,
    ok_native_drain: bool,
    rc003_gesture_mask: u16,
    rc003_gesture_participated: BTreeSet<RemoteButton>,
    gesture_native_preexisting: BTreeSet<RemoteButton>,
    gesture_native_drain: BTreeSet<RemoteButton>,
}

impl ButtonStateMerger {
    pub fn update_keyboard(&mut self, event: RawKeyboardEvent) -> Vec<ButtonEdge> {
        let Some(button) = event.button() else {
            return Vec::new();
        };
        self.apply_keyboard_button_edge(button, event.is_pressed())
    }

    /// 应用已经归因到遥控器的键盘边沿（key_gate 吞下的原始键）。
    /// 返回并集变化产生的语义边沿（与 [`Self::update_keyboard`] 一致）。
    pub fn apply_keyboard_button_edge(
        &mut self,
        button: RemoteButton,
        is_pressed: bool,
    ) -> Vec<ButtonEdge> {
        let before = self.active_buttons();
        if is_pressed {
            self.keyboard.insert(button);
        } else {
            self.keyboard.remove(&button);
        }
        if button == RemoteButton::Tv && !is_pressed {
            self.observe_native_tv_release();
        }
        if button == RemoteButton::Ok && !is_pressed {
            self.observe_native_ok_release();
        }
        if !is_pressed {
            self.observe_native_gesture_release(button);
        }
        edges_between(&before, &self.active_buttons())
    }

    /// 当前按下的语义按键集合（各独立来源的并集）。
    pub fn active_button_set(&self) -> BTreeSet<RemoteButton> {
        self.active_buttons()
    }

    pub fn update_hid_report(
        &mut self,
        report: &[u8],
    ) -> Result<Vec<ButtonEdge>, RawInputDecodeError> {
        let usages = decode_report_usages(report)?;
        Ok(self.update_hid_usages(usages))
    }

    /// 应用一份 HID 报文的 usage 集合（绝对状态），返回语义边沿。
    pub fn update_hid_usages(&mut self, usages: BTreeSet<u16>) -> Vec<ButtonEdge> {
        let before = self.active_buttons();
        let previous_hid = self.hid.clone();
        self.hid = usages.into_iter().filter_map(button_for_usage).collect();
        if previous_hid.contains(&RemoteButton::Tv) && !self.hid.contains(&RemoteButton::Tv) {
            self.observe_native_tv_release();
        }
        if previous_hid.contains(&RemoteButton::Ok) && !self.hid.contains(&RemoteButton::Ok) {
            self.observe_native_ok_release();
        }
        for button in previous_hid
            .difference(&self.hid)
            .copied()
            .collect::<Vec<_>>()
        {
            self.observe_native_gesture_release(button);
        }
        edges_between(&before, &self.active_buttons())
    }

    pub(crate) fn begin_rc003_tv(&mut self, enabled: bool) {
        self.tv_native_preexisting = enabled && !self.tv_native_drain && self.native_tv_pressed();
        self.rc003_tv_enabled = enabled;
    }

    pub(crate) fn rc003_tv_enabled(&self) -> bool {
        self.rc003_tv_enabled
    }

    pub(crate) fn rc003_tv_participated(&self) -> bool {
        self.rc003_tv_participated
    }

    pub(crate) fn native_tv_pressed(&self) -> bool {
        self.keyboard.contains(&RemoteButton::Tv) || self.hid.contains(&RemoteButton::Tv)
    }

    fn observe_native_tv_release(&mut self) {
        if !self.native_tv_pressed() {
            self.tv_native_preexisting = false;
            self.tv_native_drain = false;
        }
    }

    pub(crate) fn begin_rc003_ok(&mut self, enabled: bool) {
        self.ok_native_preexisting = enabled && !self.ok_native_drain && self.native_ok_pressed();
        self.rc003_ok_enabled = enabled;
    }

    pub(crate) fn rc003_ok_enabled(&self) -> bool {
        self.rc003_ok_enabled
    }

    pub(crate) fn rc003_ok_participated(&self) -> bool {
        self.rc003_ok_participated
    }

    pub(crate) fn native_ok_pressed(&self) -> bool {
        self.keyboard.contains(&RemoteButton::Ok) || self.hid.contains(&RemoteButton::Ok)
    }

    fn observe_native_ok_release(&mut self) {
        if !self.native_ok_pressed() {
            self.ok_native_preexisting = false;
            self.ok_native_drain = false;
        }
    }

    pub(crate) fn begin_rc003_gestures(&mut self, mask: u16) {
        self.rc003_gesture_mask = mask & RC003_GESTURE_MASK;
        self.gesture_native_preexisting = RC003_CAPTURE_BUTTONS
            .into_iter()
            .filter(|button| self.gesture_capture_enabled(*button))
            .filter(|button| !self.gesture_native_drain.contains(button))
            .filter(|button| self.native_button_pressed(*button))
            .collect();
    }

    pub(crate) fn rc003_gesture_participated(&self) -> BTreeSet<RemoteButton> {
        self.rc003_gesture_participated.clone()
    }

    fn gesture_capture_enabled(&self, button: RemoteButton) -> bool {
        self.rc003_gesture_mask & rc003_button_mask(button) != 0
    }

    fn native_button_pressed(&self, button: RemoteButton) -> bool {
        self.keyboard.contains(&button) || self.hid.contains(&button)
    }

    fn observe_native_gesture_release(&mut self, button: RemoteButton) {
        if !self.native_button_pressed(button) {
            self.gesture_native_preexisting.remove(&button);
            self.gesture_native_drain.remove(&button);
        }
    }

    /// Captured keys are authoritative only after the helper confirmed clearing their report.
    /// Native reports remain tracked, but cannot create a second mapped gesture.
    /// Source loss must not cancel a native gesture which the helper never participated in.
    pub(crate) fn cancel_rc003_tap(&mut self) -> (bool, Vec<ButtonEdge>) {
        let before = self.active_buttons();
        let tv_participated = self.rc003_tv_participated;
        if self.rc003_tv_enabled && !self.tv_native_preexisting {
            self.tv_native_drain |=
                self.rc003_tap.contains(&RemoteButton::Tv) || self.native_tv_pressed();
        }
        if self.rc003_ok_enabled && !self.ok_native_preexisting {
            self.ok_native_drain |=
                self.rc003_tap.contains(&RemoteButton::Ok) || self.native_ok_pressed();
        }
        for button in RC003_CAPTURE_BUTTONS {
            if self.gesture_capture_enabled(button)
                && !self.gesture_native_preexisting.contains(&button)
                && (self.rc003_tap.contains(&button) || self.native_button_pressed(button))
            {
                self.gesture_native_drain.insert(button);
            }
        }
        self.rc003_tap.clear();
        self.rc003_tv_enabled = false;
        self.rc003_tv_participated = false;
        self.tv_native_preexisting = false;
        self.rc003_ok_enabled = false;
        self.rc003_ok_participated = false;
        self.ok_native_preexisting = false;
        self.rc003_gesture_mask = 0;
        self.rc003_gesture_participated.clear();
        self.gesture_native_preexisting.clear();
        (
            tv_participated,
            edges_between(&before, &self.active_buttons()),
        )
    }

    /// Independently attributed helper state; invalid masks never change a source.
    pub(crate) fn update_rc003_tap(&mut self, pressed_mask: u16) -> Vec<ButtonEdge> {
        if pressed_mask & !RC003_ALL_MASK != 0
            || (pressed_mask & RC003_TV_MASK != 0 && !self.rc003_tv_enabled)
            || (pressed_mask & RC003_OK_MASK != 0 && !self.rc003_ok_enabled)
            || pressed_mask & (RC003_GESTURE_MASK & !7) & !self.rc003_gesture_mask != 0
        {
            return Vec::new();
        }
        // The helper has observed a real neutral before this confirmed press.
        // A pre-lease native DOWN can still await its delayed UP in another queue;
        // retire that old semantic hold before starting the new cleared press.
        let native_takeover = pressed_mask & RC003_TV_MASK != 0
            && !self.rc003_tv_participated
            && self.tv_native_preexisting;
        let ok_native_takeover = pressed_mask & RC003_OK_MASK != 0
            && !self.rc003_ok_participated
            && self.ok_native_preexisting;
        let gesture_native_takeovers: Vec<_> = RC003_CAPTURE_BUTTONS
            .into_iter()
            .filter(|button| self.gesture_capture_enabled(*button))
            .filter(|button| pressed_mask & rc003_button_mask(*button) != 0)
            .filter(|button| !self.rc003_gesture_participated.contains(button))
            .filter(|button| self.gesture_native_preexisting.contains(button))
            .collect();
        let before = self.active_buttons();
        self.rc003_tap = RC003_CAPTURE_BUTTONS
            .into_iter()
            .enumerate()
            .filter_map(|(bit, button)| (pressed_mask & (1 << bit) != 0).then_some(button))
            .collect();
        if pressed_mask & RC003_TV_MASK != 0 {
            self.rc003_tap.insert(RemoteButton::Tv);
            self.rc003_tv_participated = true;
            self.tv_native_preexisting = false;
        }
        if pressed_mask & RC003_OK_MASK != 0 {
            self.rc003_tap.insert(RemoteButton::Ok);
            self.rc003_ok_participated = true;
            self.ok_native_preexisting = false;
        }
        for button in RC003_CAPTURE_BUTTONS {
            if self.gesture_capture_enabled(button) && pressed_mask & rc003_button_mask(button) != 0
            {
                self.rc003_gesture_participated.insert(button);
                self.gesture_native_preexisting.remove(&button);
            }
        }
        let mut edges = edges_between(&before, &self.active_buttons());
        if native_takeover {
            edges.push(ButtonEdge {
                button: RemoteButton::Tv,
                is_pressed: false,
            });
            edges.push(ButtonEdge {
                button: RemoteButton::Tv,
                is_pressed: true,
            });
        }
        if ok_native_takeover {
            edges.push(ButtonEdge {
                button: RemoteButton::Ok,
                is_pressed: false,
            });
            edges.push(ButtonEdge {
                button: RemoteButton::Ok,
                is_pressed: true,
            });
        }
        for button in gesture_native_takeovers {
            edges.push(ButtonEdge {
                button,
                is_pressed: false,
            });
            edges.push(ButtonEdge {
                button,
                is_pressed: true,
            });
        }
        edges
    }

    pub fn release_all(&mut self) -> Vec<ButtonEdge> {
        let active = self.active_buttons();
        self.keyboard.clear();
        self.hid.clear();
        self.rc003_tap.clear();
        self.rc003_tv_enabled = false;
        self.rc003_tv_participated = false;
        self.tv_native_preexisting = false;
        self.tv_native_drain = false;
        self.rc003_ok_enabled = false;
        self.rc003_ok_participated = false;
        self.ok_native_preexisting = false;
        self.ok_native_drain = false;
        self.rc003_gesture_mask = 0;
        self.rc003_gesture_participated.clear();
        self.gesture_native_preexisting.clear();
        self.gesture_native_drain.clear();
        active
            .into_iter()
            .map(|button| ButtonEdge {
                button,
                is_pressed: false,
            })
            .collect()
    }

    fn active_buttons(&self) -> BTreeSet<RemoteButton> {
        self.keyboard
            .iter()
            .chain(&self.hid)
            .filter(|button| {
                **button != RemoteButton::Tv
                    || (!self.tv_native_drain
                        && (!self.rc003_tv_enabled || self.tv_native_preexisting))
            })
            .filter(|button| {
                **button != RemoteButton::Ok
                    || (!self.ok_native_drain
                        && (!self.rc003_ok_enabled || self.ok_native_preexisting))
            })
            .filter(|button| {
                !self.gesture_native_drain.contains(button)
                    && (!self.gesture_capture_enabled(**button)
                        || self.gesture_native_preexisting.contains(button))
            })
            .chain(&self.rc003_tap)
            .copied()
            .collect()
    }
}

fn edges_between(
    before: &BTreeSet<RemoteButton>,
    after: &BTreeSet<RemoteButton>,
) -> Vec<ButtonEdge> {
    let mut edges = Vec::new();
    edges.extend(before.difference(after).copied().map(|button| ButtonEdge {
        button,
        is_pressed: false,
    }));
    edges.extend(after.difference(before).copied().map(|button| ButtonEdge {
        button,
        is_pressed: true,
    }));
    edges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rc003_tap_deduplicates_three_sources_and_preserves_other_keys() {
        let mut merger = ButtonStateMerger::default();
        merger.apply_keyboard_button_edge(RemoteButton::Up, true);
        assert_eq!(
            merger.update_rc003_tap(1),
            vec![ButtonEdge {
                button: RemoteButton::Back,
                is_pressed: true
            }]
        );
        assert!(merger.update_rc003_tap(1).is_empty());
        assert!(merger
            .apply_keyboard_button_edge(RemoteButton::Back, true)
            .is_empty());
        assert!(merger.update_hid_usages(BTreeSet::from([0xF1])).is_empty());
        assert!(merger.update_rc003_tap(0).is_empty());
        assert!(merger
            .apply_keyboard_button_edge(RemoteButton::Back, false)
            .is_empty());
        assert_eq!(
            merger.update_hid_usages(BTreeSet::new()),
            vec![ButtonEdge {
                button: RemoteButton::Back,
                is_pressed: false
            }]
        );
        assert_eq!(
            merger.active_button_set(),
            BTreeSet::from([RemoteButton::Up])
        );
        merger.update_rc003_tap(7);
        assert!(merger.update_rc003_tap(8).is_empty());
        assert_eq!(merger.active_button_set().len(), 4);
        assert_eq!(merger.release_all().len(), 4);
        assert!(merger.release_all().is_empty());
    }

    fn report(usages: &[u16]) -> Vec<u8> {
        let mut bytes = vec![0x01, 0x00, 0x00, 0, 0, 0, 0, 0, 0];
        for (index, usage) in usages.iter().take(3).enumerate() {
            bytes[3 + index * 2..5 + index * 2].copy_from_slice(&usage.to_le_bytes());
        }
        bytes
    }

    fn keyboard(virtual_key: u16, message: u32) -> RawKeyboardEvent {
        RawKeyboardEvent {
            make_code: 0,
            flags: 0,
            virtual_key,
            message,
        }
    }

    #[test]
    fn matches_both_windows_xiaomi_remote_device_path_shapes() {
        assert!(device_path_matches_xiaomi_remote(
            r"\\?\HID#VID_2717&PID_32B8&REV_00A4#instance"
        ));
        assert!(device_path_matches_xiaomi_remote(
            r"\\?\HID#{1812}_Dev_VID&012717_PID&32B8_REV&00A4_instance"
        ));
        assert!(!device_path_matches_xiaomi_remote(
            r"\\?\HID#VID_2717&PID_0001#instance"
        ));
    }

    #[test]
    fn device_selection_fails_closed() {
        assert_eq!(
            select_single_device_path(&[]),
            Err(DevicePathError::Missing)
        );
        let paths = vec![
            r"\\?\HID#VID_2717&PID_32B8#one".to_owned(),
            r"\\?\HID#VID_2717&PID_32B8#two".to_owned(),
        ];
        assert_eq!(
            select_single_device_path(&paths),
            Err(DevicePathError::Ambiguous(2))
        );
    }

    #[test]
    fn decodes_all_supported_report_shapes_and_unknown_usages() {
        let full = report(&[0x0028, 0x0052, 0xFFFF]);
        let expected = BTreeSet::from([0x0028, 0x0052, 0xFFFF]);
        let compact = [&[0x01][..], &full[3..]].concat();
        assert_eq!(decode_report_usages(&full).unwrap(), expected);
        assert_eq!(decode_report_usages(&compact).unwrap(), expected);
        assert_eq!(decode_report_usages(&full[3..]).unwrap(), expected);
    }

    #[test]
    fn splits_multiple_raw_hid_reports() {
        let first = report(&[0x0028]);
        let second = report(&[0x0052]);
        let mut body = Vec::from(9u32.to_le_bytes());
        body.extend_from_slice(&2u32.to_le_bytes());
        body.extend_from_slice(&first);
        body.extend_from_slice(&second);
        assert_eq!(parse_raw_hid_body(&body).unwrap(), vec![&first, &second]);
    }

    #[test]
    fn repeated_keyboard_down_emits_one_edge() {
        let mut merger = ButtonStateMerger::default();
        assert_eq!(
            merger.update_keyboard(keyboard(0x26, 0x0100)),
            vec![ButtonEdge {
                button: RemoteButton::Up,
                is_pressed: true,
            }]
        );
        assert!(merger.update_keyboard(keyboard(0x26, 0x0100)).is_empty());
        assert_eq!(
            merger.update_keyboard(keyboard(0x26, 0x0101)),
            vec![ButtonEdge {
                button: RemoteButton::Up,
                is_pressed: false,
            }]
        );
    }

    #[test]
    fn filter_aliases_preserve_original_usages_and_require_keyboard_attribution() {
        for (usage, alias, vk, button) in [
            (0x0080, 0x0068, 0x7C, RemoteButton::VolumeUp),
            (0x0081, 0x0069, 0x7D, RemoteButton::VolumeDown),
            (0x00F1, 0x006A, 0x7E, RemoteButton::Back),
        ] {
            assert_eq!(button_for_usage(usage), Some(button));
            assert_eq!(button_for_usage(alias), Some(button));
            assert_eq!(keyboard(vk, 0x0100).button(), Some(button));
            assert_eq!(button_for_keyboard(vk, 0), None);
            assert_eq!(button_for_keyboard(vk, 0x6A), None);
        }
        assert_eq!(filter_alias_for_keyboard(0x74), None);
        assert_eq!(filter_alias_for_usage(0x003E), None);
    }

    #[test]
    fn home_tv_decode_only_after_keyboard_device_attribution() {
        for (vk, usage, button) in [
            (0x24, 0x004A, RemoteButton::Home),
            (0xC0, 0x0035, RemoteButton::Tv),
        ] {
            assert_eq!(button_for_usage(usage), Some(button));
            for make in [0, 0x29, 0x35, 0x47] {
                assert_eq!(button_for_keyboard(vk, make), None);
                let mut event = keyboard(vk, 0x0100);
                event.make_code = make;
                assert_eq!(event.button(), Some(button));
            }
        }
    }

    #[test]
    fn home_tv_attributed_holds_survive_repeat_and_release_cleanly_on_restart() {
        for (vk, usage, button) in [
            (0x24, 0x004A, RemoteButton::Home),
            (0xC0, 0x0035, RemoteButton::Tv),
        ] {
            let mut merger = ButtonStateMerger::default();
            let down = ButtonEdge {
                button,
                is_pressed: true,
            };
            let up = ButtonEdge {
                button,
                is_pressed: false,
            };
            assert_eq!(merger.update_keyboard(keyboard(vk, 0x0100)), vec![down]);
            assert!(merger.update_keyboard(keyboard(vk, 0x0100)).is_empty());
            assert!(merger
                .update_hid_report(&report(&[usage]))
                .unwrap()
                .is_empty());
            // A duplicate source cannot end the real remote hold prematurely.
            assert!(merger.update_keyboard(keyboard(vk, 0x0101)).is_empty());
            assert_eq!(merger.update_hid_report(&report(&[])).unwrap(), vec![up]);
            assert!(merger.update_keyboard(keyboard(vk, 0x0101)).is_empty());

            assert_eq!(merger.update_keyboard(keyboard(vk, 0x0100)), vec![down]);
            // Listener stop/device removal cancels the remote hold exactly once.
            assert_eq!(merger.release_all(), vec![up]);
            assert!(merger.release_all().is_empty());
            assert!(merger.update_keyboard(keyboard(vk, 0x0101)).is_empty());
            let mut restarted = ButtonStateMerger::default();
            assert!(restarted.update_keyboard(keyboard(vk, 0x0101)).is_empty());
            assert_eq!(restarted.update_keyboard(keyboard(vk, 0x0100)), vec![down]);
            assert_eq!(restarted.update_keyboard(keyboard(vk, 0x0101)), vec![up]);
        }
    }

    #[test]
    fn filter_alias_holds_deduplicate_sources_and_release_on_stop_or_restart() {
        for (usage, vk, button) in [
            (0x0068, 0x7C, RemoteButton::VolumeUp),
            (0x0069, 0x7D, RemoteButton::VolumeDown),
            (0x006A, 0x7E, RemoteButton::Back),
        ] {
            let mut merger = ButtonStateMerger::default();
            let down = ButtonEdge {
                button,
                is_pressed: true,
            };
            let up = ButtonEdge {
                button,
                is_pressed: false,
            };
            assert_eq!(merger.update_keyboard(keyboard(vk, 0x0100)), vec![down]);
            assert!(merger.update_keyboard(keyboard(vk, 0x0100)).is_empty());
            assert!(merger
                .update_hid_report(&report(&[usage]))
                .unwrap()
                .is_empty());
            // Device removal/listener stop during a hold releases the merged state once.
            assert_eq!(merger.release_all(), vec![up]);
            assert!(merger.release_all().is_empty());
            assert!(merger.update_keyboard(keyboard(vk, 0x0101)).is_empty());
            assert!(merger.update_hid_report(&report(&[])).unwrap().is_empty());
            // A new process has no DOWN ownership: a late UP cannot create a press.
            let mut restarted = ButtonStateMerger::default();
            assert!(restarted.update_keyboard(keyboard(vk, 0x0101)).is_empty());
            assert_eq!(restarted.update_keyboard(keyboard(vk, 0x0100)), vec![down]);
            assert_eq!(restarted.update_keyboard(keyboard(vk, 0x0101)), vec![up]);
        }
    }

    #[test]
    fn keyboard_and_hid_sources_share_one_logical_hold() {
        let mut merger = ButtonStateMerger::default();
        assert_eq!(merger.update_keyboard(keyboard(0x26, 0x0100)).len(), 1);
        assert!(merger
            .update_hid_report(&report(&[0x0052]))
            .unwrap()
            .is_empty());
        assert!(merger.update_keyboard(keyboard(0x26, 0x0101)).is_empty());
        assert_eq!(merger.update_hid_report(&report(&[])).unwrap().len(), 1);
    }

    #[test]
    fn rc003_tv_lease_requires_confirmed_source_and_deduplicates_native_echoes() {
        let mut merger = ButtonStateMerger::default();
        let tv = |is_pressed| ButtonEdge {
            button: RemoteButton::Tv,
            is_pressed,
        };
        merger.begin_rc003_tv(true);
        // A pre-neutral native press can reach Windows, but cannot claim a cleared mapping.
        assert!(merger.update_keyboard(keyboard(0xC0, 0x0100)).is_empty());
        assert!(merger
            .update_hid_report(&report(&[0x35]))
            .unwrap()
            .is_empty());
        assert!(merger.update_keyboard(keyboard(0xC0, 0x0101)).is_empty());
        assert!(merger.update_hid_report(&report(&[])).unwrap().is_empty());
        assert_eq!(merger.update_rc003_tap(8), vec![tv(true)]);
        assert!(merger.update_keyboard(keyboard(0xC0, 0x0100)).is_empty());
        assert!(merger.update_rc003_tap(8).is_empty());
        assert_eq!(merger.update_rc003_tap(0), vec![tv(false)]);
        assert!(merger.update_keyboard(keyboard(0xC0, 0x0101)).is_empty());
        // Even a delayed full native echo inside this lease cannot create another click.
        assert!(merger.update_keyboard(keyboard(0xC0, 0x0100)).is_empty());
        assert!(merger.update_keyboard(keyboard(0xC0, 0x0101)).is_empty());
        assert_eq!(merger.update_keyboard(keyboard(0x26, 0x0100)).len(), 1);
    }

    #[test]
    fn rc003_tv_loss_drains_only_unfinished_holds_across_generations() {
        let mut merger = ButtonStateMerger::default();
        let tv = |is_pressed| ButtonEdge {
            button: RemoteButton::Tv,
            is_pressed,
        };
        merger.begin_rc003_tv(true);
        assert_eq!(merger.update_rc003_tap(8), vec![tv(true)]);
        assert_eq!(merger.cancel_rc003_tap(), (true, vec![tv(false)]));
        // Unrelated HID neutral reports must not release the TV drain.
        assert!(merger.update_hid_report(&report(&[])).unwrap().is_empty());
        assert!(merger.update_keyboard(keyboard(0xC0, 0x0100)).is_empty());
        merger.begin_rc003_tv(true);
        assert_eq!(merger.update_rc003_tap(8), vec![tv(true)]);
        assert!(merger.update_keyboard(keyboard(0xC0, 0x0101)).is_empty());
        assert_eq!(merger.update_rc003_tap(0), vec![tv(false)]);
        // A completed helper cycle does not consume the first native press after stop.
        assert_eq!(merger.cancel_rc003_tap(), (true, vec![]));
        assert_eq!(
            merger.update_keyboard(keyboard(0xC0, 0x0100)),
            vec![tv(true)]
        );
        assert_eq!(
            merger.update_keyboard(keyboard(0xC0, 0x0101)),
            vec![tv(false)]
        );
    }

    #[test]
    fn rc003_tv_unrelated_helper_lifecycle_preserves_native_gesture() {
        for enabled in [false, true] {
            let mut merger = ButtonStateMerger::default();
            let tv = |is_pressed| ButtonEdge {
                button: RemoteButton::Tv,
                is_pressed,
            };
            assert_eq!(
                merger.update_keyboard(keyboard(0xC0, 0x0100)),
                vec![tv(true)]
            );
            merger.begin_rc003_tv(enabled);
            merger.update_rc003_tap(1);
            let (participated, edges) = merger.cancel_rc003_tap();
            assert!(!participated);
            assert!(edges.iter().all(|edge| edge.button == RemoteButton::Back));
            assert_eq!(
                merger.update_keyboard(keyboard(0xC0, 0x0101)),
                vec![tv(false)]
            );
        }
        let mut merger = ButtonStateMerger::default();
        assert!(merger.update_rc003_tap(8).is_empty());
        assert!(!merger.rc003_tv_participated());
    }

    #[test]
    fn rc003_ok_capture_requires_enabled_bit_and_deduplicates_native_echoes() {
        let mut merger = ButtonStateMerger::default();
        let ok = |is_pressed| ButtonEdge {
            button: RemoteButton::Ok,
            is_pressed,
        };
        assert!(merger.update_rc003_tap(16).is_empty());
        merger.begin_rc003_ok(true);
        assert!(merger.update_rc003_tap(24).is_empty()); // TV is not enabled.
        assert!(merger.update_keyboard(keyboard(0x0D, 0x0100)).is_empty());
        assert!(merger
            .update_hid_report(&report(&[0x28]))
            .unwrap()
            .is_empty());
        assert_eq!(merger.update_rc003_tap(16), vec![ok(true)]);
        assert!(merger.update_keyboard(keyboard(0x0D, 0x0100)).is_empty());
        assert!(merger.update_rc003_tap(16).is_empty());
        assert_eq!(merger.update_rc003_tap(0), vec![ok(false)]);
        assert!(merger.update_keyboard(keyboard(0x0D, 0x0101)).is_empty());
        assert!(merger.update_hid_report(&report(&[])).unwrap().is_empty());
        assert!(merger.update_keyboard(keyboard(0x0D, 0x0100)).is_empty());
        assert!(merger.update_keyboard(keyboard(0x0D, 0x0101)).is_empty());
        assert_eq!(merger.update_keyboard(keyboard(0x26, 0x0100)).len(), 1);
    }

    #[test]
    fn rc003_ok_loss_drains_held_native_echo_until_its_actual_release() {
        let mut merger = ButtonStateMerger::default();
        let ok = |is_pressed| ButtonEdge {
            button: RemoteButton::Ok,
            is_pressed,
        };
        merger.begin_rc003_ok(true);
        assert_eq!(merger.update_rc003_tap(16), vec![ok(true)]);
        assert!(merger.rc003_ok_participated());
        assert_eq!(merger.cancel_rc003_tap(), (false, vec![ok(false)]));
        assert!(!merger.rc003_ok_participated());
        // An unrelated HID neutral is not evidence that the keyboard hold ended.
        assert!(merger.update_hid_report(&report(&[])).unwrap().is_empty());
        assert!(merger.update_keyboard(keyboard(0x0D, 0x0100)).is_empty());
        assert!(merger.update_keyboard(keyboard(0x0D, 0x0100)).is_empty());
        assert!(merger.update_keyboard(keyboard(0x0D, 0x0101)).is_empty());
        assert_eq!(
            merger.update_keyboard(keyboard(0x0D, 0x0100)),
            vec![ok(true)]
        );
        assert_eq!(
            merger.update_keyboard(keyboard(0x0D, 0x0101)),
            vec![ok(false)]
        );
    }

    #[test]
    fn rc003_ok_prelease_hold_is_preserved_until_confirmed_source_takeover() {
        let mut merger = ButtonStateMerger::default();
        let ok = |is_pressed| ButtonEdge {
            button: RemoteButton::Ok,
            is_pressed,
        };
        assert_eq!(
            merger.update_keyboard(keyboard(0x0D, 0x0100)),
            vec![ok(true)]
        );
        merger.begin_rc003_ok(true);
        merger.update_rc003_tap(1);
        assert!(!merger.rc003_ok_participated());
        assert!(merger
            .cancel_rc003_tap()
            .1
            .iter()
            .all(|edge| edge.button != RemoteButton::Ok));
        merger.begin_rc003_ok(true);
        // A real source press after its neutral retires the delayed native hold.
        assert_eq!(merger.update_rc003_tap(16), vec![ok(false), ok(true)]);
        assert!(merger.update_keyboard(keyboard(0x0D, 0x0101)).is_empty());
        assert_eq!(merger.update_rc003_tap(0), vec![ok(false)]);
        assert!(merger.cancel_rc003_tap().1.is_empty());
        assert_eq!(
            merger.update_keyboard(keyboard(0x0D, 0x0100)),
            vec![ok(true)]
        );
    }

    #[test]
    fn rc003_tv_and_ok_capture_ownership_and_drain_are_independent() {
        let mut merger = ButtonStateMerger::default();
        merger.begin_rc003_tv(true);
        merger.begin_rc003_ok(true);
        assert_eq!(merger.update_rc003_tap(24).len(), 2);
        assert_eq!(
            merger.update_rc003_tap(16),
            vec![ButtonEdge {
                button: RemoteButton::Tv,
                is_pressed: false
            }]
        );
        let (_, edges) = merger.cancel_rc003_tap();
        assert_eq!(
            edges,
            vec![ButtonEdge {
                button: RemoteButton::Ok,
                is_pressed: false
            }]
        );
        assert_eq!(merger.update_keyboard(keyboard(0xC0, 0x0100)).len(), 1);
        assert!(merger.update_keyboard(keyboard(0x0D, 0x0100)).is_empty());
        assert_eq!(merger.update_keyboard(keyboard(0xC0, 0x0101)).len(), 1);
        assert!(merger.update_keyboard(keyboard(0x0D, 0x0101)).is_empty());
        assert_eq!(merger.update_keyboard(keyboard(0x0D, 0x0100)).len(), 1);
    }

    #[test]
    fn rc003_protocol_bits_cover_only_the_thirteen_ordinary_buttons() {
        let mut seen = 0;
        for (bit, button) in RC003_CAPTURE_BUTTONS.into_iter().enumerate() {
            let mask = rc003_button_mask(button);
            assert_eq!(mask, 1 << bit);
            assert_eq!(seen & mask, 0);
            seen |= mask;
        }
        assert_eq!(seen, RC003_ALL_MASK);
        assert_eq!(RC003_GESTURE_MASK, 0x1fe7);
    }

    #[test]
    fn every_extra_capture_deduplicates_native_echoes_and_requires_configured_bit() {
        for button in RC003_CAPTURE_BUTTONS {
            let mask = rc003_button_mask(button);
            if mask & RC003_GESTURE_MASK == 0 {
                continue;
            }
            let edge = |is_pressed| ButtonEdge { button, is_pressed };
            let mut merger = ButtonStateMerger::default();
            merger.begin_rc003_gestures(mask);
            assert!(merger.apply_keyboard_button_edge(button, true).is_empty());
            assert!(merger.apply_keyboard_button_edge(button, false).is_empty());
            assert_eq!(merger.update_rc003_tap(mask), vec![edge(true)]);
            assert_eq!(
                merger.rc003_gesture_participated(),
                BTreeSet::from([button])
            );
            assert!(merger.apply_keyboard_button_edge(button, true).is_empty());
            assert!(merger.apply_keyboard_button_edge(button, true).is_empty());
            assert!(merger.update_rc003_tap(mask).is_empty());
            // Rejected input cannot release or mutate a confirmed source hold.
            let unconfigured = RC003_GESTURE_MASK & !mask & !7;
            if unconfigured != 0 {
                assert!(merger.update_rc003_tap(unconfigured).is_empty());
                assert!(merger.active_button_set().contains(&button));
            }
            assert!(merger.update_rc003_tap(0x8000).is_empty());
            assert_eq!(merger.update_rc003_tap(0), vec![edge(false)]);
            assert!(merger.apply_keyboard_button_edge(button, false).is_empty());
            assert!(merger.cancel_rc003_tap().1.is_empty());
            assert!(merger.rc003_gesture_participated().is_empty());
            // An unrelated key still follows its ordinary attributed input path.
            assert_eq!(
                merger
                    .apply_keyboard_button_edge(RemoteButton::Ok, true)
                    .len(),
                1
            );
            assert_eq!(
                merger
                    .apply_keyboard_button_edge(RemoteButton::Ok, false)
                    .len(),
                1
            );
        }
    }

    #[test]
    fn every_extra_capture_preserves_preheld_native_until_confirmed_takeover() {
        for button in RC003_CAPTURE_BUTTONS {
            let mask = rc003_button_mask(button);
            if mask & RC003_GESTURE_MASK == 0 {
                continue;
            }
            let edge = |is_pressed| ButtonEdge { button, is_pressed };
            let mut merger = ButtonStateMerger::default();
            assert_eq!(
                merger.apply_keyboard_button_edge(button, true),
                vec![edge(true)]
            );
            merger.begin_rc003_gestures(mask);
            assert!(merger.update_rc003_tap(0).is_empty());
            assert!(merger.rc003_gesture_participated().is_empty());
            assert!(merger.cancel_rc003_tap().1.is_empty());
            assert!(merger.active_button_set().contains(&button));

            merger.begin_rc003_gestures(mask);
            assert_eq!(merger.update_rc003_tap(mask), vec![edge(false), edge(true)]);
            // A delayed UP from the old native hold cannot release the new source.
            assert!(merger.apply_keyboard_button_edge(button, false).is_empty());
            assert_eq!(merger.update_rc003_tap(0), vec![edge(false)]);
            assert!(merger.cancel_rc003_tap().1.is_empty());
            assert_eq!(
                merger.apply_keyboard_button_edge(button, true),
                vec![edge(true)]
            );
        }
    }

    #[test]
    fn every_extra_capture_source_loss_drains_late_edges_without_a_new_press() {
        for button in RC003_CAPTURE_BUTTONS {
            let mask = rc003_button_mask(button);
            if mask & RC003_GESTURE_MASK == 0 {
                continue;
            }
            let edge = |is_pressed| ButtonEdge { button, is_pressed };
            let mut merger = ButtonStateMerger::default();
            merger.begin_rc003_gestures(mask);
            assert_eq!(merger.update_rc003_tap(mask), vec![edge(true)]);
            assert_eq!(merger.cancel_rc003_tap().1, vec![edge(false)]);
            assert!(merger.update_hid_usages(BTreeSet::new()).is_empty());
            assert!(merger.apply_keyboard_button_edge(button, true).is_empty());
            assert!(merger.apply_keyboard_button_edge(button, true).is_empty());
            assert!(merger.apply_keyboard_button_edge(button, false).is_empty());
            assert_eq!(
                merger.apply_keyboard_button_edge(button, true),
                vec![edge(true)]
            );
            assert_eq!(
                merger.apply_keyboard_button_edge(button, false),
                vec![edge(false)]
            );
        }
    }

    #[test]
    fn extra_capture_native_hid_and_keyboard_release_must_both_finish() {
        let mut merger = ButtonStateMerger::default();
        let button = RemoteButton::Up;
        let mask = rc003_button_mask(button);
        merger.begin_rc003_gestures(mask);
        merger.update_rc003_tap(mask);
        merger.apply_keyboard_button_edge(button, true);
        merger.update_hid_usages(BTreeSet::from([0x52]));
        assert_eq!(merger.cancel_rc003_tap().1.len(), 1);
        assert!(merger.apply_keyboard_button_edge(button, false).is_empty());
        assert!(merger.apply_keyboard_button_edge(button, true).is_empty());
        assert!(merger.update_hid_usages(BTreeSet::new()).is_empty());
        assert!(merger.apply_keyboard_button_edge(button, false).is_empty());
        assert_eq!(merger.apply_keyboard_button_edge(button, true).len(), 1);
        assert_eq!(merger.release_all().len(), 1);
        assert!(merger.release_all().is_empty());
        assert!(merger.rc003_gesture_participated().is_empty());
    }

    #[test]
    fn unconfigured_passive_back_and_volume_keep_union_semantics() {
        for (bit, button) in RC003_TAP_BUTTONS.into_iter().enumerate() {
            let mut merger = ButtonStateMerger::default();
            merger.begin_rc003_gestures(rc003_button_mask(RemoteButton::Up));
            assert_eq!(merger.apply_keyboard_button_edge(button, true).len(), 1);
            assert!(merger.update_rc003_tap(1 << bit).is_empty());
            assert!(merger.update_rc003_tap(0).is_empty());
            assert_eq!(merger.apply_keyboard_button_edge(button, false).len(), 1);
            assert!(merger.rc003_gesture_participated().is_empty());
        }
    }

    #[test]
    fn voice_key_is_excluded_from_ordinary_button_path() {
        let mut merger = ButtonStateMerger::default();
        assert!(merger.update_keyboard(keyboard(0x74, 0x0100)).is_empty());
        assert!(merger
            .update_hid_report(&report(&[0x003E]))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn release_all_clears_every_source_once() {
        let mut merger = ButtonStateMerger::default();
        merger.update_keyboard(keyboard(0x27, 0x0100));
        merger.update_hid_report(&report(&[0x0028])).unwrap();
        let releases = merger.release_all();
        assert_eq!(releases.len(), 2);
        assert!(releases.iter().all(|edge| !edge.is_pressed));
        assert!(merger.release_all().is_empty());
    }
}
