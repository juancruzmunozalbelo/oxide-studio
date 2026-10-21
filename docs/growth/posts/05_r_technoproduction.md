# r/technoproduction — Built a free open-source 909/303/FM groovebox & DAW in Rust with Euclidean generators, Roar saturation, and native Ableton .als export (MIT license)

For the last few months I've been coding **Oxide Studio**, a free, open-source (MIT) electronic music workstation in Rust tailored specifically for fast techno, acid, and club sketching.

* **GitHub:** https://github.com/juancruzmunozalbelo/oxide-studio
* **Web / Demo:** https://juancruzmunozalbelo.github.io/oxide-studio/

Why I built it: I wanted a standalone binary that boots in half a second on my laptop, has the exact synth/drum/saturation tools needed for peak-time & hypnotic techno built straight into the engine, and exports directly to an **Ableton Live 12 `.als` file** so no loop ever gets stuck outside my main DAW.

### Techno-Specific Features Built In:
* **TR-909 Drum Rack + Euclidean Rhythm Generator:** Instant polymetric 909 hats, rimshots, and tom grooves (e.g., 5/16 against 7/16) with per-step velocity, humanize, and choke groups.
* **4-Op FM (Operator-style) + 303 Bassline + Wavetable:** Great for metallic FM percussive loops, industrial stabs, and resonant acid sequences with scale-fold locking in the Piano Roll.
* **Roar-Style Multiband Saturation + OTT + Glue Bus Compressor:** Built-in antiderivative antialiasing (ADAA) saturation to crush kicks and rumble tails without harsh digital foldback.
* **8-Macro XY Perform Pad with Orbit LFO:** Assign filter cutoffs, FM ratios, and distortion drive across the XY pad and let the Orbit LFO continuously morph your loop over 8 or 16 bars.
* **4-Band Stem Splitter (`Kick+Sub`, `Bass`, `Synths/Mids`, `Hats/Air`):** Drop any loop in and isolate the low-end rumble or tops immediately.
* **Bidirectional `.als` Bridge:** Export your session as a native `.als` file and open it in Ableton Live 12 with all your tracks, tempo, and MIDI clips ready to mix.

It's 100% free and open-source (no signups, no paywalls). Let me know what you think of the 909/FM/Roar engine or if there are specific techno workflow tools you'd want added next!
