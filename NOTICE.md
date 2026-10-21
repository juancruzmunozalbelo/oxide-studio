# NOTICE

## Oxide Studio (`oxide-studio`)

*Repository:* [https://github.com/juancruzmunozalbelo/oxide-studio](https://github.com/juancruzmunozalbelo/oxide-studio)  
*Showcase & Documentation:* [https://juancruzmunozalbelo.github.io/oxide-studio/](https://juancruzmunozalbelo.github.io/oxide-studio/)

Copyright (c) 2026 Juan Cruz Muñoz Albelo and Oxide Studio Contributors.

This product is dual-licensed under the MIT License and the Apache License, Version 2.0.
See `LICENSE-MIT` and `LICENSE-APACHE` for details.

---

### Clean-Room Engineering & Architectural Provenance

Oxide Studio is an independent, 100% clean-room native Rust digital audio workstation (DAW), audio DSP engine, and Model Context Protocol (MCP) server.

1. **Independent Implementation**:
   All source code across the entire workspace (`crates/*`) was designed and authored from mathematical, algorithmic, and digital signal processing (DSP) first principles. Oxide Studio contains **zero proprietary source code, decompiled C++ assemblies, SDKs, binary blobs, copyrighted sound libraries, or intellectual property from Ableton AG or Cycling '74**.

2. **Mathematical & Open Algorithmic Foundations**:
   The audio algorithms implemented within Oxide Studio derive exclusively from published academic papers, textbook DSP literature, and public domain engineering formulations:
   - **Zero-Delay Feedback (ZDF) State-Variable Filters**: Based on Andrew Simper's published linear trapezoidal integrators (*Cytomic Technical Papers*, 2011–2014).
   - **Transistor Ladder Filters**: Mystran's 2x-oversampled 4-pole zero-delay feedback ladder filter utilizing a rational $[4/4]$ Padé polynomial approximation of $\tanh(x)/x$ to ensure bounded self-oscillation without iterative solvers.
   - **Biquad Cascades & Equalization**: Robert Bristow-Johnson's *Audio EQ Cookbook* parametric biquad coefficients.
   - **Band-Limited Oscillators**: Polynomial band-limited step (PolyBLEP) anti-aliasing techniques.
   - **Audio Warping & Time-Stretching**: Classical Synchronized Overlap-Add (SOLA) granular synthesis, phase-vocoder analysis/synthesis with phase-locking, and cubic spline interpolation.
   - **Real-Time Thread Safety**: Lock-free single-producer single-consumer (SPSC) ring buffers and atomic floating-point memory barriers (`portable-atomic`, `rtrb`) ensuring zero heap allocations on the audio thread.

---

### Trademark Disclaimer & Nominative Fair Use

- **"Ableton"**, **"Ableton Live"**, **"Live Suite"**, **"Push"**, **"Operator"**, **"Simpler"**, **"Wavetable"**, and **"Max for Live"** are registered trademarks or service marks of **Ableton AG** in Germany, the United States, and other jurisdictions.
- **"RNBO"** and **"Max"** are registered trademarks of **Cycling '74 / Cycling '74 LLC**.
- **Oxide Studio is an independent open-source project and is NOT affiliated with, sponsored by, endorsed by, or associated in any way with Ableton AG or Cycling '74.**
- Any use of the terms "Ableton", "Ableton Live", or related product designations within this repository, documentation, or code comments is strictly for **nominative, comparative, interoperability, and architectural educational purposes** under applicable fair use principles to accurately describe workflow compatibility, format interchange, and engineering methodology.
