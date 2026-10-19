//! Read-only Core Audio inventory and conservative VB-CABLE route association.
//!
//! Called on the audio worker's COM MTA. Endpoint IDs remain in memory/settings;
//! diagnostics contain only counts, state categories and decision reasons.
use crate::{MicrophoneEndpoint, PlatformError};
use std::collections::BTreeSet;
use windows::core::{Interface, PCWSTR, PWSTR};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_DevNode_PropertyW, CM_Locate_DevNodeW, CM_LOCATE_DEVNODE_PHANTOM, CR_SUCCESS,
};
use windows::Win32::Devices::FunctionDiscovery::{
    PKEY_Device_FriendlyName, PKEY_Device_InstanceId,
};
use windows::Win32::Devices::Properties::{
    DEVPKEY_Device_DriverProvider, DEVPKEY_Device_Parent, DEVPKEY_Device_ProblemCode,
    DEVPKEY_Device_Service, DEVPROPTYPE, DEVPROP_TYPE_STRING, DEVPROP_TYPE_UINT32,
};
use windows::Win32::Foundation::{DEVPROPKEY, E_INVALIDARG, E_POINTER, PROPERTYKEY};
use windows::Win32::Media::Audio::{
    eCapture, eCommunications, eConsole, eRender, EDataFlow, ERole, IDeviceTopology, IMMDevice,
    IMMDeviceEnumerator, IMMEndpoint, MMDeviceEnumerator, PKEY_AudioEndpoint_FormFactor,
    DEVICE_STATE_ACTIVE, DEVICE_STATE_DISABLED, DEVICE_STATE_NOTPRESENT, DEVICE_STATE_UNPLUGGED,
};
use windows::Win32::System::Com::StructuredStorage::{
    PropVariantClear, PropVariantToStringAlloc, PropVariantToUInt32,
};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL, STGM_READ};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;

#[derive(Clone, Debug)]
struct CableParent {
    id: String,
    problem: Option<u32>,
}

#[derive(Clone, Debug)]
struct Endpoint {
    id: String,
    name: String,
    capture: bool,
    state: u32,
    form_factor: Option<u32>,
    cable_parent: Option<CableParent>,
}

impl Endpoint {
    fn active_parent(&self) -> Option<&str> {
        self.cable_parent.as_ref().and_then(|parent| {
            (self.state == DEVICE_STATE_ACTIVE.0 && parent.problem == Some(0))
                .then_some(parent.id.as_str())
        })
    }
}

