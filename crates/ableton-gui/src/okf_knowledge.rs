//! Oxide Studio Operational Knowledge Framework (OKF) Module.
//!
//! Provides the core knowledge base and query engine mapping Ableton Live 12
//! concepts, keyboard shortcuts, DSP parameters, and Model Context Protocol (MCP)
//! tools to Oxide Studio's native architecture.

use crate::panels::ai_chat::StudioLocale;

/// Discrete section within the Oxide Studio Operational Knowledge Framework.
#[derive(Debug, Clone, Copy)]
pub struct OkfSection {
    pub id: &'static str,
    pub title_en: &'static str,
    pub title_es: &'static str,
    pub keywords: &'static [&'static str],
    pub guide_en: &'static str,
    pub guide_es: &'static str,
    pub mcp_tools: &'static [&'static str],
}

/// The 8 canonical operational domains of Oxide Studio.
pub const OKF_MANUAL_SECTIONS: &[OkfSection] = &[
    // OKF-01: Workspace Architecture & Dual Views
    OkfSection {
        id: "OKF-01",
        title_en: "Workspace Architecture & Dual Views",
        title_es: "Arquitectura del Espacio de Trabajo y Vistas Duales",
        keywords: &[
            "workspace", "view", "views", "dual view", "tab", "session view", "arrangement view",
            "detail view", "bottom deck", "splitter", "horizontal splitter", "f12", "cmd+alt+b",
            "browser", "layout guard", "flexfitgrid", "anti-breakage", "interfaz", "vistas",
            "atajos", "shortcuts", "navegacion", "navegación", "pantalla", "layout",
        ],
        guide_en: "Oxide Studio implements Live 12's single-window GPU studio architecture (60 FPS via egui/wgpu):\n\
            • Workspace Views: Toggle between Session Matrix and Linear Arrangement Timeline with [Tab].\n\
            • Detail Deck: Toggle lower dock between Clip View (Piano Roll/Sample) and Device Chain with [Shift+Tab] or [F12].\n\
            • Sound Browser: Toggle left library panel with [Cmd+Alt+B] ([Ctrl+Alt+B]).\n\
            • Draggable Horizontal Splitter: 4px divider separating top workspace from bottom deck with minimum height budgets (220px upper / 180px lower).\n\
            • Anti-Breakage Layout Guard: FlexFitGrid calculator enforces 4 geometric laws preventing text clipping, viewport overflow, collisions, or tofu glyphs.",
        guide_es: "Oxide Studio implementa la arquitectura de estudio monoventana acelerada por GPU de Live 12 (60 FPS vía egui/wgpu):\n\
            • Vistas de Espacio: Alterná entre la Matriz de Session y la Línea de Tiempo de Arrangement con [Tab].\n\
            • Detail Deck: Alterná el deck inferior entre Vista de Clip (Piano Roll/Muestra) y Cadena de Dispositivos con [Shift+Tab] o [F12].\n\
            • Explorador de Sonidos: Ocultá o mostrá el panel izquierdo de librería con [Cmd+Alt+B] ([Ctrl+Alt+B]).\n\
            • Splitter Horizontal Arrastrable: Divisor de 4px que separa el espacio superior del deck inferior con presupuestos mínimos (220px superior / 180px inferior).\n\
            • Guardia Anti-Ruptura (Layout Guard): El calculador FlexFitGrid aplica 4 leyes geométricas evitando recortes de texto, desbordamiento, colisiones o glifos tofu.",
        mcp_tools: &["transport_play", "transport_stop", "browser_search"],
    },

    // OKF-02: Session View, Clips, Scenes & Launch Quantization
    OkfSection {
        id: "OKF-02",
        title_en: "Session View, Clips, Scenes & Launch Quantization",
        title_es: "Vista Session, Clips, Escenas y Cuantización de Disparo",
        keywords: &[
            "session", "clip", "clips", "scene", "scenes", "escena", "escenas", "slot", "slots",
            "launch", "quantization", "cuantizacion", "cuantización", "stop clip", "stop all",
            "disparo", "lanzamiento", "matriz", "matrix", "1 bar", "1/4", "compas", "compás",
        ],
        guide_en: "Session Matrix architecture & non-linear clip launching:\n\
            • Contiguous Matrix: 0px item spacing with 1px #111111 divider strokes and 22px row-aligned slots.\n\
            • Clip States: Empty (dashed border), Stopped (play icon), Triggered (flashing green/cyan), Playing (solid green glow), Recording (solid red).\n\
            • Scene Firing: Master scene launch buttons fire all row clips simultaneously.\n\
            • Stop Controls: Synchronized [■] stop button row below clip slots; master [■ Stop All] stops all playing session loops.\n\
            • Launch Quantization: Selectable in transport [None, 1/16, 1/8, 1/4, 1/2, 1 Bar, 2 Bars, 4 Bars] ensuring seamless downbeat musical sync.",
        guide_es: "Arquitectura de Matriz Session y lanzamiento no-lineal de clips:\n\
            • Matriz Contigua: Espaciado de 0px con líneas divisorias de 1px #111111 y slots alineados a 22px de altura.\n\
            • Estados de Clip: Vacío (borde punteado), Detenido (icono play), Disparado (parpadeo cian/verde), Reproduciendo (brillo verde), Grabando (rojo sólido).\n\
            • Disparo de Escenas: Botones de escena en columna Master disparan todos los clips de la fila al unísono.\n\
            • Controles de Parada: Fila de botones de parada [■] por pista bajo los slots; botón maestro [■ Stop All] detiene todos los bucles en reproducción.\n\
            • Cuantización de Disparo: Ajustable en barra de transporte [None, 1/16, 1/8, 1/4, 1/2, 1 Bar, 2 Bars, 4 Bars] garantizando sincronía exacta.",
        mcp_tools: &[
            "clip_launch",
            "clip_stop",
            "scene_launch",
            "clip_create_midi",
            "clip_create_drums",
        ],
    },

    // OKF-03: Studio Mixer, Signal Flow, Returns & Crossfader
    OkfSection {
        id: "OKF-03",
        title_en: "Studio Mixer, Signal Flow, Returns & Crossfader",
        title_es: "Mezclador de Estudio, Flujo de Señal, Retornos y Crossfader",
        keywords: &[
            "mixer", "mezclador", "fader", "faders", "volume", "volumen", "gain", "ganancia",
            "pan", "panorama", "bipolar pan", "vu meter", "vumetro", "vúmetro", "peak", "rms",
            "sends", "envios", "envíos", "return", "retorno", "reverb", "delay", "crossfader",
            "activator", "mute", "silenciar", "solo", "arm", "limiter", "limitador", "brickwall",
        ],
        guide_en: "Custom-painted Live 12 vertical mixer channel strip & signal flow:\n\
            • Vertical Volume Fader: 76px slot, -inf to +6 dBFS with unity 0 dB tick at 75% fader travel and digital dB pill.\n\
            • Segmented VU Meters: Stereo L/R Peak/RMS meters with Green (-60..-12 dB), Yellow (-12..-0.3 dB), and Red clip hold (> -0.3 dB).\n\
            • Bipolar Split-Bar Pan: 18px bar displaying Center (C) or degrees L/R with cyan #03c3d5 expansion.\n\
            • Rotary Aux Sends: Rotary arc controls for Aux Send A (Reverb) and Aux Send B (Echo/Delay).\n\
            • Channel Activators: Amber backlit buttons [1, 2, 3] (press [0] to mute); Solo [S] in deep blue; Record Arm [●] in crimson.\n\
            • Crossfader: Assign tracks to A, Off, or B with smooth DJ curve mixing.\n\
            • Master Brickwall Limiter: Hard ceiling at -0.3 dBFS lookahead limiting preventing inter-sample digital clipping.",
        guide_es: "Tira de canal de mezclador vertical personalizada de Live 12 y flujo de señal:\n\
            • Fader de Volumen Vertical: Recorrido de 76px, -inf a +6 dBFS con marca de 0 dB al 75% y lectura digital en píldora.\n\
            • Vúmetros Segmentados: Medidores estéreo L/R Peak/RMS con zonas Verde (-60..-12 dB), Amarillo (-12..-0.3 dB) y Rojo (> -0.3 dB).\n\
            • Panorama Bipolar: Barra de 18px indicando Centro (C) o grados L/R mediante expansión de barra cian #03c3d5.\n\
            • Envíos Auxiliares Rotativos: Controles de arco rotativo para Retorno A (Reverb) y Retorno B (Echo/Delay).\n\
            • Activadores de Pista: Botones ámbar [1, 2, 3] (tecla [0] para silenciar); Solo [S] en azul profundo; Armar Grabación [●] en rojo carmesí.\n\
            • Crossfader: Asignación de pista a bus A, Off o B con transición suave estilo DJ.\n\
            • Limitador Brickwall Master: Techo fijo a -0.3 dBFS con anticipación (lookahead) para erradicar cualquier recorte digital.",
        mcp_tools: &[
            "mixer_set_channel",
            "mixer_set_send",
            "return_track_create",
            "fx_set_limiter",
        ],
    },

    // OKF-04: Arrangement Timeline, Automation & Take Lane Comping
    OkfSection {
        id: "OKF-04",
        title_en: "Arrangement Timeline, Automation & Take Lane Comping",
        title_es: "Línea de Tiempo de Arrangement, Automatización y Comping de Tomas",
        keywords: &[
            "arrangement", "timeline", "linea de tiempo", "línea de tiempo", "automation",
            "automatizacion", "automatización", "bezier", "curvas", "comping", "take lane",
            "take lanes", "tomas", "ruler", "regla", "locators", "marcadores", "back to arrangement",
            "split", "consolidate", "cmd+e", "cmd+j",
        ],
        guide_en: "Arrangement timeline, automation editing, and take comping:\n\
            • Dual-Lane Ruler: Top lane for section locators (Intro, Build, Drop, Outro); bottom lane for bar/beat measures with zoom-adaptive subdivisions.\n\
            • Multi-Track Automation: Breakpoint lanes supporting cubic Bezier curves (Alt+drag breakpoint) for volume, pan, and filter sweeps.\n\
            • Draw Mode [Cmd+B]: Draw stepped or freehand parameter envelopes directly on the timeline.\n\
            • Take Lane Comping: Record multiple looped takes and splice best regions with 4ms click-free micro-crossfades into the master lane.\n\
            • Editing Shortcuts: Split clip at playhead [Cmd+E]; Consolidate selection into new clip [Cmd+J].\n\
            • Back-to-Arrangement [◀|■]: Amber transport button resumes timeline playback when session clips override tracks.",
        guide_es: "Línea de tiempo de Arrangement, edición de automatización y comping de tomas:\n\
            • Regla de Dos Carriles: Carril superior para marcadores (Intro, Build, Drop, Outro); carril inferior para compases y tiempos con zoom adaptable.\n\
            • Automatización Multipista: Carriles de puntos con curvas Bezier cúbicas (Alt+arrastrar) para barridos de volumen, panorama y filtros.\n\
            • Modo Dibujo [Cmd+B]: Dibujá envolventes a mano alzada o en rejilla directamente sobre la pista.\n\
            • Comping de Tomas (Take Lanes): Grabá múltiples tomas en bucle y ensamblá las mejores frases con micro-crossfades de 4ms sin clics.\n\
            • Atajos de Edición: Dividir clip en el cabezal con [Cmd+E]; Consolidar selección en un nuevo clip con [Cmd+J].\n\
            • Volver a Arrangement [◀|■]: Botón ámbar en la barra de transporte para devolver la pista al timeline tras disparar clips en Session.",
        mcp_tools: &[
            "timeline_place_clip",
            "timeline_add_automation",
            "take_lane_create",
            "take_lane_comp_region",
        ],
    },

    // OKF-05: Clip View — MIDI Piano Roll, Generative Tools & Audio Warping
    OkfSection {
        id: "OKF-05",
        title_en: "Clip View — MIDI Piano Roll, Generative Tools & Audio Warping",
        title_es: "Vista de Clip — Piano Roll MIDI, Herramientas Generativas y Warping",
        keywords: &[
            "clip view", "piano roll", "midi", "notes", "notas", "scale fold", "octave",
            "velocity", "velocidad", "chance", "probabilidad", "reverse", "invert", "legato",
            "humanize", "arpeggiate", "connect", "chord", "euclidean", "warp", "warping",
            "beats", "tones", "texture", "re-pitch", "complex", "complex pro", "mpe",
            "envelope", "envelopes", "envolventes", "modulation", "modulacion", "pitch bend", "mod wheel",
        ],
        guide_en: "Clip View inspector, MIDI piano roll, and audio warp engine:\n\
            • Sub-Tabs: Notes (Piano Roll), Envelopes (Modulation), MPE (Polyphonic Expression), Sample (Audio Warper).\n\
            • Piano Roll Navigation: Scale-Fold hides non-scale keys; [+Oct / -Oct] or [Z / X] shifts pitch octave.\n\
            • Note Stalks: Velocity stalk (1..127 with color brightness) and Chance stalk (0..100% probability) for dynamic human feel.\n\
            • Live 12 Transform Tools: Reverse (mirror time), Invert (mirror pitch), Legato (extend durations), Humanize (micro-timing ±15 ticks).\n\
            • Live 12 Generative Tools: Arpeggiate (chords to rhythmic patterns), Connect (passing tones), Chord (in-scale harmonic stacking), Euclidean (polyrhythmic pulse distribution).\n\
            • Audio Warp Modes: Beats (percussion/transients), Tones (monophonic bass/vocals), Texture (pads/drones), Re-Pitch (tape-style speed varispeed), Complex & Complex Pro (polyphonic full mixes with formant control).",
        guide_es: "Inspector de Vista de Clip, piano roll MIDI y motor de audio warping:\n\
            • Sub-Pestañas: Notas (Piano Roll), Envolventes (Modulación), MPE (Expresión Polifónica), Muestra (Warping de Audio).\n\
            • Navegación de Piano Roll: Scale-Fold oculta teclas fuera de escala; [+Oct / -Oct] o [Z / X] sube/baja octavas.\n\
            • Columnas de Expresión: Velocidad (1..127 con brillo de color) y Probabilidad/Chance (0..100%) para dinámicas orgánicas.\n\
            • Herramientas de Transformación: Reverse (invertir tiempo), Invert (espejo tonal), Legato (ligar notas), Humanize (microtiempos ±15 ticks).\n\
            • Herramientas Generativas: Arpeggiate (arpegios rítmicos), Connect (notas de paso), Chord (acordes según escala), Euclidean (pulsos euclidianos polirrítmicos).\n\
            • Modos de Warp de Audio: Beats (transitorios y percusión), Tones (bajos y voces monofónicas), Texture (pads y drones), Re-Pitch (velocidad estilo cinta analógica), Complex y Complex Pro (mezclas polifónicas con formantes).",
        mcp_tools: &[
            "midi_transform_clip",
            "midi_generate_clip",
            "audio_clip_create_warped",
            "audio_clip_set_warp_mode",
            "audio_clip_add_warp_marker",
        ],
    },

    // OKF-06: Device Chain View & 9 Native Instruments/FX
    OkfSection {
        id: "OKF-06",
        title_en: "Device Chain View & 9 Native Instruments/FX",
        title_es: "Cadena de Dispositivos y 9 Instrumentos/Efectos Nativos",
        keywords: &[
            "device", "devices", "device chain", "cadena de dispositivos", "wavetable",
            "operator", "fm", "roar", "saturation", "saturacion", "saturación", "eq eight",
            "eq8", "equalizer", "ecualizador", "glue compressor", "ssl", "g-bus", "auto filter",
            "autofilter", "zdf", "ott", "multiband dynamics", "simpler", "sampler", "drum rack",
            "add device", "macro", "macros",
        ],
        guide_en: "Live 12 Device Chain & 9 Native Studio Instruments / FX:\n\
            1. Wavetable: Dual morphing wavetable synth with analog-modeled filter pair, sub-oscillator, and flexible modulation matrix.\n\
            2. Operator: 4-operator FM synthesis workstation with 11 routing algorithms and dedicated pitch envelopes.\n\
            3. Roar: 3-stage multi-mode saturation with serial/parallel/mid-side routing, tuned feedback, and dynamic envelope follower.\n\
            4. EQ Eight: 8-band parametric equalizer featuring zero-delay feedback (ZDF) filters, real-time spectrum analysis, and band audition.\n\
            5. Glue Compressor: SSL G-Bus analog console compressor emulation with analog gain-reduction needle meter, dry/wet, and sidechain HPF.\n\
            6. Auto Filter: Analog-modeled ZDF circuit filters (Clean, OSR, MS2, SMP, PRD) with LFO modulation and envelope follower.\n\
            7. Multiband Dynamics (OTT): 3-band upward/downward compressor and expander delivering aggressive modern electronic punch and sparkle.\n\
            8. Simpler: Polyphonic sample playback with Classic ADSR, 1-Shot transient triggering, and Slicing beat-chopping modes.\n\
            9. Drum Rack: 128-pad matrix across 8 banks with nested Simpler instances, individual choke groups (1..16), and return sends.\n\
            • Add Device: Insert any native device instantly using the [+ Add Device] 3x3 picker or sound browser.",
        guide_es: "Cadena de Dispositivos de Live 12 y suite de 9 Instrumentos / Efectos Nativos:\n\
            1. Wavetable: Sintetizador de tablas de ondas duales con filtros modelados analógicos, sub-oscilador y matriz de modulación.\n\
            2. Operator: Estación de síntesis FM de 4 operadores con 11 algoritmos de ruteo y envolvente de tono dedicada.\n\
            3. Roar: Saturación multietapa de 3 bloques con ruteos serie/paralelo/mid-side, retroalimentación afinada y seguidor de envolvente.\n\
            4. EQ Eight: Ecualizador paramétrico de 8 bandas con filtros ZDF (cero retardo), analizador espectral en tiempo real y modo solo de banda.\n\
            5. Glue Compressor: Emulación de compresor de consola británica SSL G-Bus con aguja analógica de reducción de ganancia y dry/wet.\n\
            6. Auto Filter: Filtro analógico ZDF con circuitos modelados (Clean, OSR, MS2, SMP, PRD), modulación LFO y seguidor de señal.\n\
            7. Multiband Dynamics (OTT): Compresión y expansión hacia arriba/abajo en 3 bandas para máxima pegada y brillo electrónico.\n\
            8. Simpler: Reproductor de muestras polifónico con modos Clásico ADSR, One-Shot y Slicing para trocear loops por transitorios.\n\
            9. Drum Rack: Matriz de 128 pads en 8 bancos con instancias anidadas de Simpler, grupos de corte (choke 1..16) y envíos propios.\n\
            • Añadir Dispositivo: Insertá cualquier instrumento o efecto usando el selector 3x3 [+ Add Device] o desde el explorador.",
        mcp_tools: &[
            "track_create_synth",
            "device_add_to_track",
            "device_set_param",
            "device_get_chain",
            "simpler_slice_to_drum_rack",
        ],
    },

    // OKF-07: TR-909 16-Step Sequencer & Groove Engine
    OkfSection {
        id: "OKF-07",
        title_en: "TR-909 16-Step Sequencer & Groove Engine",
        title_es: "Secuenciador TR-909 de 16 Pasos y Motor de Groove",
        keywords: &[
            "tr-909", "909", "sequencer", "secuenciador", "step sequencer", "drum machine",
            "bateria", "batería", "groove", "swing", "humanize", "humanizacion", "humanización",
            "accent", "acento", "bank a", "bank b", "four-on-the-floor", "rkt", "tresillo",
            "dembow", "acid 16ths", "kick", "snare", "hi-hat", "clap", "cowbell",
        ],
        guide_en: "TR-909 16-step drum programming matrix & groove timing engine:\n\
            • 16 Discrete Voice Tracks across 2 Banks:\n\
              - Bank A (1..8): 1.Kick, 2.Snare, 3.Low Tom, 4.Mid Tom, 5.Hi Tom, 6.Rimshot, 7.Hand Clap, 8.Cowbell.\n\
              - Bank B (9..16): 9.Closed HH, 10.Open HH, 11.Crash, 12.Ride, 13.Perc 1 (Conga), 14.Perc 2 (Bongo), 15.Shaker, 16.Tambourine.\n\
            • Step Controls: 16 clickable step pills per track; Accent polygon elevates velocity to 127 in vivid amber #ffbe00.\n\
            • Groove & Timing Controls:\n\
              - Swing %: Shifts even 16th notes for authentic shuffle and swing feel.\n\
              - Humanize %: Applies micro-timing jitter (±12ms) and velocity randomization (±8 units).\n\
            • One-Click Quick Presets:\n\
              - Four-on-the-Floor: Classic 4/4 kicks on (0, 4, 8, 12) with claps on (4, 12).\n\
              - RKT / Dembow / Tresillo: Syncopated urban groove with kicks on (0, 4, 8, 12) and snares on (3, 6, 11, 14).\n\
              - Acid 16ths: Driving sixteenth-note closed hi-hats with upbeat accents on (2, 6, 10, 14).",
        guide_es: "Matriz de programación de percusión TR-909 de 16 pasos y motor de groove:\n\
            • 16 Pistas de Voz Discretas en 2 Bancos:\n\
              - Banco A (1..8): 1.Bombo, 2.Redoblante, 3.Tom Grave, 4.Tom Medio, 5.Tom Agudo, 6.Rimshot, 7.Palmas/Clap, 8.Cencerro.\n\
              - Banco B (9..16): 9.Hi-Hat Cerrado, 10.Hi-Hat Abierto, 11.Crash, 12.Ride, 13.Perc 1 (Conga), 14.Perc 2 (Bongo), 15.Shaker, 16.Pandereta.\n\
            • Controles por Paso: 16 píldoras por voz; el polígono de Acento eleva la velocidad a 127 en ámbar brillante #ffbe00.\n\
            • Controles de Groove y Microtiempo:\n\
              - Swing %: Desplaza las semicorcheas pares generando shuffle y cadencia rítmica.\n\
              - Humanize %: Aplica fluctuación de microtiempo (±12ms) y aleatoriedad de velocidad (±8 unidades).\n\
            • Patrones Rápidos en 1 Clic:\n\
              - Four-on-the-Floor: Bombos 4/4 en pasos (0, 4, 8, 12) y claps en (4, 12).\n\
              - RKT / Dembow / Tresillo: Ritmo urbano sincopado con bombos en (0, 4, 8, 12) y redobles en (3, 6, 11, 14).\n\
              - Acid 16ths: Semicorcheas continuas en hi-hat cerrado con acentos a contratiempo en (2, 6, 10, 14).",
        mcp_tools: &[
            "drums_configure_pad",
            "drums_trigger_pad",
            "generate_drum_groove_clip",
            "transport_set_swing",
        ],
    },

    // OKF-08: Global Scale Awareness, Microtonal Tuning & Export
    OkfSection {
        id: "OKF-08",
        title_en: "Global Scale Awareness, Microtonal Tuning & Export",
        title_es: "Escala Global, Afinación Microtonal y Exportación",
        keywords: &[
            "scale", "escala", "global scale", "scale lock", "tuning", "afinacion", "afinación",
            "microtonal", "12-tet", "19-edo", "24-edo", "31-edo", "bohlen-pierce", "just intonation",
            "scala", "scl", "export", "exportar", "bounce", "wav", "als", "ableton project",
            "stems", "remix", "separar stems", "cmd+b", "cmd+shift+r",
        ],
        guide_en: "Live 12 Global scale awareness, microtonal tuning systems, and multi-track export:\n\
            • Global Musical Scale: Project-wide Root Note (C..B) and Mode (Major, Minor, Dorian, Phrygian, Lydian, Mixolydian, Locrian, etc.).\n\
            • Scale Lock: Forces piano rolls, computer keyboard isomorphic playing, and generative tools into strictly harmonic in-scale pitches.\n\
            • Microtonal Tuning Systems: Native support for 12-TET, 19-EDO, 24-EDO (Quarter-tone), 31-EDO, Bohlen-Pierce (tri-scale), Just Intonation, and Scala (.scl) tuning definitions.\n\
            • 1-Click WAV Bounce [Cmd+B]: Master bounce rendering 24-bit 44.1/48 kHz master stereo audio with 32-bit float internal headroom.\n\
            • Ableton Live Project (.als) Interoperability: Two-way lossless import and export of native Live 12 gzipped XML project sets.\n\
            • 4-Stem Remix Separation [Cmd+Shift+R]: Automatically isolates and exports Drums, Bass, Other, and Vocals into independent audio files.",
        guide_es: "Escala armónica global de Live 12, sistemas microtonales y exportación multipista:\n\
            • Escala Musical Global: Tonalidad raíz (C..B) y Modo (Mayor, Menor, Dórico, Frigio, Lidio, Mixolidio, Locrio, etc.) en todo el proyecto.\n\
            • Bloqueo de Escala (Scale Lock): Fuerza al piano roll, al teclado isomórfico y a los generadores a reproducir exclusivamente notas armónicas válidas.\n\
            • Afinaciones Microtonales: Soporte nativo para 12-TET, 19-EDO, 24-EDO (cuartos de tono), 31-EDO, Bohlen-Pierce, Entonación Justa y archivos Scala (.scl).\n\
            • Master WAV en 1 Clic [Cmd+B]: Renderizado master a 24-bit 44.1/48 kHz con headroom interno de 32-bit flotante.\n\
            • Interoperabilidad con Ableton Live (.als): Importación y exportación bidireccional sin pérdidas de archivos de proyecto .als comprimidos en gzip.\n\
            • Separación de Stems para Remix [Cmd+Shift+R]: Aísla y exporta automáticamente 4 pistas independientes: Batería, Bajo, Otros y Voces.",
        mcp_tools: &[
            "scale_set_global",
            "bounce_wav",
            "render_bounce_wav",
            "project_import_als",
            "project_export_als",
            "stems_separate_remix",
            "bounce_export_stems",
        ],
    },

    // OKF-09: Suite Instruments, Audio FX, MIDI FX & Modular Patch Graph
    OkfSection {
        id: "OKF-09",
        title_en: "Suite Instruments, Audio FX, MIDI FX & Modular Patch Graph",
        title_es: "Instrumentos Suite, Efectos de Audio, Efectos MIDI y Grafo Modular",
        keywords: &[
            "suite", "instruments", "instrumentos", "drift", "wavetable", "meld", "roar",
            "operator", "collision", "tension", "electric", "audio fx", "efectos de audio",
            "midi fx", "efectos midi", "patch graph", "modular", "grafo modular", "lfo",
            "shaper", "envelope follower", "phaser-flanger", "chorus-ensemble", "hybrid reverb",
            "spectral time", "spectral blur", "delay", "reverb", "limiter",
        ],
        guide_en: "Suite synthesis instruments, studio audio effects, generative MIDI FX and modular modulation graph:\n\
            • Suite Instruments: 7 flagship synthesis engines (Drift, Wavetable, Meld bi-engine macro, Operator 4-op FM, Collision physical mallet, Tension physical string, Electric physical EP).\n\
            • Audio FX Processing: Studio-grade saturators (Roar multi-stage drive), reverbs (Hybrid Reverb convolution/algorithmic), delays, and spectral transformations (Spectral Time, Spectral Blur).\n\
            • MIDI FX Racks: Advanced generative MIDI devices (Arpeggiator, Chord, Scale, Velocity, Random, Note Length, Pitch, Strum).\n\
            • Modular Patch Graph: Dynamic lock-free signal routing graph with LFO, Shaper, and Envelope Follower modulators mapping to any internal or plugin parameter at audio rate.",
        guide_es: "Instrumentos de síntesis Suite, efectos de estudio, efectos MIDI generativos y grafo modular de modulación:\n\
            • Instrumentos Suite: 7 motores insignias de síntesis (Drift, Wavetable, Meld bi-motor macro, Operator FM de 4 operadores, Collision modelado físico de mazos, Tension cuerdas físicas, Electric piano electromecánico).\n\
            • Procesamiento Audio FX: Saturación de estudio (Roar distorsión multietapa), reverberación (Hybrid Reverb convolución/algorítmica), ecos y transformaciones espectrales (Spectral Time, Spectral Blur).\n\
            • Racks de Efectos MIDI: Procesadores MIDI generativos (Arpeggiator, Acordes, Escala, Velocidad, Aleatoriedad, Longitud de nota, Strum).\n\
            • Grafo Modular (Patch Graph): Ruteo dinámico libre de bloqueos con moduladores LFO, Shaper y Envelope Follower mapeables a cualquier parámetro interno o plugin a tasa de audio.",
        mcp_tools: &[
            "synth_list_presets",
            "synth_load_preset",
            "synth_set_parameter",
            "synth_set_macro",
            "device_add_to_track",
            "device_remove_from_track",
            "device_set_param",
            "fx_set_saturator",
            "fx_set_delay",
            "fx_set_reverb",
        ],
    },

    // OKF-10: CLAP/VST3/AU Sandboxing, Push 3 Control Surfaces, Audio-to-MIDI, Freeze/Flatten & .alp/DAWproject Formats
    OkfSection {
        id: "OKF-10",
        title_en: "CLAP/VST3/AU Sandboxing, Push 3 Control Surfaces, Audio-to-MIDI, Freeze/Flatten & .alp/DAWproject Formats",
        title_es: "Sandboxing CLAP/VST3/AU, Superficies Push 3, Audio a MIDI, Congelado/Aplanado y Formatos .alp/DAWproject",
        keywords: &[
            "clap", "vst3", "au", "plugin", "plugins", "sandbox", "sandboxing", "crash isolation",
            "push 3", "push", "control surface", "mpe", "pad matrix", "audio to midi", "audio-to-midi",
            "transcription", "groove pool", "agr", "freeze", "flatten", "congelar", "aplanar",
            "alp", "dawproject", "bitwig", "presonus", "exchange", "lossless freeze", "flac cache",
        ],
        guide_en: "External plugin hosting, hardware control surface integration, intelligent transcription and cross-DAW archives:\n\
            • Plugin Sandboxing: Multi-format host (CLAP, VST3, AU) with out-of-process crash isolation ensuring rogue plugins cannot terminate the audio engine.\n\
            • Push 3 Hardware Integration: Bi-directional high-resolution MPE pad matrix, capacitive touch strip, rotary encoder mapping, and LCD/OLED display telemetry.\n\
            • Polyphonic Audio-to-MIDI: Offline and real-time audio transcription converting drums (Kick 36, Snare 38, Hi-Hat 42), monophonic melodies (YIN f0), and polyphonic chords into MIDI clips.\n\
            • Real-Time Groove Extraction: Ableton .agr template analysis extracting 16-step micro-timing displacement (-120..+120 ticks) and dynamic velocity profiles.\n\
            • Lossless Freeze Cache & Flatten: Delta-LPC Rice-coded lossless audio compression reducing track freeze disk usage by 50..60% with bit-exact fidelity.\n\
            • Ableton Live Pack (.alp) & DAWproject Exchange: Bundles project assets with CRC32 integrity verification and imports/exports open DAWproject XML structures compatible with Bitwig and Studio One.",
        guide_es: "Alojamiento de plugins externos, superficies de control hardware, transcripción inteligente y archivos multi-DAW:\n\
            • Sandboxing de Plugins: Host multiformato (CLAP, VST3, AU) con aislamiento de fallos fuera de proceso para evitar que plugins inestables cuelguen el motor de audio.\n\
            • Integración Push 3: Matriz de pads MPE de alta resolución, cinta táctil capacitiva, mapeo de encoders rotativos y telemetría para pantallas LCD/OLED.\n\
            • Audio a MIDI Polifónico: Transcripción de audio offline y en tiempo real para baterías (Kick 36, Snare 38, Hi-Hat 42), melodías monofónicas (YIN f0) y acordes polifónicos a clips MIDI.\n\
            • Extracción de Grooves en Tiempo Real: Análisis de plantillas .agr extrayendo micro-desplazamientos de 16 pasos (-120..+120 ticks) y perfiles dinámicos de velocidad.\n\
            • Freeze/Flatten con Códec Sin Pérdidas: Compresión delta-LPC Rice que reduce el espacio en disco de pistas congeladas en 50..60% con fidelidad exacta a nivel de bit.\n\
            • Ableton Live Pack (.alp) y DAWproject: Empaquetado con verificación CRC32 e importación/exportación abierta de XML compatible con Bitwig y Studio One.",
        mcp_tools: &[
            "plugin_scan",
            "bounce_wav",
            "clip_create_midi",
            "project_save",
            "project_load",
            "project_import_als",
            "project_export_als",
        ],
    },
];

