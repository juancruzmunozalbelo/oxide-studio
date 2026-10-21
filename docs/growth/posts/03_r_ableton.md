# r/ableton — I built a free, open-source (MIT) lightweight DAW in Rust that natively imports and exports Ableton Live 12 (.als) projects

I love Live 12, but when I'm traveling with just a laptop on battery or want to sketch a quick 909/FM/acid loop in half a second without waiting for my massive VST folder to scan, I always wished I had a tiny standalone scratchpad that didn't lock my ideas into an incompatible project file.

Since Ableton `.als` files are gzipped XML under the hood, I spent the last few months building **Oxide Studio**—a free, open-source (MIT license) electronic music workstation written in native Rust that reads and writes `.als` files directly.

* **GitHub (Source & Releases):** https://github.com/juancruzmunozalbelo/oxide-studio
* **Overview / Demo:** https://juancruzmunozalbelo.github.io/oxide-studio/

### What you can actually use it for alongside Ableton:

1. **Bidirectional `.als` workflow:** You can sketch a drum groove, bassline, and chord progression in Oxide Studio, hit export to `.als`, and open the exact session/arrangement structure, MIDI clips, and tempo straight in Ableton Live 12. You can also load an existing `.als` into Oxide to strip it down or rework MIDI ideas.
2. **Built-in Live-inspired DSP (zero external plugins needed):** I coded native Rust versions of the tools I use on every track so they use almost zero CPU: a 4-op FM synth (Operator-style), Wavetable synth, TR-909 Drum Rack, 303 bassline, multiband saturation (Roar-inspired), OTT, 8-band EQ, Auto Filter, and a Glue-style bus compressor.
3. **Instant 4-band Stem Separation:** It has a built-in splitter that separates loops/audio into `Kick+Sub`, `Bass`, `Synths/Mids`, and `Hats/Air` inside the project for quick resampling or remixing.
4. **Piano Roll & Generative MIDI tools:** Scale-fold locking, Euclidean rhythm generator, Chord Stacks, Strum/Humanize, plus an 8-macro XY Perform Pad with an Orbit LFO for hands-on modulation.
5. **Programmable / Scriptable (Optional):** If you like tinkering with code or local AI agents, the entire DAW exposes a local bridge (`127.0.0.1:19384`) and 70+ MCP tools, so you can script complex parameter modulations or generate MIDI variations programmatically while the audio is playing.

It's 100% free, no telemetry, no accounts, MIT licensed. Obviously it won't load your third-party VST3s or Max for Live patches when importing a heavy `.als` project, so think of it as a fast companion groovebox/sketchpad rather than a drop-in replacement for a 120-track mixing session.

Would love to hear how the `.als` import/export behaves with your own templates if anyone gives it a spin!