/// Enumerates actual active recording devices. `render_endpoint_ids` is populated
/// only for one known VB-CABLE recording interface and its verified common parent.
pub(crate) fn list_microphones() -> Result<Vec<MicrophoneEndpoint>, PlatformError> {
    let result = (|| {
        let enumerator = enumerator()?;
        let captures = active_endpoints(&enumerator, eCapture)?;
        let renders = active_endpoints(&enumerator, eRender)?;
        let default = default_id(&enumerator, eConsole);
        let communications = default_id(&enumerator, eCommunications);
        let mut microphones: Vec<_> = captures
            .iter()
            .map(|capture| MicrophoneEndpoint {
                id: capture.id.clone(),
                name: capture.name.clone(),
                is_virtual_cable_candidate: capture.cable_parent.is_some(),
                is_default: default.as_deref() == Some(capture.id.as_str()),
                is_default_communications: communications.as_deref() == Some(capture.id.as_str()),
                render_endpoint_ids: paired_render_ids(capture, &captures, &renders),
            })
            .collect();
        microphones.sort_by(|left, right| {
            right
                .is_virtual_cable_candidate
                .cmp(&left.is_virtual_cable_candidate)
                .then_with(|| right.is_default.cmp(&left.is_default))
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        crate::ble::gatt_note(format!(
            "audio_microphones action=list terminal_result=passed active_count={} virtual_count={} paired_count={} default_detected={} communications_default_detected={}",
            microphones.len(),
            microphones.iter().filter(|mic| mic.is_virtual_cable_candidate).count(),
            microphones.iter().filter(|mic| !mic.render_endpoint_ids.is_empty()).count(),
            default.is_some(),
            communications.is_some(),
        ));
        Ok(microphones)
    })();
    if result.is_err() {
        crate::ble::gatt_note(
            "audio_microphones action=list terminal_result=failed reason=inventory_failed".into(),
        );
    }
    result
}

/// An occupied render interface may have one peer feeding the exact same
/// verified recording device. Never substitute another microphone or infer
/// this relationship from friendly names.
pub(crate) fn alternate_render_for_same_microphone(
    selected_id: &str,
) -> Result<Option<String>, PlatformError> {
    let enumerator = enumerator()?;
    let captures = active_endpoints(&enumerator, eCapture)?;
    let renders = active_endpoints(&enumerator, eRender)?;
    let (alternate, reason) = choose_alternate_render(selected_id, &captures, &renders);
    crate::ble::gatt_note(format!(
        "audio_route action=alternate_interface terminal_result={} reason={reason}",
        if alternate.is_some() {
            "passed"
        } else {
            "unresolved"
        },
    ));
    Ok(alternate)
}

fn choose_alternate_render(
    selected_id: &str,
    captures: &[Endpoint],
    renders: &[Endpoint],
) -> (Option<String>, &'static str) {
    if has_unverified_virtual_endpoint(captures, renders) {
        return (None, "unverified_virtual_identity");
    }
    let mut matching_pairs = captures.iter().filter_map(|capture| {
        let paired = paired_render_ids(capture, captures, renders);
        paired.iter().any(|id| id == selected_id).then_some(paired)
    });
    let Some(pair) = matching_pairs.next() else {
        return (None, "selected_capture_pair_unverified");
    };
    if matching_pairs.next().is_some() || pair.len() != 2 {
        return (None, "alternate_interface_ambiguous_or_absent");
    }
    let alternate = pair.into_iter().find(|id| id != selected_id);
    (alternate, "same_verified_capture_unique_peer")
}

/// Finds a replacement only for an unavailable instance of the known base
/// VB-CABLE driver. No default device, PnP state or audio stream is changed here.
/// A disabled endpoint, an unknown old identity or ambiguous alternatives must
/// be resolved explicitly by the user instead of silently changing routes.
pub(crate) fn resolve_replacement(
    saved_id: &str,
    expected_name: &str,
) -> Result<Option<String>, PlatformError> {
    let enumerator = enumerator()?;
    let id = wide(saved_id);
    let old_device = match unsafe { enumerator.GetDevice(PCWSTR(id.as_ptr())) } {
        Ok(device) => device,
        Err(error) => {
            crate::ble::gatt_note(format!(
                "audio_route action=resolve terminal_result=unresolved reason=saved_endpoint_unresolvable hresult={:08x}",
                error.code().0 as u32
            ));
            return Ok(None);
        }
    };
    let old = inspect_endpoint(&old_device, &enumerator)?;
    let captures = active_endpoints(&enumerator, eCapture)?;
    let renders = active_endpoints(&enumerator, eRender)?;
    let (replacement, reason) = choose_replacement(&old, expected_name, &captures, &renders);
    let parent_problem = old
        .cable_parent
        .as_ref()
        .and_then(|parent| parent.problem)
        .map(|problem| problem.to_string())
        .unwrap_or_else(|| "unknown".into());
    crate::ble::gatt_note(format!(
        "audio_route action=resolve terminal_result={} reason={reason} saved_state={} verified_parent={} parent_problem={} active_capture_count={} active_render_count={}",
        if replacement.is_some() { "passed" } else { "unresolved" },
        state_label(old.state),
        old.cable_parent.is_some(),
        parent_problem,
        captures.len(),
        renders.len(),
    ));
    Ok(replacement)
}

fn paired_render_ids(
    capture: &Endpoint,
    captures: &[Endpoint],
    renders: &[Endpoint],
) -> Vec<String> {
    if has_unverified_virtual_endpoint(captures, renders) {
        crate::ble::gatt_note("audio_inventory action=pair terminal_result=unverified reason=unverified_virtual_identity".into());
        return Vec::new();
    }
    let Some(parent) = capture.active_parent() else {
        return Vec::new();
    };
    // The base VB-CABLE driver has one recording side and up to two playback
    // interfaces. Multiple captures from a parent are not a documented pairing.
    if captures
        .iter()
        .filter(|item| item.active_parent() == Some(parent))
        .count()
        != 1
    {
        return Vec::new();
    }
    let mut paired: Vec<_> = renders
        .iter()
        .filter(|render| !render.capture && render.active_parent() == Some(parent))
        .collect();
    paired.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    paired.into_iter().map(|render| render.id.clone()).collect()
}

fn choose_replacement(
    old: &Endpoint,
    expected_name: &str,
    captures: &[Endpoint],
    renders: &[Endpoint],
) -> (Option<String>, &'static str) {
    if has_unverified_virtual_endpoint(captures, renders) {
        return (None, "unverified_virtual_identity");
    }
    if old.capture || old.state != DEVICE_STATE_NOTPRESENT.0 {
        return (None, "saved_endpoint_not_missing_render");
    }
    let Some(old_parent) = old.cable_parent.as_ref() else {
        return (None, "saved_driver_unverified");
    };
    let parents: BTreeSet<_> = captures
        .iter()
        .filter_map(Endpoint::active_parent)
        .collect();
    // First prefer the same device instance. Cross-instance recovery is allowed
    // only when the old driver has failed and there is one healthy base cable.
    let parent = if parents.contains(old_parent.id.as_str()) {
        old_parent.id.as_str()
    } else {
        if old_parent.problem != Some(10) {
            return (None, "saved_parent_not_failed");
        }
        if parents.len() != 1 {
            return (None, "healthy_cable_count_not_one");
        }
        *parents.first().expect("exactly one parent")
    };
    let paired_captures: Vec<_> = captures
        .iter()
        .filter(|capture| capture.capture && capture.active_parent() == Some(parent))
        .collect();
    if paired_captures.len() != 1 {
        return (None, "capture_pair_ambiguous");
    }
    let Some(form_factor) = old.form_factor else {
        return (None, "saved_interface_unknown");
    };
    let expected_kind = interface_kind(expected_name).or_else(|| interface_kind(&old.name));
    let candidates: Vec<_> = renders
        .iter()
        .filter(|render| {
            !render.capture
                && render.active_parent() == Some(parent)
                && render.form_factor == Some(form_factor)
                && expected_kind.is_none_or(|kind| interface_kind(&render.name) == Some(kind))
        })
        .collect();
    if candidates.len() != 1 {
        return (None, "render_interface_ambiguous");
    }
    (Some(candidates[0].id.clone()), "verified_virtual_route")
}

fn has_unverified_virtual_endpoint(captures: &[Endpoint], renders: &[Endpoint]) -> bool {
    // A friendly name can veto an unsafe inference, but never proves identity.
    // Do not infer a unique cable from a list with unverified cable candidates.
    captures.iter().chain(renders).any(|endpoint| {
        endpoint.cable_parent.is_none() && crate::is_virtual_cable_output_name(&endpoint.name)
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InterfaceKind {
    Multichannel,
    Standard,
}

// Names are used only to retain interface preference after the driver and
// capture/render relationship have independently been verified. They never
// establish a virtual-device identity or pair unrelated endpoints.
fn interface_kind(name: &str) -> Option<InterfaceKind> {
    let prefix: String = name
        .split('(')
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|value| !value.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    match prefix.as_str() {
        "cablein16ch" => Some(InterfaceKind::Multichannel),
        "cableinput" | "speakers" => Some(InterfaceKind::Standard),
        _ => None,
    }
}

fn enumerator() -> Result<IMMDeviceEnumerator, PlatformError> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
        .map_err(|error| api_error("创建音频设备枚举器", error))
}

fn active_endpoints(
    enumerator: &IMMDeviceEnumerator,
    flow: EDataFlow,
) -> Result<Vec<Endpoint>, PlatformError> {
    let devices = unsafe { enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) }
        .map_err(|error| api_error("枚举音频设备", error))?;
    let count =
        unsafe { devices.GetCount() }.map_err(|error| api_error("读取音频设备数量", error))?;
    let mut endpoints = Vec::new();
    for index in 0..count {
        let result = unsafe { devices.Item(index) }
            .map_err(|error| api_error("读取音频设备", error))
            .and_then(|device| inspect_endpoint(&device, enumerator));
        match result {
            Ok(endpoint) if endpoint.state == DEVICE_STATE_ACTIVE.0 => endpoints.push(endpoint),
            Ok(_) => {}
            Err(error) => {
                crate::ble::gatt_note(
                    "audio_inventory action=inspect terminal_result=failed reason=endpoint_changed_or_unreadable".into(),
                );
                // Neither recovery nor auto-selection can establish uniqueness
                // from a partial list. Preserve user intent and retry later.
                return Err(error);
            }
        }
    }
    Ok(endpoints)
}

fn inspect_endpoint(
    device: &IMMDevice,
    enumerator: &IMMDeviceEnumerator,
) -> Result<Endpoint, PlatformError> {
    let id = device_id(device).map_err(|error| api_error("读取音频设备标识", error))?;
    let state =
        unsafe { device.GetState() }.map_err(|error| api_error("读取音频设备状态", error))?;
    let endpoint: IMMEndpoint = device
        .cast()
        .map_err(|error| api_error("读取音频设备方向", error))?;
    let flow =
        unsafe { endpoint.GetDataFlow() }.map_err(|error| api_error("读取音频设备方向", error))?;
    let store = unsafe { device.OpenPropertyStore(STGM_READ) }
        .map_err(|error| api_error("读取音频设备属性", error))?;
    let name = property_string(&store, &PKEY_Device_FriendlyName)
        .unwrap_or_else(|| "未命名音频设备".into());
    let parent_result = property_string(&store, &PKEY_Device_InstanceId)
        .ok_or("endpoint_instance_property_unavailable")
        .and_then(|instance| verified_cable_parent(&instance))
        .or_else(|_| topology_cable_parent(device, enumerator));
    if let Err(reason) = parent_result.as_ref() {
        if crate::is_virtual_cable_output_name(&name) {
            crate::ble::gatt_note(format!(
                "audio_inventory action=verify_parent terminal_result=unverified reason={reason}"
            ));
        }
    }
    let cable_parent = parent_result.ok();
    Ok(Endpoint {
        id,
        name,
        capture: flow == eCapture,
        state: state.0,
        form_factor: property_u32(&store, &PKEY_AudioEndpoint_FormFactor),
        cable_parent,
    })
}

fn verified_cable_parent(instance_id: &str) -> Result<CableParent, &'static str> {
    let endpoint_node = locate_node(instance_id).ok_or("endpoint_node_unavailable")?;
    let parent_id =
        node_string(endpoint_node, &DEVPKEY_Device_Parent).ok_or("endpoint_parent_unavailable")?;
    verified_cable_node(&parent_id)
}

fn topology_cable_parent(
    device: &IMMDevice,
    enumerator: &IMMDeviceEnumerator,
) -> Result<CableParent, &'static str> {
    // Activates only the read-only topology interface, never an AudioClient,
    // volume/session control, stream or recording device.
    let topology: IDeviceTopology = unsafe { device.Activate(CLSCTX_ALL, None) }
        .map_err(|_| "endpoint_topology_unavailable")?;
    let connector =
        unsafe { topology.GetConnector(0) }.map_err(|_| "endpoint_connector_unavailable")?;
    let adapter_id = unsafe { connector.GetDeviceIdConnectedTo() }
        .and_then(take_string)
        .map_err(|_| "adapter_identity_unavailable")?;
    let adapter_id_wide = wide(&adapter_id);
    let adapter = unsafe { enumerator.GetDevice(PCWSTR(adapter_id_wide.as_ptr())) }
        .map_err(|_| "adapter_unavailable")?;
    let store = unsafe { adapter.OpenPropertyStore(STGM_READ) }
        .map_err(|_| "adapter_properties_unavailable")?;
    let instance = property_string(&store, &PKEY_Device_InstanceId)
        .ok_or("adapter_instance_property_unavailable")?;
    verified_cable_node(&instance)
}

fn verified_cable_node(parent_id: &str) -> Result<CableParent, &'static str> {
    let parent_node = locate_node(parent_id).ok_or("parent_node_unavailable")?;
    let service =
        node_string(parent_node, &DEVPKEY_Device_Service).ok_or("parent_service_unavailable")?;
    let provider = node_string(parent_node, &DEVPKEY_Device_DriverProvider)
        .ok_or("parent_provider_unavailable")?;
    if !service.eq_ignore_ascii_case("VBAudioVACMME")
        || !provider.eq_ignore_ascii_case("VB-Audio Software")
    {
        return Err("parent_driver_not_base_vb_cable");
    }
    Ok(CableParent {
        id: parent_id.to_ascii_uppercase(),
        problem: node_u32(parent_node, &DEVPKEY_Device_ProblemCode),
    })
}

