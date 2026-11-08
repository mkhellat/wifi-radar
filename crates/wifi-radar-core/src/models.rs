//! Device models and RSSI ranging (ported from Python semantics).

use std::time::{SystemTime, UNIX_EPOCH};

/// Indoor log-distance path-loss exponent.
pub const PATH_LOSS_EXPONENT_INDOOR: f64 = 3.0;
/// Typical RSSI at 1 m for an access point.
pub const RSSI_AT_1M_AP: f64 = -30.0;
/// Typical RSSI at 1 m for a client / adapter.
pub const RSSI_AT_1M_CLIENT: f64 = -35.0;

/// Classification of an observed WiFi device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    /// Broadcasting access point / hotspot.
    Hotspot,
    /// Station associated to a nearby AP.
    Client,
    /// Probe-only adapter (not associated).
    Adapter,
}

impl DeviceKind {
    /// Default RSSI-at-1m reference for this kind.
    pub fn rssi_at_1m(self) -> f64 {
        match self {
            DeviceKind::Hotspot => RSSI_AT_1M_AP,
            DeviceKind::Client | DeviceKind::Adapter => RSSI_AT_1M_CLIENT,
        }
    }
}

/// Observed WiFi device (AP, client, or probe-only adapter).
#[derive(Debug, Clone)]
pub struct WifiDevice {
    /// Normalized MAC address (lowercase, colon-separated).
    pub mac: String,
    /// Device classification.
    pub kind: DeviceKind,
    /// SSID when known.
    pub ssid: String,
    /// OUI vendor string when known.
    pub vendor: String,
    /// Channel number when known.
    pub channel: u32,
    /// Center frequency in MHz.
    pub freq_mhz: u32,
    /// Smoothed or last RSSI in dBm.
    pub rssi_dbm: f64,
    /// Unix time (seconds) of last sighting.
    pub last_seen: f64,
    /// Probe-request SSIDs (adapters).
    pub probe_ssids: Vec<String>,
    /// BSSID the client is associated to.
    pub associated_bssid: String,
    /// Calibrated / rotation bearing in degrees, if any.
    pub bearing_deg: Option<f64>,
    /// Scene-correction bearing override.
    pub bearing_override_deg: Option<f64>,
    /// Bearing confidence in \[0, 1\].
    pub bearing_confidence: f64,
    /// True when bearing came from a manual pin.
    pub bearing_manual: bool,
    /// Security summary string.
    pub security: String,
    /// True if this BSS is in-use on the local interface.
    pub in_use: bool,
    /// Optional scene distance override (metres).
    pub distance_override_m: Option<f64>,
    /// Per-MAC RSSI-at-1m calibration override.
    pub rssi_at_1m_override: Option<f64>,
    /// Anchor diagnostic tag (`anchor`, `stale-cal`, …).
    pub anchor_status: String,
    /// Short correction note for the UI.
    pub correction_note: String,
}

impl WifiDevice {
    /// Construct a hotspot observation with sensible defaults.
    pub fn hotspot(mac: impl Into<String>, rssi_dbm: f64) -> Self {
        Self {
            mac: mac.into(),
            kind: DeviceKind::Hotspot,
            ssid: String::new(),
            vendor: String::new(),
            channel: 0,
            freq_mhz: 0,
            rssi_dbm,
            last_seen: now_unix(),
            probe_ssids: Vec::new(),
            associated_bssid: String::new(),
            bearing_deg: None,
            bearing_override_deg: None,
            bearing_confidence: 0.0,
            bearing_manual: false,
            security: String::new(),
            in_use: false,
            distance_override_m: None,
            rssi_at_1m_override: None,
            anchor_status: String::new(),
            correction_note: String::new(),
        }
    }

    /// Short display label (SSID, probe SSID, or MAC suffix).
    pub fn label(&self) -> String {
        if !self.ssid.is_empty() {
            return self.ssid.chars().take(18).collect();
        }
        if let Some(probe) = self.probe_ssids.first() {
            return probe.chars().take(18).collect();
        }
        let mac = self.mac.as_str();
        if mac.len() >= 8 {
            mac[mac.len() - 8..].to_string()
        } else {
            mac.to_string()
        }
    }

    /// Effective RSSI-at-1m reference (override or kind default).
    pub fn rssi_at_1m(&self) -> f64 {
        self.rssi_at_1m_override
            .unwrap_or_else(|| self.kind.rssi_at_1m())
    }

    /// Log-distance path-loss estimate in metres (clamped to \[0.1, 120\]).
    pub fn distance_m(&self) -> f64 {
        if let Some(d) = self.distance_override_m {
            return d.clamp(0.1, 120.0);
        }
        let ref_dbm = self.rssi_at_1m();
        let mut n = PATH_LOSS_EXPONENT_INDOOR;
        if self.freq_mhz > 4000 {
            n += 0.3;
        }
        let exponent = (ref_dbm - self.rssi_dbm) / (10.0 * n);
        10f64.powf(exponent).clamp(0.1, 120.0)
    }

    /// Stable MAC-hash placeholder bearing (no physical meaning).
    pub fn placeholder_bearing(&self) -> f64 {
        placeholder_bearing_for_mac(&self.mac)
    }

    /// Bearing used for display (override → calibrated → placeholder).
    pub fn display_bearing(&self) -> f64 {
        if let Some(b) = self.bearing_override_deg {
            return b.rem_euclid(360.0);
        }
        if let Some(b) = self.bearing_deg {
            return b.rem_euclid(360.0);
        }
        self.placeholder_bearing()
    }
}

/// Unix seconds as `f64` (matches Python `time.time()` usage).
pub fn now_unix() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// MD5-based placeholder angle: first 8 hex digits mod 360 (Python parity).
pub fn placeholder_bearing_for_mac(mac: &str) -> f64 {
    let digest = md5::compute(mac.as_bytes());
    let hex = format!("{:x}", digest);
    let n = u32::from_str_radix(&hex[..8], 16).unwrap_or(0);
    f64::from(n % 360)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strong_signal_is_close() {
        let d = WifiDevice::hotspot("aa:bb:cc:dd:ee:ff", -30.0);
        assert!(d.distance_m() < 3.0);
    }

    #[test]
    fn weak_signal_is_far() {
        let d = WifiDevice::hotspot("aa:bb:cc:dd:ee:ff", -85.0);
        assert!(d.distance_m() > 20.0);
    }

    #[test]
    fn five_ghz_shorter_than_24_same_rssi() {
        let mut g2 = WifiDevice::hotspot("aa:bb:cc:dd:ee:ff", -60.0);
        g2.freq_mhz = 2437;
        let mut g5 = WifiDevice::hotspot("aa:bb:cc:dd:ee:ff", -60.0);
        g5.freq_mhz = 5180;
        assert!(g5.distance_m() < g2.distance_m());
    }

    #[test]
    fn distance_clamped() {
        let far = WifiDevice::hotspot("aa:bb:cc:dd:ee:ff", -100.0);
        assert!(far.distance_m() <= 120.0);
        let near = WifiDevice::hotspot("aa:bb:cc:dd:ee:ff", 0.0);
        assert!(near.distance_m() >= 0.1);
    }

    #[test]
    fn placeholder_bearing_stable() {
        let a = placeholder_bearing_for_mac("aa:bb:cc:dd:ee:ff");
        let b = placeholder_bearing_for_mac("aa:bb:cc:dd:ee:ff");
        assert!((a - b).abs() < f64::EPSILON);
        assert!((0.0..360.0).contains(&a));
    }
}
