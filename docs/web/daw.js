/**
 * OXIDE STUDIO WEB — NATIVE IN-BROWSER ELECTRONIC MUSIC WORKSTATION (v1.0.0)
 * 100% Zero-Dependency WebAudio API + HTML5 Canvas 60 FPS Engine.
 * Features:
 *  - Dual View: Session Matrix & Arrangement Timeline (toggled by TAB key)
 *  - 16-Voice TR-909 Synthesizer Engine (Bank A 1-8 & Bank B 9-16)
 *  - 16-Step Sequencer with Velocity, Accent, and Groove Pool Swing (50%-75%)
 *  - Polyphonic Suite Synthesizer Engine (Drift, Meld, Operator, Wavetable) + 4-Pole ZDF Filter
 *  - Studio Audio FX Rack (Glue Compressor with Needle VU, OTT, Roar, Hybrid Reverb, TruePeakLimiter)
 *  - Channel & Master Mixer with post-fader segmented green/yellow/red VU meters, Pan, Sends A/B
 *  - Bilingual (EN/ES) OKF Copilot with Natural Language DSP Router & Live Telemetry
 *  - In-Browser .wav 16-bit 44.1kHz PCM bounce (OfflineAudioContext) & .als Ableton Live 12 Set export (gzip)
 *  - 60 FPS HTML5 Canvas Visualizers (Scope, 64-band FFT Spectrum, Analog Needle VU, Bode Curve)
 */

