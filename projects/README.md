# Oxide Studio — Production Sessions (`oxide-mcp`)

This folder contains full multi-track electronic music production sessions (`.oxide.json` native format, Ableton Live 12 `.als` sets, 16-bit PCM `.wav` renders, and separated stems) built end-to-end via the 76-tool `oxide-mcp` JSON-RPC 2.0 server (`crates/oxide-mcp/tests/e2e_production_projects_mcp_test.rs`).

---

## 1. `peak_time_135_gmin` — Peak-Time Techno (`135.0 BPM`, `G Minor`)

- **Oxide Native Session:** `peak_time_135_gmin.oxide.json`
- **Ableton Live 12 Set:** `peak_time_135_gmin.als`
- **Drop Audio Bounce (8 bars at 135 BPM):** `peak_time_135_gmin_drop.wav`
- **Track Architecture:**
  - `Track 0`: `Lead Synth - G Minor Arp Hook` (`G2 -> D#3 -> G3 -> D#3 -> D3 -> C3`, `analogsaw.preset`)
  - `Track 1`: `909 Peak Techno Drums` (4x4 Kick, Clap on 2 & 4, 16th Closed Hats, Offbeat Open Hat)
  - `Track 2`: `Rolling Bass 16th G1` (24 rolling 16th-note `G1` pulse skipping downbeats, `wowbass.preset`)
  - `Track 3`: `Offbeat Sub Synth G0` (8th-note offbeat `G0` sub-punch, `aggro.preset`)
  - `Track 4`: `Syncopated Stab Hook G2` (syncopated `G2` stab hook, `metallic.preset`)
  - `Track 5`: `Sub Rumble Layer` (warped audio clip in `Beats` mode)
  - **Return Buses:** `A-Reverb`, `B-Delay`, `C-Echo` + Master Saturator, Tempo Delay, FDN Reverb, and Lookahead Brickwall Limiter (`-0.3 dBFS`).

---

## 2. `melodic_techno_126_fsharp_min` — Melodic Techno (`126.0 BPM`, `F# Minor`)

- **Oxide Native Session:** `melodic_techno_126_fsharp_min.oxide.json`
- **Ableton Live 12 Set:** `melodic_techno_126_fsharp_min.als`
- **Drop Audio Bounce (8 bars at 126 BPM):** `melodic_techno_126_fsharp_min_drop.wav`
- **Exported Stems (`melodic_techno_stems/`):**
  - **4-Stem Remix Separation:** `remix_drums.wav`, `remix_bass.wav`, `remix_vocals.wav`, `remix_other.wav`
  - **Individual Track Stems:** `track_01_lead_synth.wav`, `track_02_909_drum_rack.wav`, `track_03_rolling_bass_16th_f_m_d_a_e.wav`, `track_04_analog_poly_pads_f_m_dmaj7_a_e.wav`
- **Track Architecture:**
  - **Groove Pool:** `66%` 16th swing (`TR-909 Shuffle`) with 909 drums, 16th shaker/hat pulse, and syncopated mid-toms tuned to `F#` + `EQ Eight`, `Glue Compressor`, and `Roar`.
  - **Sound Design & Harmony:** `Melodic Lead F#m` (`analogsaw.preset`), `Rolling Bass 16th` following `F#1 -> D1 -> A1 -> E1` (`wowbass.preset`), and `Analog Poly Pads` playing `F#m -> Dmaj7 -> A -> E` (`darkpad.preset`) with automated filter cutoff ramp.
  - **6 Return Buses:** `A-Parallel Drum Bus`, `B-Room Ambience`, `C-Plate`, `D-Long Hall`, `E-Shimmer`, `F-Granular Echo`.