fn locate_node(id: &str) -> Option<u32> {
    let id = wide(id);
    let mut node = 0;
    (unsafe { CM_Locate_DevNodeW(&mut node, PCWSTR(id.as_ptr()), CM_LOCATE_DEVNODE_PHANTOM) }
        == CR_SUCCESS)
        .then_some(node)
}

fn node_property(node: u32, key: &DEVPROPKEY) -> Option<(DEVPROPTYPE, Vec<u8>)> {
    let mut property_type = DEVPROPTYPE::default();
    // Bounded property reads. Device identity/service/provider are short strings;
    // an unexpected size is treated as unverified rather than allocating freely.
    let mut buffer = vec![0; 4096];
    let mut length = buffer.len() as u32;
    let status = unsafe {
        CM_Get_DevNode_PropertyW(
            node,
            key,
            &mut property_type,
            Some(buffer.as_mut_ptr()),
            &mut length,
            0,
        )
    };
    if status != CR_SUCCESS || length as usize > buffer.len() {
        return None;
    }
    buffer.truncate(length as usize);
    Some((property_type, buffer))
}

fn node_string(node: u32, key: &DEVPROPKEY) -> Option<String> {
    let (kind, buffer) = node_property(node, key)?;
    if kind != DEVPROP_TYPE_STRING || buffer.len() % 2 != 0 {
        return None;
    }
    let chars: Vec<_> = buffer
        .chunks_exact(2)
        .map(|value| u16::from_le_bytes([value[0], value[1]]))
        .take_while(|value| *value != 0)
        .collect();
    String::from_utf16(&chars)
        .ok()
        .filter(|value| !value.is_empty())
}