/// Searches the OKF Manual for a query matching user questions in English or Spanish.
/// Returns formatted guide text and associated MCP tool badges if a match is found.
pub fn query_okf_manual(query: &str, locale: StudioLocale) -> Option<(String, Vec<String>)> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return None;
    }

    let is_es = locale == StudioLocale::Es;

    // Check for general manual/help query
    let is_general_help = q == "help"
        || q == "ayuda"
        || q == "manual"
        || q == "manual okf"
        || q == "okf"
        || q == "guia"
        || q == "guía"
        || q == "documentacion"
        || q == "documentación"
        || q == "commands"
        || q == "comandos";

    if is_general_help {
        let reply = if is_es {
            "### Manual Operativo de Oxide Studio (OKF)\n\
            Guía de referencia rápida y arquitectura Live 12:\n\
            • **OKF-01**: Arquitectura de Espacio y Vistas Duales (`Tab`, `F12`, `Cmd+Alt+B`, Splitter)\n\
            • **OKF-02**: Vista Session, Clips, Escenas y Cuantización (`1 Bar`, `1/4`, Stop)\n\
            • **OKF-03**: Mezclador de Estudio, Envíos, Crossfader y Limitador Master (`-0.3 dBFS`)\n\
            • **OKF-04**: Línea de Tiempo de Arrangement, Curvas Bezier y Comping de Tomas\n\
            • **OKF-05**: Vista de Clip — Piano Roll MIDI, Herramientas Generativas y Warping\n\
            • **OKF-06**: Cadena de Dispositivos y 9 Instrumentos/FX (Wavetable, Operator, Glue, Simpler, Roar)\n\
            • **OKF-07**: Secuenciador TR-909 de 16 Pasos y Motor de Groove (Bancos A/B, Swing %, Humanize)\n\
            • **OKF-08**: Escala Global, Afinación Microtonal y Exportación (WAV, .als, Stems)\n\
            • **OKF-09**: Instrumentos Suite, Efectos de Audio, Efectos MIDI y Grafo Modular\n\
            • **OKF-10**: Sandboxing CLAP/VST3/AU, Superficies Push 3, Audio a MIDI, Freeze/Flatten y Formatos .alp/DAWproject\n\n\
            *Preguntá \"cómo usar [dispositivo/función]\" o \"how to [feature]\" para consultar el manual detallado.*".to_string()
        } else {
            "### Oxide Studio Operational Knowledge Framework (OKF)\n\
            Quick reference guide & Live 12 architecture:\n\
            • **OKF-01**: Workspace Architecture & Dual Views (`Tab`, `F12`, `Cmd+Alt+B`, Splitter)\n\
            • **OKF-02**: Session View, Clips, Scenes & Launch Quantization (`1 Bar`, `1/4`, Stop)\n\
            • **OKF-03**: Studio Mixer, Returns, Crossfader & Brickwall Limiter (`-0.3 dBFS`)\n\
            • **OKF-04**: Arrangement Timeline, Bezier Automation & Take Lane Comping\n\
            • **OKF-05**: Clip View — MIDI Piano Roll, Generative Tools & Audio Warping\n\
            • **OKF-06**: Device Chain View & 9 Native Devices (Wavetable, Operator, Glue, Simpler, Roar)\n\
            • **OKF-07**: TR-909 16-Step Sequencer & Groove Engine (Bank A/B, Swing %, Humanize)\n\
            • **OKF-08**: Global Scale Awareness, Microtonal Tuning & Export (WAV, .als, Stems)\n\
            • **OKF-09**: Suite Instruments, Audio FX, MIDI FX & Modular Patch Graph\n\
            • **OKF-10**: CLAP/VST3/AU Sandboxing, Push 3 Control Surfaces, Audio-to-MIDI, Freeze/Flatten & .alp/DAWproject Formats\n\n\
            *Ask \"how to use [device/feature]\" or \"cómo usar [función]\" for detailed operational guidance.*".to_string()
        };
        let badges = vec![
            "live12_get_song_info".to_string(),
            "browser_search".to_string(),
        ];
        return Some((reply, badges));
    }

    // Check for general shortcuts query
    let is_shortcuts_query = q.contains("shortcut")
        || q.contains("shortcuts")
        || q.contains("atajo")
        || q.contains("atajos")
        || q.contains("teclado")
        || q.contains("hotkeys");

    if is_shortcuts_query && !q.contains("glue") && !q.contains("warp") && !q.contains("909") {
        let reply = if is_es {
            "### Atajos de Teclado de Oxide Studio (Memoria Muscular Live 12)\n\
            • **Tab**: Alternar entre Vista Session y Vista Arrangement\n\
            • **Shift+Tab / F12**: Alternar Detail Deck entre Vista de Clip y Cadena de Dispositivos\n\
            • **Espacio**: Iniciar / Detener reproducción de transporte\n\
            • **Cmd+T / Cmd+Shift+T / Cmd+Alt+T**: Crear Pista de Audio / MIDI / Retorno\n\
            • **Cmd+Shift+M**: Insertar nuevo Clip MIDI vacío\n\
            • **Cmd+D / Cmd+E / Cmd+J**: Duplicar / Dividir en cabezal / Consolidar\n\
            • **Cmd+B**: Renderizar WAV en 1 clic / Modo Dibujo de automatización\n\
            • **Cmd+Alt+B**: Mostrar u ocultar explorador lateral de librería\n\
            • **Cmd+Shift+R**: Exportar separación de 4 Stems para remix\n\
            • **0**: Silenciar (Mute) pista seleccionada | **S**: Solo pista seleccionada\n\
            • **Z / X**: Bajar / Subir octava del teclado | **C / V**: Bajar / Subir velocidad\n\
            • **? / F1**: Abrir ventana modal con guía completa de atajos".to_string()
        } else {
            "### Oxide Studio Keyboard Shortcuts (Live 12 Muscle Memory)\n\
            • **Tab**: Switch between Session View and Arrangement Timeline\n\
            • **Shift+Tab / F12**: Toggle Detail Deck between Clip View and Device Chain\n\
            • **Space**: Toggle global transport Play / Stop\n\
            • **Cmd+T / Cmd+Shift+T / Cmd+Alt+T**: Insert Audio / MIDI / Return Track\n\
            • **Cmd+Shift+M**: Insert new blank MIDI Clip\n\
            • **Cmd+D / Cmd+E / Cmd+J**: Duplicate / Split at playhead / Consolidate\n\
            • **Cmd+B**: 1-Click WAV Bounce / Draw mode automation pencil\n\
            • **Cmd+Alt+B**: Toggle left browser sound library\n\
            • **Cmd+Shift+R**: Export 4-Stem remix audio stems\n\
            • **0**: Mute/Unmute active track | **S**: Solo active track\n\
            • **Z / X**: Octave Down / Up | **C / V**: Velocity Down / Up\n\
            • **? / F1**: Open in-app keyboard shortcuts modal guide".to_string()
        };
        let badges = vec![
            "live12_get_song_info".to_string(),
            "transport_play".to_string(),
        ];
        return Some((reply, badges));
    }

    // Score all sections by matching keywords
    let mut best_match: Option<(&OkfSection, usize)> = None;

    for section in OKF_MANUAL_SECTIONS {
        let mut score = 0;

        // Direct ID match
        if q.contains(&section.id.to_lowercase()) {
            score += 20;
        }

        // Keywords match
        for &kw in section.keywords {
            if q.contains(kw) {
                // Multi-word exact match awards higher weight
                if kw.contains(' ') {
                    score += 8;
                } else {
                    score += 3;
                }
            }
        }

        // Device-specific high-priority checks
        if section.id == "OKF-06" {
            if q.contains("glue") || q.contains("compressor") || q.contains("compresor") || q.contains("ssl") {
                score += 15;
            }
            if q.contains("wavetable") || q.contains("operator") || q.contains("roar") || q.contains("eq eight") || q.contains("eq8") || q.contains("simpler") || q.contains("drum rack") || q.contains("ott") || q.contains("auto filter") {
                score += 15;
            }
        }
        if section.id == "OKF-05" && (q.contains("warp") || q.contains("warping") || q.contains("piano roll") || q.contains("generative") || q.contains("generativo") || q.contains("arpeggiate") || q.contains("euclidean")) {
            score += 15;
        }
        if section.id == "OKF-07" && (q.contains("909") || q.contains("sequencer") || q.contains("secuenciador") || q.contains("groove engine") || q.contains("swing")) {
            score += 15;
        }
        if section.id == "OKF-08" && (q.contains("microtonal") || q.contains("tuning") || q.contains("afinacion") || q.contains("afinación") || q.contains("scala") || q.contains("scl") || q.contains("als") || q.contains("stems") || q.contains("stem") || q.contains("remix")) {
            score += 15;
        }
        if section.id == "OKF-09" && (q.contains("suite") || q.contains("drift") || q.contains("meld") || q.contains("collision") || q.contains("tension") || q.contains("electric") || q.contains("modular") || q.contains("patch graph") || q.contains("midi fx") || q.contains("audio fx")) {
            score += 15;
        }
        if section.id == "OKF-10" && (q.contains("clap") || q.contains("vst") || q.contains("sandbox") || q.contains("push 3") || q.contains("audio to midi") || q.contains("audio-to-midi") || q.contains("freeze") || q.contains("flatten") || q.contains("alp") || q.contains("dawproject")) {
            score += 15;
        }
        if section.id == "OKF-03" && (q.contains("mixer") || q.contains("mezclador") || q.contains("crossfader") || q.contains("vu meter") || q.contains("vumetro") || q.contains("sends") || q.contains("envios") || q.contains("fader")) {
            score += 15;
        }
        if section.id == "OKF-04" && (q.contains("arrangement") || q.contains("timeline") || q.contains("take lane") || q.contains("comping") || q.contains("back to arrangement") || q.contains("bezier")) {
            score += 15;
        }
        if section.id == "OKF-02" && (q.contains("launch quantization") || q.contains("cuantizacion de disparo") || q.contains("cuantización de disparo") || q.contains("stop all") || q.contains("scene launch")) {
            score += 15;
        }
        if section.id == "OKF-01" && (q.contains("dual view") || q.contains("splitter") || q.contains("layout guard") || q.contains("flexfitgrid")) {
            score += 15;
        }

        if score > 0 {
            match best_match {
                Some((_, top_score)) if score > top_score => {
                    best_match = Some((section, score));
                }
                None => {
                    best_match = Some((section, score));
                }
                _ => {}
            }
        }
    }

    if let Some((section, _)) = best_match {
        let title = if is_es { section.title_es } else { section.title_en };
        let guide = if is_es { section.guide_es } else { section.guide_en };
        let formatted = format!("### {} — {}\n\n{}", section.id, title, guide);
        let badges: Vec<String> = section.mcp_tools.iter().map(|s| s.to_string()).collect();
        return Some((formatted, badges));
    }

    None
}

