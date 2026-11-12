"""Scene-wide correction models built on top of per-MAC calibration."""

from __future__ import annotations

from copy import copy
from dataclasses import dataclass
from statistics import median
from typing import Any

from wifi_radar.calib_bridge import (
    NativeCalibrationStore,
    native_scene_ready,
)
from wifi_radar.calibration import CalibrationMode, CalibrationStore
from wifi_radar.models import (
    PATH_LOSS_EXPONENT_INDOOR,
    RSSI_AT_1M_AP,
    RSSI_AT_1M_CLIENT,
    DeviceKind,
    WifiDevice,
)
from wifi_radar.native import native

CalibrationBackend = CalibrationStore | NativeCalibrationStore


@dataclass
class AnchorDiagnostic:
    """Residuals for one visible calibrated device."""

    mac: str
    distance_target_m: float | None
    current_distance_m: float | None
    distance_scale: float | None
    bearing_target_deg: float | None
    bearing_offset_deg: float | None
    consistent: bool
    reason: str = ""


@dataclass
class CorrectionSummary:
    """Display-facing summary of scene correction state."""

    mode: CalibrationMode
    visible_anchors: int = 0
    active_anchors: int = 0
    distance_scale: float = 1.0
    bearing_offset_deg: float = 0.0

    def short_label(self) -> str:
        mode_label = "anchor" if self.mode == "anchor" else "honest"
        return (
            f"mode={mode_label} anchors={self.active_anchors}/{self.visible_anchors}"
            f" distx={self.distance_scale:.2f} brg={self.bearing_offset_deg:+.0f}°"
        )


def _wrap_degrees(deg: float) -> float:
    """Wrap angular offset into [-180, 180)."""

    wrapped = (deg + 180.0) % 360.0 - 180.0
    if wrapped == -180.0:
        return 180.0
    return wrapped


def _default_ref(dev: WifiDevice) -> float:
    if dev.kind == DeviceKind.HOTSPOT:
        return RSSI_AT_1M_AP
    return RSSI_AT_1M_CLIENT


def _distance_from_ref(dev: WifiDevice, ref: float) -> float:
    n = PATH_LOSS_EXPONENT_INDOOR
    if dev.freq_mhz > 4000:
        n += 0.3
    exponent = (ref - dev.rssi_dbm) / (10.0 * n)
    return float(min(120.0, max(0.1, 10.0**exponent)))


def _build_anchor_diagnostic(
    dev: WifiDevice,
    calibration: CalibrationBackend,
) -> AnchorDiagnostic | None:
    distance_target = calibration.distance_anchor_m(dev.mac)
    distance_scale: float | None = None
    current_distance: float | None = None
    if distance_target is not None:
        current_distance = dev.distance_m()
        raw_default_distance = _distance_from_ref(dev, _default_ref(dev))
        distance_scale = distance_target / max(raw_default_distance, 0.1)

    bearing_target = calibration.manual_bearing(dev.mac)
    bearing_offset: float | None = None
    if bearing_target is not None:
        bearing_offset = _wrap_degrees(bearing_target - dev.placeholder_bearing())

    consistent = True
    reason = ""
    if distance_target is not None and current_distance is not None:
        ratio = max(current_distance, 0.1) / max(distance_target, 0.1)
        if ratio > 1.8 or ratio < 0.55:
            consistent = False
            reason = "distance drift"
    if bearing_target is not None and dev.bearing_deg is not None and not dev.bearing_manual:
        disagreement = abs(_wrap_degrees(bearing_target - dev.bearing_deg))
        if disagreement > 90.0:
            consistent = False
            reason = reason or "bearing disagreement"
    return AnchorDiagnostic(
        mac=dev.mac,
        distance_target_m=distance_target,
        current_distance_m=current_distance,
        distance_scale=distance_scale,
        bearing_target_deg=bearing_target,
        bearing_offset_deg=bearing_offset,
        consistent=consistent,
        reason=reason,
    )


def device_to_native_dict(dev: WifiDevice) -> dict[str, Any]:
    """Serialize a :class:`WifiDevice` for ``wifi_radar_native`` scene APIs."""

    return {
        "mac": dev.mac,
        "kind": dev.kind.value,
        "ssid": dev.ssid,
        "channel": dev.channel,
        "freq_mhz": dev.freq_mhz,
        "rssi_dbm": dev.rssi_dbm,
        "calibrated_bearing_deg": dev.bearing_deg,
        "bearing_override_deg": dev.bearing_override_deg,
        "bearing_confidence": dev.bearing_confidence,
        "bearing_manual": dev.bearing_manual,
        "distance_override_m": dev.distance_override_m,
        "rssi_at_1m_override": dev.rssi_at_1m_override,
        "anchor_status": dev.anchor_status,
        "correction_note": dev.correction_note,
    }