fn node_u32(node: u32, key: &DEVPROPKEY) -> Option<u32> {
    let (kind, buffer) = node_property(node, key)?;
    (kind == DEVPROP_TYPE_UINT32)
        .then(|| buffer.as_slice().try_into().ok().map(u32::from_le_bytes))
        .flatten()
}

fn property_string(store: &IPropertyStore, key: &PROPERTYKEY) -> Option<String> {
    let mut value = unsafe { store.GetValue(key) }.ok()?;
    let result = unsafe { PropVariantToStringAlloc(&value) }
        .ok()
        .and_then(|pointer| take_string(pointer).ok());
    let _ = unsafe { PropVariantClear(&mut value) };
    result.filter(|value| !value.is_empty())
}

fn property_u32(store: &IPropertyStore, key: &PROPERTYKEY) -> Option<u32> {
    let mut value = unsafe { store.GetValue(key) }.ok()?;
    let result = unsafe { PropVariantToUInt32(&value) }.ok();
    let _ = unsafe { PropVariantClear(&mut value) };
    result
}

fn device_id(device: &IMMDevice) -> windows::core::Result<String> {
    take_string(unsafe { device.GetId() }?)
}

fn take_string(pointer: PWSTR) -> windows::core::Result<String> {
    if pointer.is_null() {
        return Err(windows::core::Error::from_hresult(E_POINTER));
    }
    let result = unsafe { pointer.to_string() }
        .map_err(|_| windows::core::Error::from_hresult(E_INVALIDARG));
    unsafe { CoTaskMemFree(Some(pointer.0.cast())) };
    result
}

