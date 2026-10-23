# r/technoproduction — Built a free open-source 909/303/FM groovebox & DAW in Rust with Euclidean generators, Roar saturation, and native open .als export (MIT license)

For the last few months I've been coding **Oxide Studio**, a free, open-source (MIT/Apache-2.0) electronic music workstation in Rust tailored specifically for fast techno, acid, and club sketching.

* **GitHub & v1.0.0 Releases:** https://github.com/juancruzmunozalbelo/oxide-studio
* **Try the 909/Synth Web DAW in Browser:** https://juancruzmunozalbelo.github.io/oxide-studio/web/
* **Web / Demo:** https://juancruzmunozalbelo.github.io/oxide-studio/

Why I built it: I wanted a standalone binary that boots in half a second on my laptop, has the exact synth/drum/saturation tools needed for peak-time & hypnotic techno built straight into the engine, and exports directly to an **open `.als` project file** so no loop ever gets stuck outside my main DAW.

### Built Across 11 Modular Workspace Crates:
- `oxide-core`: Timing primitives, sample clock, scales, and lock-free SPSC command queues.
- `oxide-dsp`: Polyphonic synthesis (4-op FM, Wavetable, 303 acid bass, ZDF ladder filters).
- `oxide-drums`: TR-909 drum machine physical modeling, 16-pad drum rack, choke groups.
- `oxide-fx`: ADAA multiband saturator, 3-band OTT dynamics, 8-band EQ, Auto Filter, Glue bus compressor.
- `oxide-timeline`: Dual Session matrix and Arrangement timeline, automation curves, and take-lane comping.
- `oxide-audio`: Low-latency CoreAudio `cpal` streaming with zero heap allocations on the audio callback.
- `oxide-bounce`: Deterministic offline multi-track bounce and 4-band stem remix separation.
- `oxide-project`: Native project state and open `.als` Gzip XML project import and export.
- `oxide-plugin-scanner`: Out-of-process crash-isolated supervisor for AUv2, VST3, and CLAP plugins.
- `oxide-gui`: Single-window 60 FPS Metal/egui workstation UI with piano roll and 8-macro XY perform pad.
- `oxide-mcp`: 76-tool Model Context Protocol server and JSON-RPC bridge for scripting and local LLM workflows.

### Techno-Specific Features Built In:
* **TR-909 Drum Rack + Euclidean Rhythm Generator:** Instant polymetric 909 hats, rimshots, and tom grooves (e.g., 5/16 against 7/16) with per-step velocity, humanize, and choke groups.
* **4-Op FM + 303 Bassline + Wavetable:** Great for metallic FM percussive loops, industrial stabs, and resonant acid sequences with scale-fold locking in the Piano Roll.
* **Roar-Style Multiband Saturation + OTT + Glue Bus Compressor:** Built-in antiderivative antialiasing (ADAA) saturation to crush kicks and rumble tails without harsh digital foldback.
* **8-Macro XY Perform Pad with Orbit LFO:** Assign filter cutoffs, FM ratios, and distortion drive across the XY pad and let the Orbit LFO continuously morph your loop over 8 or 16 bars.
* **4-Band Stem Splitter (`Kick+Sub`, `Bass`, `Synths/Mids`, `Hats/Air`):** Drop any loop in and isolate the low-end rumble or tops immediately.
* **Open `.als` Project Interop:** Export your session as a native `.als` file and open it in your primary DAW with all your tracks, tempo, and MIDI clips ready to mix.

It's 100% free and open-source (no signups, no paywalls). Let me know what you think of the 909/FM/Roar engine or if there are specific techno workflow tools you'd want added next!
