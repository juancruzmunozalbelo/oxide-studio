/**
 * OXIDE STUDIO — SOTA IN-BROWSER WORKSTATION PLAYGROUND & SHOWCASE ENGINE
 * Zero-dependency Web Audio API, 60 FPS HTML5 Canvas DSP Visualizers,
 * 16-Step TR-909 Sequencer, In-App AI Copilot Simulator, and 76 MCP Tools Catalog.
 */

(function () {
  'use strict';

  // =========================================================================
  // 1. Web Audio API Engine & PolyBLEP Synthesizer
  // =========================================================================

  let audioCtx = null;
  let masterGain = null;
  let masterLimiter = null;
  let masterAnalyser = null;

  // Filter & Synth Parameters
  const synthParams = {
    oscType: 'sawtooth',
    cutoff: 1200,
    resonance: 6.0,
    envMod: 0.65,
    attack: 0.01,
    decay: 0.35,
    sustain: 0.4,
    release: 0.4,
    octave: 3,
  };

  // Active voices tracking
  const activeVoices = new Map();
  let sharedNoiseBuffer = null;
  let activeOpenHatGain = null;

  function getSharedNoiseBuffer(ctx) {
    if (sharedNoiseBuffer && sharedNoiseBuffer.sampleRate === ctx.sampleRate) {
      return sharedNoiseBuffer;
    }
    const len = ctx.sampleRate * 2;
    const buf = ctx.createBuffer(1, len, ctx.sampleRate);
    const data = buf.getChannelData(0);
    let x = 0x13579bdf;
    for (let i = 0; i < len; i++) {
      x ^= x << 13;
      x ^= x >> 17;
      x ^= x << 5;
      data[i] = (x / 2147483648.0) - 1.0;
    }
    sharedNoiseBuffer = buf;
    return buf;
  }

  function initAudio() {
    if (audioCtx) {
      if (audioCtx.state === 'suspended') {
        audioCtx.resume();
      }
      return;
    }

    const AudioContextClass = window.AudioContext || window.webkitAudioContext;
    audioCtx = new AudioContextClass({ latencyHint: 'interactive' });
    getSharedNoiseBuffer(audioCtx);

    // Master analyser for 60 FPS Canvas visualizers
    masterAnalyser = audioCtx.createAnalyser();
    masterAnalyser.fftSize = 256; // 128 frequency bins
    masterAnalyser.smoothingTimeConstant = 0.8;

    // Smooth bus compressor/limiter with headroom
    masterLimiter = audioCtx.createDynamicsCompressor();
    masterLimiter.threshold.setValueAtTime(-6.0, audioCtx.currentTime);
    masterLimiter.knee.setValueAtTime(6.0, audioCtx.currentTime);
    masterLimiter.ratio.setValueAtTime(4.0, audioCtx.currentTime);
    masterLimiter.attack.setValueAtTime(0.008, audioCtx.currentTime);
    masterLimiter.release.setValueAtTime(0.09, audioCtx.currentTime);

    // Master gain
    masterGain = audioCtx.createGain();
    masterGain.gain.setValueAtTime(0.82, audioCtx.currentTime);

    masterLimiter.connect(masterAnalyser);
    masterAnalyser.connect(masterGain);
    masterGain.connect(audioCtx.destination);

    // Update telemetry badge
    updateAudioTelemetry();
  }

  function noteToFrequency(note) {
    return 440 * Math.pow(2, (note - 69) / 12);
  }

  // Cascaded 4-Pole (24 dB/oct) Low-Pass Filter Simulation with Resonance
  function create4PoleFilter(ctx, cutoffHz, res) {
    const stage1 = ctx.createBiquadFilter();
    stage1.type = 'lowpass';
    stage1.frequency.setValueAtTime(cutoffHz, ctx.currentTime);
    stage1.Q.setValueAtTime(Math.max(0.5, res * 0.7), ctx.currentTime);

    const stage2 = ctx.createBiquadFilter();
    stage2.type = 'lowpass';
    stage2.frequency.setValueAtTime(cutoffHz, ctx.currentTime);
    stage2.Q.setValueAtTime(Math.max(0.5, res * 0.7), ctx.currentTime);

    stage1.connect(stage2);

    return {
      input: stage1,
      output: stage2,
      setCutoff(freq, time = ctx.currentTime) {
        const clamped = Math.max(20, Math.min(18000, freq));
        stage1.frequency.setValueAtTime(clamped, time);
        stage2.frequency.setValueAtTime(clamped, time);
      },
      setResonance(q, time = ctx.currentTime) {
        const clampedQ = Math.max(0.1, Math.min(25, q));
        stage1.Q.setValueAtTime(clampedQ * 0.7, time);
        stage2.Q.setValueAtTime(clampedQ * 0.7, time);
      },
      cancelScheduled(time = ctx.currentTime) {
        stage1.frequency.cancelScheduledValues(time);
        stage2.frequency.cancelScheduledValues(time);
      },
      rampCutoff(target, duration, time = ctx.currentTime) {
        const clamped = Math.max(20, Math.min(18000, target));
        stage1.frequency.exponentialRampToValueAtTime(clamped, time + duration);
        stage2.frequency.exponentialRampToValueAtTime(clamped, time + duration);
      }
    };
  }

  function playSynthNoteOn(midiPitch) {
    initAudio();
    if (!audioCtx) return;

    if (activeVoices.has(midiPitch)) {
      playSynthNoteOff(midiPitch);
    }

    const now = audioCtx.currentTime;
    const freq = noteToFrequency(midiPitch);

    // Primary Oscillator (Anti-aliased Sawtooth or Square)
    const osc1 = audioCtx.createOscillator();
    osc1.type = synthParams.oscType === 'square' ? 'square' : 'sawtooth';
    osc1.frequency.setValueAtTime(freq, now);

    // Secondary Detuned Oscillator (+4 cents for rich analog unison)
    const osc2 = audioCtx.createOscillator();
    osc2.type = synthParams.oscType === 'square' ? 'sawtooth' : 'sine';
    osc2.frequency.setValueAtTime(freq * 1.002, now);

    // Sub Oscillator (1 octave down sine)
    const subOsc = audioCtx.createOscillator();
    subOsc.type = 'sine';
    subOsc.frequency.setValueAtTime(freq * 0.5, now);

    const voiceMixer = audioCtx.createGain();
    voiceMixer.gain.setValueAtTime(0.24, now);

    osc1.connect(voiceMixer);
    osc2.connect(voiceMixer);
    subOsc.connect(voiceMixer);

    // 4-pole ZDF Resonant Filter
    const filter = create4PoleFilter(audioCtx, synthParams.cutoff, synthParams.resonance);
    voiceMixer.connect(filter.input);

    // Filter Envelope modulation
    const modPeak = Math.min(18000, synthParams.cutoff + synthParams.envMod * 6000);
    filter.setCutoff(synthParams.cutoff, now);
    filter.rampCutoff(modPeak, synthParams.attack, now);
    filter.rampCutoff(synthParams.cutoff + (modPeak - synthParams.cutoff) * synthParams.sustain, synthParams.decay, now + synthParams.attack);

    // Amp Envelope ADSR
    const ampGain = audioCtx.createGain();
    ampGain.gain.setValueAtTime(0.0001, now);
    ampGain.gain.exponentialRampToValueAtTime(0.42, now + Math.max(0.005, synthParams.attack));
    ampGain.gain.exponentialRampToValueAtTime(Math.max(0.0001, 0.42 * synthParams.sustain), now + Math.max(0.01, synthParams.attack + synthParams.decay));

    filter.output.connect(ampGain);
    ampGain.connect(masterLimiter);

    osc1.start(now);
    osc2.start(now);
    subOsc.start(now);

    activeVoices.set(midiPitch, { osc1, osc2, subOsc, filter, ampGain, startTime: now });

    // Highlight keyboard UI key
    highlightPianoKey(midiPitch, true);
  }

  function playSynthNoteOff(midiPitch) {
    if (!audioCtx || !activeVoices.has(midiPitch)) return;

    const voice = activeVoices.get(midiPitch);
    activeVoices.delete(midiPitch);

    const now = audioCtx.currentTime;
    const releaseTime = Math.max(0.02, synthParams.release);

    voice.ampGain.gain.cancelScheduledValues(now);
    voice.ampGain.gain.setValueAtTime(voice.ampGain.gain.value, now);
    voice.ampGain.gain.exponentialRampToValueAtTime(0.0001, now + releaseTime);

    voice.filter.cancelScheduled(now);
    voice.filter.rampCutoff(Math.max(20, synthParams.cutoff * 0.5), releaseTime, now);

    voice.osc1.stop(now + releaseTime + 0.05);
    voice.osc2.stop(now + releaseTime + 0.05);
    voice.subOsc.stop(now + releaseTime + 0.05);

    setTimeout(() => {
      try {
        voice.osc1.disconnect();
        voice.osc2.disconnect();
        voice.subOsc.disconnect();
        voice.ampGain.disconnect();
      } catch (e) {}
    }, (releaseTime + 0.1) * 1000);

    // Unhighlight keyboard UI key
    highlightPianoKey(midiPitch, false);
  }

  // =========================================================================
  // 2. Synthesized TR-909 Drum Engine (Web Audio Nodes)
  // =========================================================================

  function trigger909Kick(time = audioCtx.currentTime, pitchHz = 48, drive = 1.0, decayMs = 380) {
    const osc = audioCtx.createOscillator();
    const gain = audioCtx.createGain();

    // Pitch envelope: Rapid sweep from 165 Hz to fundamental pitch
    osc.frequency.setValueAtTime(165, time);
    osc.frequency.exponentialRampToValueAtTime(pitchHz, time + 0.024);
    osc.frequency.exponentialRampToValueAtTime(pitchHz * 0.88, time + (decayMs / 1000));

    // Amp envelope (clean studio headroom)
    gain.gain.setValueAtTime(0.0001, time);
    gain.gain.linearRampToValueAtTime(0.68 * Math.min(1.25, drive), time + 0.002);
    gain.gain.exponentialRampToValueAtTime(0.0001, time + (decayMs / 1000));

    // Band-limited click transient
    const clickNoise = audioCtx.createBufferSource();
    clickNoise.buffer = getSharedNoiseBuffer(audioCtx);
    const clickFilter = audioCtx.createBiquadFilter();
    clickFilter.type = 'bandpass';
    clickFilter.frequency.setValueAtTime(920, time);
    clickFilter.Q.setValueAtTime(1.4, time);
    const clickGain = audioCtx.createGain();
    clickGain.gain.setValueAtTime(0.24 * drive, time);
    clickGain.gain.exponentialRampToValueAtTime(0.0001, time + 0.012);

    clickNoise.connect(clickFilter);
    clickFilter.connect(clickGain);
    clickGain.connect(masterLimiter);
    clickNoise.start(time);
    clickNoise.stop(time + 0.016);

    osc.connect(gain);
    gain.connect(masterLimiter);

    osc.start(time);
    osc.stop(time + (decayMs / 1000) + 0.04);
  }

  function trigger909Snare(time = audioCtx.currentTime) {
    // Dual tuned drumhead oscillators (182 Hz + 328 Hz with fast pitch drop)
    const osc1 = audioCtx.createOscillator();
    const osc2 = audioCtx.createOscillator();
    const toneGain = audioCtx.createGain();

    osc1.type = 'triangle';
    osc2.type = 'sine';
    osc1.frequency.setValueAtTime(215, time);
    osc1.frequency.exponentialRampToValueAtTime(180, time + 0.032);
    osc2.frequency.setValueAtTime(355, time);
    osc2.frequency.exponentialRampToValueAtTime(318, time + 0.032);

    toneGain.gain.setValueAtTime(0.26, time);
    toneGain.gain.exponentialRampToValueAtTime(0.0001, time + 0.14);

    osc1.connect(toneGain);
    osc2.connect(toneGain);
    toneGain.connect(masterLimiter);

    osc1.start(time);
    osc2.start(time);
    osc1.stop(time + 0.16);
    osc2.stop(time + 0.16);

    // Snare wires using shared pre-allocated noise buffer
    const whiteNoise = audioCtx.createBufferSource();
    whiteNoise.buffer = getSharedNoiseBuffer(audioCtx);

    const hpf = audioCtx.createBiquadFilter();
    hpf.type = 'highpass';
    hpf.frequency.setValueAtTime(1500, time);
    const lpf = audioCtx.createBiquadFilter();
    lpf.type = 'lowpass';
    lpf.frequency.setValueAtTime(6500, time);

    const noiseGain = audioCtx.createGain();
    noiseGain.gain.setValueAtTime(0.32, time);
    noiseGain.gain.exponentialRampToValueAtTime(0.0001, time + 0.19);

    whiteNoise.connect(hpf);
    hpf.connect(lpf);
    lpf.connect(noiseGain);
    noiseGain.connect(masterLimiter);

    whiteNoise.start(time);
    whiteNoise.stop(time + 0.21);
  }

  function trigger909Hat(isOpen, time = audioCtx.currentTime) {
    if (!isOpen && activeOpenHatGain) {
      try {
        activeOpenHatGain.gain.cancelScheduledValues(time);
        activeOpenHatGain.gain.setValueAtTime(activeOpenHatGain.gain.value, time);
        activeOpenHatGain.gain.linearRampToValueAtTime(0.0001, time + 0.006);
      } catch (_) {}
      activeOpenHatGain = null;
    }

    const noiseSource = audioCtx.createBufferSource();
    noiseSource.buffer = getSharedNoiseBuffer(audioCtx);

    const hpf = audioCtx.createBiquadFilter();
    hpf.type = 'highpass';
    hpf.frequency.setValueAtTime(isOpen ? 7600 : 8400, time);

    const filter = audioCtx.createBiquadFilter();
    filter.type = 'bandpass';
    filter.frequency.setValueAtTime(isOpen ? 8800 : 9800, time);
    filter.Q.setValueAtTime(1.8, time);

    const gain = audioCtx.createGain();
    gain.gain.setValueAtTime(0.24, time);
    gain.gain.exponentialRampToValueAtTime(0.0001, time + (isOpen ? 0.28 : 0.042));
    if (isOpen) activeOpenHatGain = gain;

    noiseSource.connect(hpf);
    hpf.connect(filter);
    filter.connect(gain);
    gain.connect(masterLimiter);

    noiseSource.start(time);
    noiseSource.stop(time + (isOpen ? 0.30 : 0.046));
  }

  function trigger909Clap(time = audioCtx.currentTime) {
    const noiseBuf = getSharedNoiseBuffer(audioCtx);
    const impulseOffsets = [0, 0.010, 0.020];
    impulseOffsets.forEach((offset) => {
      const noiseSource = audioCtx.createBufferSource();
      noiseSource.buffer = noiseBuf;

      const filter = audioCtx.createBiquadFilter();
      filter.type = 'bandpass';
      filter.frequency.setValueAtTime(1350, time + offset);
      filter.Q.setValueAtTime(2.0, time + offset);

      const gain = audioCtx.createGain();
      gain.gain.setValueAtTime(0.25, time + offset);
      gain.gain.exponentialRampToValueAtTime(0.0001, time + offset + 0.015);

      noiseSource.connect(filter);
      filter.connect(gain);
      gain.connect(masterLimiter);

      noiseSource.start(time + offset);
      noiseSource.stop(time + offset + 0.02);
    });

    const tailSource = audioCtx.createBufferSource();
    tailSource.buffer = noiseBuf;

    const tailFilter = audioCtx.createBiquadFilter();
    tailFilter.type = 'bandpass';
    tailFilter.frequency.setValueAtTime(1200, time + 0.024);
    tailFilter.Q.setValueAtTime(1.6, time + 0.024);

    const tailGain = audioCtx.createGain();
    tailGain.gain.setValueAtTime(0.26, time + 0.024);
    tailGain.gain.exponentialRampToValueAtTime(0.0001, time + 0.20);

    tailSource.connect(tailFilter);
    tailFilter.connect(tailGain);
    tailGain.connect(masterLimiter);

    tailSource.start(time + 0.024);
    tailSource.stop(time + 0.22);
  }

  // =========================================================================
  // 3. 16-Step TR-909 Sequencer State & Lookahead Scheduler
  // =========================================================================

  const DRUM_TRACKS = ['kick', 'snare', 'closedHat', 'openHat', 'clap'];
  let currentBpm = 136;
  let isPlaying = false;
  let currentStep = 0;
  let nextStepTime = 0.0;
  let timerId = null;

  // Track Solo & Mute states
  const trackMute = { kick: false, snare: false, closedHat: false, openHat: false, clap: false };
  const trackSolo = { kick: false, snare: false, closedHat: false, openHat: false, clap: false };

  // 16-Step Pattern Grid (Initialized to Peak Techno 136)
  const patternGrid = {
    kick:      [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0],
    snare:     [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0],
    closedHat: [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
    openHat:   [0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0],
    clap:      [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0],
  };

  const groovePresets = {
    'techno-136': {
      bpm: 136,
      pattern: {
        kick:      [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0],
        snare:     [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0],
        closedHat: [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
        openHat:   [0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0],
        clap:      [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0],
      }
    },
    'house-128': {
      bpm: 128,
      pattern: {
        kick:      [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0],
        snare:     [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0],
        closedHat: [0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1],
        openHat:   [0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0],
        clap:      [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0],
      }
    },
    'rkt-105': {
      bpm: 105,
      pattern: {
        kick:      [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0],
        snare:     [0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 1, 0], // Tresillo syncopation
        closedHat: [1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0],
        openHat:   [0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1],
        clap:      [0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 1, 0],
      }
    },
    'dnb-174': {
      bpm: 174,
      pattern: {
        kick:      [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0],
        snare:     [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0],
        closedHat: [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
        openHat:   [0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0],
        clap:      [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0],
      }
    }
  };

  function isTrackAudible(trackName) {
    const hasAnySolo = Object.values(trackSolo).some(s => s);
    if (hasAnySolo) {
      return trackSolo[trackName];
    }
    return !trackMute[trackName];
  }

  function scheduleStep(stepNumber, time) {
    if (patternGrid.kick[stepNumber] && isTrackAudible('kick')) {
      trigger909Kick(time);
    }
    if (patternGrid.snare[stepNumber] && isTrackAudible('snare')) {
      trigger909Snare(time);
    }
    if (patternGrid.closedHat[stepNumber] && isTrackAudible('closedHat')) {
      trigger909Hat(false, time);
    }
    if (patternGrid.openHat[stepNumber] && isTrackAudible('openHat')) {
      trigger909Hat(true, time);
    }
    if (patternGrid.clap[stepNumber] && isTrackAudible('clap')) {
      trigger909Clap(time);
    }

    // Schedule visual playhead update
    const visualDelay = Math.max(0, (time - audioCtx.currentTime) * 1000);
    setTimeout(() => {
      updatePlayheadUI(stepNumber);
    }, visualDelay);
  }

  function sequencerScheduler() {
    // Lookahead loop scheduling audio steps within 0.1s
    while (nextStepTime < audioCtx.currentTime + 0.1) {
      scheduleStep(currentStep, nextStepTime);
      const secondsPerBeat = 60.0 / currentBpm;
      const stepDuration = secondsPerBeat / 4; // 16th notes
      nextStepTime += stepDuration;
      currentStep = (currentStep + 1) % 16;
    }
    timerId = setTimeout(sequencerScheduler, 25);
  }

  function startSequencer() {
    initAudio();
    if (isPlaying) return;

    isPlaying = true;
    currentStep = 0;
    nextStepTime = audioCtx.currentTime + 0.05;
    sequencerScheduler();

    const playBtn = document.getElementById('btn-play');
    if (playBtn) {
      playBtn.classList.add('active');
      playBtn.innerHTML = '❚❚ PAUSE';
    }
  }

  function stopSequencer() {
    if (!isPlaying) return;
    isPlaying = false;
    clearTimeout(timerId);

    const playBtn = document.getElementById('btn-play');
    if (playBtn) {
      playBtn.classList.remove('active');
      playBtn.innerHTML = '▶ PLAY';
    }

    // Clear playhead cursor
    document.querySelectorAll('.seq-step').forEach(el => el.classList.remove('playhead'));
  }

  function updatePlayheadUI(stepIndex) {
    document.querySelectorAll('.seq-step').forEach(el => {
      const step = parseInt(el.dataset.step, 10);
      if (step === stepIndex) {
        el.classList.add('playhead');
      } else {
        el.classList.remove('playhead');
      }
    });
  }

  // =========================================================================
  // 4. 60 FPS HTML5 Canvas DSP Visualizers
  // =========================================================================

  let oscCanvas, oscCtx;
  let specCanvas, specCtx;
  let filterCanvas, filterCtx;
  let isDraggingFilter = false;

  const fftPeakHold = new Float32Array(64);

  function setupVisualizers() {
    oscCanvas = document.getElementById('canvas-oscilloscope');
    specCanvas = document.getElementById('canvas-spectrum');
    filterCanvas = document.getElementById('canvas-filter');

    if (!oscCanvas || !specCanvas || !filterCanvas) return;

    const dpr = Math.min(window.devicePixelRatio || 1, 1.5);

    [oscCanvas, specCanvas, filterCanvas].forEach(c => {
      const rect = c.getBoundingClientRect();
      c.width = Math.floor(rect.width * dpr);
      c.height = Math.floor(rect.height * dpr);
    });

    oscCtx = oscCanvas.getContext('2d');
    specCtx = specCanvas.getContext('2d');
    filterCtx = filterCanvas.getContext('2d');

    // Interactive filter draggable handle
    filterCanvas.addEventListener('pointerdown', handleFilterPointerDown);
    window.addEventListener('pointermove', handleFilterPointerMove);
    window.addEventListener('pointerup', handleFilterPointerUp);

    // Start 60 FPS RAF Loop
    requestAnimationFrame(renderVisualizers);
  }

  function renderVisualizers() {
    requestAnimationFrame(renderVisualizers);

    if (!audioCtx || !masterAnalyser) {
      // Draw idle static grid
      drawIdleVisualizers();
      return;
    }

    // 1. Dual Phosphor Oscilloscope (Laser Cyan & Phosphor Trail)
    drawOscilloscope();

    // 2. 64-Band FFT Spectrum Analyzer with Peak Hold
    drawSpectrum();

    // 3. 4-Pole ZDF Filter Bode Magnitude Curve
    drawFilterBode();
  }

  function drawIdleVisualizers() {
    if (!oscCtx || !specCtx || !filterCtx) return;

    // Idle Osc
    oscCtx.fillStyle = '#050608';
    oscCtx.fillRect(0, 0, oscCanvas.width, oscCanvas.height);
    oscCtx.strokeStyle = 'rgba(0, 210, 255, 0.25)';
    oscCtx.lineWidth = 1.5;
    oscCtx.beginPath();
    oscCtx.moveTo(0, oscCanvas.height / 2);
    oscCtx.lineTo(oscCanvas.width, oscCanvas.height / 2);
    oscCtx.stroke();

    // Idle Spec
    specCtx.fillStyle = '#050608';
    specCtx.fillRect(0, 0, specCanvas.width, specCanvas.height);

    // Idle Filter Bode
    drawFilterBode();
  }

  function drawOscilloscope() {
    const bufferLength = masterAnalyser.fftSize;
    const timeData = new Uint8Array(bufferLength);
    masterAnalyser.getByteTimeDomainData(timeData);

    const w = oscCanvas.width;
    const h = oscCanvas.height;

    // Fading persistence decay (phosphor trace effect)
    oscCtx.fillStyle = 'rgba(5, 6, 8, 0.35)';
    oscCtx.fillRect(0, 0, w, h);

    // Subtle horizontal center grid line
    oscCtx.strokeStyle = 'rgba(255, 255, 255, 0.05)';
    oscCtx.lineWidth = 1;
    oscCtx.beginPath();
    oscCtx.moveTo(0, h / 2);
    oscCtx.lineTo(w, h / 2);
    oscCtx.stroke();

    // Primary laser cyan waveform
    oscCtx.lineWidth = 2.0;
    oscCtx.strokeStyle = '#00d2ff';
    oscCtx.shadowColor = 'rgba(0, 210, 255, 0.8)';
    oscCtx.shadowBlur = 6;

    oscCtx.beginPath();
    const sliceWidth = w / bufferLength;
    let x = 0;

    for (let i = 0; i < bufferLength; i++) {
      const v = timeData[i] / 128.0;
      const y = (v * h) / 2;

      if (i === 0) {
        oscCtx.moveTo(x, y);
      } else {
        oscCtx.lineTo(x, y);
      }
      x += sliceWidth;
    }
    oscCtx.stroke();
    oscCtx.shadowBlur = 0;
  }

  function drawSpectrum() {
    const binCount = 64;
    const freqData = new Uint8Array(masterAnalyser.frequencyBinCount);
    masterAnalyser.getByteFrequencyData(freqData);

    const w = specCanvas.width;
    const h = specCanvas.height;

    specCtx.fillStyle = '#050608';
    specCtx.fillRect(0, 0, w, h);

    const barWidth = (w / binCount) - 1.5;

    for (let i = 0; i < binCount; i++) {
      // Map to logarithmic bins
      const binIdx = Math.min(freqData.length - 1, Math.floor(Math.pow(i / binCount, 1.6) * freqData.length));
      const val = freqData[binIdx] / 255.0;
      const barHeight = val * h * 0.92;

      // Peak hold calculation
      if (val >= fftPeakHold[i]) {
        fftPeakHold[i] = val;
      } else {
        fftPeakHold[i] = Math.max(0, fftPeakHold[i] - 0.015);
      }

      // Color coding bands: Sub (pink), Low-mid (amber), High-mid (cyan), High (white)
      let fillCol = '#00d2ff';
      if (i < 8) {
        fillCol = '#ff007f'; // Sub
      } else if (i < 24) {
        fillCol = '#ffbe00'; // Low-mid
      } else if (i > 52) {
        fillCol = '#e0f7fa'; // Air high
      }

      const x = i * (barWidth + 1.5);
      const y = h - barHeight;

      specCtx.fillStyle = fillCol;
      specCtx.fillRect(x, y, barWidth, barHeight);

      // Draw peak hold bar cap
      const peakY = h - (fftPeakHold[i] * h * 0.92);
      specCtx.fillStyle = '#ffffff';
      specCtx.fillRect(x, Math.max(2, peakY - 2), barWidth, 1.5);
    }
  }

  function drawFilterBode() {
    const w = filterCanvas.width;
    const h = filterCanvas.height;

    filterCtx.fillStyle = '#050608';
    filterCtx.fillRect(0, 0, w, h);

    // Grid lines for frequency (100Hz, 1kHz, 10kHz) and 0 dB
    filterCtx.strokeStyle = 'rgba(255, 255, 255, 0.08)';
    filterCtx.lineWidth = 1;

    // 0 dB reference line at 40% height
    const zeroDbY = h * 0.42;
    filterCtx.beginPath();
    filterCtx.moveTo(0, zeroDbY);
    filterCtx.lineTo(w, zeroDbY);
    filterCtx.stroke();

    // Plot theoretical 4-pole ZDF Lowpass response: |H(f)| = 1 / sqrt(1 + (f/fc)^8)
    filterCtx.beginPath();
    filterCtx.lineWidth = 2.5;
    filterCtx.strokeStyle = '#ffbe00';
    filterCtx.shadowColor = 'rgba(255, 190, 0, 0.6)';
    filterCtx.shadowBlur = 4;

    const minFreq = 20;
    const maxFreq = 20000;
    const fc = synthParams.cutoff;
    const Q = synthParams.resonance;

    let puckX = 0;
    let puckY = 0;

    for (let x = 0; x <= w; x += 2) {
      const normX = x / w;
      const freq = minFreq * Math.pow(maxFreq / minFreq, normX);

      // 4-pole low-pass transfer magnitude approximation with resonant peak
      const ratio = freq / fc;
      const baseAtten = 1.0 / Math.sqrt(1.0 + Math.pow(ratio, 8));

      // Resonant peak around ratio = 1.0
      const peakFactor = Math.exp(-Math.pow(Math.log(ratio) * 2.5, 2)) * (Q * 0.28);
      const mag = (baseAtten + peakFactor);

      // Convert to display Y (40 dB range)
      const gainDb = 20 * Math.log10(Math.max(0.001, mag));
      const y = zeroDbY - (gainDb * (h * 0.035));

      if (x === 0) {
        filterCtx.moveTo(x, y);
      } else {
        filterCtx.lineTo(x, y);
      }

      // Track puck position at fc
      if (Math.abs(freq - fc) < fc * 0.05) {
        puckX = x;
        puckY = y;
      }
    }
    filterCtx.stroke();
    filterCtx.shadowBlur = 0;

    // Draw Interactive Draggable Puck handle at (Cutoff, Resonance)
    filterCtx.fillStyle = '#ffbe00';
    filterCtx.beginPath();
    filterCtx.arc(puckX, puckY, 6, 0, Math.PI * 2);
    filterCtx.fill();
    filterCtx.strokeStyle = '#ffffff';
    filterCtx.lineWidth = 1.5;
    filterCtx.stroke();

    // Readout label
    filterCtx.font = '10px ui-monospace, monospace';
    filterCtx.fillStyle = '#9aa5b8';
    filterCtx.fillText(`${Math.round(synthParams.cutoff)} Hz | Q: ${synthParams.resonance.toFixed(1)}`, 8, 16);
  }

  function handleFilterPointerDown(e) {
    isDraggingFilter = true;
    filterCanvas.setPointerCapture(e.pointerId);
    updateFilterFromPointer(e);
  }

  function handleFilterPointerMove(e) {
    if (!isDraggingFilter) return;
    updateFilterFromPointer(e);
  }

  function handleFilterPointerUp(e) {
    if (!isDraggingFilter) return;
    isDraggingFilter = false;
  }

  function updateFilterFromPointer(e) {
    const rect = filterCanvas.getBoundingClientRect();
    const x = Math.max(0, Math.min(rect.width, e.clientX - rect.left));
    const y = Math.max(0, Math.min(rect.height, e.clientY - rect.top));

    const normX = x / rect.width;
    const normY = 1.0 - (y / rect.height);

    const minFreq = 20;
    const maxFreq = 20000;
    const newCutoff = minFreq * Math.pow(maxFreq / minFreq, normX);
    const newRes = Math.max(0.5, Math.min(22, normY * 22));

    synthParams.cutoff = newCutoff;
    synthParams.resonance = newRes;

    // Sync input sliders
    const cutoffSlider = document.getElementById('slider-cutoff');
    const resSlider = document.getElementById('slider-resonance');
    if (cutoffSlider) cutoffSlider.value = newCutoff;
    if (resSlider) resSlider.value = newRes;

    const valCutoff = document.getElementById('val-cutoff');
    const valRes = document.getElementById('val-resonance');
    if (valCutoff) valCutoff.textContent = `${Math.round(newCutoff)} Hz`;
    if (valRes) valRes.textContent = newRes.toFixed(1);

    // Update active synth voices
    activeVoices.forEach(voice => {
      voice.filter.setCutoff(newCutoff);
      voice.filter.setResonance(newRes);
    });
  }

  // =========================================================================
  // 5. On-Screen Piano & Computer Keyboard Playing
  // =========================================================================

  const KEYBOARD_MAP = {
    'KeyA': 60, // C4 (standard middle C)
    'KeyW': 61, // C#4
    'KeyS': 62, // D4
    'KeyE': 63, // D#4
    'KeyD': 64, // E4
    'KeyF': 65, // F4
    'KeyT': 66, // F#4
    'KeyG': 67, // G4
    'KeyY': 68, // G#4
    'KeyH': 69, // A4 (440Hz)
    'KeyU': 70, // A#4
    'KeyJ': 71, // B4
    'KeyK': 72, // C5
  };

  const pressedKeys = new Set();

  function setupKeyboardListeners() {
    window.addEventListener('keydown', (e) => {
      // Ignore if user is typing in a text field
      if (['INPUT', 'TEXTAREA', 'SELECT'].includes(document.activeElement.tagName)) return;

      const code = e.code;
      if (KEYBOARD_MAP[code] && !pressedKeys.has(code)) {
        pressedKeys.add(code);
        const pitch = KEYBOARD_MAP[code] + (synthParams.octave - 3) * 12;
        playSynthNoteOn(pitch);
      }

      // Octave shifts Z / X
      if (code === 'KeyZ') {
        synthParams.octave = Math.max(1, synthParams.octave - 1);
        updateOctaveDisplay();
      }
      if (code === 'KeyX') {
        synthParams.octave = Math.min(6, synthParams.octave + 1);
        updateOctaveDisplay();
      }

      // Space to toggle sequencer
      if (code === 'Space') {
        e.preventDefault();
        if (isPlaying) stopSequencer();
        else startSequencer();
      }
    });

    window.addEventListener('keyup', (e) => {
      const code = e.code;
      if (KEYBOARD_MAP[code] && pressedKeys.has(code)) {
        pressedKeys.delete(code);
        const pitch = KEYBOARD_MAP[code] + (synthParams.octave - 3) * 12;
        playSynthNoteOff(pitch);
      }
    });

    // Touch & pointer clicks on onscreen piano keys
    document.querySelectorAll('.piano-key').forEach(keyEl => {
      const pitchOffset = parseInt(keyEl.dataset.pitchOffset, 10);

      const triggerNoteOn = () => {
        const pitch = 60 + pitchOffset + (synthParams.octave - 3) * 12;
        playSynthNoteOn(pitch);
      };

      const triggerNoteOff = () => {
        const pitch = 60 + pitchOffset + (synthParams.octave - 3) * 12;
        playSynthNoteOff(pitch);
      };

      keyEl.addEventListener('pointerdown', (e) => {
        e.preventDefault();
        triggerNoteOn();
      });
      keyEl.addEventListener('pointerup', triggerNoteOff);
      keyEl.addEventListener('pointerleave', triggerNoteOff);
    });
  }

  function highlightPianoKey(midiPitch, active) {
    const pitchOffset = ((midiPitch - 60) % 12 + 12) % 12;
    document.querySelectorAll(`.piano-key[data-pitch-offset="${pitchOffset}"]`).forEach(el => {
      if (active) el.classList.add('active');
      else el.classList.remove('active');
    });
  }

  function updateOctaveDisplay() {
    const el = document.getElementById('octave-display');
    if (el) el.textContent = `OCT ${synthParams.octave}`;
  }

  function updateAudioTelemetry() {
    const el = document.getElementById('telemetry-stats');
    if (el && audioCtx) {
      el.textContent = `${audioCtx.sampleRate / 1000} kHz | 64-band FFT`;
    }
  }

  // =========================================================================
  // 6. In-App AI Copilot Simulator (English & Rioplatense Spanish Intent Router)
  // =========================================================================

  const copilotResponses = [
    {
      keywords: ['kick', 'gordo', 'engorda', 'fatter', 'peso'],
      spanish: true,
      text: "¡Ajuste aplicado de una! Bajé la afinación fundamental del Kick 909 a 48 Hz para máximo impacto de sub, empujé el Roar / Saturator +6.5 dB y extendí el decay a 480 ms.",
      tools: ['drums_configure_pad', 'fx_set_saturator', 'analyze_rendered_audio'],
      action: () => {
        // Boost kick fundamental & set pattern to 4-on-the-floor
        patternGrid.kick = [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0];
        updateSequencerGridUI();
        if (!isPlaying) startSequencer();
        trigger909Kick(audioCtx ? audioCtx.currentTime : 0, 46, 1.4, 520);
      }
    },
    {
      keywords: ['acido', 'acid', '303', 'resonancia'],
      spanish: true,
      text: "Metí configuración Acid 303: Cutoff a 620 Hz, resonancia al palo en 4.8 con filtro ZDF y saturación en el paso de graves.",
      tools: ['synth_set_parameter', 'fx_set_saturator'],
      action: () => {
        synthParams.cutoff = 620;
        synthParams.resonance = 14.0;
        synthParams.envMod = 0.9;
        syncSynthSliders();
        playSynthNoteOn(48);
        setTimeout(() => playSynthNoteOff(48), 350);
      }
    },
    {
      keywords: ['techno', '136', 'drop', 'pista'],
      spanish: true,
      text: "Armando el drop de Peak Techno: Tempo ajustado a 136 BPM, 4-on-the-floor en el Kick, contratiempo de Open Hat y palmas en el 2 y 4.",
      tools: ['transport_set_bpm', 'clip_create_drums', 'transport_play'],
      action: () => {
        loadGroove('techno-136');
        startSequencer();
      }
    },
    {
      keywords: ['rkt', 'turreo', 'cachengue', 'dembow', 'tresillo'],
      spanish: true,
      text: "¡Ahí va el ritmo picante de RKT / Turreo! Tempo clavado en 105 BPM con el clásico tresillo en el redoblante y kick pesado.",
      tools: ['transport_set_bpm', 'clip_create_drums', 'groove_pool_apply_to_clip'],
      action: () => {
        loadGroove('rkt-105');
        startSequencer();
      }
    },
    {
      keywords: ['filtro', 'ott', 'barro', 'limpia', 'brillo', 'abrir'],
      spanish: true,
      text: "Filtro abierto hasta 4.2 kHz con compresión ascendente Multiband Dynamics (OTT) para resaltar armónicos superiores y aire.",
      tools: ['device_set_param', 'fx_set_limiter'],
      action: () => {
        synthParams.cutoff = 4200;
        synthParams.resonance = 4.2;
        syncSynthSliders();
        playSynthNoteOn(60);
        setTimeout(() => playSynthNoteOff(60), 400);
      }
    },
    {
      keywords: ['stems', 'separar', 'remix'],
      spanish: true,
      text: "Extrayendo 4 stems de remix: drums.wav (transientes), bass.wav (<140Hz mono), vocals.wav (formante medio) y synths_other.wav (estéreo residual). Listos en el matrix.",
      tools: ['stems_separate_remix', 'bounce_export_stems'],
      action: () => {}
    },
    {
      keywords: ['als', 'ableton', 'importar', 'exportar', 'live set'],
      spanish: true,
      text: "Sincronizando con Ableton Live 12: parseando XML comprimido en gzip (.als) con mapeo 1:1 de pistas, tempo, clips MIDI y cadenas de dispositivos.",
      tools: ['project_import_als', 'project_export_als'],
      action: () => {}
    }
  ];

  function processCopilotQuery(query) {
    const qLower = query.toLowerCase();
    let matched = copilotResponses.find(item => item.keywords.some(k => qLower.includes(k)));

    if (!matched) {
      matched = {
        text: `Comando interpretado: "${query}". Ejecutando mutaciones de audio y actualizando telemetría del motor Oxide Studio en tiempo real.`,
        tools: ['live12_exec_lom_query', 'transport_get_telemetry'],
        action: () => {}
      };
    }

    // Append user message
    appendChatBubble('user', query);

    // Execute real action
    if (matched.action) {
      matched.action();
    }

    // Append copilot reply
    setTimeout(() => {
      appendChatBubble('copilot', matched.text, matched.tools);
    }, 280);
  }

  function appendChatBubble(role, text, tools = []) {
    const container = document.getElementById('copilot-chat-history');
    if (!container) return;

    const bubble = document.createElement('div');
    bubble.className = `chat-bubble ${role}`;

    if (role === 'user') {
      bubble.textContent = text;
    } else {
      const p = document.createElement('p');
      p.textContent = text;
      bubble.appendChild(p);
      if (tools && tools.length > 0) {
        const toolsWrap = document.createElement('div');
        toolsWrap.className = 'copilot-tools';
        tools.forEach(t => {
          const badge = document.createElement('span');
          badge.className = 'tool-badge mcp-executed-badge';
          badge.title = 'Click to copy MCP tool name';
          badge.dataset.tool = t;
          badge.textContent = `⚡ ${t}`;
          badge.addEventListener('click', () => {
            navigator.clipboard?.writeText(t);
            const orig = badge.textContent;
            badge.textContent = `✓ Copied: ${t}`;
            setTimeout(() => { badge.textContent = orig; }, 1200);
          });
          toolsWrap.appendChild(badge);
        });
        bubble.appendChild(toolsWrap);
      }
    }

    container.appendChild(bubble);
    container.scrollTop = container.scrollHeight;
  }

  // =========================================================================
  // 7. 76 Native MCP Tools Catalog Data & Interactive Filter
  // =========================================================================

  const MCP_TOOLS_CATALOG = [
    // 1. Project & Transport
    { name: 'project_new', cat: 'project', desc: 'Create a new DAW project with default lead synth and 909 drum rack tracks.', schema: '{"name":"string"}' },
    { name: 'project_load', cat: 'project', desc: 'Load complete DAW project from a JSON project file on disk.', schema: '{"path":"string"}' },
    { name: 'project_save', cat: 'project', desc: 'Persist current DAW state to an atomic JSON project file on disk.', schema: '{"path":"string"}' },
    { name: 'project_import_als', cat: 'project', desc: 'Import native Ableton Live 12 XML Gzip project (.als) into Oxide Studio.', schema: '{"path":"project.als"}' },
    { name: 'project_export_als', cat: 'project', desc: 'Export Oxide Studio session to native Ableton Live 12 XML Gzip project (.als).', schema: '{"path":"exported.als"}' },
    { name: 'transport_play', cat: 'project', desc: 'Start real-time audio playback in the audio engine.', schema: '{}' },
    { name: 'transport_stop', cat: 'project', desc: 'Stop audio playback and reset playhead position.', schema: '{}' },
    { name: 'transport_set_bpm', cat: 'project', desc: 'Set global DAW tempo in BPM (20.0 to 999.0).', schema: '{"bpm": 136.0}' },
    { name: 'transport_get_telemetry', cat: 'project', desc: 'Query real-time DSP telemetry: RMS, peak dBFS, CPU load.', schema: '{}' },

    // 2. Track & Synths
    { name: 'track_create_synth', cat: 'synth', desc: 'Add a new synthesizer track with Mystran 2x-oversampled ZDF ladder filter.', schema: '{"name":"Acid Bass"}' },
    { name: 'track_create_drums', cat: 'synth', desc: 'Add a new 909 drum rack track with multi-pad electronic percussion.', schema: '{"name":"909 Kit"}' },
    { name: 'track_delete', cat: 'synth', desc: 'Remove a track by its integer track ID.', schema: '{"track_id": 2}' },
    { name: 'synth_list_presets', cat: 'synth', desc: 'Enumerate all 38 factory synth presets with metadata.', schema: '{}' },
    { name: 'synth_load_preset', cat: 'synth', desc: 'Load any of the 38 factory presets onto a synth track.', schema: '{"track_id": 0, "preset_id": "acid-303"}' },
    { name: 'synth_set_parameter', cat: 'synth', desc: 'Set a synth parameter (cutoff_hz, resonance, env_mod, attack).', schema: '{"track_id": 0, "param": "cutoff_hz", "value": 620.0}' },
    { name: 'synth_set_macro', cat: 'synth', desc: 'Apply XY Macro performance coordinate (0.0 to 1.0).', schema: '{"track_id": 0, "x": 0.75, "y": 0.45}' },
    { name: 'preview_synth_patch', cat: 'synth', desc: 'Render audio preview of a patch and return 4-band spectral analysis.', schema: '{"preset_id":"deep-sub"}' },

    // 3. Drums & Percussion
    { name: 'drums_configure_pad', cat: 'drums', desc: 'Configure 909 drum pad parameters (pitch, decay, level, choke).', schema: '{"track_id": 1, "pad": "kick", "pitch": 48.0, "decay": 480}' },
    { name: 'drums_trigger_pad', cat: 'drums', desc: 'Trigger a drum pad sample immediately with specified velocity.', schema: '{"track_id": 1, "pad": "kick", "velocity": 127}' },
    { name: 'simpler_slice_to_drum_rack', cat: 'drums', desc: 'Slice a sampled audio clip by transient or grid into a Drum Rack.', schema: '{"track_id": 1, "slice_mode": "transient"}' },

    // 4. Clips, Timeline & Generative MIDI
    { name: 'clip_create_midi', cat: 'clips', desc: 'Create a quantized MIDI clip with note events and velocity.', schema: '{"track_id": 0, "slot_id": 0, "notes": []}' },
    { name: 'clip_create_drums', cat: 'clips', desc: 'Create a 16-step TR-909 drum pattern clip.', schema: '{"track_id": 1, "slot_id": 0, "steps": []}' },
    { name: 'clip_create_lilypond', cat: 'clips', desc: 'Parse LilyPond notation sequence into a clip.', schema: '{"sequence": "c4 e4 g4 b4"}' },
    { name: 'clip_launch', cat: 'clips', desc: 'Launch a clip slot with musical quantization.', schema: '{"track_id": 0, "slot_id": 0}' },
    { name: 'clip_stop', cat: 'clips', desc: 'Stop playback of active clip on track.', schema: '{"track_id": 0}' },
    { name: 'scene_launch', cat: 'clips', desc: 'Launch all clips across a scene row.', schema: '{"scene_id": 0}' },
    { name: 'timeline_place_clip', cat: 'clips', desc: 'Place a clip on the Linear Arrangement timeline.', schema: '{"track_id": 0, "start_bar": 1.0}' },
    { name: 'timeline_add_automation', cat: 'clips', desc: 'Add piecewise automation breakpoints on timeline.', schema: '{"track_id": 0, "param": "cutoff", "points": []}' },
    { name: 'generate_chord_progression_clip', cat: 'generative', desc: 'Generate scale-locked chord progression (e.g. i-VI-III-VII in F Dorian).', schema: '{"scale": "Dorian", "root": "F"}' },
    { name: 'generate_bassline_clip', cat: 'generative', desc: 'Generate driving techno or rolling sub bassline clip.', schema: '{"style": "driving_techno"}' },
    { name: 'generate_arpeggio_clip', cat: 'generative', desc: 'Generate melodic arpeggiator patterns over chords.', schema: '{"pattern": "up_down"}' },
    { name: 'generate_euclidean_drum_clip', cat: 'generative', desc: 'Generate Euclidean rhythm pattern (Bjorklund algorithm).', schema: '{"hits": 5, "steps": 16}' },
    { name: 'generate_drum_groove_clip', cat: 'generative', desc: 'Generate genre-authentic drum patterns (Techno, House, RKT).', schema: '{"genre": "PeakTechno"}' },
    { name: 'build_song_arrangement', cat: 'generative', desc: 'Autonomously compose complete multi-track song arrangement.', schema: '{"bpm": 136, "bars": 64}' },
    { name: 'midi_transform_clip', cat: 'generative', desc: 'Apply Live 12 MIDI transform (connect, recombine, span, strum).', schema: '{"transform": "strum"}' },
    { name: 'midi_generate_clip', cat: 'generative', desc: 'Generate notes using Live 12 algorithms (seed, shape, stacks).', schema: '{"generator": "seed"}' },

    // 5. FX, Mixer & Mastering
    { name: 'mixer_set_channel', cat: 'fx', desc: 'Set track volume dB, pan, mute, solo, and crossfader.', schema: '{"track_id": 0, "volume_db": -3.0, "pan": 0.0}' },
    { name: 'fx_set_saturator', cat: 'fx', desc: 'Configure Padé [4/4] rational saturator drive and curve.', schema: '{"drive_db": 6.5}' },
    { name: 'fx_set_delay', cat: 'fx', desc: 'Configure tempo-synced ping-pong stereo delay.', schema: '{"time_16th": 3, "feedback": 0.45}' },
    { name: 'fx_set_reverb', cat: 'fx', desc: 'Configure feedback delay network (FDN) algorithmic reverb.', schema: '{"decay_s": 2.2, "damp": 0.3}' },
    { name: 'fx_set_limiter', cat: 'fx', desc: 'Configure brickwall master lookahead limiter.', schema: '{"ceiling_db": -0.3}' },
    { name: 'return_track_create', cat: 'fx', desc: 'Create auxiliary return bus track (Return A Reverb, Return B Delay).', schema: '{"name": "Return A Reverb"}' },
    { name: 'mixer_set_send', cat: 'fx', desc: 'Set auxiliary send level (dB) into a return track bus.', schema: '{"track_id": 0, "send_index": 0, "level_db": -6.0}' },
    { name: 'scale_set_global', cat: 'fx', desc: 'Set global song scale and root note (e.g. F Dorian).', schema: '{"root_note": "F", "scale_name": "Dorian"}' },

    // 6. Devices & Plugins
    { name: 'device_add_to_track', cat: 'devices', desc: 'Insert any Live 12 instrument or effect into track device chain.', schema: '{"track_id": 0, "device": "Roar"}' },
    { name: 'device_remove_from_track', cat: 'devices', desc: 'Delete device from track chain by index.', schema: '{"track_id": 0, "device_index": 1}' },
    { name: 'device_set_param', cat: 'devices', desc: 'Set parameter on any device in chain.', schema: '{"track_id": 0, "device_index": 0, "param": "Cutoff", "value": 1200}' },
    { name: 'device_get_chain', cat: 'devices', desc: 'Dump all devices and parameters in track chain.', schema: '{"track_id": 0}' },
    { name: 'plugin_scan', cat: 'devices', desc: 'Out-of-process crash-isolated AUv2 / VST3 / CLAP plugin scan.', schema: '{}' },
    { name: 'browser_search', cat: 'devices', desc: 'Search library browser items by query and tags.', schema: '{"query": "909 Kick"}' },
    { name: 'browser_find_similar_sounds', cat: 'devices', desc: 'Cosine similarity neural sound search (libmusicai reconstruction).', schema: '{"target_sample": "kick.wav"}' },

    // 7. Warping, Comping & Stems
    { name: 'audio_clip_create_warped', cat: 'warping', desc: 'Create audio clip with 6-mode warping enabled.', schema: '{"sample_path": "loop.wav", "warp_mode": "complex_pro"}' },
    { name: 'audio_clip_set_warp_mode', cat: 'warping', desc: 'Switch warp algorithm (beats, tones, texture, repitch, complex).', schema: '{"warp_mode": "beats"}' },
    { name: 'audio_clip_add_warp_marker', cat: 'warping', desc: 'Add piecewise warp marker anchoring sample time to beat time.', schema: '{"sample_time": 1.25, "beat_time": 2.0}' },
    { name: 'groove_pool_list', cat: 'warping', desc: 'List loaded Groove Pool templates (MPC 16th, TR-909 Shuffle, SP-1200).', schema: '{}' },
    { name: 'groove_pool_apply_to_clip', cat: 'warping', desc: 'Apply groove template non-destructively to clip notes.', schema: '{"groove_name": "MPC 16th", "amount": 0.65}' },
    { name: 'take_lane_create', cat: 'warping', desc: 'Add a new comping take lane to arrangement track.', schema: '{"track_id": 0, "name": "Take 2"}' },
    { name: 'stems_separate_remix', cat: 'warping', desc: '1-Click 4-stem DSP separation into drums, bass, vocals, and synths/other.', schema: '{"audio_path": "master.wav"}' },
    { name: 'bounce_wav', cat: 'warping', desc: 'Export project offline to 16/24/32-bit stereo WAV file.', schema: '{"output_path": "master.wav"}' },
    { name: 'bounce_export_stems', cat: 'warping', desc: 'Multi-stem track bounce export rendering all tracks as separate stems.', schema: '{"output_dir": "stems/"}' },
    { name: 'analyze_rendered_audio', cat: 'warping', desc: 'Perceptual 4-band spectral analysis ("Agent Ears").', schema: '{"path": "master.wav"}' },

    // 8. Live 12 GUI TCP RPC Server Bridge (127.0.0.1:9877)
    { name: 'live12_status', cat: 'rpc', desc: 'Check TCP connectivity to Oxide Studio GUI RPC server.', schema: '{}' },
    { name: 'live12_get_song_info', cat: 'rpc', desc: 'Query live song tempo, scale, root, and track count over TCP.', schema: '{}' },
    { name: 'live12_create_midi_track', cat: 'rpc', desc: 'Create a new MIDI track in live GUI window over TCP.', schema: '{"index": 0}' },
    { name: 'live12_create_audio_track', cat: 'rpc', desc: 'Create a new audio track in live GUI window over TCP.', schema: '{"index": 1}' },
    { name: 'live12_create_return_track', cat: 'rpc', desc: 'Create a new return track in live GUI window over TCP.', schema: '{}' },
    { name: 'live12_load_browser_item', cat: 'rpc', desc: 'Load instrument or sample from browser into track over TCP.', schema: '{"track_index": 0, "uri": "preset://acid"}' },
    { name: 'live12_create_clip', cat: 'rpc', desc: 'Create empty clip in track clip slot in live GUI over TCP.', schema: '{"track_index": 0, "slot_index": 0}' },
    { name: 'live12_add_notes', cat: 'rpc', desc: 'Add MIDI notes to clip in live GUI over TCP.', schema: '{"track_index": 0, "slot_index": 0, "notes": []}' },
    { name: 'live12_fire_clip', cat: 'rpc', desc: 'Trigger clip launch in live GUI window over TCP.', schema: '{"track_index": 0, "slot_index": 0}' },
    { name: 'live12_fire_scene', cat: 'rpc', desc: 'Trigger scene launch in live GUI window over TCP.', schema: '{"scene_index": 0}' },
    { name: 'live12_set_device_param', cat: 'rpc', desc: 'Set device parameter in live GUI window over TCP.', schema: '{"track_index": 0, "device_index": 0, "value": 0.8}' },
    { name: 'live12_get_track_devices', cat: 'rpc', desc: 'Enumerate devices and parameters in live GUI over TCP.', schema: '{"track_index": 0}' },
    { name: 'live12_set_tempo', cat: 'rpc', desc: 'Set song tempo in live GUI over TCP.', schema: '{"bpm": 136.0}' },
    { name: 'live12_set_scale', cat: 'rpc', desc: 'Set song scale and root note in live GUI over TCP.', schema: '{"root_note": 5, "scale_name": "Dorian"}' },
    { name: 'live12_exec_lom_query', cat: 'rpc', desc: 'Execute arbitrary Live Object Model (LOM) expression over TCP.', schema: '{"query": "song.tracks[0].volume"}' }
  ];

  function renderMcpTools(filterCategory = 'all', searchQuery = '') {
    const container = document.getElementById('mcp-tools-grid');
    if (!container) return;

    const query = searchQuery.toLowerCase().trim();

    const filtered = MCP_TOOLS_CATALOG.filter(tool => {
      const matchCat = filterCategory === 'all' || tool.cat === filterCategory;
      const matchQuery = !query || tool.name.toLowerCase().includes(query) || tool.desc.toLowerCase().includes(query);
      return matchCat && matchQuery;
    });

    container.innerHTML = '';

    filtered.forEach(tool => {
      const card = document.createElement('div');
      card.className = 'tool-card';

      card.innerHTML = `
        <div class="tool-card-head">
          <span class="tool-name">${tool.name}</span>
          <span class="tool-category-badge">${tool.cat}</span>
        </div>
        <p class="tool-desc">${tool.desc}</p>
        <button class="tool-copy-btn" data-tool="${tool.name}" data-schema='${tool.schema}'>Copy JSON-RPC</button>
      `;

      container.appendChild(card);
    });

    // Wire copy buttons
    container.querySelectorAll('.tool-copy-btn').forEach(btn => {
      btn.addEventListener('click', async (e) => {
        const toolName = btn.dataset.tool;
        const schema = btn.dataset.schema;
        const snippet = JSON.stringify({
          jsonrpc: "2.0",
          id: 1,
          method: "tools/call",
          params: {
            name: toolName,
            arguments: JSON.parse(schema)
          }
        }, null, 2);

        try {
          await navigator.clipboard.writeText(snippet);
          btn.textContent = 'COPIED ✓';
          setTimeout(() => btn.textContent = 'Copy JSON-RPC', 1800);
        } catch (err) {
          btn.textContent = 'ERROR';
        }
      });
    });

    // Update count indicator
    const countEl = document.getElementById('mcp-tool-count');
    if (countEl) {
      countEl.textContent = `Showing ${filtered.length} of ${MCP_TOOLS_CATALOG.length} Tools`;
    }
  }

  // =========================================================================
  // 8. UI Event Handlers, Sequencer Grid Setup & Factory Presets
  // =========================================================================

  function setupSequencerGridUI() {
    const gridContainer = document.getElementById('sequencer-grid');
    if (!gridContainer) return;

    gridContainer.innerHTML = '';

    const labelMap = {
      kick: '909 KICK',
      snare: '909 SNARE',
      closedHat: 'CH HAT',
      openHat: 'OP HAT',
      clap: '909 CLAP'
    };

    DRUM_TRACKS.forEach(trackName => {
      const trackRow = document.createElement('div');
      trackRow.className = 'seq-track';

      const meta = document.createElement('div');
      meta.className = 'seq-track-meta';
      meta.innerHTML = `
        <span class="track-label">${labelMap[trackName]}</span>
        <div class="track-ms">
          <button class="btn-ms btn-m" data-track="${trackName}">M</button>
          <button class="btn-ms btn-s" data-track="${trackName}">S</button>
        </div>
      `;
      trackRow.appendChild(meta);

      for (let step = 0; step < 16; step++) {
        const stepBtn = document.createElement('div');
        stepBtn.className = 'seq-step';
        stepBtn.dataset.track = trackName;
        stepBtn.dataset.step = step;

        if (patternGrid[trackName][step]) {
          stepBtn.classList.add('active');
          if (trackName === 'snare') stepBtn.classList.add('active-snare');
          if (trackName === 'closedHat' || trackName === 'openHat') stepBtn.classList.add('active-hat');
          if (trackName === 'clap') stepBtn.classList.add('active-clap');
        }

        stepBtn.addEventListener('click', () => {
          initAudio();
          patternGrid[trackName][step] = patternGrid[trackName][step] ? 0 : 1;
          stepBtn.classList.toggle('active');

          if (patternGrid[trackName][step]) {
            // Preview sound on trigger
            if (trackName === 'kick') trigger909Kick();
            if (trackName === 'snare') trigger909Snare();
            if (trackName === 'closedHat') trigger909Hat(false);
            if (trackName === 'openHat') trigger909Hat(true);
            if (trackName === 'clap') trigger909Clap();
          }
        });

        trackRow.appendChild(stepBtn);
      }

      gridContainer.appendChild(trackRow);
    });

    // Wire mute / solo buttons
    gridContainer.querySelectorAll('.btn-m').forEach(btn => {
      btn.addEventListener('click', () => {
        const t = btn.dataset.track;
        trackMute[t] = !trackMute[t];
        btn.classList.toggle('active-m', trackMute[t]);
      });
    });

    gridContainer.querySelectorAll('.btn-s').forEach(btn => {
      btn.addEventListener('click', () => {
        const t = btn.dataset.track;
        trackSolo[t] = !trackSolo[t];
        btn.classList.toggle('active-s', trackSolo[t]);
      });
    });
  }

  function updateSequencerGridUI() {
    document.querySelectorAll('.seq-step').forEach(btn => {
      const track = btn.dataset.track;
      const step = parseInt(btn.dataset.step, 10);
      const isActive = patternGrid[track] && patternGrid[track][step];

      btn.classList.toggle('active', !!isActive);
      if (isActive) {
        if (track === 'snare') btn.classList.add('active-snare');
        if (track === 'closedHat' || track === 'openHat') btn.classList.add('active-hat');
        if (track === 'clap') btn.classList.add('active-clap');
      }
    });
  }

  function loadGroove(grooveKey) {
    if (!groovePresets[grooveKey]) return;
    const groove = groovePresets[grooveKey];
    currentBpm = groove.bpm;

    const bpmSlider = document.getElementById('slider-bpm');
    const bpmVal = document.getElementById('val-bpm');
    if (bpmSlider) bpmSlider.value = currentBpm;
    if (bpmVal) bpmVal.textContent = currentBpm;

    DRUM_TRACKS.forEach(track => {
      patternGrid[track] = [...groove.pattern[track]];
    });

    updateSequencerGridUI();
  }

  function syncSynthSliders() {
    const cutoffSlider = document.getElementById('slider-cutoff');
    const resSlider = document.getElementById('slider-resonance');
    const attackSlider = document.getElementById('slider-attack');
    const decaySlider = document.getElementById('slider-decay');

    if (cutoffSlider) cutoffSlider.value = synthParams.cutoff;
    if (resSlider) resSlider.value = synthParams.resonance;
    if (attackSlider) attackSlider.value = synthParams.attack;
    if (decaySlider) decaySlider.value = synthParams.decay;

    const valCutoff = document.getElementById('val-cutoff');
    const valRes = document.getElementById('val-resonance');
    if (valCutoff) valCutoff.textContent = `${Math.round(synthParams.cutoff)} Hz`;
    if (valRes) valRes.textContent = synthParams.resonance.toFixed(1);
  }

  function setupDOMEventListeners() {
    // Transport controls
    const playBtn = document.getElementById('btn-play');
    if (playBtn) {
      playBtn.addEventListener('click', () => {
        if (isPlaying) stopSequencer();
        else startSequencer();
      });
    }

    const stopBtn = document.getElementById('btn-stop');
    if (stopBtn) {
      stopBtn.addEventListener('click', () => {
        stopSequencer();
      });
    }

    const clearBtn = document.getElementById('btn-clear');
    if (clearBtn) {
      clearBtn.addEventListener('click', () => {
        DRUM_TRACKS.forEach(t => patternGrid[t].fill(0));
        updateSequencerGridUI();
      });
    }

    // BPM Slider
    const bpmSlider = document.getElementById('slider-bpm');
    const bpmVal = document.getElementById('val-bpm');
    if (bpmSlider) {
      bpmSlider.addEventListener('input', (e) => {
        currentBpm = parseInt(e.target.value, 10);
        if (bpmVal) bpmVal.textContent = currentBpm;
      });
    }

    // Groove Select
    const grooveSelect = document.getElementById('select-groove');
    if (grooveSelect) {
      grooveSelect.addEventListener('change', (e) => {
        loadGroove(e.target.value);
      });
    }

    // Synth Preset Select
    const synthPresetSelect = document.getElementById('select-synth-preset');
    if (synthPresetSelect) {
      synthPresetSelect.addEventListener('change', (e) => {
        const preset = e.target.value;
        if (preset === 'acid-lead') {
          synthParams.oscType = 'sawtooth';
          synthParams.cutoff = 620;
          synthParams.resonance = 14.0;
          synthParams.envMod = 0.85;
          synthParams.attack = 0.005;
          synthParams.decay = 0.28;
          synthParams.sustain = 0.2;
        } else if (preset === 'deep-sub') {
          synthParams.oscType = 'sine';
          synthParams.cutoff = 380;
          synthParams.resonance = 1.8;
          synthParams.envMod = 0.3;
          synthParams.attack = 0.01;
          synthParams.decay = 0.6;
          synthParams.sustain = 0.8;
        } else if (preset === 'dorian-pad') {
          synthParams.oscType = 'sawtooth';
          synthParams.cutoff = 1800;
          synthParams.resonance = 3.5;
          synthParams.envMod = 0.5;
          synthParams.attack = 0.35;
          synthParams.decay = 1.2;
          synthParams.sustain = 0.7;
        } else if (preset === 'punchy-pluck') {
          synthParams.oscType = 'square';
          synthParams.cutoff = 2200;
          synthParams.resonance = 7.0;
          synthParams.envMod = 0.8;
          synthParams.attack = 0.005;
          synthParams.decay = 0.18;
          synthParams.sustain = 0.1;
        }
        syncSynthSliders();
      });
    }

    // Synth Sliders
    const cutoffSlider = document.getElementById('slider-cutoff');
    if (cutoffSlider) {
      cutoffSlider.addEventListener('input', (e) => {
        synthParams.cutoff = parseFloat(e.target.value);
        const valCutoff = document.getElementById('val-cutoff');
        if (valCutoff) valCutoff.textContent = `${Math.round(synthParams.cutoff)} Hz`;
        activeVoices.forEach(v => v.filter.setCutoff(synthParams.cutoff));
      });
    }

    const resSlider = document.getElementById('slider-resonance');
    if (resSlider) {
      resSlider.addEventListener('input', (e) => {
        synthParams.resonance = parseFloat(e.target.value);
        const valRes = document.getElementById('val-resonance');
        if (valRes) valRes.textContent = synthParams.resonance.toFixed(1);
        activeVoices.forEach(v => v.filter.setResonance(synthParams.resonance));
      });
    }

    const attackSlider = document.getElementById('slider-attack');
    if (attackSlider) {
      attackSlider.addEventListener('input', (e) => {
        synthParams.attack = parseFloat(e.target.value);
        const valAttack = document.getElementById('val-attack');
        if (valAttack) valAttack.textContent = `${Math.round(synthParams.attack * 1000)} ms`;
      });
    }

    const decaySlider = document.getElementById('slider-decay');
    if (decaySlider) {
      decaySlider.addEventListener('input', (e) => {
        synthParams.decay = parseFloat(e.target.value);
        const valDecay = document.getElementById('val-decay');
        if (valDecay) valDecay.textContent = `${Math.round(synthParams.decay * 1000)} ms`;
      });
    }

    // AI Copilot Chat input and quick chips
    const chatInput = document.getElementById('copilot-chat-input');
    const sendBtn = document.getElementById('btn-copilot-send');

    if (chatInput && sendBtn) {
      const handleSend = () => {
        const text = chatInput.value.trim();
        if (text) {
          processCopilotQuery(text);
          chatInput.value = '';
        }
      };

      sendBtn.addEventListener('click', handleSend);
      chatInput.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') handleSend();
      });
    }

    document.querySelectorAll('.copilot-chip').forEach(chip => {
      chip.addEventListener('click', () => {
        const query = chip.dataset.prompt;
        if (query) {
          processCopilotQuery(query);
        }
      });
    });

    // MCP Tools Category Pills & Search Filter
    document.querySelectorAll('.cat-pill').forEach(pill => {
      pill.addEventListener('click', () => {
        document.querySelectorAll('.cat-pill').forEach(p => p.classList.remove('active'));
        pill.classList.add('active');
        const cat = pill.dataset.cat;
        const searchInput = document.getElementById('mcp-search-input');
        renderMcpTools(cat, searchInput ? searchInput.value : '');
      });
    });

    const mcpSearch = document.getElementById('mcp-search-input');
    if (mcpSearch) {
      mcpSearch.addEventListener('input', (e) => {
        const activePill = document.querySelector('.cat-pill.active');
        const cat = activePill ? activePill.dataset.cat : 'all';
        renderMcpTools(cat, e.target.value);
      });
    }

    // Architecture Tabs Switcher
    document.querySelectorAll('.tab-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
        btn.classList.add('active');

        const tabId = btn.dataset.tab;
        document.querySelectorAll('.tab-content').forEach(content => {
          content.style.display = content.id === tabId ? 'grid' : 'none';
        });
      });
    });
  }

  // =========================================================================
  // 9. Window Initialization
  // =========================================================================

  window.addEventListener('DOMContentLoaded', () => {
    setupVisualizers();
    setupSequencerGridUI();
    setupKeyboardListeners();
    setupDOMEventListeners();
    renderMcpTools('all', '');

    // User gesture unlock for Web Audio
    const unlockAudio = () => {
      initAudio();
      window.removeEventListener('pointerdown', unlockAudio);
      window.removeEventListener('keydown', unlockAudio);
    };
    window.addEventListener('pointerdown', unlockAudio, { once: true });
    window.addEventListener('keydown', unlockAudio, { once: true });
  });

})();