def device_from_native_row(row: dict[str, Any], template: WifiDevice) -> WifiDevice:
    """Merge native scene-correction fields back onto a snapshot copy."""

    out = copy(template)
    if "calibrated_bearing_deg" in row:
        cal_b = row["calibrated_bearing_deg"]
        out.bearing_deg = None if cal_b is None else float(cal_b)
    if "bearing_override_deg" in row:
        ov = row["bearing_override_deg"]
        out.bearing_override_deg = None if ov is None else float(ov)
    if "distance_override_m" in row:
        dist = row["distance_override_m"]
        out.distance_override_m = None if dist is None else float(dist)
    if "rssi_at_1m_override" in row:
        ref = row["rssi_at_1m_override"]
        out.rssi_at_1m_override = None if ref is None else float(ref)
    if "bearing_manual" in row and row["bearing_manual"] is not None:
        out.bearing_manual = bool(row["bearing_manual"])
    if "bearing_confidence" in row and row["bearing_confidence"] is not None:
        out.bearing_confidence = float(row["bearing_confidence"])
    if "anchor_status" in row and row["anchor_status"] is not None:
        out.anchor_status = str(row["anchor_status"])
    if "correction_note" in row and row["correction_note"] is not None:
        out.correction_note = str(row["correction_note"])
    return out


def _summary_from_native(summary: dict[str, Any]) -> CorrectionSummary:
    mode_raw = str(summary.get("mode", "honest"))
    mode: CalibrationMode = "anchor" if mode_raw == "anchor" else "honest"
    return CorrectionSummary(
        mode=mode,
        visible_anchors=int(summary.get("visible_anchors", 0)),
        active_anchors=int(summary.get("active_anchors", 0)),
        distance_scale=float(summary.get("distance_scale", 1.0)),
        bearing_offset_deg=float(summary.get("bearing_offset_deg", 0.0)),
    )


def _apply_scene_corrections_native(
    devices: list[WifiDevice],
    calibration: NativeCalibrationStore,
) -> tuple[list[WifiDevice], CorrectionSummary]:
    assert native is not None
    rows_in = [device_to_native_dict(d) for d in devices]
    rows_out, summary_raw = native.apply_scene_corrections(rows_in, calibration.py_store)
    by_mac = {str(r["mac"]): r for r in rows_out}
    out: list[WifiDevice] = []
    for dev in devices:
        row = by_mac.get(dev.mac)
        out.append(device_from_native_row(row, dev) if row is not None else dev)
    return out, _summary_from_native(summary_raw)


def _apply_scene_corrections_python(
    devices: list[WifiDevice],
    calibration: CalibrationBackend,
) -> tuple[list[WifiDevice], CorrectionSummary]:
    mode = calibration.mode()
    diagnostics: list[AnchorDiagnostic] = []
    for dev in devices:
        if calibration.has_distance_calibration(dev.mac) or calibration.has_manual_bearing(dev.mac):
            diag = _build_anchor_diagnostic(dev, calibration)
            if diag is not None:
                diagnostics.append(diag)

    scales = [
        d.distance_scale
        for d in diagnostics
        if d.consistent and d.distance_scale is not None
    ]
    offsets = [
        d.bearing_offset_deg
        for d in diagnostics
        if d.consistent and d.bearing_offset_deg is not None
    ]
    distance_scale = median(scales) if scales else 1.0
    bearing_offset = median(offsets) if offsets else 0.0
    summary = CorrectionSummary(
        mode=mode,
        visible_anchors=len(diagnostics),
        active_anchors=len([d for d in diagnostics if d.consistent]),
        distance_scale=distance_scale,
        bearing_offset_deg=bearing_offset,
    )

    diag_by_mac = {d.mac: d for d in diagnostics}
    for dev in devices:
        diag = diag_by_mac.get(dev.mac)
        if diag is not None:
            if diag.consistent:
                dev.anchor_status = "anchor"
            else:
                dev.anchor_status = "stale-cal"
                if diag.reason:
                    dev.correction_note = diag.reason
            continue

        if mode == "honest":
            if scales:
                raw_distance = _distance_from_ref(dev, _default_ref(dev))
                dev.distance_override_m = raw_distance * distance_scale
                dev.correction_note = "scene distance"
        else:
            raw_distance = _distance_from_ref(dev, _default_ref(dev))
            dev.distance_override_m = raw_distance * distance_scale
            dev.bearing_override_deg = dev.placeholder_bearing() + bearing_offset
            dev.correction_note = "scene anchor"

    return devices, summary


def apply_scene_corrections(
    devices: list[WifiDevice],
    calibration: CalibrationBackend,
) -> tuple[list[WifiDevice], CorrectionSummary]:
    """Apply scene correction heuristics to snapshot copies.

    Prefers ``wifi_radar_native.apply_scene_corrections`` when the extension
    and a :class:`NativeCalibrationStore` are available; otherwise uses the
    pure-Python implementation.
    """

    if isinstance(calibration, NativeCalibrationStore) and native_scene_ready():
        return _apply_scene_corrections_native(devices, calibration)
    return _apply_scene_corrections_python(devices, calibration)