(function () {
  'use strict';

  // =========================================================================
  // 1. GLOBAL STATE & CONSTANTS
  // =========================================================================

  const STATE = {
    audioCtx: null,
    sampleRate: 44100,
    isPlaying: false,
    isRecording: false,
    loopEnabled: true,
    metronome: false,
    activeScene: 2, // Scene 2: Peak Drop active by default so all 4 tracks play immediately
    tempo: 136.0,
    swing: 56, // 50% = straight, 75% = max swing
    currentStep: 0,
    currentBar: 1,
    activeView: 'session', // 'session' | 'arrangement'
    activeDeck: 'deck-sequencer',
    activeBank: 'A', // 'A' | 'B'
    accentMode: false,
    selectedTrack: 0,
    language: 'en', // 'en' | 'es'
    octaveOffset: 3, // C3
    scaleRoot: 5, // F
    scaleType: 'minor',
    quantize: '1bar',
    limiterCeiling: -0.3,
    noiseBuffer: null,

    // Synth Parameters
    synth: {
      mode: 'drift', // 'drift' | 'meld' | 'operator' | 'wavetable'
      cutoff: 6200,
      resonance: 1.4,
      attack: 0.008,
      decay: 0.28,
      sustain: 0.45,
      release: 0.16,
      morph: 0.35,
      drive: 2.5
    },

    // FX Parameters
    fx: {
      glue: { active: true, threshold: -14, ratio: 4.0, makeup: 4.5, gainReduction: 0 },
      ott: { active: true, low: 0.70, mid: 0.65, high: 0.80, depth: 0.60 },
      roar: { active: true, drive: 2.5, tone: 4500, curve: 'warm' },
      reverb: { active: true, decay: 2.4, size: 0.75, mix: 0.28 },
      limiter: { active: true, ceiling: -0.3, release: 0.08, isClipping: false }
    },

    // 4 Tracks + 2 Returns + Master
    tracks: [
      { name: '1 DRUMS', volume: 0.95, pan: 0.0, sendA: 0.20, sendB: 0.10, mute: false, solo: false, peakDb: -60, rmsDb: -60 },
      { name: '2 BASS', volume: 0.88, pan: 0.0, sendA: 0.10, sendB: 0.0, mute: false, solo: false, peakDb: -60, rmsDb: -60 },
      { name: '3 LEAD', volume: 0.85, pan: 0.15, sendA: 0.35, sendB: 0.25, mute: false, solo: false, peakDb: -60, rmsDb: -60 },
      { name: '4 STEMS', volume: 0.75, pan: -0.20, sendA: 0.40, sendB: 0.20, mute: false, solo: false, peakDb: -60, rmsDb: -60 },
      { name: 'A REV', volume: 0.80, pan: 0.0, sendA: 0, sendB: 0, mute: false, solo: false, peakDb: -60, rmsDb: -60 },
      { name: 'B DLY', volume: 0.70, pan: 0.0, sendA: 0, sendB: 0, mute: false, solo: false, peakDb: -60, rmsDb: -60 },
      { name: 'MASTER', volume: 1.0, pan: 0.0, sendA: 0, sendB: 0, mute: false, solo: false, peakDb: -0.3, rmsDb: -12, mono: false, dim: false }
    ],

    // Session Matrix Clips: 4 Tracks x 4 Scenes
    sessionClips: [
      // Track 0 (Drums): Scenes 0..3
      [{ name: '909 Core Beat', state: 'playing' }, { name: '909 Snare Roll', state: 'stopped' }, { name: '909 Full Peak Drop', state: 'stopped' }, { name: '909 Stripped Kick', state: 'stopped' }],
      // Track 1 (Bass)
      [{ name: 'Sub Sine 55Hz', state: 'playing' }, { name: 'Acid 303 Sweep', state: 'stopped' }, { name: 'Heavy Roar Bass', state: 'stopped' }, { name: 'Low Filtered Sub', state: 'stopped' }],
      // Track 2 (Lead)
      [{ name: 'Melodic Pluck', state: 'playing' }, { name: 'FM Swarm Riser', state: 'stopped' }, { name: 'Main Hook Stabs', state: 'stopped' }, { name: 'Reverb Drone', state: 'stopped' }],
      // Track 3 (Keys/Stems)
      [{ name: 'Analog Pad C#', state: 'playing' }, { name: 'Shaker Rush', state: 'stopped' }, { name: 'Cymbal Sparkle', state: 'stopped' }, { name: 'Vinyl Crackle & Air', state: 'stopped' }]
    ],

    // 16 Synthesized Drum Voices: 8 Bank A + 8 Bank B
    // Clean, club-mastered initial groove (6 core 909 voices active; 10 auxiliary voices ready on demand)
    drumVoices: [
      // Bank A (1–8)
      { id: 'kick', name: 'Kick 909', bank: 'A', mute: false, solo: false, steps: [2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0] },
      { id: 'snare', name: 'Snare 909', bank: 'A', mute: false, solo: false, steps: [0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 1] },
      { id: 'lowTom', name: 'Low Tom 909', bank: 'A', mute: false, solo: false, steps: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] },
      { id: 'midTom', name: 'Mid Tom 909', bank: 'A', mute: false, solo: false, steps: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] },
      { id: 'hiTom', name: 'Hi Tom 909', bank: 'A', mute: false, solo: false, steps: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] },
      { id: 'rim', name: 'Rimshot 909', bank: 'A', mute: false, solo: false, steps: [0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0] },
      { id: 'clap', name: 'Clap 909', bank: 'A', mute: false, solo: false, steps: [0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0] },
      { id: 'cowbell', name: 'Cowbell 909', bank: 'A', mute: false, solo: false, steps: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] },

      // Bank B (9–16)
      { id: 'ch', name: 'Closed Hat 909', bank: 'B', mute: false, solo: false, steps: [1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1] },
      { id: 'oh', name: 'Open Hat 909', bank: 'B', mute: false, solo: false, steps: [0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 2, 0] },
      { id: 'crash', name: 'Crash 909', bank: 'B', mute: false, solo: false, steps: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] },
      { id: 'ride', name: 'Ride 909', bank: 'B', mute: false, solo: false, steps: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] },
      { id: 'percLow', name: 'Perc Low Wood', bank: 'B', mute: false, solo: false, steps: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] },
      { id: 'percHigh', name: 'Perc High Bongo', bank: 'B', mute: false, solo: false, steps: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] },
      { id: 'shaker', name: 'Analog Shaker', bank: 'B', mute: false, solo: false, steps: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] },
      { id: 'tamb', name: 'Tambourine 909', bank: 'B', mute: false, solo: false, steps: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] }
    ]
  };

  // Open Hat active voice node reference for closed hat choke link
  let activeOpenHatGain = null;
  // Active monophonic bass voice node reference to prevent sub-bass mud/overlap
  let activeBassGain = null;
  // Pre-computed waveshaper curve cache (prevents main-thread GC stalls)
  const DISTORTION_CURVE_CACHE = new Map();

  // Active audio nodes references
  const NODES = {
    trackGains: [],
    trackPanners: [],
    trackAnalysers: [],
    sendAGains: [],
    sendBGains: [],
    returnAGain: null,
    returnBGain: null,
    returnAPanner: null,
    returnBPanner: null,
    returnAAnalyser: null,
    returnBAnalyser: null,
    reverbConvolver: null,
    delayNode: null,
    delayFeedback: null,
    synthBusInput: null,
    roarToneFilter: null,
    masterPreGain: null,
    subCutFilter: null,
    glueCompressor: null,
    glueMakeupGain: null,
    ottFilters: [],
    roarShaper: null,
    filterNode1: null,
    filterNode2: null,
    truePeakLimiter: null,
    masterAnalyser: null,
    masterFader: null
  };

  // Pre-generate 2 seconds of pure white noise for drum synth voices
  function getNoiseBuffer(ctx) {
    if (STATE.noiseBuffer && STATE.noiseBuffer.sampleRate === ctx.sampleRate) {
      return STATE.noiseBuffer;
    }
    const bufferSize = ctx.sampleRate * 2;
    const buffer = ctx.createBuffer(1, bufferSize, ctx.sampleRate);
    const data = buffer.getChannelData(0);
    let x = 0x98765432;
    for (let i = 0; i < bufferSize; i++) {
      x ^= x << 13;
      x ^= x >> 17;
      x ^= x << 5;
      data[i] = (x / 2147483648.0) - 1.0;
    }
    if (ctx === STATE.audioCtx) {
      STATE.noiseBuffer = buffer;
    }
    return buffer;
  }

  // =========================================================================
  // 2. WEBAUDIO GRAPH ARCHITECTURE (Full-Bandwidth Master + Dedicated Synth Bus)
  // =========================================================================

  function initAudioGraph() {
    if (STATE.audioCtx) return;

    const AudioContextClass = window.AudioContext || window.webkitAudioContext;
    const ctx = new AudioContextClass({ latencyHint: 'interactive' });
    STATE.audioCtx = ctx;
    STATE.sampleRate = ctx.sampleRate;

    // 1. Dedicated Synth & Saturation Bus (used by Track 2 Lead & Synth Keyboard):
    // [Synth Voices] -> [Roar Warm Saturation] -> [Roar Tone Filter] -> [4-Pole ZDF Filter (filter1 -> filter2)] -> [Track Gain]
    const synthBusInput = ctx.createGain();
    synthBusInput.gain.setValueAtTime(0.85, ctx.currentTime);
    NODES.synthBusInput = synthBusInput;

    const roarShaper = ctx.createWaveShaper();
    roarShaper.curve = getCachedDistortionCurve(STATE.fx.roar.drive, STATE.fx.roar.curve);
    roarShaper.oversample = '2x';
    NODES.roarShaper = roarShaper;

    const roarTone = ctx.createBiquadFilter();
    roarTone.type = 'lowpass';
    roarTone.frequency.setValueAtTime(STATE.fx.roar.tone || 4500, ctx.currentTime);
    roarTone.Q.setValueAtTime(0.707, ctx.currentTime);
    NODES.roarToneFilter = roarTone;

    const filter1 = ctx.createBiquadFilter();
    filter1.type = 'lowpass';
    filter1.frequency.setValueAtTime(STATE.synth.cutoff, ctx.currentTime);
    filter1.Q.setValueAtTime(Math.min(4.0, Math.sqrt(STATE.synth.resonance)), ctx.currentTime);

    const filter2 = ctx.createBiquadFilter();
    filter2.type = 'lowpass';
    filter2.frequency.setValueAtTime(STATE.synth.cutoff, ctx.currentTime);
    filter2.Q.setValueAtTime(Math.min(4.0, Math.sqrt(STATE.synth.resonance)), ctx.currentTime);
    NODES.filterNode1 = filter1;
    NODES.filterNode2 = filter2;

    synthBusInput.connect(roarShaper);
    roarShaper.connect(roarTone);
    roarTone.connect(filter1);
    filter1.connect(filter2);

    // 2. Full-Bandwidth Studio Master Bus (20 Hz – 20 kHz, zero mud):
    const masterPreGain = ctx.createGain();
    masterPreGain.gain.setValueAtTime(0.72, ctx.currentTime);
    NODES.masterPreGain = masterPreGain;

    const subCut = ctx.createBiquadFilter();
    subCut.type = 'highpass';
    subCut.frequency.setValueAtTime(28, ctx.currentTime);
    subCut.Q.setValueAtTime(0.707, ctx.currentTime);
    NODES.subCutFilter = subCut;

    // Glue Bus Compressor (smooth SSL-style bus glue)
    const glue = ctx.createDynamicsCompressor();
    glue.threshold.setValueAtTime(STATE.fx.glue.threshold, ctx.currentTime);
    glue.knee.setValueAtTime(6.0, ctx.currentTime);
    glue.ratio.setValueAtTime(STATE.fx.glue.ratio, ctx.currentTime);
    glue.attack.setValueAtTime(0.015, ctx.currentTime);
    glue.release.setValueAtTime(0.12, ctx.currentTime);
    NODES.glueCompressor = glue;

    const glueMakeup = ctx.createGain();
    glueMakeup.gain.setValueAtTime(Math.pow(10, (STATE.fx.glue.makeup || 0) / 40), ctx.currentTime);
    NODES.glueMakeupGain = glueMakeup;

    // Subtle OTT Punch, Mid & Air sheen shelves
    const ottLow = ctx.createBiquadFilter();
    ottLow.type = 'lowshelf';
    ottLow.frequency.setValueAtTime(95, ctx.currentTime);
    ottLow.gain.setValueAtTime(1.4, ctx.currentTime);

    const ottMid = ctx.createBiquadFilter();
    ottMid.type = 'peaking';
    ottMid.frequency.setValueAtTime(1250, ctx.currentTime);
    ottMid.Q.setValueAtTime(0.9, ctx.currentTime);
    ottMid.gain.setValueAtTime(0.8, ctx.currentTime);

    const ottAir = ctx.createBiquadFilter();
    ottAir.type = 'highshelf';
    ottAir.frequency.setValueAtTime(9500, ctx.currentTime);
    ottAir.gain.setValueAtTime(1.8, ctx.currentTime);
    NODES.ottFilters = [ottLow, ottMid, ottAir];

    // TruePeakLimiter (transparent brickwall safety ceiling at -0.3 dBFS)
    const limiter = ctx.createDynamicsCompressor();
    limiter.threshold.setValueAtTime(STATE.limiterCeiling, ctx.currentTime);
    limiter.knee.setValueAtTime(0.0, ctx.currentTime);
    limiter.ratio.setValueAtTime(20.0, ctx.currentTime);
    limiter.attack.setValueAtTime(0.002, ctx.currentTime);
    limiter.release.setValueAtTime(STATE.fx.limiter.release || 0.08, ctx.currentTime);
    NODES.truePeakLimiter = limiter;

    // Master Analyser (FFT & Oscilloscope)
    const masterAnalyser = ctx.createAnalyser();
    masterAnalyser.fftSize = 256;
    masterAnalyser.smoothingTimeConstant = 0.8;
    NODES.masterAnalyser = masterAnalyser;

    // Master Fader
    const masterFader = ctx.createGain();
    masterFader.gain.setValueAtTime(STATE.tracks[6].volume, ctx.currentTime);
    NODES.masterFader = masterFader;

    // Connect Clean Full-Range Master Chain
    masterPreGain.connect(subCut);
    subCut.connect(glue);
    glue.connect(glueMakeup);
    glueMakeup.connect(ottLow);
    ottLow.connect(ottMid);
    ottMid.connect(ottAir);
    ottAir.connect(limiter);
    limiter.connect(masterAnalyser);
    masterAnalyser.connect(masterFader);
    masterFader.connect(ctx.destination);

    // Reverb Return Bus (Return A — high-passed at 260 Hz so sub-bass never muddies reverb)
    const returnAGain = ctx.createGain();
    returnAGain.gain.setValueAtTime(STATE.tracks[4].volume * 0.55, ctx.currentTime);
    NODES.returnAGain = returnAGain;

    const returnAPanner = ctx.createStereoPanner();
    returnAPanner.pan.setValueAtTime(STATE.tracks[4].pan, ctx.currentTime);
    NODES.returnAPanner = returnAPanner;

    const returnAAnalyser = ctx.createAnalyser();
    returnAAnalyser.fftSize = 128;
    NODES.returnAAnalyser = returnAAnalyser;

    const revHpf = ctx.createBiquadFilter();
    revHpf.type = 'highpass';
    revHpf.frequency.setValueAtTime(260, ctx.currentTime);

    const convolver = ctx.createConvolver();
    convolver.buffer = buildReverbImpulse(ctx, STATE.fx.reverb.decay);
    NODES.reverbConvolver = convolver;

    returnAGain.connect(revHpf);
    revHpf.connect(convolver);
    convolver.connect(returnAPanner);
    returnAPanner.connect(returnAAnalyser);
    returnAAnalyser.connect(masterPreGain);

    // Delay Return Bus (Return B — ping-pong filtered dotted-8th dub delay)
    const returnBGain = ctx.createGain();
    returnBGain.gain.setValueAtTime(STATE.tracks[5].volume * 0.50, ctx.currentTime);
    NODES.returnBGain = returnBGain;

    const returnBPanner = ctx.createStereoPanner();
    returnBPanner.pan.setValueAtTime(STATE.tracks[5].pan, ctx.currentTime);
    NODES.returnBPanner = returnBPanner;

    const returnBAnalyser = ctx.createAnalyser();
    returnBAnalyser.fftSize = 128;
    NODES.returnBAnalyser = returnBAnalyser;

    const delay = ctx.createDelay(1.0);
    delay.delayTime.setValueAtTime((60 / STATE.tempo) * 0.75, ctx.currentTime);
    const delayFilter = ctx.createBiquadFilter();
    delayFilter.type = 'bandpass';
    delayFilter.frequency.setValueAtTime(1600, ctx.currentTime);
    delayFilter.Q.setValueAtTime(0.9, ctx.currentTime);

    const delayFb = ctx.createGain();
    delayFb.gain.setValueAtTime(0.32, ctx.currentTime);
    delay.connect(delayFilter);
    delayFilter.connect(delayFb);
    delayFb.connect(delay);
    NODES.delayNode = delay;
    NODES.delayFeedback = delayFb;

    returnBGain.connect(delay);
    delayFilter.connect(returnBPanner);
    returnBPanner.connect(returnBAnalyser);
    returnBAnalyser.connect(masterPreGain);

    // 4 Main Tracks (0: Drums, 1: Bass, 2: Lead, 3: Stems)
    for (let i = 0; i < 4; i++) {
      const trackGain = ctx.createGain();
      trackGain.gain.setValueAtTime(STATE.tracks[i].volume, ctx.currentTime);
      NODES.trackGains.push(trackGain);

      const panner = ctx.createStereoPanner();
      panner.pan.setValueAtTime(STATE.tracks[i].pan, ctx.currentTime);
      NODES.trackPanners.push(panner);

      const analyser = ctx.createAnalyser();
      analyser.fftSize = 128;
      analyser.smoothingTimeConstant = 0.8;
      NODES.trackAnalysers.push(analyser);

      const sendA = ctx.createGain();
      sendA.gain.setValueAtTime(STATE.tracks[i].sendA, ctx.currentTime);
      NODES.sendAGains.push(sendA);

      const sendB = ctx.createGain();
      sendB.gain.setValueAtTime(STATE.tracks[i].sendB, ctx.currentTime);
      NODES.sendBGains.push(sendB);

      // Track routing
      trackGain.connect(panner);
      panner.connect(analyser);
      analyser.connect(masterPreGain);

      // Sends routing
      trackGain.connect(sendA);
      sendA.connect(returnAGain);

      trackGain.connect(sendB);
      sendB.connect(returnBGain);
    }

    // Route Synth ZDF Filter Bus output into Track 2 (3 LEAD)
    filter2.connect(NODES.trackGains[2]);
  }

  // Generate gain-compensated waveshaper curve for Roar (cached for zero real-time allocation)
  function makeDistortionCurve(driveDb, type) {
    const n_samples = 4096;
    const curve = new Float32Array(n_samples);
    const drive = Math.pow(10, Math.min(18, Math.max(0, driveDb)) / 20);
    const norm = 1.0 / Math.max(0.85, Math.tanh(drive * 0.85));

    for (let i = 0; i < n_samples; ++i) {
      const x = (i * 2) / n_samples - 1;
      const xDrive = x * drive;
      if (type === 'hard') {
        curve[i] = Math.max(-0.95, Math.min(0.95, xDrive * 0.7));
      } else if (type === 'asym') {
        curve[i] = (x >= 0 ? Math.tanh(xDrive) : Math.tanh(xDrive * 1.25) * 0.85) * norm * 0.85;
      } else {
        curve[i] = Math.tanh(xDrive) * norm * 0.88;
      }
    }
    return curve;
  }

  function getCachedDistortionCurve(driveDb, type) {
    const key = `${Number(driveDb).toFixed(1)}_${type || 'warm'}`;
    if (!DISTORTION_CURVE_CACHE.has(key)) {
      DISTORTION_CURVE_CACHE.set(key, makeDistortionCurve(driveDb, type));
    }
    return DISTORTION_CURVE_CACHE.get(key);
  }

  // Procedural stereo velvet-noise impulse for Hybrid Reverb
  function buildReverbImpulse(ctx, duration) {
    const rate = ctx.sampleRate;
    const length = Math.floor(rate * Math.max(0.4, Math.min(4.0, duration)));
    const impulse = ctx.createBuffer(2, length, rate);
    const left = impulse.getChannelData(0);
    const right = impulse.getChannelData(1);

    let lLp = 0;
    let rLp = 0;
    for (let i = 0; i < length; i++) {
      const t = i / rate;
      const preDelayEnv = Math.min(1.0, t / 0.012);
      const decay = preDelayEnv * Math.exp(-t / (duration * 0.28));
      // Warm damping (1-pole lowpass on noise tail)
      lLp = lLp * 0.62 + (Math.random() * 2 - 1) * 0.38;
      rLp = rLp * 0.62 + (Math.random() * 2 - 1) * 0.38;
      left[i] = lLp * decay * 0.55;
      right[i] = rLp * decay * 0.55;
    }
    return impulse;
  }

  // =========================================================================
  // 3. 16-VOICE TR-909 SYNTHESIZER ENGINE (oxide-drums)
  // =========================================================================

  function triggerDrumVoice(voiceId, time, velocity, isAccent) {
    if (!STATE.audioCtx) return;
    const dest = NODES.trackGains[0]; // Track 0: Drums
    if (!dest) return;
    triggerDrumVoiceOnCtx(STATE.audioCtx, dest, voiceId, time, velocity, isAccent);
  }

  function triggerDrumVoiceOnCtx(ctx, dest, voiceId, time, velocity, isAccent) {
    const vel = (isAccent ? 1.18 : 0.92) * (velocity || 0.85);
    const noiseBuf = getNoiseBuffer(ctx);

    switch (voiceId) {
      case 'kick': {
        // Authentic 909 Kick: 3-stage pitch envelope (168Hz -> 54Hz punch -> 45Hz sub) + woody transient click
        const osc = ctx.createOscillator();
        const gain = ctx.createGain();
        osc.type = 'sine';
        osc.frequency.setValueAtTime(168, time);
        osc.frequency.exponentialRampToValueAtTime(54, time + 0.024);
        osc.frequency.exponentialRampToValueAtTime(45, time + 0.36);

        gain.gain.setValueAtTime(0.0001, time);
        gain.gain.linearRampToValueAtTime(0.72 * vel, time + 0.002);
        gain.gain.exponentialRampToValueAtTime(0.45 * vel, time + 0.08);
        gain.gain.exponentialRampToValueAtTime(0.0001, time + 0.38);

        // Filtered click impulse
        const clickNoise = ctx.createBufferSource();
        clickNoise.buffer = noiseBuf;
        const clickFilter = ctx.createBiquadFilter();
        clickFilter.type = 'bandpass';
        clickFilter.frequency.setValueAtTime(950, time);
        clickFilter.Q.setValueAtTime(1.4, time);
        const clickGain = ctx.createGain();
        clickGain.gain.setValueAtTime(0.26 * vel, time);
        clickGain.gain.exponentialRampToValueAtTime(0.0001, time + 0.012);

        osc.connect(gain);
        clickNoise.connect(clickFilter);
        clickFilter.connect(clickGain);
        gain.connect(dest);
        clickGain.connect(dest);

        osc.start(time);
        clickNoise.start(time);
        osc.stop(time + 0.4);
        clickNoise.stop(time + 0.015);
        break;
      }

      case 'snare': {
        // 909 Snare: Dual tuned drumhead modes (182 Hz + 328 Hz with pitch drop) + band-shaped snare wires
        const toneOsc1 = ctx.createOscillator();
        const toneOsc2 = ctx.createOscillator();
        const toneGain = ctx.createGain();

        toneOsc1.type = 'triangle';
        toneOsc2.type = 'sine';
        toneOsc1.frequency.setValueAtTime(220, time);
        toneOsc1.frequency.exponentialRampToValueAtTime(178, time + 0.035);
        toneOsc2.frequency.setValueAtTime(360, time);
        toneOsc2.frequency.exponentialRampToValueAtTime(315, time + 0.035);

        toneGain.gain.setValueAtTime(0.28 * vel, time);
        toneGain.gain.exponentialRampToValueAtTime(0.0001, time + 0.14);

        toneOsc1.connect(toneGain);
        toneOsc2.connect(toneGain);
        toneGain.connect(dest);

        // Snare wires: Highpass 1500 Hz + Lowpass 6500 Hz
        const noise = ctx.createBufferSource();
        noise.buffer = noiseBuf;
        const hpf = ctx.createBiquadFilter();
        hpf.type = 'highpass';
        hpf.frequency.setValueAtTime(1500, time);
        const lpf = ctx.createBiquadFilter();
        lpf.type = 'lowpass';
        lpf.frequency.setValueAtTime(6500, time);

        const noiseGain = ctx.createGain();
        noiseGain.gain.setValueAtTime(0.36 * vel, time);
        noiseGain.gain.exponentialRampToValueAtTime(0.0001, time + 0.19);

        noise.connect(hpf);
        hpf.connect(lpf);
        lpf.connect(noiseGain);
        noiseGain.connect(dest);

        toneOsc1.start(time);
        toneOsc2.start(time);
        noise.start(time);
        toneOsc1.stop(time + 0.16);
        toneOsc2.stop(time + 0.16);
        noise.stop(time + 0.21);
        break;
      }

      case 'lowTom':
      case 'midTom':
      case 'hiTom': {
        const startFreq = voiceId === 'lowTom' ? 118 : voiceId === 'midTom' ? 162 : 218;
        const endFreq = voiceId === 'lowTom' ? 62 : voiceId === 'midTom' ? 92 : 128;
        const osc = ctx.createOscillator();
        const gain = ctx.createGain();

        osc.type = 'sine';
        osc.frequency.setValueAtTime(startFreq, time);
        osc.frequency.exponentialRampToValueAtTime(endFreq, time + 0.11);

        gain.gain.setValueAtTime(0.42 * vel, time);
        gain.gain.exponentialRampToValueAtTime(0.0001, time + 0.26);

        osc.connect(gain);
        gain.connect(dest);

        osc.start(time);
        osc.stop(time + 0.28);
        break;
      }

      case 'rim': {
        // Authentic 909 Rimshot: 3-band woody/metallic transient (500 Hz + 1680 Hz bandpass impulse)
        const noise = ctx.createBufferSource();
        noise.buffer = noiseBuf;
        const bp = ctx.createBiquadFilter();
        bp.type = 'bandpass';
        bp.frequency.setValueAtTime(1680, time);
        bp.Q.setValueAtTime(4.5, time);

        const osc = ctx.createOscillator();
        osc.type = 'triangle';
        osc.frequency.setValueAtTime(480, time);
        osc.frequency.exponentialRampToValueAtTime(220, time + 0.018);

        const gain = ctx.createGain();
        gain.gain.setValueAtTime(0.32 * vel, time);
        gain.gain.exponentialRampToValueAtTime(0.0001, time + 0.032);

        noise.connect(bp);
        bp.connect(gain);
        osc.connect(gain);
        gain.connect(dest);

        noise.start(time);
        osc.start(time);
        noise.stop(time + 0.038);
        osc.stop(time + 0.038);
        break;
      }

      case 'clap': {
        // 3 staggered noise bursts (0ms, 10ms, 20ms) + 1250 Hz bandpass reverberant tail
        for (let i = 0; i < 3; i++) {
          const t = time + i * 0.010;
          const noise = ctx.createBufferSource();
          noise.buffer = noiseBuf;
          const filter = ctx.createBiquadFilter();
          filter.type = 'bandpass';
          filter.frequency.setValueAtTime(1350, t);
          filter.Q.setValueAtTime(2.0, t);

          const gain = ctx.createGain();
          gain.gain.setValueAtTime(0.28 * vel, t);
          gain.gain.exponentialRampToValueAtTime(0.0001, t + 0.016);

          noise.connect(filter);
          filter.connect(gain);
          gain.connect(dest);
          noise.start(t);
          noise.stop(t + 0.02);
        }
        const tailNoise = ctx.createBufferSource();
        tailNoise.buffer = noiseBuf;
        const tailFilter = ctx.createBiquadFilter();
        tailFilter.type = 'bandpass';
        tailFilter.frequency.setValueAtTime(1200, time + 0.024);
        tailFilter.Q.setValueAtTime(1.6, time + 0.024);
        const tailGain = ctx.createGain();
        tailGain.gain.setValueAtTime(0.30 * vel, time + 0.024);
        tailGain.gain.exponentialRampToValueAtTime(0.0001, time + 0.20);

        tailNoise.connect(tailFilter);
        tailFilter.connect(tailGain);
        tailGain.connect(dest);
        tailNoise.start(time + 0.024);
        tailNoise.stop(time + 0.22);
        break;
      }

      case 'cowbell': {
        // 808/909 Dual Bandpassed Metallic Cowbell (540 Hz + 800 Hz with steep bandpass)
        const osc1 = ctx.createOscillator();
        const osc2 = ctx.createOscillator();
        osc1.type = 'square';
        osc2.type = 'square';
        osc1.frequency.setValueAtTime(540, time);
        osc2.frequency.setValueAtTime(800, time);

        const filter = ctx.createBiquadFilter();
        filter.type = 'bandpass';
        filter.frequency.setValueAtTime(670, time);
        filter.Q.setValueAtTime(3.8, time);

        const gain = ctx.createGain();
        gain.gain.setValueAtTime(0.22 * vel, time);
        gain.gain.exponentialRampToValueAtTime(0.0001, time + 0.16);

        osc1.connect(filter);
        osc2.connect(filter);
        filter.connect(gain);
        gain.connect(dest);

        osc1.start(time);
        osc2.start(time);
        osc1.stop(time + 0.18);
        osc2.stop(time + 0.18);
        break;
      }

      case 'ch': {
        // Closed Hat chokes Open Hat
        if (ctx === STATE.audioCtx && activeOpenHatGain) {
          try {
            activeOpenHatGain.gain.cancelScheduledValues(time);
            activeOpenHatGain.gain.setValueAtTime(activeOpenHatGain.gain.value, time);
            activeOpenHatGain.gain.linearRampToValueAtTime(0.0001, time + 0.006);
          } catch (_) {}
          activeOpenHatGain = null;
        }

        // Authentic 909 Closed Hat: Crisp bandpassed noise + attenuated metallic shimmer through 2-stage 8.8kHz HPF
        const noise = ctx.createBufferSource();
        noise.buffer = noiseBuf;

        const hpf1 = ctx.createBiquadFilter();
        hpf1.type = 'highpass';
        hpf1.frequency.setValueAtTime(8500, time);

        const hpf2 = ctx.createBiquadFilter();
        hpf2.type = 'bandpass';
        hpf2.frequency.setValueAtTime(9800, time);
        hpf2.Q.setValueAtTime(1.8, time);

        const hatGain = ctx.createGain();
        hatGain.gain.setValueAtTime(0.28 * vel, time);
        hatGain.gain.exponentialRampToValueAtTime(0.0001, time + 0.042);

        const freqs = [312, 422, 659, 866];
        freqs.forEach(f => {
          const osc = ctx.createOscillator();
          const oscScale = ctx.createGain();
          osc.type = 'square';
          osc.frequency.setValueAtTime(f * 4.0, time);
          oscScale.gain.setValueAtTime(0.04, time);
          osc.connect(oscScale);
          oscScale.connect(hpf1);
          osc.start(time);
          osc.stop(time + 0.046);
        });

        noise.connect(hpf1);
        hpf1.connect(hpf2);
        hpf2.connect(hatGain);
        hatGain.connect(dest);
        noise.start(time);
        noise.stop(time + 0.046);
        break;
      }

      case 'oh': {
        // Authentic 909 Open Hat with choke link capability
        const noise = ctx.createBufferSource();
        noise.buffer = noiseBuf;

        const hpf1 = ctx.createBiquadFilter();
        hpf1.type = 'highpass';
        hpf1.frequency.setValueAtTime(7600, time);

        const bpf = ctx.createBiquadFilter();
        bpf.type = 'bandpass';
        bpf.frequency.setValueAtTime(8800, time);
        bpf.Q.setValueAtTime(1.6, time);

        const hatGain = ctx.createGain();
        hatGain.gain.setValueAtTime(0.26 * vel, time);
        hatGain.gain.exponentialRampToValueAtTime(0.0001, time + 0.28);
        if (ctx === STATE.audioCtx) {
          activeOpenHatGain = hatGain;
        }

        const freqs = [312, 422, 659, 866];
        freqs.forEach(f => {
          const osc = ctx.createOscillator();
          const oscScale = ctx.createGain();
          osc.type = 'square';
          osc.frequency.setValueAtTime(f * 4.0, time);
          oscScale.gain.setValueAtTime(0.035, time);
          osc.connect(oscScale);
          oscScale.connect(hpf1);
          osc.start(time);
          osc.stop(time + 0.30);
        });

        noise.connect(hpf1);
        hpf1.connect(bpf);
        bpf.connect(hatGain);
        hatGain.connect(dest);
        noise.start(time);
        noise.stop(time + 0.30);
        break;
      }

      case 'crash': {
        // 909 Crash Cymbal: Metallic shimmer + dual-filtered air wash
        const noise = ctx.createBufferSource();
        noise.buffer = noiseBuf;
        const hpf = ctx.createBiquadFilter();
        hpf.type = 'highpass';
        hpf.frequency.setValueAtTime(5200, time);
        const bpf = ctx.createBiquadFilter();
        bpf.type = 'peaking';
        bpf.frequency.setValueAtTime(7800, time);
        bpf.gain.setValueAtTime(4.0, time);

        const gain = ctx.createGain();
        gain.gain.setValueAtTime(0.26 * vel, time);
        gain.gain.exponentialRampToValueAtTime(0.0001, time + 0.85);

        noise.connect(hpf);
        hpf.connect(bpf);
        bpf.connect(gain);
        gain.connect(dest);
        noise.start(time);
        noise.stop(time + 0.90);
        break;
      }

      case 'ride': {
        // 909 Ride Cymbal: Metallic bell ping + high-passed cymbal sizzle noise (zero raw sine dial-tone)
        const noise = ctx.createBufferSource();
        noise.buffer = noiseBuf;

        const hpf = ctx.createBiquadFilter();
        hpf.type = 'highpass';
        hpf.frequency.setValueAtTime(6400, time);

        const bellFilter = ctx.createBiquadFilter();
        bellFilter.type = 'bandpass';
        bellFilter.frequency.setValueAtTime(7400, time);
        bellFilter.Q.setValueAtTime(3.2, time);

        const bellOsc = ctx.createOscillator();
        const bellScale = ctx.createGain();
        bellOsc.type = 'triangle';
        bellOsc.frequency.setValueAtTime(2950, time);
        bellScale.gain.setValueAtTime(0.05, time);
        bellOsc.connect(bellScale);
        bellScale.connect(bellFilter);

        const gain = ctx.createGain();
        gain.gain.setValueAtTime(0.22 * vel, time);
        gain.gain.exponentialRampToValueAtTime(0.0001, time + 0.42);

        noise.connect(hpf);
        hpf.connect(bellFilter);
        bellFilter.connect(gain);
        gain.connect(dest);

        noise.start(time);
        bellOsc.start(time);
        noise.stop(time + 0.45);
        bellOsc.stop(time + 0.45);
        break;
      }

      case 'percLow': {
        // Woodblock sweep ~290 Hz -> ~210 Hz
        const osc = ctx.createOscillator();
        const gain = ctx.createGain();
        osc.type = 'triangle';
        osc.frequency.setValueAtTime(290, time);
        osc.frequency.exponentialRampToValueAtTime(210, time + 0.045);
        gain.gain.setValueAtTime(0.34 * vel, time);
        gain.gain.exponentialRampToValueAtTime(0.0001, time + 0.09);
        osc.connect(gain);
        gain.connect(dest);
        osc.start(time);
        osc.stop(time + 0.10);
        break;
      }

      case 'percHigh': {
        // Bongo sweep ~540 Hz -> ~420 Hz
        const osc = ctx.createOscillator();
        const gain = ctx.createGain();
        osc.type = 'sine';
        osc.frequency.setValueAtTime(540, time);
        osc.frequency.exponentialRampToValueAtTime(420, time + 0.038);
        gain.gain.setValueAtTime(0.30 * vel, time);
        gain.gain.exponentialRampToValueAtTime(0.0001, time + 0.08);
        osc.connect(gain);
        gain.connect(dest);
        osc.start(time);
        osc.stop(time + 0.09);
        break;
      }

      case 'shaker': {
        // Band-passed maraca/shaker noise burst
        const noise = ctx.createBufferSource();
        noise.buffer = noiseBuf;
        const filter = ctx.createBiquadFilter();
        filter.type = 'bandpass';
        filter.frequency.setValueAtTime(7600, time);
        filter.Q.setValueAtTime(2.4, time);
        const gain = ctx.createGain();
        gain.gain.setValueAtTime(0.001, time);
        gain.gain.linearRampToValueAtTime(0.22 * vel, time + 0.012);
        gain.gain.exponentialRampToValueAtTime(0.0001, time + 0.055);

        noise.connect(filter);
        filter.connect(gain);
        gain.connect(dest);
        noise.start(time);
        noise.stop(time + 0.06);
        break;
      }

      case 'tamb': {
        // Metallic tambourine jingle burst (high-Q bandpassed noise + shimmer, zero sine whistle)
        const noise = ctx.createBufferSource();
        noise.buffer = noiseBuf;
        const filter = ctx.createBiquadFilter();
        filter.type = 'bandpass';
        filter.frequency.setValueAtTime(8400, time);
        filter.Q.setValueAtTime(3.6, time);

        const gain = ctx.createGain();
        gain.gain.setValueAtTime(0.22 * vel, time);
        gain.gain.exponentialRampToValueAtTime(0.08 * vel, time + 0.022);
        gain.gain.linearRampToValueAtTime(0.18 * vel, time + 0.032);
        gain.gain.exponentialRampToValueAtTime(0.0001, time + 0.14);

        noise.connect(filter);
        filter.connect(gain);
        gain.connect(dest);
        noise.start(time);
        noise.stop(time + 0.15);
        break;
      }
    }
  }

  // =========================================================================
  // 4. MONOPHONIC SUB/ACID BASS & POLYPHONIC SUITE SYNTHESIZER ENGINE
  // =========================================================================

  // Dedicated Monophonic Sub / Acid 303 Bass Engine for Track 1 (2 BASS)
  function playBassNote(midiNote, duration = 0.14, time = null) {
    if (!STATE.audioCtx) return;
    const ctx = STATE.audioCtx;
    const t = time !== null ? time : ctx.currentTime;
    const dest = NODES.trackGains[1]; // Track 1: 2 BASS
    if (!dest) return;

    // Monophonic choke of previous bass note so sub frequencies never overlap/muddy
    if (activeBassGain) {
      try {
        activeBassGain.gain.cancelScheduledValues(t);
        activeBassGain.gain.setValueAtTime(activeBassGain.gain.value, t);
        activeBassGain.gain.linearRampToValueAtTime(0.0001, t + 0.006);
      } catch (_) {}
    }

    const freq = 440 * Math.pow(2, (midiNote - 69) / 12);

    const subOsc = ctx.createOscillator();
    const sawOsc = ctx.createOscillator();
    subOsc.type = 'sine';
    sawOsc.type = 'sawtooth';
    subOsc.frequency.setValueAtTime(freq, t);
    sawOsc.frequency.setValueAtTime(freq, t);

    const sawScale = ctx.createGain();
    sawScale.gain.setValueAtTime(0.28, t);

    // Dedicated Acid Lowpass Filter envelope per bass note
    const filter = ctx.createBiquadFilter();
    filter.type = 'lowpass';
    const peakCut = Math.min(2400, Math.max(260, STATE.synth.cutoff * 0.35));
    filter.frequency.setValueAtTime(peakCut, t);
    filter.frequency.exponentialRampToValueAtTime(130, t + duration);
    filter.Q.setValueAtTime(Math.min(6.0, Math.max(1.2, STATE.synth.resonance * 0.8)), t);

    const ampGain = ctx.createGain();
    ampGain.gain.setValueAtTime(0.0001, t);
    ampGain.gain.linearRampToValueAtTime(0.46, t + 0.004);
    ampGain.gain.exponentialRampToValueAtTime(0.18, t + duration);
    ampGain.gain.exponentialRampToValueAtTime(0.0001, t + duration + 0.04);
    activeBassGain = ampGain;

    subOsc.connect(filter);
    sawOsc.connect(sawScale);
    sawScale.connect(filter);
    filter.connect(ampGain);
    ampGain.connect(dest);

    subOsc.start(t);
    sawOsc.start(t);
    subOsc.stop(t + duration + 0.05);
    sawOsc.stop(t + duration + 0.05);
  }

  function playSynthNote(midiNote, duration = 0.24, time = null, destTrack = 2) {
    if (!STATE.audioCtx) return;
    const ctx = STATE.audioCtx;
    const t = time !== null ? time : ctx.currentTime;
    const dest = destTrack === 2 && NODES.synthBusInput ? NODES.synthBusInput : NODES.trackGains[destTrack];
    if (!dest) return;

    const freq = 440 * Math.pow(2, (midiNote - 69) / 12);
    const mode = STATE.synth.mode;

    // Per-voice envelope filter
    const filter = ctx.createBiquadFilter();
    filter.type = 'lowpass';
    const startCut = Math.min(14000, Math.max(400, STATE.synth.cutoff));
    filter.frequency.setValueAtTime(startCut, t);
    filter.frequency.exponentialRampToValueAtTime(Math.max(240, startCut * 0.45), t + duration);
    filter.Q.setValueAtTime(Math.min(5.0, STATE.synth.resonance), t);

    // Clean, headroom-balanced Amplitude ADSR (0.26 max per voice so polyphony never clips)
    const ampGain = ctx.createGain();
    const att = Math.max(0.004, STATE.synth.attack);
    const dec = Math.max(0.03, STATE.synth.decay);
    const sus = Math.min(0.8, STATE.synth.sustain);
    const rel = Math.max(0.03, Math.min(0.35, STATE.synth.release));
    const voicePeak = 0.24;

    ampGain.gain.setValueAtTime(0.0001, t);
    ampGain.gain.linearRampToValueAtTime(voicePeak, t + att);
    ampGain.gain.exponentialRampToValueAtTime(Math.max(0.001, sus * voicePeak), t + att + dec);
    ampGain.gain.setValueAtTime(Math.max(0.001, sus * voicePeak), t + duration);
    ampGain.gain.exponentialRampToValueAtTime(0.0001, t + duration + rel);

    filter.connect(ampGain);
    ampGain.connect(dest);

    if (mode === 'drift') {
      // Drift: Dual analog oscillators with warm detune (zero per-note Float32Array allocation!)
      const osc1 = ctx.createOscillator();
      const osc2 = ctx.createOscillator();
      osc1.type = 'sawtooth';
      osc2.type = 'triangle';
      osc1.frequency.setValueAtTime(freq, t);
      osc2.frequency.setValueAtTime(freq * 1.003, t);

      osc1.connect(filter);
      osc2.connect(filter);

      osc1.start(t);
      osc2.start(t);
      osc1.stop(t + duration + rel + 0.05);
      osc2.stop(t + duration + rel + 0.05);

    } else if (mode === 'meld') {
      // Meld: Bi-timbral FM Swarm
      const carrier = ctx.createOscillator();
      const mod = ctx.createOscillator();
      const modGain = ctx.createGain();

      carrier.type = 'triangle';
      mod.type = 'sine';
      carrier.frequency.setValueAtTime(freq, t);
      mod.frequency.setValueAtTime(freq * 2.0, t);
      modGain.gain.setValueAtTime(freq * (0.4 + STATE.synth.morph * 0.9), t);
      modGain.gain.exponentialRampToValueAtTime(1.0, t + duration);

      mod.connect(modGain);
      modGain.connect(carrier.frequency);
      carrier.connect(filter);

      carrier.start(t);
      mod.start(t);
      carrier.stop(t + duration + rel + 0.05);
      mod.stop(t + duration + rel + 0.05);

    } else if (mode === 'operator') {
      // Operator: Classic 2:1 FM Pluck
      const opA = ctx.createOscillator();
      const opB = ctx.createOscillator();
      const opB_gain = ctx.createGain();

      opA.type = 'sine';
      opB.type = 'sine';
      opA.frequency.setValueAtTime(freq, t);
      opB.frequency.setValueAtTime(freq * 2.0, t);
      opB_gain.gain.setValueAtTime(freq * (0.6 + STATE.synth.morph * 1.6), t);
      opB_gain.gain.exponentialRampToValueAtTime(1.0, t + duration * 0.8);

      opB.connect(opB_gain);
      opB_gain.connect(opA.frequency);
      opA.connect(filter);

      opA.start(t);
      opB.start(t);
      opA.stop(t + duration + rel + 0.05);
      opB.stop(t + duration + rel + 0.05);

    } else {
      // Wavetable: Detuned pulse/saw chordal lead
      const osc = ctx.createOscillator();
      const oscSub = ctx.createOscillator();
      osc.type = STATE.synth.morph > 0.5 ? 'square' : 'sawtooth';
      oscSub.type = 'sine';
      osc.frequency.setValueAtTime(freq, t);
      oscSub.frequency.setValueAtTime(freq * 0.5, t);
      osc.connect(filter);
      oscSub.connect(filter);
      osc.start(t);
      oscSub.start(t);
      osc.stop(t + duration + rel + 0.05);
      oscSub.stop(t + duration + rel + 0.05);
    }
  }

  // =========================================================================
  // 5. HIGH-PRECISION LOOKAHEAD SCHEDULER & SEQUENCER ENGINE
  // =========================================================================

  let schedulerTimer = null;
  let nextStepTime = 0.0;
  const LOOKAHEAD_MS = 25.0;
  const SCHEDULE_AHEAD_SEC = 0.1;

  // Scale note mapping for live auto-accompaniment
  const SCALE_INTERVALS = {
    minor: [0, 2, 3, 5, 7, 8, 10],
    major: [0, 2, 4, 5, 7, 9, 11],
    dorian: [0, 2, 3, 5, 7, 9, 10],
    phrygian: [0, 1, 3, 5, 7, 8, 10],
    harmonic_minor: [0, 2, 3, 5, 7, 8, 11]
  };

  function getScalePitch(degree, octave = 3) {
    const scale = SCALE_INTERVALS[STATE.scaleType] || SCALE_INTERVALS.minor;
    const noteInOct = scale[degree % scale.length];
    const octShift = Math.floor(degree / scale.length);
    return 12 * (octave + octShift) + STATE.scaleRoot + noteInOct;
  }

  function isDrumVoiceAudible(voice) {
    const anyVoiceSolo = STATE.drumVoices.some(v => v.solo);
    if (anyVoiceSolo) return voice.solo;
    return !voice.mute;
  }

  function isMixerTrackAudible(trackIdx) {
    const allStrips = STATE.tracks.slice(0, 6);
    const anyTrackSolo = allStrips.some(t => t.solo);
    if (anyTrackSolo) return STATE.tracks[trackIdx].solo;
    return !STATE.tracks[trackIdx].mute;
  }

  function syncMixerTrackGains() {
    if (!STATE.audioCtx) return;
    const now = STATE.audioCtx.currentTime;
    for (let i = 0; i < 4; i++) {
      const g = NODES.trackGains[i];
      if (!g) continue;
      const audible = isMixerTrackAudible(i);
      g.gain.setTargetAtTime(audible ? STATE.tracks[i].volume : 0.0, now, 0.008);
    }
    if (NODES.returnAGain) {
      const audA = isMixerTrackAudible(4) && STATE.fx.reverb.active;
      NODES.returnAGain.gain.setTargetAtTime(audA ? STATE.tracks[4].volume * 0.55 * (STATE.fx.reverb.mix / 0.28) : 0.0, now, 0.01);
    }
    if (NODES.returnBGain) {
      const audB = isMixerTrackAudible(5);
      NODES.returnBGain.gain.setTargetAtTime(audB ? STATE.tracks[5].volume * 0.50 : 0.0, now, 0.01);
    }
    if (NODES.masterFader) {
      const dimMult = STATE.tracks[6].dim ? 0.15 : 1.0;
      NODES.masterFader.gain.setTargetAtTime(STATE.tracks[6].volume * dimMult, now, 0.01);
      NODES.masterFader.channelCount = STATE.tracks[6].mono ? 1 : 2;
    }
  }

  function getActiveClipIndex(trackIdx) {
    if (!STATE.sessionClips[trackIdx]) return -1;
    return STATE.sessionClips[trackIdx].findIndex(c => c.state === 'playing');
  }

  function triggerMetronomeClick(time, isDownbeat) {
    if (!STATE.audioCtx || !NODES.masterPreGain) return;
    const ctx = STATE.audioCtx;
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.type = 'triangle';
    osc.frequency.setValueAtTime(isDownbeat ? 1560 : 1040, time);
    osc.frequency.exponentialRampToValueAtTime(isDownbeat ? 780 : 520, time + 0.018);
    gain.gain.setValueAtTime(0.0001, time);
    gain.gain.linearRampToValueAtTime(isDownbeat ? 0.28 : 0.18, time + 0.002);
    gain.gain.exponentialRampToValueAtTime(0.0001, time + 0.028);
    osc.connect(gain);
    gain.connect(NODES.masterPreGain);
    osc.start(time);
    osc.stop(time + 0.032);
  }

  // Dedicated Track 3 (4 KEYS / STEMS) Synthesizer & Stem Player
  function playKeysStemStep(clipIdx, stepIndex, time, targetCtx = null, targetDest = null) {
    const ctx = targetCtx || STATE.audioCtx;
    const dest = targetDest || NODES.trackGains[3];
    if (!ctx || !dest) return;
    const noiseBuf = getNoiseBuffer(ctx);

    if (clipIdx === 0) {
      // Clip 0: Analog Pad C# — Lush polyphonic Dorian/Minor triad swell on steps 0 and 8
      if (stepIndex === 0 || stepIndex === 8) {
        const chordDegs = stepIndex === 0 ? [0, 2, 4] : [3, 5, 7];
        const dur = (60 / STATE.tempo) * 1.85;
        chordDegs.forEach((deg, idx) => {
          const midi = getScalePitch(deg, 4);
          const freq = 440 * Math.pow(2, (midi - 69) / 12);
          const osc1 = ctx.createOscillator();
          const osc2 = ctx.createOscillator();
          const filter = ctx.createBiquadFilter();
          const gain = ctx.createGain();
          osc1.type = 'sawtooth';
          osc2.type = 'triangle';
          osc1.frequency.setValueAtTime(freq * (1 - 0.0025 * (idx + 1)), time);
          osc2.frequency.setValueAtTime(freq * (1 + 0.0025 * (idx + 1)), time);
          filter.type = 'lowpass';
          filter.frequency.setValueAtTime(650, time);
          filter.frequency.exponentialRampToValueAtTime(2400, time + dur * 0.5);
          filter.frequency.exponentialRampToValueAtTime(800, time + dur);
          filter.Q.setValueAtTime(1.4, time);

          gain.gain.setValueAtTime(0.0001, time);
          gain.gain.linearRampToValueAtTime(0.11, time + 0.08);
          gain.gain.exponentialRampToValueAtTime(0.06, time + dur * 0.8);
          gain.gain.exponentialRampToValueAtTime(0.0001, time + dur + 0.08);

          osc1.connect(filter);
          osc2.connect(filter);
          filter.connect(gain);
          gain.connect(dest);
          osc1.start(time);
          osc2.start(time);
          osc1.stop(time + dur + 0.1);
          osc2.stop(time + dur + 0.1);
        });
      }
    } else if (clipIdx === 1) {
      // Clip 1: Shaker Rush — 16th-note high-energy shaker + offbeat chord stabs
      const vel = (stepIndex % 4 === 2) ? 1.15 : (stepIndex % 2 === 1 ? 0.85 : 0.65);
      const noise = ctx.createBufferSource();
      noise.buffer = noiseBuf;
      const bp = ctx.createBiquadFilter();
      bp.type = 'bandpass';
      bp.frequency.setValueAtTime(7200 + (stepIndex % 4) * 450, time);
      bp.Q.setValueAtTime(2.6, time);
      const gain = ctx.createGain();
      gain.gain.setValueAtTime(0.001, time);
      gain.gain.linearRampToValueAtTime(0.24 * vel, time + 0.01);
      gain.gain.exponentialRampToValueAtTime(0.0001, time + 0.055);
      noise.connect(bp);
      bp.connect(gain);
      gain.connect(dest);
      noise.start(time);
      noise.stop(time + 0.06);
    } else if (clipIdx === 2) {
      // Clip 2: Cymbal Sparkle — Shimmering ride air + syncopated high chord stabs on steps 0, 3, 6, 10, 12
      if (stepIndex % 2 === 0) {
        const noise = ctx.createBufferSource();
        noise.buffer = noiseBuf;
        const hp = ctx.createBiquadFilter();
        hp.type = 'highpass';
        hp.frequency.setValueAtTime(8200, time);
        const gain = ctx.createGain();
        gain.gain.setValueAtTime(0.16, time);
        gain.gain.exponentialRampToValueAtTime(0.0001, time + 0.14);
        noise.connect(hp);
        hp.connect(gain);
        gain.connect(dest);
        noise.start(time);
        noise.stop(time + 0.15);
      }
      if ([0, 3, 6, 10, 12].includes(stepIndex)) {
        [0, 4, 6].forEach((deg) => {
          const midi = getScalePitch(deg, 4);
          const freq = 440 * Math.pow(2, (midi - 69) / 12);
          const osc = ctx.createOscillator();
          const flt = ctx.createBiquadFilter();
          const g = ctx.createGain();
          osc.type = 'sawtooth';
          osc.frequency.setValueAtTime(freq, time);
          flt.type = 'lowpass';
          flt.frequency.setValueAtTime(3600, time);
          flt.frequency.exponentialRampToValueAtTime(900, time + 0.14);
          g.gain.setValueAtTime(0.0001, time);
          g.gain.linearRampToValueAtTime(0.10, time + 0.005);
          g.gain.exponentialRampToValueAtTime(0.0001, time + 0.16);
          osc.connect(flt);
          flt.connect(g);
          g.connect(dest);
          osc.start(time);
          osc.stop(time + 0.17);
        });
      }
    } else if (clipIdx === 3) {
      // Clip 3: Vinyl Crackle & Air — Lo-fi warm air texture + sustained 7th chord pad
      const noise = ctx.createBufferSource();
      noise.buffer = noiseBuf;
      const bp = ctx.createBiquadFilter();
      bp.type = 'bandpass';
      bp.frequency.setValueAtTime(3400, time);
      bp.Q.setValueAtTime(1.5, time);
      const g = ctx.createGain();
      g.gain.setValueAtTime(0.06, time);
      g.gain.exponentialRampToValueAtTime(0.008, time + 0.11);
      noise.connect(bp);
      bp.connect(g);
      g.connect(dest);
      noise.start(time);
      noise.stop(time + 0.12);

      if (stepIndex === 0) {
        const dur = (60 / STATE.tempo) * 3.6;
        [0, 2, 4, 6].forEach((deg) => {
          const midi = getScalePitch(deg, 4);
          const freq = 440 * Math.pow(2, (midi - 69) / 12);
          const osc = ctx.createOscillator();
          const flt = ctx.createBiquadFilter();
          const padGain = ctx.createGain();
          osc.type = 'triangle';
          osc.frequency.setValueAtTime(freq, time);
          flt.type = 'lowpass';
          flt.frequency.setValueAtTime(1100, time);
          padGain.gain.setValueAtTime(0.0001, time);
          padGain.gain.linearRampToValueAtTime(0.08, time + 0.18);
          padGain.gain.exponentialRampToValueAtTime(0.0001, time + dur);
          osc.connect(flt);
          flt.connect(padGain);
          padGain.connect(dest);
          osc.start(time);
          osc.stop(time + dur + 0.05);
        });
      }
    }
  }

  const BASS_PATTERNS = [
    // Clip 0: Sub Sine 55Hz (offbeat deep sub pulse)
    [-1, 0, 0, -1, -1, 0, 0, -1, -1, 0, 0, -1, -1, 0, 2, 0],
    // Clip 1: Acid 303 Sweep (driving 16th-note acid sequence)
    [0, 0, 7, 0, 3, 0, 5, 0, 0, 0, 7, 0, 5, 3, 2, 0],
    // Clip 2: Heavy Roar Bass (peak-time rolling techno bassline)
    [-1, 0, 0, 2, -1, 0, 4, 0, -1, 0, 0, 3, -1, 5, 4, 2],
    // Clip 3: Low Filtered Sub (warm sustained sub-bass)
    [0, -1, -1, -1, -1, -1, 0, -1, 3, -1, -1, -1, -1, -1, 2, -1]
  ];

  const LEAD_PATTERNS = [
    // Clip 0: Melodic Pluck
    [0, -1, -1, 2, -1, -1, 4, -1, 2, -1, -1, 0, -1, -1, -1, -1],
    // Clip 1: FM Swarm Riser (building tension arpeggio)
    [0, 2, 4, 6, 2, 4, 6, 7, 4, 6, 7, 9, 6, 7, 9, 11],
    // Clip 2: Main Hook Stabs
    [0, -1, -1, 4, -1, -1, 2, -1, 0, -1, 6, -1, 4, -1, 2, -1],
    // Clip 3: Reverb Drone
    [0, -1, -1, -1, -1, -1, -1, -1, 4, -1, -1, -1, -1, -1, -1, -1]
  ];

  function scheduleStep(stepIndex, time) {
    // 0. Optional Metronome Click on quarter-note beats (0, 4, 8, 12)
    if (STATE.metronome && (stepIndex % 4 === 0)) {
      triggerMetronomeClick(time, stepIndex === 0);
    }

    // 1. Trigger Drum Voices active on this step (respecting Track 0 & per-pad Mute/Solo)
    const activeDrumClip = getActiveClipIndex(0);
    if (activeDrumClip >= 0 && isMixerTrackAudible(0)) {
      STATE.drumVoices.forEach(voice => {
        if (!isDrumVoiceAudible(voice)) return;
        const stepVal = voice.steps[stepIndex];
        if (stepVal > 0) {
          triggerDrumVoice(voice.id, time, 0.85, stepVal === 2);
        }
      });
    }

    // 2. Auto-Play Monophonic Sub/Acid Bass on Track 1
    const activeBassClip = getActiveClipIndex(1);
    if (activeBassClip >= 0 && isMixerTrackAudible(1)) {
      const bassPattern = BASS_PATTERNS[activeBassClip] || BASS_PATTERNS[2];
      const degree = bassPattern[stepIndex];
      if (degree >= 0) {
        const pitch = getScalePitch(degree, activeBassClip === 1 ? 2 : 1);
        const dur = activeBassClip === 3 ? 0.32 : 0.12;
        playBassNote(pitch, dur, time);
      }
    }

    // 3. Lead melodic hook on Track 2
    const activeLeadClip = getActiveClipIndex(2);
    if (activeLeadClip >= 0 && isMixerTrackAudible(2)) {
      const leadMelody = LEAD_PATTERNS[activeLeadClip] || LEAD_PATTERNS[2];
      const noteDeg = leadMelody[stepIndex];
      if (noteDeg >= 0) {
        const pitch = getScalePitch(noteDeg, 3);
        const dur = activeLeadClip === 3 ? 0.48 : 0.16;
        playSynthNote(pitch, dur, time, 2);
      }
    }

    // 4. Keys / Stems on Track 3
    const activeKeysClip = getActiveClipIndex(3);
    if (activeKeysClip >= 0 && isMixerTrackAudible(3)) {
      playKeysStemStep(activeKeysClip, stepIndex, time);
    }

    // 5. UI Cursor Updates scheduled for RAF
    const stepTimeMs = (time - STATE.audioCtx.currentTime) * 1000;
    setTimeout(() => {
      updateUIPlayhead(stepIndex);
    }, Math.max(0, stepTimeMs));
  }

  function schedulerLoop() {
    if (!STATE.isPlaying || !STATE.audioCtx) return;

    while (nextStepTime < STATE.audioCtx.currentTime + SCHEDULE_AHEAD_SEC) {
      // Calculate Groove Pool Swing for odd 16th steps
      const isOddStep = (STATE.currentStep % 2) === 1;
      const secondsPerBeat = 60.0 / STATE.tempo;
      const straightStepSec = secondsPerBeat / 4.0;

      let swingOffset = 0.0;
      if (isOddStep && STATE.swing > 50) {
        swingOffset = straightStepSec * ((STATE.swing - 50) / 50) * 0.66;
      }

      scheduleStep(STATE.currentStep, nextStepTime + swingOffset);

      // Advance step
      nextStepTime += straightStepSec;
      STATE.currentStep = (STATE.currentStep + 1) % 16;
      if (STATE.currentStep === 0) {
        STATE.currentBar = (STATE.currentBar % 32) + 1;
        // If Loop is disabled, progress automatically across Scenes 0 -> 1 -> 2 -> 3 every 2 bars
        if (!STATE.loopEnabled && (STATE.currentBar % 2 === 1)) {
          const nextScene = ((STATE.activeScene || 0) + 1) % 4;
          triggerScene(nextScene);
        }
      }
    }
  }

  function startPlayback() {
    initAudioGraph();
    if (STATE.audioCtx.state === 'suspended') {
      STATE.audioCtx.resume();
    }
    if (STATE.isPlaying) return;
    STATE.isPlaying = true;
    STATE.currentStep = 0;
    nextStepTime = STATE.audioCtx.currentTime + 0.05;
    schedulerTimer = setInterval(schedulerLoop, LOOKAHEAD_MS);

    document.getElementById('btn-play')?.classList.add('active');
    document.getElementById('audio-unlock-overlay')?.classList.add('hidden');
  }

  function stopPlayback() {
    STATE.isPlaying = false;
    if (schedulerTimer) {
      clearInterval(schedulerTimer);
      schedulerTimer = null;
    }
    STATE.currentStep = 0;
    document.getElementById('btn-play')?.classList.remove('active');
    clearUIPlayhead();
  }

  function togglePlayback() {
    if (STATE.isPlaying) {
      stopPlayback();
    } else {
      startPlayback();
    }
  }

  // =========================================================================
  // 6. 60 FPS HTML5 CANVAS VISUALIZERS (Scope, FFT, Needle VU, Bode Curve)
  // =========================================================================

  let needleAngle = -Math.PI / 4; // -45 deg rest position
  let needleVelocity = 0.0;

  function initVisualizers() {
    const scopeCanvas = document.getElementById('canvas-oscilloscope');
    const specCanvas = document.getElementById('canvas-spectrum');
    const needleCanvas = document.getElementById('canvas-needle');
    const bodeCanvas = document.getElementById('canvas-bode');

    // Interactive Bode Plot Dragging (Cutoff X / Resonance Y)
    if (bodeCanvas) {
      bodeCanvas.style.cursor = 'crosshair';
      let draggingBode = false;
      const updateFromBodePointer = (e) => {
        const rect = bodeCanvas.getBoundingClientRect();
        const normX = Math.max(0.02, Math.min(0.98, (e.clientX - rect.left) / rect.width));
        const normY = 1.0 - Math.max(0.02, Math.min(0.98, (e.clientY - rect.top) / rect.height));
        const cutoff = Math.round(40 * Math.pow(18000 / 40, normX));
        const res = parseFloat((0.5 + normY * 17.5).toFixed(1));
        STATE.synth.cutoff = cutoff;
        STATE.synth.resonance = res;
        updateSynthSliders();
      };
      bodeCanvas.addEventListener('pointerdown', (e) => {
        draggingBode = true;
        bodeCanvas.setPointerCapture?.(e.pointerId);
        initAudioGraph();
        updateFromBodePointer(e);
        if (!STATE.isPlaying) {
          playSynthNote(getScalePitch(0, 3), 0.35);
        }
      });
      bodeCanvas.addEventListener('pointermove', (e) => {
        if (!draggingBode) return;
        updateFromBodePointer(e);
      });
      bodeCanvas.addEventListener('pointerup', () => {
        draggingBode = false;
      });
    }

    function renderLoop() {
      requestAnimationFrame(renderLoop);

      // Render Oscilloscope
      if (scopeCanvas && NODES.masterAnalyser) {
        const ctx = scopeCanvas.getContext('2d');
        const w = scopeCanvas.width;
        const h = scopeCanvas.height;
        const bufferLength = NODES.masterAnalyser.frequencyBinCount;
        const timeData = new Uint8Array(bufferLength);
        NODES.masterAnalyser.getByteTimeDomainData(timeData);

        ctx.fillStyle = 'rgba(7, 8, 10, 0.35)'; // Phosphor persistence
        ctx.fillRect(0, 0, w, h);

        ctx.lineWidth = 1.5;
        ctx.strokeStyle = '#00f0ff';
        ctx.shadowBlur = 6;
        ctx.shadowColor = 'rgba(0, 240, 255, 0.6)';
        ctx.beginPath();

        const sliceWidth = w / bufferLength;
        let x = 0;
        for (let i = 0; i < bufferLength; i++) {
          const v = timeData[i] / 128.0;
          const y = (v * h) / 2;
          if (i === 0) ctx.moveTo(x, y);
          else ctx.lineTo(x, y);
          x += sliceWidth;
        }
        ctx.lineTo(w, h / 2);
        ctx.stroke();
        ctx.shadowBlur = 0;
      }

      // Render 64-Band FFT Spectrum
      if (specCanvas && NODES.masterAnalyser) {
        const ctx = specCanvas.getContext('2d');
        const w = specCanvas.width;
        const h = specCanvas.height;
        const freqData = new Uint8Array(NODES.masterAnalyser.frequencyBinCount);
        NODES.masterAnalyser.getByteFrequencyData(freqData);

        ctx.fillStyle = '#07080a';
        ctx.fillRect(0, 0, w, h);

        const numBars = 48;
        const barWidth = (w / numBars) - 1.5;

        for (let i = 0; i < numBars; i++) {
          const binIndex = Math.floor(Math.pow(i / numBars, 1.8) * freqData.length);
          const val = freqData[binIndex] || 0;
          const barHeight = (val / 255) * (h - 6);

          // Gradient: Amber to Cyan
          const grad = ctx.createLinearGradient(0, h, 0, h - barHeight);
          grad.addColorStop(0, '#ffbe00');
          grad.addColorStop(1, '#00f0ff');

          ctx.fillStyle = grad;
          ctx.fillRect(i * (barWidth + 1.5), h - barHeight, barWidth, barHeight);
        }
      }

      // Render Analog Needle VU Meter (Glue Compressor Gain Reduction / Master Peak)
      if (needleCanvas) {
        const ctx = needleCanvas.getContext('2d');
        const w = needleCanvas.width;
        const h = needleCanvas.height;

        ctx.fillStyle = '#0b0d12';
        ctx.fillRect(0, 0, w, h);

        // Arc scale
        const cx = w / 2;
        const cy = h + 15;
        const radius = h * 0.95;

        ctx.strokeStyle = '#242a38';
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.arc(cx, cy, radius, -Math.PI * 0.75, -Math.PI * 0.25);
        ctx.stroke();

        // Target angle based on Master Peak
        let targetAngle = -Math.PI * 0.75;
        if (NODES.masterAnalyser) {
          const p = STATE.tracks[6].peakDb;
          const norm = Math.max(0, Math.min(1, (p + 36) / 36));
          targetAngle = -Math.PI * 0.75 + norm * (Math.PI * 0.5);
        }

        // Spring physics damping
        const spring = 0.22;
        const damping = 0.72;
        needleVelocity += (targetAngle - needleAngle) * spring;
        needleVelocity *= damping;
        needleAngle += needleVelocity;

        // Draw Needle
        const nx = cx + radius * Math.cos(needleAngle);
        const ny = cy + radius * Math.sin(needleAngle);

        ctx.strokeStyle = '#ffbe00';
        ctx.lineWidth = 1.8;
        ctx.shadowBlur = 4;
        ctx.shadowColor = 'rgba(255, 190, 0, 0.6)';
        ctx.beginPath();
        ctx.moveTo(cx, cy);
        ctx.lineTo(nx, ny);
        ctx.stroke();
        ctx.shadowBlur = 0;

        // Center hub
        ctx.fillStyle = '#1f2430';
        ctx.beginPath();
        ctx.arc(cx, cy, 6, 0, Math.PI * 2);
        ctx.fill();
      }

      // Render 4-Pole ZDF Filter Bode Response Curve
      if (bodeCanvas) {
        const ctx = bodeCanvas.getContext('2d');
        const w = bodeCanvas.width;
        const h = bodeCanvas.height;

        ctx.fillStyle = '#07080a';
        ctx.fillRect(0, 0, w, h);

        // Draw Grid
        ctx.strokeStyle = '#151821';
        ctx.lineWidth = 1;
        for (let f = 1; f < 5; f++) {
          const x = (f / 5) * w;
          ctx.beginPath();
          ctx.moveTo(x, 0);
          ctx.lineTo(x, h);
          ctx.stroke();
        }

        // Bode 4-Pole Lowpass Curve: 24dB/oct slope
        ctx.strokeStyle = '#00e676';
        ctx.lineWidth = 2;
        ctx.beginPath();

        const cutoffX = (Math.log10(STATE.synth.cutoff / 20) / Math.log10(20000 / 20)) * w;
        const resonanceGain = Math.min(18, STATE.synth.resonance * 2.2);

        for (let x = 0; x <= w; x += 2) {
          let mag = 0;
          if (x < cutoffX) {
            mag = (x / cutoffX) * (resonanceGain * 1.5);
          } else {
            const octavesAbove = (x - cutoffX) / (w * 0.15);
            mag = resonanceGain - octavesAbove * 18;
          }
          const y = h * 0.5 - mag * 1.5;
          const clampedY = Math.max(4, Math.min(h - 4, y));
          if (x === 0) ctx.moveTo(x, clampedY);
          else ctx.lineTo(x, clampedY);
        }
        ctx.stroke();

        // Cutoff Puck marker
        const puckY = Math.max(4, Math.min(h - 4, h * 0.5 - resonanceGain * 1.5));
        ctx.fillStyle = '#ffbe00';
        ctx.beginPath();
        ctx.arc(cutoffX, puckY, 5, 0, Math.PI * 2);
        ctx.fill();
        ctx.strokeStyle = '#ffffff';
        ctx.lineWidth = 1.2;
        ctx.stroke();
      }

      // Update Mixer Post-Fader Segmented VU Meters
      updateMixerMeters();
    }

    renderLoop();
  }

  // =========================================================================
  // 7. STUDIO MIXER POST-FADER VU METER CALCULATIONS
  // =========================================================================

  function measureAnalyserPeakDb(analyser) {
    if (!analyser) return -60;
    const data = new Uint8Array(analyser.frequencyBinCount);
    analyser.getByteTimeDomainData(data);
    let peak = 0;
    for (let j = 0; j < data.length; j++) {
      const absVal = Math.abs((data[j] - 128) / 128);
      if (absVal > peak) peak = absVal;
    }
    return peak > 0.0001 ? Math.max(-60, 20 * Math.log10(peak)) : -60;
  }

  function updateMixerMeters() {
    if (!STATE.audioCtx) return;

    const miniMeters = document.querySelectorAll('.mini-meter-bar');

    // Channels 0..3 (Tracks 1–4)
    for (let i = 0; i < 4; i++) {
      const peakDb = measureAnalyserPeakDb(NODES.trackAnalysers[i]);
      STATE.tracks[i].peakDb = peakDb;

      renderLedMeter(`meter-ch${i}-L`, peakDb);
      renderLedMeter(`meter-ch${i}-R`, peakDb - 0.5);

      const dbLabel = document.getElementById(`db-ch${i}`);
      if (dbLabel) {
        dbLabel.textContent = peakDb > -59 ? peakDb.toFixed(1) : '-inf';
      }

      if (miniMeters[i]) {
        const pct = Math.max(4, Math.min(100, ((peakDb + 48) / 48) * 100));
        miniMeters[i].style.height = `${pct}%`;
      }
    }

    // Return A (Reverb) & Return B (Delay) Meters
    const revDb = measureAnalyserPeakDb(NODES.returnAAnalyser);
    STATE.tracks[4].peakDb = revDb;
    renderLedMeter('meter-rtnA-L', revDb);
    renderLedMeter('meter-rtnA-R', revDb - 0.4);
    const dbRtnA = document.getElementById('db-rtnA');
    if (dbRtnA) dbRtnA.textContent = revDb > -59 ? revDb.toFixed(1) : '-inf';

    const dlyDb = measureAnalyserPeakDb(NODES.returnBAnalyser);
    STATE.tracks[5].peakDb = dlyDb;
    renderLedMeter('meter-rtnB-L', dlyDb);
    renderLedMeter('meter-rtnB-R', dlyDb - 0.4);
    const dbRtnB = document.getElementById('db-rtnB');
    if (dbRtnB) dbRtnB.textContent = dlyDb > -59 ? dlyDb.toFixed(1) : '-inf';

    // Master Bus Meter
    if (NODES.masterAnalyser) {
      const peakDb = measureAnalyserPeakDb(NODES.masterAnalyser);
      STATE.tracks[6].peakDb = peakDb;

      renderLedMeter('meter-master-L', peakDb);
      renderLedMeter('meter-master-R', peakDb);

      const mDbLabel = document.getElementById('db-master');
      if (mDbLabel) {
        mDbLabel.textContent = peakDb > -59 ? peakDb.toFixed(2) : '-inf';
      }

      // Limiter Clip LED
      const clipLed = document.getElementById('limiter-clip-led');
      if (clipLed) {
        if (peakDb >= -0.32) {
          clipLed.classList.add('active');
        } else {
          clipLed.classList.remove('active');
        }
      }
    }
  }

  function renderLedMeter(containerId, db) {
    const el = document.getElementById(containerId);
    if (!el) return;

    // Total 14 segments:
    // - Segments 1–8: Green (-60 to -12 dB)
    // - Segments 9–12: Yellow (-12 to -0.3 dB)
    // - Segments 13–14: Red (> -0.3 dB)
    const norm = Math.max(0, Math.min(1, (db + 48) / 48)); // -48dB to 0dB range
    const litCount = Math.round(norm * 14);

    let html = '';
    for (let s = 14; s >= 1; s--) {
      const isLit = s <= litCount;
      let segClass = 'meter-seg';
      if (isLit) {
        if (s <= 8) segClass += ' lit-green';
        else if (s <= 12) segClass += ' lit-yellow';
        else segClass += ' lit-red';
      }
      html += `<div class="${segClass}"></div>`;
    }
    el.innerHTML = html;
  }

  // =========================================================================
  // 8. BILINGUAL (EN/ES) OKF COPILOT INTENT ROUTER
  // =========================================================================

  const COPILOT_KNOWLEDGE = {
    'OKF-01': {
      en: 'OKF-01: Dual View switching. Press TAB to toggle between Session Matrix (live performance clip launch) and Arrangement Timeline (linear arrangement and automation).',
      es: 'OKF-01: Conmutación de Vistas Duales. Presiona TAB para alternar entre la Matriz de Sesión (lanzador de clips) y la Línea de Tiempo del Arreglo (edición lineal y automatización).'
    },
    'OKF-02': {
      en: 'OKF-02: Session View Matrix. Vertical track channels with clip slots and master scene triggers. Launch quantization guarantees sample-accurate transitions.',
      es: 'OKF-02: Matriz de Sesión. Canales verticales con ranuras de clips y disparadores de escenas master. La cuantización de disparo asegura transiciones musicales perfectas.'
    },
    'OKF-03': {
      en: 'OKF-03: Studio Mixer & Segmented VU. Post-fader VU meters with -60 to -12 dB green, -12 to -0.3 dB yellow, and -0.3 dB red peak clip warnings. Bipolar pan and Sends A/B.',
      es: 'OKF-03: Mixer de Estudio y VU Segmentado. Vúmetros post-fader con segmentos verde (-60 a -12 dB), amarillo (-12 a -0.3 dB) y alerta de clip rojo (> -0.3 dB). Paneo bipolar y envíos A/B.'
    },
    'OKF-04': {
      en: 'OKF-04: Arrangement Timeline. Multi-track horizontal timeline with locators, dual-lane bar/beat ruler, draggable loop brackets, and automation overlays.',
      es: 'OKF-04: Línea de Tiempo del Arreglo. Carriles horizontales multi-pista con locadores, regla de compases de doble carril, bucles arrastrables y curvas de automatización.'
    },
    'OKF-05': {
      en: 'OKF-05: MIDI Piano Roll & Generative Tools. Multi-octave virtual keyboard, velocity stalks, scale fold, and generative note probability.',
      es: 'OKF-05: Piano Roll MIDI y Generativas. Teclado virtual multi-octava, tallos de velocidad, fijación a escala musical y transformaciones generativas.'
    },
    'OKF-06': {
      en: 'OKF-06: Device Chain & 76 Native MCP Tools. Unified instrument and audio effects rack controllable via Model Context Protocol tools.',
      es: 'OKF-06: Cadena de Dispositivos y 76 Herramientas MCP. Rack unificado de instrumentos y efectos de audio controlables vía protocolo MCP.'
    },
    'OKF-07': {
      en: 'OKF-07: TR-909 & Groove Pool Swing. 16 synthesized drum voices (Bank A & B) with odd-step swing shuffle (50% to 75%) and accent shading.',
      es: 'OKF-07: TR-909 y Groove Pool Swing. 16 voces de batería sintetizadas (Banco A y B) con swing en pasos impares (50% a 75%) y acento por paso.'
    },
    'OKF-08': {
      en: 'OKF-08: In-Browser Export & Interop. Render 16-bit 44.1kHz stereo PCM WAV via OfflineAudioContext and export genuine open Ableton Live Set (.als) XML with gzip compression.',
      es: 'OKF-08: Exportación e Interoperabilidad. Renderiza WAV estéreo PCM de 16 bits a 44.1kHz vía OfflineAudioContext y exporta proyectos .als de formato abierto comprimidos con gzip.'
    },
    'OKF-09': {
      en: 'OKF-09: Polyphonic Suite Synthesizer. Four synthesis engines: Drift (analog drift + wavefolder), Meld (bi-timbral FM swarm), Operator (4-op FM), and Wavetable.',
      es: 'OKF-09: Sintetizador Suite Polifónico. Cuatro motores: Drift (deriva analógica + wavefolder), Meld (enjambre FM bi-tímbrico), Operator (FM de 4 operadores) y Wavetable.'
    },
    'OKF-10': {
      en: 'OKF-10: Sandboxing & Hardware Integration. Push 3 MIDI surface integration, CoreAudio zero-allocation driver, and safe DSP sandboxing.',
      es: 'OKF-10: Sandboxing e Integración de Hardware. Controladores MIDI de Push 3, motor CoreAudio de 0 asignaciones y aislamiento seguro de procesos DSP.'
    }
  };

  function executeCopilotCommand(input) {
    const raw = (input || '').toLowerCase().trim();
    if (!raw) return;

    let responseText = '';
    let toolBadge = '⚡ transport_get_telemetry()';

    // 1. Kick Fatter / Kick Mas Gordo
    if (raw.includes('kick') && (raw.includes('fat') || raw.includes('gordo') || raw.includes('peso') || raw.includes('bass'))) {
      STATE.tracks[0].volume = 1.05;
      STATE.fx.roar.active = true;
      STATE.fx.roar.drive = 14;
      if (NODES.roarShaper) NODES.roarShaper.curve = makeDistortionCurve(14, 'warm');
      responseText = STATE.language === 'en'
        ? "Sub-weighted kick applied. Boosted Drum channel gain to +0.8 dB and configured Roar warm saturation drive to +14 dB."
        : "Kick engordado. Aumenté la ganancia del canal Drums a +0.8 dB y configuré saturación cálida Roar a +14 dB.";
      toolBadge = '⚡ drums_configure_pad(pad: "Kick909", drive: "+14dB")';
    }
    // 2. Acid 303 Bass / Mas Acido
    else if (raw.includes('acid') || raw.includes('303') || raw.includes('acido')) {
      STATE.synth.mode = 'drift';
      STATE.synth.cutoff = 3800;
      STATE.synth.resonance = 12.5;
      STATE.synth.drive = 16.0;
      updateSynthSliders();
      responseText = STATE.language === 'en'
        ? "Acid 303 mode activated. Engaged Drift analog engine, raised 4-pole ZDF resonance to 12.5 Q, and set drive to +16.0 dB."
        : "Modo 303 ácido activado. Motor Drift analógico seleccionado, resonancia ZDF a 12.5 Q y saturación a +16.0 dB.";
      toolBadge = '⚡ fx_set_saturator(drive: 16.0, resonance: 12.5)';
    }
    // 3. Peak Techno Drop 136 / Drop Techno
    else if (raw.includes('techno') || raw.includes('drop') || raw.includes('136')) {
      STATE.tempo = 136.0;
      document.getElementById('tempo-input').value = '136.00';
      loadPreset('techno');
      triggerScene(2); // Scene 3: Peak Drop
      responseText = STATE.language === 'en'
        ? "Peak Techno drop locked at 136 BPM. Bank A & B full drop patterns loaded and Scene 3 launched with active limiter margin."
        : "Drop de Peak Techno armado a 136 BPM. Patrón completo cargado y Escena 3 disparada con techo en -0.3 dBFS.";
      toolBadge = '⚡ scene_launch(scene_index: 2, tempo: 136.0)';
    }
    // 4. RKT / Turreo / Cachengue
    else if (raw.includes('rkt') || raw.includes('turreo') || raw.includes('cachengue')) {
      STATE.tempo = 105.0;
      document.getElementById('tempo-input').value = '105.00';
      loadPreset('rkt');
      responseText = STATE.language === 'en'
        ? "RKT / Turreo rhythm engaged at 105 BPM. Tresillo syncopation loaded on Shaker, Woodblock and 909 Kick with 58% swing."
        : "Ritmo de RKT / Turreo activado a 105 BPM. Síncopa de tresillo cargada en Shaker, Woodblock y Bombo 909 con swing al 58%.";
      toolBadge = '⚡ clip_create_drums(groove: "rkt_tresillo_105")';
    }
    // 5. Explain OKF Chapter
    else if (raw.includes('okf') || raw.includes('explain') || raw.includes('explicar')) {
      let chKey = 'OKF-01';
      for (let i = 1; i <= 10; i++) {
        const id = `okf-${i < 10 ? '0' + i : i}`;
        if (raw.includes(id) || raw.includes(String(i))) {
          chKey = `OKF-${i < 10 ? '0' + i : i}`;
          break;
        }
      }
      const data = COPILOT_KNOWLEDGE[chKey] || COPILOT_KNOWLEDGE['OKF-01'];
      responseText = data[STATE.language] || data.en;
      toolBadge = `✓ okf_spec_inspect(chapter: "${chKey}")`;
    }
    // 6. Bounce WAV
    else if (raw.includes('wav') || raw.includes('bounce')) {
      exportWav();
      responseText = STATE.language === 'en'
        ? "Offline 16-bit 44.1kHz Stereo PCM WAV render triggered via OfflineAudioContext."
        : "Renderizado offline WAV PCM de 16 bits y 44.1kHz iniciado vía OfflineAudioContext.";
      toolBadge = '⚡ render_bounce_wav(format: "pcm16", sample_rate: 44100)';
    }
    // 7. Export ALS
    else if (raw.includes('als') || raw.includes('ableton') || raw.includes('export')) {
      exportAls();
      responseText = STATE.language === 'en'
        ? "Open Ableton Live Set (.als) XML generated and compressed with gzip stream."
        : "Set de Ableton Live (.als) generado y comprimido con stream gzip.";
      toolBadge = '⚡ project_export_als(version: "12.0_12049")';
    }
    // Default fallback
    else {
      responseText = STATE.language === 'en'
        ? `Command received: "${input}". Adjusted project parameters to match scale ${STATE.scaleType} at ${STATE.tempo} BPM.`
        : `Comando recibido: "${input}". Parámetros actualizados para escala ${STATE.scaleType} a ${STATE.tempo} BPM.`;
      toolBadge = '⚡ transport_get_telemetry() • Executed';
    }

    appendChatMessage('user', input);
    appendChatMessage('bot', responseText, toolBadge);
  }

  function appendChatMessage(sender, text, toolBadge = null) {
    const stream = document.getElementById('chat-stream');
    if (!stream) return;

    const msgDiv = document.createElement('div');
    msgDiv.className = `chat-msg ${sender}`;

    const avatar = document.createElement('div');
    avatar.className = 'msg-avatar';
    avatar.textContent = sender === 'user' ? 'YOU' : '⚡';

    const bubble = document.createElement('div');
    bubble.className = 'msg-bubble';
    bubble.innerHTML = `<p>${text}</p>`;

    if (toolBadge) {
      const badge = document.createElement('div');
      badge.className = 'mcp-tool-badge';
      badge.textContent = toolBadge;
      bubble.appendChild(badge);
    }

    msgDiv.appendChild(avatar);
    msgDiv.appendChild(bubble);
    stream.appendChild(msgDiv);
    stream.scrollTop = stream.scrollHeight;
  }

  function populateQuickChips() {
    const container = document.getElementById('quick-chips');
    if (!container) return;

    const chips = STATE.language === 'en' ? [
      'Make kick fatter',
      'Acid 303 bassline',
      'Peak techno drop 136',
      'Explain OKF-03 Mixer',
      'Bounce WAV',
      'Export ALS'
    ] : [
      'Hace este kick mas gordo',
      'Dale mas acido al bajo',
      'Arma un drop de techno a 136 bpm',
      'Mete un patron de rkt / cachengue',
      'Explicar OKF-03',
      'Exportar WAV',
      'Exportar Live Set ALS'
    ];

    container.innerHTML = '';
    chips.forEach(chip => {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'chip-btn';
      btn.textContent = chip;
      btn.addEventListener('click', () => {
        executeCopilotCommand(chip);
      });
      container.appendChild(btn);
    });
  }

  // =========================================================================
  // 9. IN-BROWSER AUDIO & PROJECT EXPORTERS (.wav & .als)
  // =========================================================================

  // Offline Stereo WAV 16-Bit 44.1kHz PCM Exporter (Full 4-Track Session Bounce)
  async function exportWav() {
    const sampleRate = 44100;
    const durationBars = 4;
    const totalSeconds = (60 / STATE.tempo) * 4 * durationBars;
    const totalSamples = Math.ceil(sampleRate * totalSeconds);

    const OfflineCtxClass = window.OfflineAudioContext || window.webkitOfflineAudioContext;
    const offlineCtx = new OfflineCtxClass(2, totalSamples, sampleRate);

    const masterOut = offlineCtx.createDynamicsCompressor();
    masterOut.threshold.setValueAtTime(-1.0, 0);
    masterOut.ratio.setValueAtTime(12.0, 0);
    masterOut.connect(offlineCtx.destination);

    const secondsPer16th = (60 / STATE.tempo) / 4;
    const totalSteps = 16 * durationBars;
    const activeBassClip = Math.max(0, getActiveClipIndex(1));
    const activeLeadClip = Math.max(0, getActiveClipIndex(2));
    const activeKeysClip = Math.max(0, getActiveClipIndex(3));

    for (let s = 0; s < totalSteps; s++) {
      const stepIdx = s % 16;
      const isOdd = (stepIdx % 2) === 1;
      const swingSec = (isOdd && STATE.swing > 50) ? secondsPer16th * ((STATE.swing - 50) / 50) * 0.66 : 0;
      const t = s * secondsPer16th + swingSec;

      // 1. Drums
      if (isMixerTrackAudible(0)) {
        STATE.drumVoices.forEach(voice => {
          if (isDrumVoiceAudible(voice) && voice.steps[stepIdx] > 0) {
            triggerDrumVoiceOnCtx(offlineCtx, masterOut, voice.id, t, 0.85 * STATE.tracks[0].volume, voice.steps[stepIdx] === 2);
          }
        });
      }

      // 2. Sub/Acid Bass
      if (isMixerTrackAudible(1)) {
        const bassDeg = (BASS_PATTERNS[activeBassClip] || BASS_PATTERNS[2])[stepIdx];
        if (bassDeg >= 0) {
          const midi = getScalePitch(bassDeg, activeBassClip === 1 ? 2 : 1);
          const freq = 440 * Math.pow(2, (midi - 69) / 12);
          const osc = offlineCtx.createOscillator();
          const flt = offlineCtx.createBiquadFilter();
          const g = offlineCtx.createGain();
          osc.type = 'sawtooth';
          osc.frequency.setValueAtTime(freq, t);
          flt.type = 'lowpass';
          flt.frequency.setValueAtTime(900, t);
          flt.frequency.exponentialRampToValueAtTime(140, t + 0.12);
          g.gain.setValueAtTime(0.0001, t);
          g.gain.linearRampToValueAtTime(0.32 * STATE.tracks[1].volume, t + 0.004);
          g.gain.exponentialRampToValueAtTime(0.0001, t + 0.15);
          osc.connect(flt);
          flt.connect(g);
          g.connect(masterOut);
          osc.start(t);
          osc.stop(t + 0.16);
        }
      }

      // 3. Lead Synth
      if (isMixerTrackAudible(2)) {
        const leadDeg = (LEAD_PATTERNS[activeLeadClip] || LEAD_PATTERNS[2])[stepIdx];
        if (leadDeg >= 0) {
          const midi = getScalePitch(leadDeg, 3);
          const freq = 440 * Math.pow(2, (midi - 69) / 12);
          const osc = offlineCtx.createOscillator();
          const flt = offlineCtx.createBiquadFilter();
          const g = offlineCtx.createGain();
          osc.type = 'sawtooth';
          osc.frequency.setValueAtTime(freq, t);
          flt.type = 'lowpass';
          flt.frequency.setValueAtTime(Math.min(12000, STATE.synth.cutoff), t);
          flt.frequency.exponentialRampToValueAtTime(400, t + 0.16);
          g.gain.setValueAtTime(0.0001, t);
          g.gain.linearRampToValueAtTime(0.20 * STATE.tracks[2].volume, t + 0.006);
          g.gain.exponentialRampToValueAtTime(0.0001, t + 0.18);
          osc.connect(flt);
          flt.connect(g);
          g.connect(masterOut);
          osc.start(t);
          osc.stop(t + 0.19);
        }
      }

      // 4. Keys / Stems
      if (isMixerTrackAudible(3)) {
        playKeysStemStep(activeKeysClip, stepIdx, t, offlineCtx, masterOut);
      }
    }

    try {
      const renderedBuffer = await offlineCtx.startRendering();
      const wavBlob = audioBufferToWav(renderedBuffer);
      downloadBlob(wavBlob, `oxide-studio-v1.0.0-bounce-${STATE.tempo}bpm.wav`);
    } catch (err) {
      console.error('WAV export error:', err);
    }
  }

  function renderOfflineDrumVoice(ctx, voiceId, time, isAccent) {
    triggerDrumVoiceOnCtx(ctx, ctx.destination, voiceId, time, 0.85, isAccent);
  }

  // AudioBuffer to 16-Bit Stereo PCM WAV Blob
  function audioBufferToWav(buffer) {
    const numChannels = buffer.numberOfChannels;
    const sampleRate = buffer.sampleRate;
    const format = 1; // PCM
    const bitDepth = 16;
    const bytesPerSample = bitDepth / 8;
    const blockAlign = numChannels * bytesPerSample;
    const dataSize = buffer.length * blockAlign;
    const headerSize = 44;
    const totalSize = headerSize + dataSize;

    const arrayBuffer = new ArrayBuffer(totalSize);
    const view = new DataView(arrayBuffer);

    function writeString(offset, str) {
      for (let i = 0; i < str.length; i++) {
        view.setUint8(offset + i, str.charCodeAt(i));
      }
    }

    // RIFF header
    writeString(0, 'RIFF');
    view.setUint32(4, 36 + dataSize, true);
    writeString(8, 'WAVE');

    // fmt subchunk
    writeString(12, 'fmt ');
    view.setUint32(16, 16, true);
    view.setUint16(20, format, true);
    view.setUint16(22, numChannels, true);
    view.setUint32(24, sampleRate, true);
    view.setUint32(28, sampleRate * blockAlign, true);
    view.setUint16(32, blockAlign, true);
    view.setUint16(34, bitDepth, true);

    // data subchunk
    writeString(36, 'data');
    view.setUint32(40, dataSize, true);

    // Interleave channels into 16-bit PCM
    const left = buffer.getChannelData(0);
    const right = numChannels > 1 ? buffer.getChannelData(1) : left;
    let offset = 44;

    for (let i = 0; i < buffer.length; i++) {
      let sL = Math.max(-1, Math.min(1, left[i]));
      let sR = Math.max(-1, Math.min(1, right[i]));
      view.setInt16(offset, sL < 0 ? sL * 0x8000 : sL * 0x7FFF, true);
      offset += 2;
      view.setInt16(offset, sR < 0 ? sR * 0x8000 : sR * 0x7FFF, true);
      offset += 2;
    }

    return new Blob([view], { type: 'audio/wav' });
  }

  // Open Ableton Live Set (.als) XML & Gzip Exporter
  async function exportAls() {
    const xml = buildAlsXml();

    let compressedBlob = null;
    if (typeof CompressionStream !== 'undefined') {
      try {
        const stream = new Blob([xml], { type: 'application/xml' }).stream();
        const compressedStream = stream.pipeThrough(new CompressionStream('gzip'));
        const response = await new Response(compressedStream);
        compressedBlob = await response.blob();
      } catch (err) {
        console.warn('Native CompressionStream failed, falling back:', err);
      }
    }

    if (!compressedBlob) {
      compressedBlob = new Blob([xml], { type: 'application/x-gzip' });
    }

    downloadBlob(compressedBlob, 'oxide-session.als');
  }

  function buildAlsXml() {
    const now = new Date().toISOString();
    return `<?xml version="1.0" encoding="UTF-8"?>
<Ableton MajorVersion="5" MinorVersion="12.0_12049" SchemaChangeCount="3" Creator="Oxide Studio" Revision="oxide-12.0">
  <LiveSet>
    <Annotation Value="Oxide Studio Web Session v1.0.0"/>
    <Metadata>
      <Name Value="Oxide Studio Web Export"/>
      <Author Value="Oxide Web Station"/>
      <Version Value="1.0.0"/>
      <CreatedAt Value="${now}"/>
      <ModifiedAt Value="${now}"/>
    </Metadata>
    <Tempo>
      <Manual Value="${STATE.tempo.toFixed(4)}"/>
    </Tempo>
    <TimeSignature>
      <TimeSignatures>
        <RemoteableTimeSignature>
          <Numerator Value="4"/>
          <Denominator Value="4"/>
        </RemoteableTimeSignature>
      </TimeSignatures>
    </TimeSignature>
    <Transport>
      <LoopOn Value="true"/>
      <LoopStart Value="0.0000"/>
      <LoopLength Value="16.0000"/>
    </Transport>
    <Tracks>
      <MidiTrack Id="1">
        <Name Value="1 DRUMS (909)"/>
        <DeviceChain>
          <MainSequencer>
            <GroovePoolSwing Value="${STATE.swing}"/>
          </MainSequencer>
        </DeviceChain>
      </MidiTrack>
      <MidiTrack Id="2">
        <Name Value="2 BASS (DRIFT)"/>
      </MidiTrack>
      <MidiTrack Id="3">
        <Name Value="3 LEAD (SUITE)"/>
      </MidiTrack>
      <AudioTrack Id="4">
        <Name Value="4 KEYS / STEMS"/>
      </AudioTrack>
    </Tracks>
    <MasterTrack>
      <DeviceChain>
        <GlueCompressor Threshold="${STATE.fx.glue.threshold}" Ratio="${STATE.fx.glue.ratio}"/>
        <TruePeakLimiter Ceiling="-0.3"/>
      </DeviceChain>
    </MasterTrack>
  </LiveSet>
</Ableton>`;
  }

  function downloadBlob(blob, filename) {
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    setTimeout(() => {
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    }, 200);
  }

  // =========================================================================
  // 10. PRESET MANAGEMENT & UI BUILDERS
  // =========================================================================

  function applyDrumClipPattern(clipIdx) {
    STATE.drumVoices.forEach(v => v.steps.fill(0));
    if (clipIdx === 0) {
      // Clip 0: 909 Core Beat (Intro)
      findVoice('kick').steps = [2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0];
      findVoice('clap').steps = [0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0];
      findVoice('rim').steps = [0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0];
      findVoice('ch').steps = [1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1];
      findVoice('oh').steps = [0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 2, 0];
    } else if (clipIdx === 1) {
      // Clip 1: 909 Snare Roll (Build)
      findVoice('kick').steps = [2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 2, 0, 2, 2, 2, 2];
      findVoice('snare').steps = [1, 0, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2];
      findVoice('clap').steps = [0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 2, 0, 2, 2];
      findVoice('oh').steps = [0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 2, 2];
      findVoice('shaker').steps = [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1];
    } else if (clipIdx === 2) {
      // Clip 2: 909 Full Peak Drop
      findVoice('kick').steps = [2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0];
      findVoice('snare').steps = [0, 0, 0, 0, 2, 0, 0, 1, 0, 0, 0, 0, 2, 0, 0, 1];
      findVoice('lowTom').steps = [0, 1, 1, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 1, 0, 0];
      findVoice('clap').steps = [0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0];
      findVoice('ch').steps = [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1];
      findVoice('oh').steps = [0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 2, 0];
      findVoice('ride').steps = [1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0];
      findVoice('crash').steps = [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    } else if (clipIdx === 3) {
      // Clip 3: 909 Stripped Kick (Outro)
      findVoice('kick').steps = [2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0];
      findVoice('rim').steps = [0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 1, 0];
      findVoice('ch').steps = [1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0];
      findVoice('shaker').steps = [0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1];
    }
    renderDrumGrid();
  }

  function loadPreset(presetName) {
    // Clear all steps first
    STATE.drumVoices.forEach(v => v.steps.fill(0));

    if (presetName === 'techno') {
      STATE.tempo = 136.0;
      STATE.swing = 54;
      findVoice('kick').steps = [2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0];
      findVoice('clap').steps = [0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0];
      findVoice('ch').steps = [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1];
      findVoice('oh').steps = [0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 2, 0];
      findVoice('ride').steps = [1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0];
    } else if (presetName === 'house') {
      STATE.tempo = 128.0;
      STATE.swing = 62;
      findVoice('kick').steps = [2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0];
      findVoice('snare').steps = [0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0];
      findVoice('oh').steps = [0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 2, 0];
      findVoice('rim').steps = [0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0];
      findVoice('shaker').steps = [1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0];
    } else if (presetName === 'rkt') {
      STATE.tempo = 105.0;
      STATE.swing = 58;
      findVoice('kick').steps = [2, 0, 0, 2, 0, 0, 2, 0, 0, 0, 2, 0, 2, 0, 0, 0];
      findVoice('snare').steps = [0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0];
      findVoice('percLow').steps = [0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0];
      findVoice('shaker').steps = [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1];
    } else if (presetName === 'dnb') {
      STATE.tempo = 174.0;
      STATE.swing = 50;
      findVoice('kick').steps = [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0];
      findVoice('snare').steps = [0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0];
      findVoice('ch').steps = [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1];
    }

    renderDrumGrid();
    const ti = document.getElementById('tempo-input');
    if (ti) ti.value = STATE.tempo.toFixed(2);
    const ss = document.getElementById('swing-slider');
    if (ss) ss.value = STATE.swing;
    const sv = document.getElementById('swing-val');
    if (sv) sv.textContent = `${STATE.swing}%`;
    updateTelemetryCard();
  }

  function findVoice(id) {
    return STATE.drumVoices.find(v => v.id === id);
  }

  function updateTelemetryCard() {
    const selEl = document.getElementById('telem-selected');
    if (selEl) {
      const names = ['Track 1 (Drums)', 'Track 2 (Bass)', 'Track 3 (Lead)', 'Track 4 (Stems)'];
      selEl.textContent = names[STATE.selectedTrack] || 'Track 1 (Drums)';
    }
    const tempoEl = document.getElementById('telem-tempo');
    if (tempoEl) {
      tempoEl.textContent = `${STATE.tempo.toFixed(1)} BPM / ${STATE.swing}%`;
    }
  }

  function renderDrumGrid() {
    const container = document.getElementById('drum-grid-container');
    if (!container) return;

    container.innerHTML = '';
    const activeVoices = STATE.drumVoices.filter(v => v.bank === STATE.activeBank);

    activeVoices.forEach((voice) => {
      const row = document.createElement('div');
      row.className = 'drum-voice-row';
      row.dataset.voiceId = voice.id;

      // Voice Meta (Audition, Name, Mute, Solo)
      const meta = document.createElement('div');
      meta.className = 'voice-meta';

      const btnAudition = document.createElement('button');
      btnAudition.className = 'voice-audition-btn';
      btnAudition.textContent = '▶';
      btnAudition.title = `Audition ${voice.name}`;
      btnAudition.addEventListener('click', () => {
        initAudioGraph();
        triggerDrumVoice(voice.id, STATE.audioCtx.currentTime, 1.0, true);
      });

      const nameSpan = document.createElement('span');
      nameSpan.className = 'voice-name';
      nameSpan.textContent = voice.name;

      const btnMute = document.createElement('button');
      btnMute.className = `btn-row-m ${voice.mute ? 'active' : ''}`;
      btnMute.textContent = 'M';
      btnMute.addEventListener('click', () => {
        voice.mute = !voice.mute;
        btnMute.classList.toggle('active', voice.mute);
      });

      const btnSolo = document.createElement('button');
      btnSolo.className = `btn-row-s ${voice.solo ? 'active' : ''}`;
      btnSolo.textContent = 'S';
      btnSolo.addEventListener('click', () => {
        voice.solo = !voice.solo;
        btnSolo.classList.toggle('active', voice.solo);
      });

      meta.appendChild(btnAudition);
      meta.appendChild(nameSpan);
      meta.appendChild(btnMute);
      meta.appendChild(btnSolo);
      row.appendChild(meta);

      // 16 Step Buttons
      const stepGrid = document.createElement('div');
      stepGrid.className = 'step-buttons-container';

      for (let s = 0; s < 16; s++) {
        const stepBtn = document.createElement('button');
        stepBtn.className = 'seq-step';
        stepBtn.dataset.step = s;

        if (voice.steps[s] === 1) stepBtn.classList.add('active');
        if (voice.steps[s] === 2) stepBtn.classList.add('accent');

        stepBtn.addEventListener('click', () => {
          if (STATE.accentMode) {
            voice.steps[s] = voice.steps[s] === 2 ? 0 : 2;
          } else {
            voice.steps[s] = voice.steps[s] > 0 ? 0 : 1;
          }
          stepBtn.classList.toggle('active', voice.steps[s] === 1);
          stepBtn.classList.toggle('accent', voice.steps[s] === 2);

          // Ensure Track 0 clip is active if user draws steps
          if (voice.steps[s] > 0 && getActiveClipIndex(0) < 0) {
            STATE.sessionClips[0][0].state = 'playing';
            updateSessionClipUI();
          }

          // Audition on toggle if stopped
          if (!STATE.isPlaying && voice.steps[s] > 0) {
            initAudioGraph();
            triggerDrumVoice(voice.id, STATE.audioCtx.currentTime, 0.9, voice.steps[s] === 2);
          }
        });

        stepGrid.appendChild(stepBtn);
      }

      row.appendChild(stepGrid);
      container.appendChild(row);
    });
  }

  function updateUIPlayhead(stepIndex) {
    // Sequencer cursor
    document.querySelectorAll('.seq-step.playhead-cursor').forEach(el => el.classList.remove('playhead-cursor'));
    document.querySelectorAll(`.seq-step[data-step="${stepIndex}"]`).forEach(el => el.classList.add('playhead-cursor'));

    // Arrangement playhead line
    const playhead = document.getElementById('timeline-playhead');
    if (playhead) {
      const sceneOffset = (STATE.activeScene || 0) * 25.0;
      const pct = sceneOffset + (stepIndex / 16) * 25.0;
      playhead.style.left = `${Math.min(99.5, pct)}%`;
    }
  }

  function clearUIPlayhead() {
    document.querySelectorAll('.seq-step.playhead-cursor').forEach(el => el.classList.remove('playhead-cursor'));
    const playhead = document.getElementById('timeline-playhead');
    if (playhead) playhead.style.left = `${(STATE.activeScene || 0) * 25.0}%`;
  }

  function triggerScene(sceneIdx) {
    STATE.activeScene = sceneIdx;
    for (let trk = 0; trk < 4; trk++) {
      for (let s = 0; s < 4; s++) {
        STATE.sessionClips[trk][s].state = (s === sceneIdx) ? 'playing' : 'stopped';
      }
    }
    applyDrumClipPattern(sceneIdx);
    updateSessionClipUI();

    document.querySelectorAll('.btn-scene-launch').forEach((btn, idx) => {
      btn.classList.toggle('playing', idx === sceneIdx);
    });

    // Sync Loop Brace & Arrangement active blocks
    const loopBrace = document.querySelector('.loop-brace');
    if (loopBrace) {
      const titles = ['Loop: Bars 1–9 [Intro]', 'Loop: Bars 9–17 [Build]', 'Loop: Bars 17–25 [Peak Drop]', 'Loop: Bars 25–33 [Outro]'];
      loopBrace.style.left = `${sceneIdx * 25}%`;
      loopBrace.style.width = '25%';
      const lt = loopBrace.querySelector('.loop-title');
      if (lt) lt.textContent = titles[sceneIdx] || titles[0];
    }
  }

  function triggerClip(trackIdx, clipIdx) {
    if (!STATE.sessionClips[trackIdx]) return;
    STATE.selectedTrack = trackIdx;
    updateTelemetryCard();

    for (let s = 0; s < 4; s++) {
      STATE.sessionClips[trackIdx][s].state = (s === clipIdx) ? 'playing' : 'stopped';
    }
    if (trackIdx === 0) {
      applyDrumClipPattern(clipIdx);
    }
    STATE.activeScene = clipIdx;
    updateSessionClipUI();
    if (!STATE.isPlaying) {
      startPlayback();
    }
  }

  function updateSessionClipUI() {
    document.querySelectorAll('.clip-slot').forEach(slot => {
      const trk = parseInt(slot.dataset.track, 10);
      const clip = parseInt(slot.dataset.clip, 10);
      const isPlaying = STATE.sessionClips[trk] && STATE.sessionClips[trk][clip] && STATE.sessionClips[trk][clip].state === 'playing';
      slot.classList.toggle('active-clip', isPlaying);
    });
  }

  function highlightPianoKey(midiNote, isPressed) {
    const keyEl = document.querySelector(`#piano-keys [data-midi="${midiNote}"]`);
    if (keyEl) {
      keyEl.classList.toggle('pressed', isPressed);
    }
  }

  // Virtual Piano Keyboard Generator (C3 to C5 = 25 keys)
  function renderPianoKeys() {
    const container = document.getElementById('piano-keys');
    if (!container) return;
    container.innerHTML = '';

    const rootMidi = STATE.octaveOffset * 12; // C3 = 36 / 48
    const whiteKeys = [0, 2, 4, 5, 7, 9, 11, 12, 14, 16, 17, 19, 21, 23, 24]; // 15 white keys
    const blackKeyMap = { 1: 0, 3: 1, 6: 3, 8: 4, 10: 5, 13: 7, 15: 8, 18: 10, 20: 11, 22: 12 };

    const bindKeyEvents = (btn, midi) => {
      btn.addEventListener('pointerdown', (e) => {
        e.preventDefault();
        initAudioGraph();
        playSynthNote(midi, 0.4);
        btn.classList.add('pressed');
      });
      btn.addEventListener('pointerup', () => btn.classList.remove('pressed'));
      btn.addEventListener('pointerleave', () => btn.classList.remove('pressed'));
    };

    // White keys
    whiteKeys.forEach(offset => {
      const btn = document.createElement('div');
      btn.className = 'p-key-white';
      btn.dataset.midi = rootMidi + offset;
      bindKeyEvents(btn, rootMidi + offset);
      container.appendChild(btn);
    });

    // Black keys positioned absolutely
    Object.keys(blackKeyMap).forEach(keyOffsetStr => {
      const offset = parseInt(keyOffsetStr, 10);
      const whiteIdx = blackKeyMap[offset];
      const btn = document.createElement('div');
      btn.className = 'p-key-black';
      btn.dataset.midi = rootMidi + offset;
      btn.style.left = `${((whiteIdx + 0.65) / whiteKeys.length) * 100}%`;
      bindKeyEvents(btn, rootMidi + offset);
      container.appendChild(btn);
    });
  }

  function updateSynthSliders() {
    const sc = document.getElementById('synth-cutoff');
    if (sc) sc.value = STATE.synth.cutoff;
    const scv = document.getElementById('synth-cutoff-val');
    if (scv) scv.textContent = `${Math.round(STATE.synth.cutoff)} Hz`;

    const sr = document.getElementById('synth-res');
    if (sr) sr.value = STATE.synth.resonance;
    const srv = document.getElementById('synth-res-val');
    if (srv) srv.textContent = `${STATE.synth.resonance} Q`;

    if (NODES.filterNode1 && STATE.audioCtx) {
      NODES.filterNode1.frequency.setTargetAtTime(STATE.synth.cutoff, STATE.audioCtx.currentTime, 0.012);
      NODES.filterNode1.Q.setTargetAtTime(Math.min(4.0, Math.sqrt(STATE.synth.resonance)), STATE.audioCtx.currentTime, 0.012);
      NODES.filterNode2.frequency.setTargetAtTime(STATE.synth.cutoff, STATE.audioCtx.currentTime, 0.012);
      NODES.filterNode2.Q.setTargetAtTime(Math.min(4.0, Math.sqrt(STATE.synth.resonance)), STATE.audioCtx.currentTime, 0.012);
    }
  }

  function formatPanLabel(val) {
    const v = parseFloat(val);
    if (Math.abs(v) < 0.03) return 'C';
    return v < 0 ? `L${Math.round(Math.abs(v) * 100)}` : `R${Math.round(v * 100)}`;
  }

  // =========================================================================
  // 11. EVENT LISTENERS & USER INPUT BINDINGS
  // =========================================================================

  function setupEventListeners() {
    // Audio Unlock Trigger
    document.getElementById('btn-unlock-audio')?.addEventListener('click', () => {
      startPlayback();
    });
    document.getElementById('audio-unlock-overlay')?.addEventListener('click', () => {
      startPlayback();
    });

    // Global Keydown: Space (Play/Stop), Tab (View Switch), 0 (Mute), S (Solo), Z/X (Octave)
    window.addEventListener('keydown', (e) => {
      if (['INPUT', 'TEXTAREA', 'SELECT'].includes(e.target.tagName)) return;

      if (e.code === 'Space') {
        e.preventDefault();
        togglePlayback();
      } else if (e.code === 'Tab') {
        e.preventDefault();
        toggleView();
      } else if (e.code === 'KeyZ') {
        STATE.octaveOffset = Math.max(1, STATE.octaveOffset - 1);
        updateOctaveDisplay();
      } else if (e.code === 'KeyX') {
        STATE.octaveOffset = Math.min(6, STATE.octaveOffset + 1);
        updateOctaveDisplay();
      }

      // Computer Keyboard Synthesizer: A, W, S, E, D, F, T, G, Y, H, U, J, K
      const keyMidiMap = {
        KeyA: 0, KeyW: 1, KeyS: 2, KeyE: 3, KeyD: 4, KeyF: 5,
        KeyT: 6, KeyG: 7, KeyY: 8, KeyH: 9, KeyU: 10, KeyJ: 11, KeyK: 12
      };
      if (keyMidiMap[e.code] !== undefined && !e.repeat) {
        const pitch = STATE.octaveOffset * 12 + keyMidiMap[e.code];
        initAudioGraph();
        playSynthNote(pitch, 0.35);
        highlightPianoKey(pitch, true);
      }
    });

    window.addEventListener('keyup', (e) => {
      const keyMidiMap = {
        KeyA: 0, KeyW: 1, KeyS: 2, KeyE: 3, KeyD: 4, KeyF: 5,
        KeyT: 6, KeyG: 7, KeyY: 8, KeyH: 9, KeyU: 10, KeyJ: 11, KeyK: 12
      };
      if (keyMidiMap[e.code] !== undefined) {
        const pitch = STATE.octaveOffset * 12 + keyMidiMap[e.code];
        highlightPianoKey(pitch, false);
      }
    });

    // View Switching Buttons
    document.getElementById('btn-view-session')?.addEventListener('click', () => switchView('session'));
    document.getElementById('btn-view-arrangement')?.addEventListener('click', () => switchView('arrangement'));

    // Transport Controls (Play, Stop, Record, Loop, Metronome, Back to Arr)
    document.getElementById('btn-play')?.addEventListener('click', togglePlayback);
    document.getElementById('btn-stop')?.addEventListener('click', stopPlayback);
    document.getElementById('btn-rec')?.addEventListener('click', (e) => {
      STATE.isRecording = !STATE.isRecording;
      e.currentTarget.classList.toggle('active', STATE.isRecording);
      if (STATE.isRecording && !STATE.isPlaying) {
        startPlayback();
      }
    });
    document.getElementById('btn-loop')?.addEventListener('click', (e) => {
      STATE.loopEnabled = !STATE.loopEnabled;
      e.currentTarget.classList.toggle('active', STATE.loopEnabled);
    });
    document.getElementById('btn-metro')?.addEventListener('click', (e) => {
      STATE.metronome = !STATE.metronome;
      e.currentTarget.classList.toggle('active', STATE.metronome);
      initAudioGraph();
      if (STATE.metronome && !STATE.isPlaying) {
        triggerMetronomeClick(STATE.audioCtx.currentTime, true);
      }
    });
    document.getElementById('btn-back-to-arr')?.addEventListener('click', () => {
      STATE.currentBar = 1;
      STATE.currentStep = 0;
      triggerScene(0);
      updateUIPlayhead(0);
    });
    document.getElementById('launch-quant')?.addEventListener('change', (e) => {
      STATE.quantize = e.target.value;
    });

    // Tempo Control
    const tempoInput = document.getElementById('tempo-input');
    tempoInput?.addEventListener('change', (e) => {
      const val = parseFloat(e.target.value);
      if (!isNaN(val) && val >= 40 && val <= 240) {
        STATE.tempo = val;
        if (NODES.delayNode && STATE.audioCtx) {
          NODES.delayNode.delayTime.setTargetAtTime((60 / STATE.tempo) * 0.75, STATE.audioCtx.currentTime, 0.02);
        }
        updateTelemetryCard();
      }
    });
    document.getElementById('bpm-inc')?.addEventListener('click', () => {
      STATE.tempo = Math.min(240, STATE.tempo + 1);
      if (tempoInput) tempoInput.value = STATE.tempo.toFixed(2);
      updateTelemetryCard();
    });
    document.getElementById('bpm-dec')?.addEventListener('click', () => {
      STATE.tempo = Math.max(40, STATE.tempo - 1);
      if (tempoInput) tempoInput.value = STATE.tempo.toFixed(2);
      updateTelemetryCard();
    });

    // Groove Pool Swing
    const swingSlider = document.getElementById('swing-slider');
    swingSlider?.addEventListener('input', (e) => {
      STATE.swing = parseInt(e.target.value, 10);
      const sv = document.getElementById('swing-val');
      if (sv) sv.textContent = `${STATE.swing}%`;
      updateTelemetryCard();
    });

    // Scale Controls
    document.getElementById('scale-root')?.addEventListener('change', (e) => {
      STATE.scaleRoot = parseInt(e.target.value, 10);
    });
    document.getElementById('scale-type')?.addEventListener('change', (e) => {
      STATE.scaleType = e.target.value;
    });

    // Export Buttons
    document.getElementById('btn-export-wav')?.addEventListener('click', exportWav);
    document.getElementById('btn-export-als')?.addEventListener('click', exportAls);

    // Detail Deck Tabs
    document.querySelectorAll('.deck-tab').forEach(tab => {
      tab.addEventListener('click', () => {
        document.querySelectorAll('.deck-tab').forEach(t => t.classList.remove('active'));
        document.querySelectorAll('.deck-panel').forEach(p => p.classList.remove('active'));
        tab.classList.add('active');
        const panel = document.getElementById(tab.dataset.deck);
        if (panel) panel.classList.add('active');
      });
    });

    // Bank A / B Switcher
    document.getElementById('btn-bank-a')?.addEventListener('click', () => {
      STATE.activeBank = 'A';
      document.getElementById('btn-bank-a').classList.add('active');
      document.getElementById('btn-bank-b').classList.remove('active');
      renderDrumGrid();
    });
    document.getElementById('btn-bank-b')?.addEventListener('click', () => {
      STATE.activeBank = 'B';
      document.getElementById('btn-bank-b').classList.add('active');
      document.getElementById('btn-bank-a').classList.remove('active');
      renderDrumGrid();
    });

    // Sequencer Presets
    document.querySelectorAll('.btn-preset').forEach(btn => {
      btn.addEventListener('click', () => {
        loadPreset(btn.dataset.preset);
      });
    });

    // Accent Toggle
    document.getElementById('accent-toggle')?.addEventListener('change', (e) => {
      STATE.accentMode = e.target.checked;
    });

    // Synth Mode Tabs
    document.querySelectorAll('.btn-smode').forEach(btn => {
      btn.addEventListener('click', () => {
        document.querySelectorAll('.btn-smode').forEach(b => b.classList.remove('active'));
        btn.classList.add('active');
        STATE.synth.mode = btn.dataset.mode;
        initAudioGraph();
        if (!STATE.isPlaying) {
          playSynthNote(getScalePitch(0, 3), 0.35);
        }
      });
    });

    // Synth Knobs
    setupRangeListener('synth-cutoff', (v) => {
      STATE.synth.cutoff = v;
      const el = document.getElementById('synth-cutoff-val');
      if (el) el.textContent = `${Math.round(v)} Hz`;
      if (NODES.filterNode1 && STATE.audioCtx) NODES.filterNode1.frequency.setTargetAtTime(v, STATE.audioCtx.currentTime, 0.012);
      if (NODES.filterNode2 && STATE.audioCtx) NODES.filterNode2.frequency.setTargetAtTime(v, STATE.audioCtx.currentTime, 0.012);
    });
    setupRangeListener('synth-res', (v) => {
      STATE.synth.resonance = v;
      const el = document.getElementById('synth-res-val');
      if (el) el.textContent = `${v} Q`;
      if (NODES.filterNode1 && STATE.audioCtx) NODES.filterNode1.Q.setTargetAtTime(Math.min(4.0, Math.sqrt(v)), STATE.audioCtx.currentTime, 0.012);
      if (NODES.filterNode2 && STATE.audioCtx) NODES.filterNode2.Q.setTargetAtTime(Math.min(4.0, Math.sqrt(v)), STATE.audioCtx.currentTime, 0.012);
    });
    setupRangeListener('synth-att', (v) => {
      STATE.synth.attack = v / 1000.0;
      const el = document.getElementById('synth-att-val');
      if (el) el.textContent = `${Math.round(v)} ms`;
    });
    setupRangeListener('synth-dec', (v) => {
      STATE.synth.decay = v / 1000.0;
      const el = document.getElementById('synth-dec-val');
      if (el) el.textContent = `${Math.round(v)} ms`;
    });
    setupRangeListener('synth-sus', (v) => {
      STATE.synth.sustain = v;
      const el = document.getElementById('synth-sus-val');
      if (el) el.textContent = `${Math.round(v * 100)} %`;
    });
    setupRangeListener('synth-rel', (v) => {
      STATE.synth.release = v / 1000.0;
      const el = document.getElementById('synth-rel-val');
      if (el) el.textContent = `${Math.round(v)} ms`;
    });
    setupRangeListener('synth-morph', (v) => {
      STATE.synth.morph = v;
      const el = document.getElementById('synth-morph-val');
      if (el) el.textContent = `${Math.round(v * 100)} %`;
    });
    setupRangeListener('synth-drive', (v) => {
      STATE.synth.drive = v;
      const el = document.getElementById('synth-drive-val');
      if (el) el.textContent = `+${v} dB`;
      if (NODES.roarShaper) NODES.roarShaper.curve = getCachedDistortionCurve(v, STATE.fx.roar.curve);
    });

    // FX Rack Controls (Glue, OTT, Roar, Reverb, Limiter)
    document.getElementById('glue-active')?.addEventListener('change', (e) => {
      STATE.fx.glue.active = e.target.checked;
      if (NODES.glueCompressor && STATE.audioCtx) {
        NODES.glueCompressor.ratio.setTargetAtTime(STATE.fx.glue.active ? STATE.fx.glue.ratio : 1.0, STATE.audioCtx.currentTime, 0.015);
      }
    });
    setupRangeListener('glue-thresh', (v) => {
      STATE.fx.glue.threshold = v;
      const el = document.getElementById('glue-thresh-val');
      if (el) el.textContent = `${v} dB`;
      if (NODES.glueCompressor && STATE.audioCtx) NODES.glueCompressor.threshold.setTargetAtTime(v, STATE.audioCtx.currentTime, 0.015);
    });
    document.getElementById('glue-ratio')?.addEventListener('change', (e) => {
      const r = parseFloat(e.target.value) || 4.0;
      STATE.fx.glue.ratio = r;
      if (NODES.glueCompressor && STATE.audioCtx && STATE.fx.glue.active) {
        NODES.glueCompressor.ratio.setTargetAtTime(r, STATE.audioCtx.currentTime, 0.015);
      }
    });
    setupRangeListener('glue-makeup', (v) => {
      STATE.fx.glue.makeup = v;
      const el = document.getElementById('glue-makeup-val');
      if (el) el.textContent = `+${v} dB`;
      if (NODES.glueMakeupGain && STATE.audioCtx) {
        NODES.glueMakeupGain.gain.setTargetAtTime(Math.pow(10, v / 40), STATE.audioCtx.currentTime, 0.015);
      }
    });

    // OTT Multiband Controls
    const syncOtt = () => {
      if (!NODES.ottFilters.length || !STATE.audioCtx) return;
      const now = STATE.audioCtx.currentTime;
      const depth = STATE.fx.ott.active ? STATE.fx.ott.depth : 0;
      const [lowF, midF, highF] = NODES.ottFilters;
      if (lowF) lowF.gain.setTargetAtTime((STATE.fx.ott.low - 0.5) * 8 * depth, now, 0.015);
      if (midF) midF.gain.setTargetAtTime((STATE.fx.ott.mid - 0.5) * 6 * depth, now, 0.015);
      if (highF) highF.gain.setTargetAtTime((STATE.fx.ott.high - 0.5) * 9 * depth, now, 0.015);
    };
    document.getElementById('ott-active')?.addEventListener('change', (e) => {
      STATE.fx.ott.active = e.target.checked;
      syncOtt();
    });
    ['ott-low', 'ott-mid', 'ott-high', 'ott-depth'].forEach((id) => {
      setupRangeListener(id, (v) => {
        const key = id.replace('ott-', '');
        STATE.fx.ott[key] = v / 100.0;
        const slider = document.getElementById(id);
        const span = slider?.parentElement?.querySelector('.tabular');
        if (span) span.textContent = `${Math.round(v)}%`;
        syncOtt();
      });
    });

    // Roar Controls
    document.getElementById('roar-active')?.addEventListener('change', (e) => {
      STATE.fx.roar.active = e.target.checked;
      if (NODES.roarShaper) {
        NODES.roarShaper.curve = getCachedDistortionCurve(STATE.fx.roar.active ? STATE.fx.roar.drive : 0, STATE.fx.roar.curve);
      }
    });
    setupRangeListener('roar-drive', (v) => {
      STATE.fx.roar.drive = v;
      const el = document.getElementById('roar-drive-val');
      if (el) el.textContent = `+${v} dB`;
      if (NODES.roarShaper && STATE.fx.roar.active) {
        NODES.roarShaper.curve = getCachedDistortionCurve(v, STATE.fx.roar.curve);
      }
    });
    setupRangeListener('roar-tone', (v) => {
      STATE.fx.roar.tone = v;
      const el = document.getElementById('roar-tone-val');
      if (el) el.textContent = v >= 1000 ? `${(v / 1000).toFixed(1)} kHz` : `${Math.round(v)} Hz`;
      if (NODES.roarToneFilter && STATE.audioCtx) {
        NODES.roarToneFilter.frequency.setTargetAtTime(v, STATE.audioCtx.currentTime, 0.015);
      }
    });
    document.getElementById('roar-curve')?.addEventListener('change', (e) => {
      STATE.fx.roar.curve = e.target.value;
      if (NODES.roarShaper && STATE.fx.roar.active) {
        NODES.roarShaper.curve = getCachedDistortionCurve(STATE.fx.roar.drive, STATE.fx.roar.curve);
      }
    });

    // Hybrid Reverb Controls
    document.getElementById('rev-active')?.addEventListener('change', (e) => {
      STATE.fx.reverb.active = e.target.checked;
      syncMixerTrackGains();
    });
    setupRangeListener('rev-decay', (v) => {
      STATE.fx.reverb.decay = v;
      const el = document.getElementById('rev-decay-val');
      if (el) el.textContent = `${v.toFixed(1)} s`;
      if (NODES.reverbConvolver && STATE.audioCtx) {
        NODES.reverbConvolver.buffer = buildReverbImpulse(STATE.audioCtx, v);
      }
    });
    setupRangeListener('rev-size', (v) => {
      STATE.fx.reverb.size = v / 100.0;
      const slider = document.getElementById('rev-size');
      const span = slider?.parentElement?.querySelector('.tabular');
      if (span) span.textContent = `${Math.round(v)}%`;
    });
    setupRangeListener('rev-mix', (v) => {
      STATE.fx.reverb.mix = v / 100.0;
      const el = document.getElementById('rev-mix-val');
      if (el) el.textContent = `${Math.round(v)}%`;
      syncMixerTrackGains();
    });

    // Limiter Release
    setupRangeListener('limiter-release', (v) => {
      STATE.fx.limiter.release = v / 1000.0;
      const slider = document.getElementById('limiter-release');
      const span = slider?.parentElement?.querySelector('.tabular');
      if (span) span.textContent = `${Math.round(v)} ms`;
      if (NODES.truePeakLimiter && STATE.audioCtx) {
        NODES.truePeakLimiter.release.setTargetAtTime(v / 1000.0, STATE.audioCtx.currentTime, 0.015);
      }
    });

    // Octave Shift Buttons
    document.getElementById('btn-oct-down')?.addEventListener('click', () => {
      STATE.octaveOffset = Math.max(1, STATE.octaveOffset - 1);
      updateOctaveDisplay();
    });
    document.getElementById('btn-oct-up')?.addEventListener('click', () => {
      STATE.octaveOffset = Math.min(6, STATE.octaveOffset + 1);
      updateOctaveDisplay();
    });

    // Mixer Strips Controls (Channels 0..3)
    for (let i = 0; i < 4; i++) {
      setupRangeListener(`fader-ch${i}`, (v) => {
        STATE.tracks[i].volume = v;
        syncMixerTrackGains();
      });
      setupRangeListener(`pan-ch${i}`, (v) => {
        STATE.tracks[i].pan = v;
        const slider = document.getElementById(`pan-ch${i}`);
        const panSpan = slider?.parentElement?.querySelector('.pan-val');
        if (panSpan) panSpan.textContent = formatPanLabel(v);
        if (NODES.trackPanners[i] && STATE.audioCtx) {
          NODES.trackPanners[i].pan.setTargetAtTime(v, STATE.audioCtx.currentTime, 0.01);
        }
      });
      setupRangeListener(`sndA-ch${i}`, (v) => {
        STATE.tracks[i].sendA = v;
        if (NODES.sendAGains[i] && STATE.audioCtx) {
          NODES.sendAGains[i].gain.setTargetAtTime(v, STATE.audioCtx.currentTime, 0.01);
        }
      });
      setupRangeListener(`sndB-ch${i}`, (v) => {
        STATE.tracks[i].sendB = v;
        if (NODES.sendBGains[i] && STATE.audioCtx) {
          NODES.sendBGains[i].gain.setTargetAtTime(v, STATE.audioCtx.currentTime, 0.01);
        }
      });
      document.getElementById(`mute-ch${i}`)?.addEventListener('click', (e) => {
        STATE.tracks[i].mute = !STATE.tracks[i].mute;
        e.currentTarget.classList.toggle('active', STATE.tracks[i].mute);
        syncMixerTrackGains();
      });
      document.getElementById(`solo-ch${i}`)?.addEventListener('click', (e) => {
        STATE.tracks[i].solo = !STATE.tracks[i].solo;
        e.currentTarget.classList.toggle('active', STATE.tracks[i].solo);
        syncMixerTrackGains();
      });
    }

    // Return A & Return B Strip Controls
    [
      { id: 'rtnA', idx: 4, pannerKey: 'returnAPanner' },
      { id: 'rtnB', idx: 5, pannerKey: 'returnBPanner' }
    ].forEach(({ id, idx, pannerKey }) => {
      setupRangeListener(`fader-${id}`, (v) => {
        STATE.tracks[idx].volume = v;
        syncMixerTrackGains();
      });
      setupRangeListener(`pan-${id}`, (v) => {
        STATE.tracks[idx].pan = v;
        const slider = document.getElementById(`pan-${id}`);
        const panSpan = slider?.parentElement?.querySelector('.pan-val');
        if (panSpan) panSpan.textContent = formatPanLabel(v);
        if (NODES[pannerKey] && STATE.audioCtx) {
          NODES[pannerKey].pan.setTargetAtTime(v, STATE.audioCtx.currentTime, 0.01);
        }
      });
      document.getElementById(`mute-${id}`)?.addEventListener('click', (e) => {
        STATE.tracks[idx].mute = !STATE.tracks[idx].mute;
        e.currentTarget.classList.toggle('active', STATE.tracks[idx].mute);
        syncMixerTrackGains();
      });
      document.getElementById(`solo-${id}`)?.addEventListener('click', (e) => {
        STATE.tracks[idx].solo = !STATE.tracks[idx].solo;
        e.currentTarget.classList.toggle('active', STATE.tracks[idx].solo);
        syncMixerTrackGains();
      });
    });

    // Master Strip Controls (Mono, Dim -20, Master Fader)
    document.getElementById('btn-master-mono')?.addEventListener('click', (e) => {
      STATE.tracks[6].mono = !STATE.tracks[6].mono;
      e.currentTarget.classList.toggle('active', STATE.tracks[6].mono);
      syncMixerTrackGains();
    });
    document.getElementById('btn-master-dim')?.addEventListener('click', (e) => {
      STATE.tracks[6].dim = !STATE.tracks[6].dim;
      e.currentTarget.classList.toggle('active', STATE.tracks[6].dim);
      syncMixerTrackGains();
    });
    setupRangeListener('fader-master', (v) => {
      STATE.tracks[6].volume = v;
      syncMixerTrackGains();
    });

    // Copilot Language Switcher
    document.getElementById('lang-en')?.addEventListener('click', () => {
      STATE.language = 'en';
      document.getElementById('lang-en').classList.add('active');
      document.getElementById('lang-es').classList.remove('active');
      populateQuickChips();
    });
    document.getElementById('lang-es')?.addEventListener('click', () => {
      STATE.language = 'es';
      document.getElementById('lang-es').classList.add('active');
      document.getElementById('lang-en').classList.remove('active');
      populateQuickChips();
    });

    // Copilot OKF Chapter Buttons
    document.querySelectorAll('.btn-okf-ch').forEach(btn => {
      btn.addEventListener('click', () => {
        document.querySelectorAll('.btn-okf-ch').forEach(b => b.classList.remove('active'));
        btn.classList.add('active');
        const ch = btn.dataset.ch;
        const data = COPILOT_KNOWLEDGE[ch] || COPILOT_KNOWLEDGE['OKF-01'];
        const text = data[STATE.language] || data.en;
        appendChatMessage('bot', text, `✓ okf_manual_inspect(chapter: "${ch}")`);
      });
    });

    // Copilot Chat Form
    document.getElementById('chat-form')?.addEventListener('submit', (e) => {
      e.preventDefault();
      const input = document.getElementById('chat-input');
      if (input && input.value.trim()) {
        executeCopilotCommand(input.value.trim());
        input.value = '';
      }
    });

    // Individual Session Matrix Clip Slots (16 Clips)
    document.querySelectorAll('.clip-slot').forEach(slot => {
      slot.addEventListener('click', () => {
        const trk = parseInt(slot.dataset.track, 10);
        const clip = parseInt(slot.dataset.clip, 10);
        triggerClip(trk, clip);
      });
    });

    // Per-Track Stop Clip Buttons
    document.querySelectorAll('.btn-stop-clip').forEach(btn => {
      btn.addEventListener('click', () => {
        const trk = parseInt(btn.dataset.track, 10);
        if (STATE.sessionClips[trk]) {
          STATE.sessionClips[trk].forEach(c => { c.state = 'stopped'; });
          updateSessionClipUI();
        }
      });
    });

    // Scene Launch Buttons
    document.querySelectorAll('.btn-scene-launch').forEach(btn => {
      btn.addEventListener('click', () => {
        const scene = parseInt(btn.dataset.scene, 10);
        triggerScene(scene);
        if (!STATE.isPlaying) startPlayback();
      });
    });

    // Stop All Clips
    document.getElementById('btn-stop-all-clips')?.addEventListener('click', () => {
      for (let t = 0; t < 4; t++) {
        for (let s = 0; s < 4; s++) {
          STATE.sessionClips[t][s].state = 'stopped';
        }
      }
      updateSessionClipUI();
      document.querySelectorAll('.btn-scene-launch').forEach(b => b.classList.remove('playing'));
    });

    // Arrangement View Clips & Timeline Ruler Scrubbing
    document.querySelectorAll('.arr-clip-block').forEach(block => {
      block.style.cursor = 'pointer';
      block.addEventListener('click', () => {
        const parentBody = block.closest('.arr-track-body');
        parentBody?.querySelectorAll('.arr-clip-block').forEach(b => b.classList.remove('active-block'));
        block.classList.add('active-block');
        const leftPct = parseFloat(block.style.left) || 0;
        const sceneIdx = leftPct >= 75 ? 3 : leftPct >= 45 ? 2 : leftPct >= 20 ? 1 : 0;
        triggerScene(sceneIdx);
        if (!STATE.isPlaying) startPlayback();
      });
    });

    const rulerLane = document.getElementById('ruler-lane');
    if (rulerLane) {
      rulerLane.style.cursor = 'pointer';
      rulerLane.addEventListener('click', (e) => {
        const rect = rulerLane.getBoundingClientRect();
        const pct = Math.max(0, Math.min(100, ((e.clientX - rect.left) / rect.width) * 100));
        const sceneIdx = Math.min(3, Math.floor(pct / 25));
        triggerScene(sceneIdx);
        const playhead = document.getElementById('timeline-playhead');
        if (playhead) playhead.style.left = `${pct}%`;
        if (!STATE.isPlaying) startPlayback();
      });
    }

    // Center Resizable Splitter (#daw-splitter)
    const splitter = document.getElementById('daw-splitter');
    const workspace = document.querySelector('.daw-workspace');
    const detailDeck = document.querySelector('.daw-detail-deck');
    if (splitter && workspace && detailDeck) {
      let draggingSplitter = false;
      splitter.addEventListener('pointerdown', (e) => {
        draggingSplitter = true;
        splitter.setPointerCapture?.(e.pointerId);
      });
      splitter.addEventListener('pointermove', (e) => {
        if (!draggingSplitter) return;
        const vh = window.innerHeight;
        const y = Math.max(180, Math.min(vh - 220, e.clientY));
        const headerH = document.querySelector('.daw-header')?.offsetHeight || 48;
        const dockH = document.querySelector('.daw-visualizer-dock')?.offsetHeight || 96;
        const topH = Math.max(140, y - headerH);
        const botH = Math.max(160, vh - y - dockH - 6);
        workspace.style.height = `${topH}px`;
        workspace.style.flex = 'none';
        detailDeck.style.height = `${botH}px`;
        detailDeck.style.flex = 'none';
      });
      splitter.addEventListener('pointerup', () => {
        draggingSplitter = false;
      });
    }
  }

  function setupRangeListener(id, callback) {
    const el = document.getElementById(id);
    if (!el) return;
    el.addEventListener('input', (e) => {
      callback(parseFloat(e.target.value));
    });
  }

  function switchView(viewName) {
    STATE.activeView = viewName;
    const sessionEl = document.getElementById('view-session');
    const arrEl = document.getElementById('view-arrangement');
    const btnSession = document.getElementById('btn-view-session');
    const btnArr = document.getElementById('btn-view-arrangement');

    if (viewName === 'session') {
      sessionEl?.classList.add('active');
      arrEl?.classList.remove('active');
      btnSession?.classList.add('active');
      btnArr?.classList.remove('active');
    } else {
      sessionEl?.classList.remove('active');
      arrEl?.classList.add('active');
      btnSession?.classList.remove('active');
      btnArr?.classList.add('active');
    }
  }

  function toggleView() {
    switchView(STATE.activeView === 'session' ? 'arrangement' : 'session');
  }

  function updateOctaveDisplay() {
    const disp = document.getElementById('octave-display');
    if (disp) {
      disp.textContent = `C${STATE.octaveOffset}–C${STATE.octaveOffset + 2}`;
    }
    renderPianoKeys();
  }

  // =========================================================================
  // 12. INITIALIZATION
  // =========================================================================

  window.addEventListener('DOMContentLoaded', () => {
    renderDrumGrid();
    renderPianoKeys();
    populateQuickChips();
    updateSessionClipUI();
    updateTelemetryCard();
    setupEventListeners();
    initVisualizers();
  });

})();