/// Compiles the concise OKF Operational Reference into an LLM system prompt for the AI Copilot.
pub fn build_copilot_okf_system_prompt(locale: StudioLocale) -> String {
    let is_es = locale == StudioLocale::Es;

    if is_es {
        let mut prompt = String::from(
            "Sos el Copiloto de Producción de Oxide Studio, una DAW profesional en Rust inspirada en Ableton Live 12.\n\
            Tenés acceso directo a telemetría DSP, parámetros de mezcla y ejecución de herramientas MCP.\n\
            Respondé de forma directa, técnica y profesional en español rioplatense (voseo natural).\n\n\
            MARCO DE CONOCIMIENTO OPERATIVO (OKF):\n"
        );

        for s in OKF_MANUAL_SECTIONS {
            prompt.push_str(&format!("• [{} - {}]: {}\n  Herramientas MCP: {}\n\n",
                s.id, s.title_es, s.guide_es.replace('\n', " "), s.mcp_tools.join(", ")
            ));
        }

        prompt.push_str(
            "REGLAS DE OPERACIÓN:\n\
            1. Ante comandos de mezcla (mute, solo, volumen), ejecutá o reportá la mutación del canal vía mixer_set_channel.\n\
            2. Ante consultas de ayuda o funcionamiento de dispositivos (Glue Compressor, Simpler, Roar, Wavetable, Warping, 909, etc.), explicá su arquitectura operativa precisa.\n\
            3. Priorizá siempre la precisión técnica, ergonomía de producción electrónica y atajos de teclado estándar de Live 12.\n"
        );

        prompt
    } else {
        let mut prompt = String::from(
            "You are the Oxide Studio Producer Copilot, a professional Rust DAW assistant built on Ableton Live 12 architecture.\n\
            You have direct access to DSP telemetry, mixer parameters, and MCP tool execution.\n\
            Provide direct, concise, studio-grade guidance for electronic music production.\n\n\
            OPERATIONAL KNOWLEDGE FRAMEWORK (OKF):\n"
        );

        for s in OKF_MANUAL_SECTIONS {
            prompt.push_str(&format!("• [{} - {}]: {}\n  MCP Tools: {}\n\n",
                s.id, s.title_en, s.guide_en.replace('\n', " "), s.mcp_tools.join(", ")
            ));
        }

        prompt.push_str(
            "OPERATIONAL RULES:\n\
            1. For mixer commands (mute, solo, volume adjustments), report and execute channel modifications via mixer_set_channel.\n\
            2. For feature queries (Glue Compressor, Simpler, Roar, Wavetable, Warping, TR-909, etc.), provide exact operational architecture and parameter guidelines.\n\
            3. Strictly maintain professional music production terminology, Live 12 muscle-memory shortcuts, and non-blocking DSP workflows.\n"
        );

        prompt
    }
}