fn default_id(enumerator: &IMMDeviceEnumerator, role: ERole) -> Option<String> {
    unsafe { enumerator.GetDefaultAudioEndpoint(eCapture, role) }
        .ok()
        .and_then(|device| device_id(&device).ok())
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn state_label(state: u32) -> &'static str {
    if state == DEVICE_STATE_ACTIVE.0 {
        "active"
    } else if state == DEVICE_STATE_DISABLED.0 {
        "disabled"
    } else if state == DEVICE_STATE_NOTPRESENT.0 {
        "not_present"
    } else if state == DEVICE_STATE_UNPLUGGED.0 {
        "unplugged"
    } else {
        "unknown"
    }
}

fn api_error(action: &str, error: windows::core::Error) -> PlatformError {
    // HRESULTs are useful without arbitrary COM messages that may expose IDs.
    PlatformError::Audio(format!("{action}失败（0x{:08X}）", error.code().0 as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(id: &str, parent: &str, capture: bool, name: &str, factor: u32) -> Endpoint {
        Endpoint {
            id: id.into(),
            name: name.into(),
            capture,
            state: DEVICE_STATE_ACTIVE.0,
            form_factor: Some(factor),
            cable_parent: Some(CableParent {
                id: parent.into(),
                problem: Some(0),
            }),
        }
    }

    fn missing_old() -> Endpoint {
        let mut old = endpoint(
            "old-render",
            "old-parent",
            false,
            "CABLE In 16ch (VB-Audio Virtual Cable)",
            2,
        );
        old.state = DEVICE_STATE_NOTPRESENT.0;
        old.cable_parent.as_mut().unwrap().problem = Some(10);
        old
    }

    fn healthy_route() -> (Vec<Endpoint>, Vec<Endpoint>) {
        (
            vec![endpoint("capture", "new-parent", true, "CABLE Output", 2)],
            vec![
                endpoint(
                    "render-16",
                    "new-parent",
                    false,
                    "CABLE In 16 Ch (2- VB-Audio Virtual Cable)",
                    2,
                ),
                endpoint(
                    "render-normal",
                    "new-parent",
                    false,
                    "Speakers (2- VB-Audio Virtual Cable)",
                    1,
                ),
            ],
        )
    }

    #[test]
    fn two_render_interfaces_pair_to_one_verified_capture() {
        let (captures, mut renders) = healthy_route();
        let mut unrelated = endpoint(
            "ordinary-speaker",
            "other-parent",
            false,
            "Speakers (Realtek Audio)",
            1,
        );
        unrelated.cable_parent = None;
        renders.push(unrelated);
        assert_eq!(
            paired_render_ids(&captures[0], &captures, &renders),
            vec!["render-16", "render-normal"]
        );
    }

    #[test]
    fn busy_render_can_use_only_its_same_verified_capture_peer() {
        let (mut captures, mut renders) = healthy_route();
        captures.push(endpoint(
            "other-capture",
            "other-parent",
            true,
            "CABLE Output",
            2,
        ));
        renders.push(endpoint(
            "other-render",
            "other-parent",
            false,
            "CABLE Input",
            1,
        ));
        assert_eq!(
            choose_alternate_render("render-16", &captures, &renders)
                .0
                .as_deref(),
            Some("render-normal")
        );
        assert_eq!(
            choose_alternate_render("render-normal", &captures, &renders)
                .0
                .as_deref(),
            Some("render-16")
        );
        assert!(choose_alternate_render("other-render", &captures, &renders)
            .0
            .is_none());
        assert!(choose_alternate_render("unknown", &captures, &renders)
            .0
            .is_none());
    }

    #[test]
    fn busy_render_never_uses_missing_ambiguous_or_unverified_peers() {
        let (captures, mut renders) = healthy_route();
        renders.pop();
        assert!(choose_alternate_render("render-16", &captures, &renders)
            .0
            .is_none());
        let (captures, mut renders) = healthy_route();
        renders.push(endpoint(
            "third-render",
            "new-parent",
            false,
            "CABLE Input",
            1,
        ));
        assert!(choose_alternate_render("render-16", &captures, &renders)
            .0
            .is_none());
        let (mut captures, renders) = healthy_route();
        captures.push(endpoint(
            "duplicate-capture",
            "new-parent",
            true,
            "CABLE Output",
            2,
        ));
        assert!(choose_alternate_render("render-16", &captures, &renders)
            .0
            .is_none());
        let (captures, mut renders) = healthy_route();
        renders[1].cable_parent = None;
        assert!(choose_alternate_render("render-16", &captures, &renders)
            .0
            .is_none());
    }

    #[test]
    fn recovering_failed_duplicate_preserves_16_channel_interface() {
        let old = missing_old();
        let (captures, renders) = healthy_route();
        assert_eq!(
            choose_replacement(&old, &old.name, &captures, &renders)
                .0
                .as_deref(),
            Some("render-16")
        );
    }

    #[test]
    fn two_healthy_cables_are_ambiguous_even_if_names_match() {
        let old = missing_old();
        let (mut captures, renders) = healthy_route();
        captures.push(endpoint(
            "second-capture",
            "second-parent",
            true,
            "CABLE Output",
            2,
        ));
        assert_eq!(
            choose_replacement(&old, &old.name, &captures, &renders),
            (None, "healthy_cable_count_not_one")
        );
    }

    #[test]
    fn multiple_captures_do_not_claim_a_pair() {
        let old = missing_old();
        let (mut captures, renders) = healthy_route();
        captures.push(endpoint(
            "second-capture",
            "new-parent",
            true,
            "CABLE Output",
            2,
        ));
        assert!(paired_render_ids(&captures[0], &captures, &renders).is_empty());
        assert_eq!(
            choose_replacement(&old, &old.name, &captures, &renders),
            (None, "capture_pair_ambiguous")
        );
    }

    #[test]
    fn unverified_virtual_endpoint_prevents_unique_cable_inference() {
        let old = missing_old();
        let (captures, mut renders) = healthy_route();
        let mut unverified = endpoint(
            "unverified-render",
            "unknown-parent",
            false,
            "CABLE Input (VB-Audio Virtual Cable)",
            1,
        );
        unverified.cable_parent = None;
        renders.push(unverified);
        assert!(paired_render_ids(&captures[0], &captures, &renders).is_empty());
        assert_eq!(
            choose_replacement(&old, &old.name, &captures, &renders),
            (None, "unverified_virtual_identity")
        );
    }

    #[test]
    fn unknown_driver_and_disabled_original_do_not_rebind() {
        let mut old = missing_old();
        let (captures, renders) = healthy_route();
        old.state = DEVICE_STATE_DISABLED.0;
        assert!(choose_replacement(&old, &old.name, &captures, &renders)
            .0
            .is_none());
        old.state = DEVICE_STATE_NOTPRESENT.0;
        old.cable_parent = None;
        assert_eq!(
            choose_replacement(&old, &old.name, &captures, &renders),
            (None, "saved_driver_unverified")
        );
    }

    #[test]
    fn healthy_or_unknown_original_parent_prevents_cross_instance_switch() {
        let mut old = missing_old();
        let (captures, renders) = healthy_route();
        for problem in [Some(0), Some(22), Some(28), None] {
            old.cable_parent.as_mut().unwrap().problem = problem;
            assert_eq!(
                choose_replacement(&old, &old.name, &captures, &renders),
                (None, "saved_parent_not_failed")
            );
        }
    }

    #[test]
    fn unknown_or_duplicate_interface_is_not_guessed() {
        let mut old = missing_old();
        let (captures, mut renders) = healthy_route();
        old.form_factor = None;
        assert_eq!(
            choose_replacement(&old, &old.name, &captures, &renders),
            (None, "saved_interface_unknown")
        );
        old.form_factor = Some(2);
        renders.push(endpoint(
            "duplicate",
            "new-parent",
            false,
            "CABLE In 16 Ch",
            2,
        ));
        assert_eq!(
            choose_replacement(&old, &old.name, &captures, &renders),
            (None, "render_interface_ambiguous")
        );
    }

    #[test]
    fn same_parent_is_preferred_over_other_healthy_cables() {
        let mut old = missing_old();
        let (mut captures, renders) = healthy_route();
        old.cable_parent = Some(CableParent {
            id: "new-parent".into(),
            problem: Some(0),
        });
        captures.push(endpoint("other", "other-parent", true, "CABLE Output", 2));
        assert_eq!(
            choose_replacement(&old, &old.name, &captures, &renders)
                .0
                .as_deref(),
            Some("render-16")
        );
    }

    #[test]
    #[ignore = "Read-only local hardware check; requires retained settings fixture"]
    fn audio_inventory_live() {
        wasapi::initialize_mta()
            .ok()
            .expect("COM MTA initialization");
        let result = (|| {
            let microphones = list_microphones().expect("capture inventory");
            let path = std::env::var("SAYALL_AUDIO_PROBE_SETTINGS")
                .expect("set private settings fixture path without printing it");
            let settings: serde_json::Value =
                serde_json::from_slice(&std::fs::read(path).expect("settings fixture readable"))
                    .expect("valid settings fixture");
            let saved_id = settings["audio_endpoint_id"]
                .as_str()
                .expect("saved audio ID");
            let saved_name = settings["audio_endpoint_name"]
                .as_str()
                .expect("saved audio name");
            let enumerator = enumerator().expect("audio enumerator");
            let id = wide(saved_id);
            let old_device = unsafe { enumerator.GetDevice(PCWSTR(id.as_ptr())) }
                .expect("retained old endpoint queryable");
            let old =
                inspect_endpoint(&old_device, &enumerator).expect("inspect retained old endpoint");
            let old_store =
                unsafe { old_device.OpenPropertyStore(STGM_READ) }.expect("old properties");
            let old_instance = property_string(&old_store, &PKEY_Device_InstanceId);
            let old_node = old_instance.as_deref().and_then(locate_node);
            let captures = active_endpoints(&enumerator, eCapture).expect("capture facts");
            let renders = active_endpoints(&enumerator, eRender).expect("render facts");
            let (replacement, reason) = choose_replacement(&old, saved_name, &captures, &renders);
            let public_replacement =
                resolve_replacement(saved_id, saved_name).expect("read-only replacement resolver");
            assert_eq!(replacement.is_some(), public_replacement.is_some());
            println!(
                "{}",
                serde_json::json!({
                    "read_only": true,
                    "active_microphone_count": microphones.len(),
                    "virtual_microphone_count": microphones.iter().filter(|mic| mic.is_virtual_cable_candidate).count(),
                    "default_capture_detected": microphones.iter().any(|mic| mic.is_default),
                    "communications_default_detected": microphones.iter().any(|mic| mic.is_default_communications),
                    "paired_render_counts": microphones.iter().filter(|mic| mic.is_virtual_cable_candidate).map(|mic| mic.render_endpoint_ids.len()).collect::<Vec<_>>(),
                    "saved_state": state_label(old.state),
                    "saved_parent_verified": old.cable_parent.is_some(),
                "saved_parent_verify_reason": old_instance.as_deref().map(|id| verified_cable_parent(id).err()),
                "saved_topology_verify_reason": topology_cable_parent(&old_device, &enumerator).err(),
                "saved_instance_is_endpoint_node": old_instance.as_deref().is_some_and(|id| id.to_ascii_uppercase().starts_with("SWD\\MMDEVAPI\\")),
                "saved_instance_is_base_cable": old_node.and_then(|node| node_string(node, &DEVPKEY_Device_Service)).is_some_and(|service| service.eq_ignore_ascii_case("VBAudioVACMME")),
                    "saved_parent_problem": old.cable_parent.as_ref().and_then(|parent| parent.problem),
                    "replacement_found": replacement.is_some(),
                    "replacement_reason": reason,
                })
            );
            assert!(
                !microphones.is_empty(),
                "live machine has capture endpoints"
            );
            assert!(
                replacement.is_some(),
                "retained missing VB route resolves conservatively"
            );
        })();
        wasapi::deinitialize();
        result
    }
}
