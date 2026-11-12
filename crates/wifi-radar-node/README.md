# wifi-radar-node

Sensing **node agent**: one scan → one `NodeObservation` on stdout (NDJSON or
pretty JSON). Pair with `wifi-radar-fuse` for multi-node fusion.

Passive observation only — **not** survey-grade; **not** a product 3D claim.
TCP `--publish` lands in a follow-up.

```bash
# Offline / CI
cargo run -p wifi-radar-node -- --demo --node-id node-a --pose 0,0,1,0

# Live radio (nl80211, iw fallback)
cargo run -p wifi-radar-node -- -i wlan0 --node-id node-a --pose 0,0,1,0
```