/// Deterministic cross-platform golden audio and shader uniform telemetry verification helper.
pub struct GoldenTelemetryVerifier;

impl GoldenTelemetryVerifier {
    /// Deterministically verifies that audio render buffers and GPU shader uniforms
    /// conform to cross-platform golden invariants.
    ///
    /// Invariants:
    /// 1. Shader Viewport: width and height between 64 and 8192, aspect ratio between 0.1 and 10.0.
    /// 2. Audio Buffer: non-empty, all samples finite (no NaN, no Inf), peak amplitude <= 1.05 (clean digital headroom),
    ///    and RMS energy >= 0.001 (active audible signal).
    pub fn verify_golden_audio_and_shader_uniforms(
        samples: &[f32],
        width: u32,
        height: u32,
    ) -> bool {
        // 1. Viewport & uniform invariants
        if width < 64 || height < 64 || width > 8192 || height > 8192 {
            return false;
        }
        let aspect = width as f32 / height as f32;
        if aspect < 0.1 || aspect > 10.0 {
            return false;
        }

        // 2. Audio buffer invariants
        if samples.is_empty() {
            return false;
        }

        let mut sum_sq = 0.0f64;
        let mut peak = 0.0f32;

        for &s in samples {
            if !s.is_finite() || s.is_nan() {
                return false;
            }
            let abs_s = s.abs();
            if abs_s > peak {
                peak = abs_s;
            }
            sum_sq += (s as f64) * (s as f64);
        }

        // Must not exceed digital ceiling (1.05 = 0 dBFS with slight numerical margin)
        if peak > 1.05 {
            return false;
        }

        // Must have non-zero audible RMS energy
        let rms = (sum_sq / samples.len() as f64).sqrt();
        if rms < 0.001 {
            return false;
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_okf_manual_sections_census() {
        assert_eq!(OKF_MANUAL_SECTIONS.len(), 10);
        assert_eq!(OKF_MANUAL_SECTIONS[0].id, "OKF-01");
        assert_eq!(OKF_MANUAL_SECTIONS[7].id, "OKF-08");
        assert_eq!(OKF_MANUAL_SECTIONS[8].id, "OKF-09");
        assert_eq!(OKF_MANUAL_SECTIONS[9].id, "OKF-10");

        for section in OKF_MANUAL_SECTIONS {
            assert!(!section.keywords.is_empty());
            assert!(!section.mcp_tools.is_empty());
            assert!(!section.guide_en.is_empty());
            assert!(!section.guide_es.is_empty());
        }
    }

    #[test]
    fn test_query_okf_manual_general_help() {
        let (en_reply, en_badges) = query_okf_manual("help", StudioLocale::En).unwrap();
        assert!(en_reply.contains("OKF-01"));
        assert!(en_reply.contains("OKF-08"));
        assert!(en_badges.contains(&"live12_get_song_info".to_string()));

        let (es_reply, es_badges) = query_okf_manual("ayuda", StudioLocale::Es).unwrap();
        assert!(es_reply.contains("OKF-01"));
        assert!(es_reply.contains("OKF-08"));
        assert!(es_badges.contains(&"live12_get_song_info".to_string()));
    }

    #[test]
    fn test_query_okf_manual_shortcuts() {
        let (reply_en, badges_en) = query_okf_manual("shortcuts", StudioLocale::En).unwrap();
        assert!(reply_en.contains("Tab"));
        assert!(reply_en.contains("Shift+Tab"));
        assert!(badges_en.contains(&"live12_get_song_info".to_string()));

        let (reply_es, badges_es) = query_okf_manual("atajos de teclado", StudioLocale::Es).unwrap();
        assert!(reply_es.contains("Tab"));
        assert!(reply_es.contains("Shift+Tab"));
        assert!(badges_es.contains(&"live12_get_song_info".to_string()));
    }

    #[test]
    fn test_query_okf_manual_devices_and_glue_compressor() {
        let (reply, badges) = query_okf_manual("how to use glue compressor", StudioLocale::En).unwrap();
        assert!(reply.contains("OKF-06"));
        assert!(reply.contains("Glue Compressor"));
        assert!(badges.contains(&"device_add_to_track".to_string()));

        let (reply_es, badges_es) = query_okf_manual("cómo usar simpler", StudioLocale::Es).unwrap();
        assert!(reply_es.contains("OKF-06"));
        assert!(reply_es.contains("Simpler"));
        assert!(badges_es.contains(&"simpler_slice_to_drum_rack".to_string()));
    }

    #[test]
    fn test_query_okf_manual_audio_warping() {
        let (reply, badges) = query_okf_manual("what are the audio warp modes?", StudioLocale::En).unwrap();
        assert!(reply.contains("OKF-05"));
        assert!(reply.contains("Beats"));
        assert!(reply.contains("Complex Pro"));
        assert!(badges.contains(&"audio_clip_set_warp_mode".to_string()));
    }

    #[test]
    fn test_query_okf_manual_mixer_and_crossfader() {
        let (reply, badges) = query_okf_manual("explain mixer crossfader and limiter", StudioLocale::En).unwrap();
        assert!(reply.contains("OKF-03"));
        assert!(reply.contains("Brickwall Limiter"));
        assert!(badges.contains(&"mixer_set_channel".to_string()));
    }

    #[test]
    fn test_query_okf_manual_tr909_sequencer() {
        let (reply, badges) = query_okf_manual("tr-909 step sequencer groove", StudioLocale::En).unwrap();
        assert!(reply.contains("OKF-07"));
        assert!(reply.contains("Bank A"));
        assert!(reply.contains("Four-on-the-Floor"));
        assert!(badges.contains(&"drums_configure_pad".to_string()));
    }

    #[test]
    fn test_query_okf_manual_export_stems_als() {
        let (reply, badges) = query_okf_manual("how to export stems and als", StudioLocale::En).unwrap();
        assert!(reply.contains("OKF-08"));
        assert!(reply.contains(".als"));
        assert!(reply.contains("4-Stem"));
        assert!(badges.contains(&"stems_separate_remix".to_string()));
    }

    #[test]
    fn test_build_copilot_okf_system_prompt() {
        let prompt_en = build_copilot_okf_system_prompt(StudioLocale::En);
        assert!(prompt_en.contains("OKF-01"));
        assert!(prompt_en.contains("OKF-08"));
        assert!(prompt_en.contains("OKF-09"));
        assert!(prompt_en.contains("OKF-10"));
        assert!(prompt_en.contains("Live 12"));

        let prompt_es = build_copilot_okf_system_prompt(StudioLocale::Es);
        assert!(prompt_es.contains("OKF-01"));
        assert!(prompt_es.contains("OKF-08"));
        assert!(prompt_es.contains("OKF-09"));
        assert!(prompt_es.contains("OKF-10"));
        assert!(prompt_es.contains("rioplatense"));
    }

    #[test]
    fn test_golden_telemetry_verifier_valid_audio_and_shader_bounds() {
        // Generate valid sine wave audio buffer
        let samples: Vec<f32> = (0..1024)
            .map(|i| (i as f32 * 0.05).sin() * 0.7)
            .collect();

        // 1920x1080 standard HD viewport
        assert!(GoldenTelemetryVerifier::verify_golden_audio_and_shader_uniforms(
            &samples, 1920, 1080
        ));

        // 4K resolution (3840x2160)
        assert!(GoldenTelemetryVerifier::verify_golden_audio_and_shader_uniforms(
            &samples, 3840, 2160
        ));
    }

    #[test]
    fn test_golden_telemetry_verifier_catches_nan_and_clipping() {
        let valid_samples: Vec<f32> = (0..512).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();

        // Audio with NaN
        let mut nan_samples = valid_samples.clone();
        nan_samples[10] = f32::NAN;
        assert!(!GoldenTelemetryVerifier::verify_golden_audio_and_shader_uniforms(
            &nan_samples, 1280, 800
        ));

        // Audio with Inf
        let mut inf_samples = valid_samples.clone();
        inf_samples[5] = f32::INFINITY;
        assert!(!GoldenTelemetryVerifier::verify_golden_audio_and_shader_uniforms(
            &inf_samples, 1280, 800
        ));

        // Audio with severe digital clipping (> 1.05)
        let mut clipped_samples = valid_samples.clone();
        clipped_samples[20] = 1.85;
        assert!(!GoldenTelemetryVerifier::verify_golden_audio_and_shader_uniforms(
            &clipped_samples, 1280, 800
        ));

        // Empty audio buffer
        assert!(!GoldenTelemetryVerifier::verify_golden_audio_and_shader_uniforms(
            &[], 1280, 800
        ));

        // Near-zero energy silence
        let silent_samples = vec![0.0f32; 512];
        assert!(!GoldenTelemetryVerifier::verify_golden_audio_and_shader_uniforms(
            &silent_samples, 1280, 800
        ));
    }

    #[test]
    fn test_golden_telemetry_verifier_catches_invalid_shader_uniforms() {
        let valid_samples: Vec<f32> = (0..512).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();

        // 0x0 viewport dimensions
        assert!(!GoldenTelemetryVerifier::verify_golden_audio_and_shader_uniforms(
            &valid_samples, 0, 0
        ));

        // Dimensions below minimum budget (< 64px)
        assert!(!GoldenTelemetryVerifier::verify_golden_audio_and_shader_uniforms(
            &valid_samples, 32, 32
        ));

        // Absurd aspect ratio (e.g. 5000x10)
        assert!(!GoldenTelemetryVerifier::verify_golden_audio_and_shader_uniforms(
            &valid_samples, 5000, 10
        ));
    }

    #[test]
    fn test_query_okf_manual_suite_and_clap_formats() {
        let (suite_reply, suite_badges) =
            query_okf_manual("tell me about suite instruments and roar", StudioLocale::En).unwrap();
        assert!(suite_reply.contains("OKF-09"));
        assert!(suite_reply.contains("Drift"));
        assert!(suite_reply.contains("Roar"));
        assert!(suite_badges.contains(&"synth_load_preset".to_string()));

        let (clap_reply, clap_badges) =
            query_okf_manual("cómo funciona el sandboxing de plugins clap y vst3", StudioLocale::Es).unwrap();
        assert!(clap_reply.contains("OKF-10"));
        assert!(clap_reply.contains("CLAP"));
        assert!(clap_reply.contains("aislamiento"));
        assert!(clap_badges.contains(&"plugin_scan".to_string()));
    }
}
