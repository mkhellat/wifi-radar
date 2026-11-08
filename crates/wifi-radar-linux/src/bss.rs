//! Decode nl80211 nested `NL80211_ATTR_BSS` attributes into [`WifiDevice`].
//!
//! Pure parsing — no netlink socket. Live dump/trigger lands in a follow-up.

use wifi_radar_core::{normalize_mac, DeviceKind, WifiDevice};

/// `enum nl80211_bss` values from `<linux/nl80211.h>` (kernel uapi).
pub mod attr {
    /// BSSID of the BSS (6 octets).
    pub const BSSID: u16 = 1;
    /// Frequency in MHz (u32).
    pub const FREQUENCY: u16 = 2;
    /// Binary encoded probe-response IEs.
    pub const INFORMATION_ELEMENTS: u16 = 6;
    /// Signal in mBm (`100 * dBm`), i32.
    pub const SIGNAL_MBM: u16 = 7;
    /// Unitless signal strength 0–100 (u8).
    pub const SIGNAL_UNSPEC: u16 = 8;
}

/// One netlink attribute (type + payload), host-decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawAttr {
    /// Attribute type (`nla_type`, without flags).
    pub attr_type: u16,
    /// Attribute payload bytes.
    pub payload: Vec<u8>,
}

/// Parse a stream of netlink attributes (nested BSS body or top-level message).
pub fn parse_nla_stream(buf: &[u8]) -> Vec<RawAttr> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 4 <= buf.len() {
        let nla_len = u16::from_ne_bytes([buf[i], buf[i + 1]]) as usize;
        let nla_type = u16::from_ne_bytes([buf[i + 2], buf[i + 3]]) & 0x3fff;
        if nla_len < 4 || i + nla_len > buf.len() {
            break;
        }
        let payload = buf[i + 4..i + nla_len].to_vec();
        out.push(RawAttr {
            attr_type: nla_type,
            payload,
        });
        // NLA alignment to 4 bytes.
        i += (nla_len + 3) & !3;
    }
    out
}

/// Build a single aligned NLA record (for fixtures / tests).
pub fn encode_nla(attr_type: u16, payload: &[u8]) -> Vec<u8> {
    let nla_len = 4 + payload.len();
    let mut buf = Vec::with_capacity((nla_len + 3) & !3);
    buf.extend_from_slice(&(nla_len as u16).to_ne_bytes());
    buf.extend_from_slice(&attr_type.to_ne_bytes());
    buf.extend_from_slice(payload);
    while buf.len() % 4 != 0 {
        buf.push(0);
    }
    buf
}

/// Convert nested BSS attributes into a hotspot device, if BSSID is present.
pub fn device_from_bss_attrs(attrs: &[RawAttr]) -> Option<WifiDevice> {
    let mut mac = None;
    let mut freq = 0u32;
    let mut signal = -90.0f64;
    let mut ssid = String::new();
    let mut have_signal = false;

    for a in attrs {
        match a.attr_type {
            attr::BSSID if a.payload.len() >= 6 => {
                mac = Some(format_mac(&a.payload[..6]));
            }
            attr::FREQUENCY if a.payload.len() >= 4 => {
                freq = u32::from_ne_bytes(a.payload[..4].try_into().ok()?);
            }
            attr::SIGNAL_MBM if a.payload.len() >= 4 => {
                let mbm = i32::from_ne_bytes(a.payload[..4].try_into().ok()?);
                signal = f64::from(mbm) / 100.0;
                have_signal = true;
            }
            attr::SIGNAL_UNSPEC if !have_signal && !a.payload.is_empty() => {
                // Map 0..100 unitless into a rough dBm band (same order as iw dumps).
                let u = a.payload[0] as f64;
                signal = -100.0 + u * 0.7;
            }
            attr::INFORMATION_ELEMENTS => {
                if let Some(s) = ssid_from_ies(&a.payload) {
                    ssid = s;
                }
            }
            _ => {}
        }
    }

    let mac = mac?;
    let channel = freq_to_channel(freq);
    let mut dev = WifiDevice::hotspot(normalize_mac(&mac), signal);
    dev.kind = DeviceKind::Hotspot;
    dev.ssid = ssid;
    dev.channel = channel;
    dev.freq_mhz = freq;
    Some(dev)
}

/// Decode every nested `NL80211_ATTR_BSS` (type 47) in a top-level attr stream.
pub fn devices_from_nl80211_attrs(top: &[RawAttr]) -> Vec<WifiDevice> {
    const ATTR_BSS: u16 = 47;
    let mut devices = Vec::new();
    for a in top {
        if a.attr_type != ATTR_BSS {
            continue;
        }
        let nested = parse_nla_stream(&a.payload);
        if let Some(d) = device_from_bss_attrs(&nested) {
            devices.push(d);
        }
    }
    devices
}

fn format_mac(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn ssid_from_ies(ies: &[u8]) -> Option<String> {
    let mut i = 0usize;
    while i + 2 <= ies.len() {
        let id = ies[i];
        let len = ies[i + 1] as usize;
        i += 2;
        if i + len > ies.len() {
            break;
        }
        if id == 0 {
            // SSID element.
            let raw = &ies[i..i + len];
            if raw.is_empty() {
                return Some(String::new());
            }
            return Some(String::from_utf8_lossy(raw).into_owned());
        }
        i += len;
    }
    None
}

fn freq_to_channel(freq: u32) -> u32 {
    if (2412..=2484).contains(&freq) {
        if freq == 2484 {
            return 14;
        }
        return (freq - 2407) / 5;
    }
    if (5180..=5825).contains(&freq) {
        return (freq - 5000) / 5;
    }
    if (5955..=7115).contains(&freq) {
        return (freq - 5950) / 5;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_nla_and_bss_device() {
        let mut nested = Vec::new();
        nested.extend(encode_nla(
            attr::BSSID,
            &[0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0x01],
        ));
        nested.extend(encode_nla(attr::FREQUENCY, &2437u32.to_ne_bytes()));
        // -45.00 dBm => -4500 mBm
        nested.extend(encode_nla(attr::SIGNAL_MBM, &(-4500i32).to_ne_bytes()));
        // IE: SSID "TestNet"
        let mut ies = vec![0u8, 7];
        ies.extend(b"TestNet");
        nested.extend(encode_nla(attr::INFORMATION_ELEMENTS, &ies));

        let top = encode_nla(47, &nested);
        let attrs = parse_nla_stream(&top);
        let devices = devices_from_nl80211_attrs(&attrs);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].mac, "aa:bb:cc:dd:ee:01");
        assert_eq!(devices[0].ssid, "TestNet");
        assert_eq!(devices[0].channel, 6);
        assert_eq!(devices[0].freq_mhz, 2437);
        assert!((devices[0].rssi_dbm - (-45.0)).abs() < 0.01);
    }

    #[test]
    fn missing_bssid_skipped() {
        let nested = encode_nla(attr::FREQUENCY, &5180u32.to_ne_bytes());
        assert!(device_from_bss_attrs(&parse_nla_stream(&nested)).is_none());
    }
}
