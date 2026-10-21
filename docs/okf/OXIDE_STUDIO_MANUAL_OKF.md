# Oxide Studio Operational Knowledge Framework (OKF)
### Comprehensive Live 12 Architecture, DSP Engine, and MCP Automation Manual

---

## Executive Overview
The **Oxide Studio Operational Knowledge Framework (OKF)** serves as the definitive reference manual for electronic music producers and AI agent copilots operating within the Oxide Studio digital audio workstation. Designed around Ableton Live 12 muscle memory, zero-allocation lock-free DSP pipelines, and deep Model Context Protocol (MCP) tool integration, this framework standardizes operational vocabulary, parameter specifications, layout invariants, and automation surfaces across both English and Spanish (`EN` / `ES`).

---

## Table of Contents
1. [OKF-01: Workspace Architecture & Dual Views](#okf-01-workspace-architecture--dual-views)
2. [OKF-02: Session View, Clips, Scenes & Launch Quantization](#okf-02-session-view-clips-scenes--launch-quantization)
3. [OKF-03: Studio Mixer, Signal Flow, Returns & Crossfader](#okf-03-studio-mixer-signal-flow-returns--crossfader)
4. [OKF-04: Arrangement Timeline, Automation & Take Lane Comping](#okf-04-arrangement-timeline-automation--take-lane-comping)
5. [OKF-05: Clip View — MIDI Piano Roll, Generative Tools & Audio Warping](#okf-05-clip-view--midi-piano-roll-generative-tools--audio-warping)
6. [OKF-06: Device Chain View & 9 Native Instruments/FX](#okf-06-device-chain-view--9-native-instrumentsfx)
7. [OKF-07: TR-909 16-Step Sequencer & Groove Engine](#okf-07-tr-909-16-step-sequencer--groove-engine)
8. [OKF-08: Global Scale Awareness, Microtonal Tuning & Export](#okf-08-global-scale-awareness-microtonal-tuning--export)
9. [Appendix A: Complete Keyboard Shortcuts Matrix](#appendix-a-complete-keyboard-shortcuts-matrix)
10. [Appendix B: MCP Automation & Live RPC Bridge Reference](#appendix-b-mcp-automation--live-rpc-bridge-reference)

---

## OKF-01: Workspace Architecture & Dual Views

### 1. Architectural Philosophy
Oxide Studio implements Live 12's dual-view paradigm within a single-window GPU-accelerated interface rendered at 60 FPS via `egui` and `wgpu`. The interface eliminates nested modal dialogs and window clutter, organizing production into a primary workspace (Session Matrix vs Linear Arrangement) and a persistent, multi-purpose Detail Deck.

- **Session View (Non-Linear Clip Matrix)**: Matrix grid designed for real-time improvisation, sketch composition, loop auditioning, and live performance.
- **Arrangement View (Linear Timeline)**: Time-linear multi-track recording, locator navigation, automation editing, and take lane comping.
- **Detail Bottom Deck**: Unified lower workspace housing the MIDI/Audio Clip View, Device Chain, TR-909 16-Step Sequencer, and XY Perform Pad.
- **Collapsible Left Browser**: 3-tier tagging browser with instant search, sound similarity indexing, and category filtering.
- **Right AI Copilot Panel**: Context-aware producer assistant with real-time spectrum telemetry, parameter synthesis, and MCP routing.

### 2. View Switching & Splitter Dynamics
- **Workspace Toggle (`Tab`)**: Instantly toggles between `SessionMatrix` and `ArrangementTimeline`.
- **Detail View Toggle (`Shift+Tab` / `F12`)**: Switches the bottom deck between `ClipView` and `DeviceChain`.
- **Browser Toggle (`Cmd+Alt+B` / `Ctrl+Alt+B`)**: Collapses or reveals the sound browser library.
- **Draggable Horizontal Splitter**: A persistent 4.0px divider separating the primary workspace from the bottom deck. It supports responsive dynamic budgeting:
  - Upper workspace minimum height: `220.0px`
  - Bottom deck minimum height: `180.0px`
  - Default split ratio: `55% Upper / 45% Lower`
- **Anti-Breakage Layout Guard (`layout_guard`)**:
  - `FlexFitGrid`: Proportional auto-fit calculator ensuring that drum rows and clip grids dynamically scale without bottom clipping.
  - Four Geometric Laws:
    1. *Zero Viewport Overflow*: Shapes are strictly bounded to `screen_rect`.
    2. *Zero Text Cut-Off*: Text bounds are validated against clip rects.
    3. *Zero Text Collision*: Overlapping text elements in identical layers are forbidden.
    4. *Zero Tofu Glyphs*: All characters strictly adhere to verified UTF-8 font tables.

### 3. Relevant MCP Tools
- `transport_play`: Start real-time audio playback.
- `transport_stop`: Stop transport and reset playhead position.
- `browser_search`: Search devices, presets, samples, and clips by query or tag.

---

## OKF-02: Session View, Clips, Scenes & Launch Quantization

### 1. Contiguous Architectural Clip Matrix
The Session Matrix organizes musical material into vertical track columns and horizontal scene rows:
- **0px Item Spacing with 1px Divider Strokes**: Implements an airtight, clean studio look with `#111111` divider strokes.
- **Category Colored Track Headers**: Bold color-coded identifiers (`#6baace` Synth, `#e0aa2a` Drums, `#52ba46` Bass, `#954eb2` FX).
- **Uniform 22.0px Clip Slots**: Every slot maintains exact row alignment across all tracks, return channels, and master scenes.
- **Slot States**:
  - `Empty`: Dashed subtle border, ready for recording or MIDI clip generation.
  - `Stopped`: Displays clip title, length in bars, and play button icon.
  - `Triggered (Flashing Cyan/Green)`: Awaiting launch quantization downbeat.
  - `Playing (Solid Green Glow)`: Active playback with animated playback position indicator.
  - `Recording (Solid Red Glow)`: Real-time audio or MIDI recording.

### 2. Scene Firing & Global Stop Controls
- **Master Scene Launchers**: Located in the Master right column. Firing a scene triggers all clips along that row simultaneously according to launch quantization.
- **Synchronized Stop Clip Row (`[■]`)**: Dedicated stop button row positioned immediately below clip slots to halt playback per track without stopping global transport.
- **Stop All Clips (`[■ Stop All]`)**: Master control button halting all playing session clips simultaneously.

### 3. Launch Quantization Modes
Controls the rhythmic boundary at which triggered clips and scenes synchronize:
- `None`: Immediate execution without rhythmic alignment.
- `1/16`: Sixteenth-note sync (fast stutter/fill launches).
- `1/8`: Eighth-note sync.
- `1/4`: Quarter-note sync (beat-locked).
- `1/2`: Half-note sync.
- `1 Bar` (Default): Downbeat synchronization at the beginning of the next measure.
- `2 Bars` / `4 Bars`: Extended phrase synchronization for longer musical movements.

### 4. Relevant MCP Tools
- `clip_launch`: Fire a specific clip slot with quantize adherence.
- `clip_stop`: Stop clip playback on a given track.
- `scene_launch`: Trigger an entire horizontal scene.
- `clip_create_midi`: Instantiate a new MIDI clip pattern in target slot.
- `clip_create_drums`: Generate drum pattern clips directly into the session matrix.

---

## OKF-03: Studio Mixer, Signal Flow, Returns & Crossfader

### 1. Channel Strip Architecture & Vertical Faders
Each track channel strip features custom-painted Ableton Live 12 controls:
- **Vertical Volume Faders (76.0px Height)**:
  - Range: `-inf dB` to `+6.0 dBFS` (Unity `0.0 dB` located at 75% fader travel).
  - Taper: Logarithmic audio-taper response curve with digital readout badge.
- **Segmented Peak / RMS Stereo VU Meters**:
  - Dual vertical bar LED simulation (`L` / `R`).
  - Color Zones:
    - Normal (`-60 dB` to `-12 dB`): `#00f758` Studio Green
    - Caution (`-12 dB` to `-0.3 dB`): `#ffd100` Electric Yellow
    - True Peak / Clip (`>-0.3 dB`): `#ff0a0a` Saturated Red with peak-hold tick.
- **Bipolar Split-Bar Pan Control (18.0px Height)**:
  - Center detent (`C`), displaying degrees left (`1L` to `50L`) or right (`1R` to `50R`).
  - Visualized as a cyan `#03c3d5` bar expanding bilaterally from center.
- **Rotary Arc Sends (`A Reverb`, `B Delay`)**:
  - Dual concentric rotary controls with logarithmic gain curves feeding Aux return buses.

### 2. Track Routing & Activators
- **Track Activators (`1`, `2`, `3`)**: Amber `#ffad56` backlit buttons. When clicked or toggled via `0`, mutes track audio processing (inactive: `#363636`).
- **Solo (`S`)**: Deep Blue `#3c6ab6` latching button. Solos the target channel in the monitor bus.
- **Record Arm (`●`)**: High-visibility Crimson `#ff5559` button. Arms track for incoming MIDI or audio input.
- **Crossfader Assignment (`A | Off | B`)**: Assigns track to Crossfader Bus A, Bus B, or bypass (Off).

### 3. Master Bus & Brickwall Limiter
- **Master Fader**: Primary gain stage controlling studio output level.
- **Integrated Master Brickwall Limiter**: Hard ceiling clamped to `-0.3 dBFS` with lookahead true-peak limiting, ensuring inter-sample peaks never exceed full scale during live synthesis or multi-track bounce.

### 4. Relevant MCP Tools
- `mixer_set_channel`: Configure track volume (`volume_db` / `linear`), pan, mute, and solo.
- `mixer_set_send`: Adjust aux send levels A & B per track.
- `return_track_create`: Spawn new auxiliary return tracks with custom FX chains.
- `fx_set_limiter`: Adjust ceiling, release, and lookahead parameters on the master limiter.

---

## OKF-04: Arrangement Timeline, Automation & Take Lane Comping

### 1. Dual-Lane Ruler & Navigation
- **Top Ruler (Locators & Markers)**: Houses user song markers (`Intro`, `Build`, `Drop`, `Outro`), loop brackets, and punch-in/out flags.
- **Bottom Ruler (Bars & Beats)**: Precise measure numbering (`1.1.1`, `5.1.1`, `9.1.1`) with adaptive fractional subdivisions (`1/4`, `1/8`, `1/16`, `1/32`) responding to horizontal zoom (`Cmd + Scroll`).

### 2. Multi-Track Automation Lanes & Bezier Curves
- **Per-Parameter Automation Envelopes**: Volume, Pan, Cutoff, Resonance, and Device Macros rendered directly overlaid on clips or in dedicated automation sub-lanes.
- **Cubic Bezier Curve Interpolation**: Hold `Alt` and drag breakpoint lines to create smooth non-linear logarithmic, exponential, or S-curve transitions.
- **Draw Mode (`Cmd+B`)**: Enables freehand pencil drawing and stepped grid automation.

### 3. Take Lane Comping & Audio Crossfading
- **Take Lanes**: Record multiple continuous audio or MIDI takes in looped playback mode.
- **Comping Blade Tool**: Highlight best phrases across takes; selected regions are instantly comped into the main master track lane with automatic 4ms micro-crossfades preventing clicks.
- **Split at Playhead (`Cmd+E`)**: Splits active clips cleanly across selected tracks.
- **Consolidate Selection (`Cmd+J`)**: Bounces selected arrangement cuts into a single unified audio or MIDI clip.

### 4. Back-to-Arrangement Control
- When session clips are launched during arrangement playback, affected tracks switch to Session Override mode.
- An amber **Back-to-Arrangement** button (`◀ | ■`) illuminates in the transport bar. Clicking it returns playback synchronization immediately to timeline arrangement data.

### 5. Relevant MCP Tools
- `timeline_place_clip`: Place clips at exact musical beat offsets in arrangement tracks.
- `timeline_add_automation`: Inject automation envelopes with Bezier control points.
- `take_lane_create`: Instantiate take lanes for comping sessions.
- `take_lane_comp_region`: Splice chosen take lane sub-regions into the master timeline.

---

## OKF-05: Clip View — MIDI Piano Roll, Generative Tools & Audio Warping

### 1. Clip Inspector Tabs
The Clip View bottom deck contains 4 specialized sub-tabs:
1. **Notes Tab**: High-resolution MIDI piano roll editor with velocity and probability stalks.
2. **Envelopes Tab**: Clip-level modulation envelopes for pitch bend, mod wheel, and MPE.
3. **MPE Tab**: Per-note polyphonic expression (Pressure, Slide, Pitch Bend).
4. **Sample Tab**: Audio clip sample inspector with warp modes, loop markers, and gain.

### 2. MIDI Piano Roll Controls
- **Scale-Fold**: When enabled, hides non-scale keys, leaving only root, mode, and chord tones visible.
- **Octave Navigation (`+Oct / -Oct` or `Z / X`)**: Shifts view octave range upward or downward.
- **Velocity (`VEL`) & Chance (`CHANCE`) Stalks**:
  - Velocity range: `1..127` with color-intensity shading.
  - Probability range: `0%..100%` triggering stochastic note events for generative rhythms.
- **Live 12 Transform Tools**:
  - `Reverse`: Flips note sequence horizontally along the time axis.
  - `Invert`: Mirrors pitch intervals vertically around the center pitch.
  - `Legato`: Extends note durations to touch subsequent note start times.
  - `Humanize`: Introduces subtle micro-timing variations (±15 ticks) and dynamic velocity variations.
- **Live 12 Generative Tools**:
  - `Arpeggiate`: Converts chord blocks into rhythmic arpeggios (Up, Down, Up-Down, Random).
  - `Connect`: Fills rhythmic gaps between sparse notes with passing tones.
  - `Chord`: Adds intervals (3rd, 5th, 7th, 9th) conforming strictly to the active scale.
  - `Euclidean`: Distributes `K` pulses evenly across `N` step grids for afro-latin, polyrhythmic, and techno rhythms.

### 3. Audio Warping Engine & Modes
Oxide Studio synchronizes audio clips to arbitrary DAW project tempos via time-stretching:
- **Warp Markers**: Transient markers pinned to musical beats.
- **Warp Modes**:
  - `Beats`: Optimized for rhythmic percussion; uses transient preservation with selectable loop/stop/back-and-forth degradation.
  - `Tones`: Granular time-stretching optimized for monophonic sources (vocals, basslines).
  - `Texture`: Granular time-stretching with adjustable grain size for pads, polyphonic textures, and ambient drones.
  - `Re-Pitch`: Classic turntable tape speed pitch-shifting; speed varies with tempo.
  - `Complex`: Multi-band polyphonic time-stretching for full mixes and dense instrumental stems.
  - `Complex Pro`: High-resolution formant-preserving polyphonic time-stretching.

### 4. Relevant MCP Tools
- `midi_transform_clip`: Execute Reverse, Invert, Legato, or Humanize on clip notes.
- `midi_generate_clip`: Generate chord progressions, arpeggios, or Euclidean patterns.
- `audio_clip_create_warped`: Load sample as a tempo-synced audio clip.
- `audio_clip_set_warp_mode`: Set warping algorithm (`Beats`, `Complex Pro`, etc.).
- `audio_clip_add_warp_marker`: Pin sample transients to timeline beat positions.

---

## OKF-06: Device Chain View & 9 Native Instruments/FX

### 1. Device Chain Architecture
Located in the Bottom Deck (`Shift+Tab`), track device chains support serial and parallel audio processing. Devices can be reordered via drag-and-drop, automated via Macro controls, and inserted via the `+ Add Device` 3x3 picker grid.

### 2. Comprehensive 9-Device Suite Reference
1. **`Wavetable` (Flagship Dual-Oscillator Synth)**:
   - Features: Dual morphing wavetable oscillators, sub-oscillator, analog-modeled filter pair, 2 LFOs, 3 envelopes, and a flexible modulation matrix.
   - Parameters: `wavetable_pos` (0.0..1.0), `filter_cutoff` (20..20000 Hz), `unison_voices` (1..8).
2. **`Operator` (4-Operator FM Synthesizer)**:
   - Features: 4 FM operators (A, B, C, D) with sine, square, saw waveforms, 11 routing algorithms, per-operator loopable envelopes, and pitch envelope.
   - Parameters: `coarse_ratio` (0.5..32.0), `fine_tune`, `fm_level` (0.0..1.0).
3. **`Roar` (Multi-Stage Saturation & Waveshaping)**:
   - Features: 3 saturation stages with Serial, Parallel, Multi-band, and Mid/Side routing, feedback loop with frequency tuning, and built-in compressor.
   - Parameters: `drive_db` (0..36 dB), `feedback_amt`, `tone`, `bias`.
4. **`EQ Eight` (8-Band Parametric Equalizer)**:
   - Features: 8 fully configurable filter bands (Bell, Low Cut, High Cut, Low Shelf, High Shelf, Notch) with zero-delay feedback (ZDF) filters and real-time spectrum overlay.
   - Parameters: `band_freq` (10..22000 Hz), `band_gain` (-15..+15 dB), `band_q` (0.1..18.0).
5. **`Glue Compressor` (Analog Console SSL G-Bus Emulation)**:
   - Features: VCA bus compressor modeled after the legendary British console center section, featuring an analog gain-reduction needle meter and dry/wet blend.
   - Parameters: `threshold_db` (-40..0 dB), `ratio` (2:1, 4:1, 10:1), `attack_ms` (0.1..30 ms), `release_s` (0.1..1.2s or Auto), `makeup_db`.
6. **`Auto Filter` (Analog-Modeled ZDF Filter)**:
   - Features: Lowpass, Highpass, Bandpass, and Morph filter models with 12/24 dB slopes and analog circuit modeling (Clean, OSR, MS2, SMP, PRD) with rate-synced LFO.
   - Parameters: `cutoff_hz` (20..20000 Hz), `resonance` (0.0..5.0), `lfo_amount`, `circuit_type`.
7. **`Multiband Dynamics (OTT)` (3-Band Upward/Downward Dynamics)**:
   - Features: Independent Low, Mid, and High band compression and expansion with signature OTT aggressive electronic sparkle and sub-bass control.
   - Parameters: `above_threshold`, `below_threshold`, `crossover_low` (default 120 Hz), `crossover_high` (default 2500 Hz), `dry_wet`.
8. **`Simpler` (Sample Instrument & Slicer)**:
   - Features: Sample playback instrument supporting Classic mode (polyphonic ADSR), 1-Shot mode (monophonic transient triggers), and Slicing mode (beat/transient slice distribution).
   - Parameters: `playback_mode`, `slice_sensitivity`, `start_marker`, `fade_in`, `fade_out`.
9. **`Drum Rack` (128-Pad 8-Bank Sampler Matrix)**:
   - Features: 8 banks of 16 velocity-sensitive pads (Pad 1 to Pad 128) with per-pad simpler instances, choke groups, send return buses, and macro mapping.
   - Parameters: `pad_note`, `pad_volume`, `choke_group` (1..16).

### 3. Relevant MCP Tools
- `track_create_synth`: Instantiate synthesizer track (Wavetable or Operator).
- `device_add_to_track`: Insert native device into target track chain.
- `device_set_param`: Modulate specific device parameter value.
- `device_get_chain`: Retrieve list of active devices and parameter values.
- `simpler_slice_to_drum_rack`: Slice a Simpler loop into a multi-pad Drum Rack.

---

## OKF-07: TR-909 16-Step Sequencer & Groove Engine

### 1. 16-Step Voice Matrix & Dual Banks
The TR-909 Sequencer delivers an authentic analog step programming interface divided into two 8-voice banks:
- **Bank A (Acoustic / Core Drums)**:
  - Voice 1: Kick Drum (Bassdrum 909)
  - Voice 2: Snare Drum
  - Voice 3: Low Tom
  - Voice 4: Mid Tom
  - Voice 5: Hi Tom
  - Voice 6: Rimshot
  - Voice 7: Hand Clap
  - Voice 8: Cowbell
- **Bank B (Cymbals & Latin Percussion)**:
  - Voice 9: Closed Hi-Hat
  - Voice 10: Open Hi-Hat
  - Voice 11: Crash Cymbal
  - Voice 12: Ride Cymbal
  - Voice 13: Percussion 1 (Low Conga)
  - Voice 14: Percussion 2 (High Bongo)
  - Voice 15: Shaker
  - Voice 16: Tambourine

### 2. Step Modifiers & Velocity Shading
- **Active Step Toggle**: Click any of the 16 step pills to toggle note trigger.
- **Accent Polygon**: Toggling accent elevates velocity to `127` and brightens pad color to saturated amber `#ffbe00`.
- **Velocity Shading**: Step background brightness reflects dynamic velocity (`1..127`).
- **Probability / Micro-Timing**: Allows probabilistic ghost hits and delayed flams.

### 3. Groove Engine, Swing & Built-in Patterns
- **Swing %**: Shifts even-numbered 16th notes backward in time (50% = Straight, 66% = Triplet Swing, 75% = Hard Shuffle).
- **Humanize %**: Randomizes micro-timing (±12ms) and velocity (±8 units).
- **One-Click Quick Patterns**:
  - `Four-on-the-Floor`: Classic Techno/House kick on steps `0, 4, 8, 12` with claps on `4, 12`.
  - `RKT / Dembow / Tresillo`: Syncopated Latin urban groove with kick on `0, 4, 8, 12` and snares on `3, 6, 11, 14`.
  - `Acid 16ths`: Driving continuous sixteenth-note closed hi-hats with accents on offbeat upbeats (`2, 6, 10, 14`).

### 4. Relevant MCP Tools
- `drums_configure_pad`: Configure synthesis or sample parameters on a drum pad.
- `drums_trigger_pad`: Trigger a drum pad note audition with velocity.
- `generate_drum_groove_clip`: Generate 16-step patterns into clip slots.
- `transport_set_swing`: Adjust global swing percentage.

---

## OKF-08: Global Scale Awareness, Microtonal Tuning & Export

### 1. Global Musical Scale & Tuning Systems
Live 12 introduces global harmonic coherence across instruments, MIDI editors, and visualizers:
- **Global Root Note**: Selectable across 12 pitch classes (`C`, `C#`, `D`, `D#`, `E`, `F`, `F#`, `G`, `G#`, `A`, `A#`, `B`).
- **Musical Scale Modes**: Major, Minor, Dorian, Phrygian, Lydian, Mixolydian, Locrian, Harmonic Minor, Melodic Minor, Pentatonic, Blues, Diminished, Whole Tone.
- **Scale Lock**: Locks piano rolls, isomorphic keyboard, and generative tools to in-scale pitches only.
- **Microtonal & Alternative Tunings**:
  - `12-TET`: Standard 12-tone equal temperament.
  - `19-EDO`: 19-tone equal temperament (better major 3rds).
  - `24-EDO`: Quarter-tone system for Arabic and contemporary experimental music.
  - `31-EDO`: 31-tone equal temperament (near-perfect meantone).
  - `Bohlen-Pierce`: Non-octave tritave-based 13-division scale.
  - `Just Intonation`: Pure harmonic ratios based on integer overtone fractions.
  - `Scala (.scl)`: Import custom microtonal tuning definition files.

### 2. Export & Interoperability
- **1-Click WAV Bounce (`Cmd+B` / `app.export_demo_wav`)**: Renders high-fidelity 44.1/48 kHz 24-bit stereo WAV with 32-bit float internal headroom.
- **Ableton Live Project (`.als`) Import & Export**:
  - Import `.als` files: Parses track hierarchies, MIDI clips, locator data, and mixer levels.
  - Export `.als` files: Serializes current Oxide Studio matrix and timeline to native gzipped XML Ableton Live 12 project files.
- **4-Stem Remix Separation (`Cmd+Shift+R`)**:
  - Exports discrete isolated stem tracks: `Drums`, `Bass`, `Other (Synths/Keys)`, and `Vocals`.
  - Automatic silence trimming and normalization.

### 3. Relevant MCP Tools
- `scale_set_global`: Set project musical root note, scale mode, and scale lock.
- `bounce_wav` / `render_bounce_wav`: Trigger master project audio bounce to WAV.
- `project_import_als`: Import external Ableton Live `.als` project.
- `project_export_als`: Save current project state as `.als` file.
- `stems_separate_remix` / `bounce_export_stems`: Render isolated stem tracks for remixing.

---

## Appendix A: Complete Keyboard Shortcuts Matrix

| Shortcut | Action | Description |
|:---|:---|:---|
| `Tab` | Switch Workspace | Toggles between Session View and Arrangement Timeline |
| `Shift+Tab` / `F12` | Toggle Detail View | Switches bottom deck between Clip View and Device Chain |
| `Space` | Transport Play/Stop | Toggles global playback from current position |
| `Cmd+T` | Insert Audio Track | Adds a new audio track with stereo input routing |
| `Cmd+Shift+T` | Insert MIDI Track | Adds a new MIDI track with default synth chain |
| `Cmd+Alt+T` | Insert Return Track | Adds a new Aux Return bus (A Reverb, B Delay) |
| `Cmd+Shift+M` | New MIDI Clip | Inserts a 1-bar blank MIDI clip in selected slot |
| `Cmd+D` | Duplicate | Duplicates selected track, clip, or device |
| `Cmd+E` | Split Clip | Splits clip at playhead position in arrangement |
| `Cmd+J` | Consolidate | Bounces and consolidates selected arrangement regions |
| `Cmd+B` | Bounce WAV / Draw Mode | Quick bounce project to WAV or toggle pencil draw |
| `Cmd+Alt+B` | Toggle Browser | Toggles left browser library visibility |
| `Cmd+Shift+R` | Stem Export | Opens dialog or triggers 4-stem remix separation |
| `0` | Toggle Track Mute | Mutes or unmutes the active channel strip |
| `S` | Toggle Track Solo | Solos or unsolos the active channel strip |
| `Z` / `X` | Octave Down / Up | Shifts computer keyboard input down/up one octave |
| `C` / `V` | Velocity Down / Up | Decrements/increments input velocity by 10 units |
| `?` / `F1` | Shortcut Guide Modal | Displays in-app shortcut reference overlay |

---

## Appendix B: MCP Automation & Live RPC Bridge Reference

Oxide Studio runs an embedded non-blocking JSON-RPC server listening on `127.0.0.1:9877`. External agents, CLI commands, and Model Context Protocol servers can interact with the live studio session in real time.

### Available RPC Methods:
- `live12_get_song_info`: Returns tempo, scale, playhead beat, and track census.
- `live12_set_tempo`: Mutates BPM with lock-free audio engine notification.
- `live12_set_scale`: Updates global scale root, mode, and scale lock.
- `live12_create_midi_track`: Appends a new MIDI track with channel strip.
- `live12_create_audio_track`: Appends a new Audio track.
- `live12_create_return_track`: Appends Aux Return track.
- `live12_create_clip`: Spawns empty MIDI clip on track/slot.
- `live12_add_notes`: Appends discrete `MidiNote` array into targeted clip.
- `live12_fire_clip`: Launches clip slot with immediate/quantized trigger.
- `live12_fire_scene`: Launches all clips in scene row.
- `live12_load_browser_item`: Loads instrument/FX device onto track.
- `live12_set_device_param`: Sets parameter index value on device.
- `live12_get_track_devices`: Enumerates devices and parameters for a track.
