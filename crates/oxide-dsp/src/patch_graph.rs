//! Modular Patch Graph & Live DSP Runtime (`Max for Live / .amxd Equivalent`).
//!
//! Provides a zero-allocation fixed-size modular DSP graph:
//! - Up to 32 patch nodes and 64 patch cables.
//! - Node kinds: `Oscillator`, `StateVariableFilter`, `EnvFollower`, `LfoModulator`,
//!   `RingMod`, `DelayLine`, `MathAdd`, `MathMul`, `LomParamMapper`.
//! - Topological sorting with cycle-safe execution handling (1-sample feedback delay).
//! - Frame-accurate audio evaluation and Live Object Model (LOM) parameter modulation.
//! - Presets: `preset_lfo_filter_modulator`, `preset_fm_ring_effect`.

use std::f32::consts::PI;

pub const MAX_PATCH_NODES: usize = 32;
pub const MAX_PATCH_CABLES: usize = 64;
pub const MAX_DELAY_SAMPLES: usize = 4096;

/// Waveform shapes for patch oscillator nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchOscShape {
    Sine,
    Saw,
    Square,
    Triangle,
}

/// Filter modes for patch state variable filter nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchFilterMode {
    Lowpass,
    Highpass,
    Bandpass,
    Notch,
}

/// Errors returned by patch graph operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchGraphError {
    NodeCapacityExceeded,
    CableCapacityExceeded,
    InvalidNodeId(u32),
    InvalidOutlet(u32),
    InvalidInlet(u32),
    CableNotFound(u32),
}

/// Kind and static configuration of a DSP patch node.
#[derive(Debug, Clone, PartialEq)]
pub enum PatchNodeKind {
    /// Audio oscillator with frequency and waveform shape.
    Oscillator { freq: f32, shape: PatchOscShape },
    /// Chamberlin 2-pole state variable filter.
    StateVariableFilter { cutoff: f32, resonance: f32, mode: PatchFilterMode },
    /// Peak / RMS envelope follower.
    EnvFollower { attack_ms: f32, release_ms: f32 },
    /// Low frequency modulator.
    LfoModulator { rate_hz: f32, depth: f32 },
    /// Ring modulator / frequency shifter.
    RingMod { carrier_freq: f32 },
    /// Circular buffer delay line with feedback.
    DelayLine { delay_samples: usize, feedback: f32 },
    /// Mathematical addition / offset.
    MathAdd { operand: f32 },
    /// Mathematical multiplication / gain.
    MathMul { factor: f32 },
    /// Maps node signal into Live Object Model (LOM) parameter value.
    LomParamMapper {
        target_track: u32,
        target_device: u32,
        target_param: u32,
        min_val: f32,
        max_val: f32,
    },
}

/// Connection cable between two nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatchCable {
    pub id: u32,
    pub from_node: u32,
    pub from_outlet: u32,
    pub to_node: u32,
    pub to_inlet: u32,
    pub active: bool,
}

/// Internal DSP state and scratch buffers for a patch node.
#[derive(Clone)]
pub struct NodeDspState {
    pub phase: f32,
    pub svf_ic1eq: f32,
    pub svf_ic2eq: f32,
    pub env_val: f32,
    pub delay_buf: Vec<f32>,
    pub delay_write: usize,
    pub carrier_phase: f32,
    pub inlets: [f32; 4],
    pub inlet_connected: [bool; 4],
    pub outlets: [f32; 4],
}

impl Default for NodeDspState {
    fn default() -> Self {
        Self {
            phase: 0.0,
            svf_ic1eq: 0.0,
            svf_ic2eq: 0.0,
            env_val: 0.0,
            delay_buf: vec![0.0; MAX_DELAY_SAMPLES],
            delay_write: 0,
            carrier_phase: 0.0,
            inlets: [0.0; 4],
            inlet_connected: [false; 4],
            outlets: [0.0; 4],
        }
    }
}

/// Container for a single patch node in the graph.
#[derive(Debug, Clone, PartialEq)]
pub struct PatchNode {
    pub id: u32,
    pub kind: PatchNodeKind,
    pub active: bool,
}

/// Zero-allocation modular patch graph runtime.
pub struct ModularPatchGraph {
    pub sample_rate: f32,
    nodes: [Option<PatchNode>; MAX_PATCH_NODES],
    states: [NodeDspState; MAX_PATCH_NODES],
    cables: [Option<PatchCable>; MAX_PATCH_CABLES],
    execution_order: [u32; MAX_PATCH_NODES],
    execution_order_len: usize,
    node_order_rank: [usize; MAX_PATCH_NODES],
    pub audio_input_node: Option<u32>,
    pub audio_output_node_l: Option<u32>,
    pub audio_output_node_r: Option<u32>,
    pub last_lom_target: Option<(u32, u32, u32, f32)>,
}

impl ModularPatchGraph {
    pub fn new(sample_rate: f32) -> Self {
        let nodes = std::array::from_fn(|_| None);
        let states = std::array::from_fn(|_| NodeDspState::default());
        let cables = std::array::from_fn(|_| None);

        let mut graph = Self {
            sample_rate: if sample_rate <= 0.0 { 44100.0 } else { sample_rate },
            nodes,
            states,
            cables,
            execution_order: [0; MAX_PATCH_NODES],
            execution_order_len: 0,
            node_order_rank: [usize::MAX; MAX_PATCH_NODES],
            audio_input_node: None,
            audio_output_node_l: None,
            audio_output_node_r: None,
            last_lom_target: None,
        };
        graph.compile();
        graph
    }

    /// Add a new node to the modular patch graph.
    pub fn add_node(&mut self, kind: PatchNodeKind) -> Result<u32, PatchGraphError> {
        for id in 0..MAX_PATCH_NODES {
            if self.nodes[id].is_none() {
                self.nodes[id] = Some(PatchNode {
                    id: id as u32,
                    kind,
                    active: true,
                });
                self.states[id] = NodeDspState::default();
                self.compile();
                return Ok(id as u32);
            }
        }
        Err(PatchGraphError::NodeCapacityExceeded)
    }

    /// Connect an outlet of `from_node` to an inlet of `to_node`.
    pub fn connect_cable(
        &mut self,
        from_node: u32,
        from_outlet: u32,
        to_node: u32,
        to_inlet: u32,
    ) -> Result<u32, PatchGraphError> {
        let from_idx = from_node as usize;
        let to_idx = to_node as usize;

        if from_idx >= MAX_PATCH_NODES || self.nodes[from_idx].is_none() {
            return Err(PatchGraphError::InvalidNodeId(from_node));
        }
        if to_idx >= MAX_PATCH_NODES || self.nodes[to_idx].is_none() {
            return Err(PatchGraphError::InvalidNodeId(to_node));
        }
        if from_outlet >= 4 {
            return Err(PatchGraphError::InvalidOutlet(from_outlet));
        }
        if to_inlet >= 4 {
            return Err(PatchGraphError::InvalidInlet(to_inlet));
        }

        for cable_idx in 0..MAX_PATCH_CABLES {
            if self.cables[cable_idx].is_none() {
                self.cables[cable_idx] = Some(PatchCable {
                    id: cable_idx as u32,
                    from_node,
                    from_outlet,
                    to_node,
                    to_inlet,
                    active: true,
                });
                self.compile();
                return Ok(cable_idx as u32);
            }
        }
        Err(PatchGraphError::CableCapacityExceeded)
    }

    /// Disconnect an existing cable by ID.
    pub fn disconnect_cable(&mut self, cable_id: u32) -> Result<(), PatchGraphError> {
        let idx = cable_id as usize;
        if idx < MAX_PATCH_CABLES && self.cables[idx].is_some() {
            self.cables[idx] = None;
            self.compile();
            Ok(())
        } else {
            Err(PatchGraphError::CableNotFound(cable_id))
        }
    }

    /// Remove an existing node and automatically prune all cables connected to it.
    pub fn remove_node(&mut self, node_id: u32) -> Result<(), PatchGraphError> {
        let idx = node_id as usize;
        if idx >= MAX_PATCH_NODES || self.nodes[idx].is_none() {
            return Err(PatchGraphError::InvalidNodeId(node_id));
        }
        self.nodes[idx] = None;
        self.states[idx] = NodeDspState::default();

        for cable_opt in &mut self.cables {
            if let Some(cable) = cable_opt {
                if cable.from_node == node_id || cable.to_node == node_id {
                    *cable_opt = None;
                }
            }
        }

        if self.audio_input_node == Some(node_id) {
            self.audio_input_node = None;
        }
        if self.audio_output_node_l == Some(node_id) {
            self.audio_output_node_l = None;
        }
        if self.audio_output_node_r == Some(node_id) {
            self.audio_output_node_r = None;
        }

        self.compile();
        Ok(())
    }

    /// Toggle node active state (enable or bypass).
    pub fn set_node_active(&mut self, node_id: u32, active: bool) -> Result<(), PatchGraphError> {
        let idx = node_id as usize;
        if idx >= MAX_PATCH_NODES || self.nodes[idx].is_none() {
            return Err(PatchGraphError::InvalidNodeId(node_id));
        }
        if let Some(ref mut node) = self.nodes[idx] {
            node.active = active;
        }
        self.compile();
        Ok(())
    }

    /// Toggle cable active state (enable or mute connection).
    pub fn set_cable_active(&mut self, cable_id: u32, active: bool) -> Result<(), PatchGraphError> {
        let idx = cable_id as usize;
        if idx >= MAX_PATCH_CABLES || self.cables[idx].is_none() {
            return Err(PatchGraphError::CableNotFound(cable_id));
        }
        if let Some(ref mut cable) = self.cables[idx] {
            cable.active = active;
        }
        self.compile();
        Ok(())
    }

    /// Get total number of allocated nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.iter().flatten().count()
    }

    /// Get count of active nodes.
    pub fn active_nodes_count(&self) -> usize {
        self.nodes.iter().flatten().filter(|n| n.active).count()
    }

    /// Get total number of connected cables.
    pub fn cable_count(&self) -> usize {
        self.cables.iter().flatten().count()
    }

    /// Get count of active cables.
    pub fn active_cables_count(&self) -> usize {
        self.cables.iter().flatten().filter(|c| c.active).count()
    }

    /// Inspect a node by ID.
    pub fn get_node(&self, node_id: u32) -> Option<&PatchNode> {
        let idx = node_id as usize;
        if idx < MAX_PATCH_NODES {
            self.nodes[idx].as_ref()
        } else {
            None
        }
    }

    /// Inspect a cable by ID.
    pub fn get_cable(&self, cable_id: u32) -> Option<&PatchCable> {
        let idx = cable_id as usize;
        if idx < MAX_PATCH_CABLES {
            self.cables[idx].as_ref()
        } else {
            None
        }
    }

    /// Query the current output value of a node outlet for real-time observation / telemetry.
    pub fn get_node_outlet(&self, node_id: u32, outlet: u32) -> Option<f32> {
        let idx = node_id as usize;
        if idx < MAX_PATCH_NODES && self.nodes[idx].is_some() && (outlet as usize) < 4 {
            Some(self.states[idx].outlets[outlet as usize])
        } else {
            None
        }
    }

    /// Query the current input accumulator value of a node inlet.
    pub fn get_node_inlet(&self, node_id: u32, inlet: u32) -> Option<f32> {
        let idx = node_id as usize;
        if idx < MAX_PATCH_NODES && self.nodes[idx].is_some() && (inlet as usize) < 4 {
            Some(self.states[idx].inlets[inlet as usize])
        } else {
            None
        }
    }

    /// Clear all nodes and cables from the graph.
    pub fn clear(&mut self) {
        self.nodes = std::array::from_fn(|_| None);
        self.states = std::array::from_fn(|_| NodeDspState::default());
        self.cables = std::array::from_fn(|_| None);
        self.execution_order = [0; MAX_PATCH_NODES];
        self.execution_order_len = 0;
        self.node_order_rank = [usize::MAX; MAX_PATCH_NODES];
        self.audio_input_node = None;
        self.audio_output_node_l = None;
        self.audio_output_node_r = None;
        self.last_lom_target = None;
    }

    /// Perform cycle-safe topological sort to determine execution order.
    pub fn compile(&mut self) {
        let mut in_degrees = [0u32; MAX_PATCH_NODES];
        let mut active_nodes = [false; MAX_PATCH_NODES];

        for i in 0..MAX_PATCH_NODES {
            if let Some(ref node) = self.nodes[i] {
                if node.active {
                    active_nodes[i] = true;
                }
            }
        }

        // Calculate in-degrees based on active cables
        for cable_opt in &self.cables {
            if let Some(cable) = cable_opt {
                if cable.active
                    && active_nodes[cable.from_node as usize]
                    && active_nodes[cable.to_node as usize]
                {
                    in_degrees[cable.to_node as usize] += 1;
                }
            }
        }

        // Kahn's topological sort
        let mut queue = [0u32; MAX_PATCH_NODES];
        let mut q_head = 0;
        let mut q_tail = 0;

        for i in 0..MAX_PATCH_NODES {
            if active_nodes[i] && in_degrees[i] == 0 {
                queue[q_tail] = i as u32;
                q_tail += 1;
            }
        }

        let mut order_idx = 0;
        let mut visited = [false; MAX_PATCH_NODES];

        while q_head < q_tail {
            let u = queue[q_head];
            q_head += 1;
            visited[u as usize] = true;
            self.execution_order[order_idx] = u;
            order_idx += 1;

            for cable_opt in &self.cables {
                if let Some(cable) = cable_opt {
                    if cable.active && cable.from_node == u {
                        let v = cable.to_node as usize;
                        if in_degrees[v] > 0 {
                            in_degrees[v] -= 1;
                            if in_degrees[v] == 0 {
                                queue[q_tail] = v as u32;
                                q_tail += 1;
                            }
                        }
                    }
                }
            }
        }

        // Cycle handling: any active unvisited nodes are part of a feedback cycle.
        // Append them directly — cycle feedback will safely use 1-sample delayed previous-frame output.
        for i in 0..MAX_PATCH_NODES {
            if active_nodes[i] && !visited[i] {
                self.execution_order[order_idx] = i as u32;
                order_idx += 1;
            }
        }

        self.execution_order_len = order_idx;
        self.node_order_rank = [usize::MAX; MAX_PATCH_NODES];
        for i in 0..order_idx {
            self.node_order_rank[self.execution_order[i] as usize] = i;
        }
    }

    /// Real-time frame processing routine with zero heap allocations.
    /// Returns stereo audio (left, right) and an optional LOM modulation tuple:
    /// `(target_device, target_param, value)`.
    pub fn process_frame(
        &mut self,
        input_l: f32,
        input_r: f32,
    ) -> (f32, f32, Option<(u32, u32, f32)>) {
        let mut lom_out: Option<(u32, u32, f32)> = None;

        // Reset inlet accumulators and clear connection flags
        for i in 0..self.execution_order_len {
            let node_id = self.execution_order[i] as usize;
            self.states[node_id].inlets = [0.0; 4];
            self.states[node_id].inlet_connected = [false; 4];
        }

        // Sum signals from active feedback cables (1-sample feedback delay for cycles)
        for cable_opt in &self.cables {
            if let Some(cable) = cable_opt {
                if cable.active {
                    let from = cable.from_node as usize;
                    let to = cable.to_node as usize;
                    let from_active = self.nodes[from].as_ref().map_or(false, |n| n.active);
                    let to_active = self.nodes[to].as_ref().map_or(false, |n| n.active);
                    if from_active && to_active && self.node_order_rank[from] >= self.node_order_rank[to] {
                        let out_idx = (cable.from_outlet as usize).min(3);
                        let in_idx = (cable.to_inlet as usize).min(3);
                        let signal = self.states[from].outlets[out_idx];
                        self.states[to].inlets[in_idx] += signal;
                        self.states[to].inlet_connected[in_idx] = true;
                    }
                }
            }
        }

        // Inject external audio inputs
        if let Some(in_node) = self.audio_input_node {
            let idx = in_node as usize;
            if idx < MAX_PATCH_NODES && self.nodes[idx].as_ref().map_or(false, |n| n.active) {
                self.states[idx].inlets[0] += input_l;
                self.states[idx].inlets[1] += input_r;
                self.states[idx].inlet_connected[0] = true;
                self.states[idx].inlet_connected[1] = true;
            }
        }

        // Evaluate nodes in topological order
        let sr = self.sample_rate;
        for i in 0..self.execution_order_len {
            let node_id = self.execution_order[i] as usize;
            let node_kind = match &self.nodes[node_id] {
                Some(n) if n.active => n.kind.clone(),
                _ => continue,
            };

            let state = &mut self.states[node_id];

            match node_kind {
                PatchNodeKind::Oscillator { freq, shape } => {
                    let mod_freq = freq + state.inlets[0];
                    let step = (mod_freq / sr).clamp(0.0, 0.49);
                    state.phase = (state.phase + step).fract();

                    let val = match shape {
                        PatchOscShape::Sine => (2.0 * PI * state.phase).sin(),
                        PatchOscShape::Saw => 2.0 * state.phase - 1.0,
                        PatchOscShape::Square => {
                            if state.phase < 0.5 { 1.0 } else { -1.0 }
                        }
                        PatchOscShape::Triangle => {
                            4.0 * (state.phase - 0.5).abs() - 1.0
                        }
                    };
                    state.outlets[0] = val;
                }
                PatchNodeKind::StateVariableFilter { cutoff, resonance, mode } => {
                    let in_sig = state.inlets[0];
                    let mod_cutoff = (cutoff + state.inlets[1]).clamp(20.0, sr * 0.45);
                    let q = (resonance + state.inlets[2]).clamp(0.1, 10.0);

                    // Chamberlin SVF topology
                    let f = 2.0 * (PI * mod_cutoff / sr).sin();
                    let damp = 1.0 / q;

                    let hp = in_sig - state.svf_ic2eq - damp * state.svf_ic1eq;
                    let bp = state.svf_ic1eq + f * hp;
                    let lp = state.svf_ic2eq + f * bp;

                    state.svf_ic1eq = bp;
                    state.svf_ic2eq = lp;

                    let out_val = match mode {
                        PatchFilterMode::Lowpass => lp,
                        PatchFilterMode::Highpass => hp,
                        PatchFilterMode::Bandpass => bp,
                        PatchFilterMode::Notch => lp + hp,
                    };
                    state.outlets[0] = out_val;
                    state.outlets[1] = bp;
                    state.outlets[2] = hp;
                    state.outlets[3] = lp;
                }
                PatchNodeKind::EnvFollower { attack_ms, release_ms } => {
                    let in_sig = state.inlets[0].abs();
                    let att_coef = (-1.0 / ((attack_ms * 0.001 * sr).max(1.0))).exp();
                    let rel_coef = (-1.0 / ((release_ms * 0.001 * sr).max(1.0))).exp();

                    if in_sig > state.env_val {
                        state.env_val = in_sig + att_coef * (state.env_val - in_sig);
                    } else {
                        state.env_val = in_sig + rel_coef * (state.env_val - in_sig);
                    }
                    state.outlets[0] = state.env_val;
                }
                PatchNodeKind::LfoModulator { rate_hz, depth } => {
                    let mod_rate = (rate_hz + state.inlets[0]).clamp(0.01, 100.0);
                    let step = mod_rate / sr;
                    state.phase = (state.phase + step).fract();
                    let val = (2.0 * PI * state.phase).sin() * depth;
                    state.outlets[0] = val;
                }
                PatchNodeKind::RingMod { carrier_freq } => {
                    let in_sig = state.inlets[0];
                    let carrier = if state.inlet_connected[1] {
                        state.inlets[1]
                    } else {
                        let step = carrier_freq / sr;
                        state.carrier_phase = (state.carrier_phase + step).fract();
                        (2.0 * PI * state.carrier_phase).sin()
                    };
                    state.outlets[0] = in_sig * carrier;
                }
                PatchNodeKind::DelayLine { delay_samples, feedback } => {
                    let in_sig = state.inlets[0];
                    let d = delay_samples.clamp(1, MAX_DELAY_SAMPLES - 1);
                    let fb = feedback.clamp(0.0, 0.98);

                    let read_pos = (state.delay_write + MAX_DELAY_SAMPLES - d) % MAX_DELAY_SAMPLES;
                    let delayed = state.delay_buf[read_pos];

                    state.delay_buf[state.delay_write] = in_sig + delayed * fb;
                    state.delay_write = (state.delay_write + 1) % MAX_DELAY_SAMPLES;
                    state.outlets[0] = delayed;
                }
                PatchNodeKind::MathAdd { operand } => {
                    let a = state.inlets[0];
                    let b = if state.inlet_connected[1] {
                        state.inlets[1]
                    } else {
                        operand
                    };
                    state.outlets[0] = a + b;
                }
                PatchNodeKind::MathMul { factor } => {
                    let a = state.inlets[0];
                    let b = if state.inlet_connected[1] {
                        state.inlets[1]
                    } else {
                        factor
                    };
                    state.outlets[0] = a * b;
                }
                PatchNodeKind::LomParamMapper {
                    target_track,
                    target_device,
                    target_param,
                    min_val,
                    max_val,
                } => {
                    let in_sig = state.inlets[0];
                    // Normalize [-1.0, 1.0] to [0.0, 1.0] if signed or pass directly
                    let norm = (in_sig * 0.5 + 0.5).clamp(0.0, 1.0);
                    let mapped = min_val + norm * (max_val - min_val);

                    state.outlets[0] = mapped;
                    lom_out = Some((target_device, target_param, mapped));
                    self.last_lom_target = Some((target_track, target_device, target_param, mapped));
                }
            }

            // Propagate fresh output to downstream forward cables with 0-sample latency
            for cable_opt in &self.cables {
                if let Some(cable) = cable_opt {
                    if cable.active && cable.from_node == (node_id as u32) {
                        let to = cable.to_node as usize;
                        let to_active = self.nodes[to].as_ref().map_or(false, |n| n.active);
                        if to_active && self.node_order_rank[node_id] < self.node_order_rank[to] {
                            let out_idx = (cable.from_outlet as usize).min(3);
                            let in_idx = (cable.to_inlet as usize).min(3);
                            let signal = self.states[node_id].outlets[out_idx];
                            self.states[to].inlets[in_idx] += signal;
                            self.states[to].inlet_connected[in_idx] = true;
                        }
                    }
                }
            }
        }

        // Determine left and right audio outputs
        let out_l = if let Some(out_node_l) = self.audio_output_node_l {
            if (out_node_l as usize) < MAX_PATCH_NODES && self.nodes[out_node_l as usize].as_ref().map_or(false, |n| n.active) {
                self.states[out_node_l as usize].outlets[0]
            } else {
                0.0
            }
        } else if self.execution_order_len > 0 {
            let last_node = self.execution_order[self.execution_order_len - 1] as usize;
            self.states[last_node].outlets[0]
        } else {
            0.0
        };

        let out_r = if let Some(out_node_r) = self.audio_output_node_r {
            if (out_node_r as usize) < MAX_PATCH_NODES && self.nodes[out_node_r as usize].as_ref().map_or(false, |n| n.active) {
                self.states[out_node_r as usize].outlets[0]
            } else {
                out_l
            }
        } else {
            out_l
        };

        (out_l, out_r, lom_out)
    }

    /// Process a stereo block of audio samples with zero heap allocations.
    /// Returns the number of stereo frames processed.
    pub fn process_buffer(
        &mut self,
        input_l: &[f32],
        input_r: &[f32],
        output_l: &mut [f32],
        output_r: &mut [f32],
    ) -> usize {
        let n = input_l.len().min(input_r.len()).min(output_l.len()).min(output_r.len());
        for i in 0..n {
            let (l, r, _) = self.process_frame(input_l[i], input_r[i]);
            output_l[i] = l;
            output_r[i] = r;
        }
        n
    }

    /// Preset builder: LFO modulating a filter cutoff and target LOM parameter.
    pub fn preset_lfo_filter_modulator() -> Self {
        let mut graph = Self::new(44100.0);

        let osc = graph
            .add_node(PatchNodeKind::Oscillator {
                freq: 220.0,
                shape: PatchOscShape::Saw,
            })
            .expect("Add osc");

        let lfo = graph
            .add_node(PatchNodeKind::LfoModulator {
                rate_hz: 2.0,
                depth: 600.0,
            })
            .expect("Add lfo");

        let filter = graph
            .add_node(PatchNodeKind::StateVariableFilter {
                cutoff: 1000.0,
                resonance: 1.5,
                mode: PatchFilterMode::Lowpass,
            })
            .expect("Add filter");

        let mapper = graph
            .add_node(PatchNodeKind::LomParamMapper {
                target_track: 0,
                target_device: 1,
                target_param: 2,
                min_val: 200.0,
                max_val: 2000.0,
            })
            .expect("Add mapper");

        // Connect osc -> filter audio inlet
        graph.connect_cable(osc, 0, filter, 0).expect("Osc to filter");
        // Connect lfo -> filter cutoff inlet
        graph.connect_cable(lfo, 0, filter, 1).expect("LFO to filter cutoff");
        // Connect lfo -> LOM mapper
        graph.connect_cable(lfo, 0, mapper, 0).expect("LFO to mapper");

        graph.audio_output_node_l = Some(filter);
        graph.audio_output_node_r = Some(filter);
        graph.compile();
        graph
    }

    /// Preset builder: FM Ring Modulator effect with envelope follower follower gain.
    pub fn preset_fm_ring_effect() -> Self {
        let mut graph = Self::new(44100.0);

        let carrier_osc = graph
            .add_node(PatchNodeKind::Oscillator {
                freq: 330.0,
                shape: PatchOscShape::Sine,
            })
            .expect("Add carrier");

        let ring_mod = graph
            .add_node(PatchNodeKind::RingMod {
                carrier_freq: 110.0,
            })
            .expect("Add ringmod");

        let env_follower = graph
            .add_node(PatchNodeKind::EnvFollower {
                attack_ms: 5.0,
                release_ms: 80.0,
            })
            .expect("Add env follower");

        let math_mul = graph
            .add_node(PatchNodeKind::MathMul { factor: 1.0 })
            .expect("Add math mul");

        // Connect carrier_osc -> ring_mod inlet 0
        graph.connect_cable(carrier_osc, 0, ring_mod, 0).expect("Carrier to ringmod");
        // Connect carrier_osc -> env_follower inlet 0
        graph.connect_cable(carrier_osc, 0, env_follower, 0).expect("Carrier to env");
        // Connect ring_mod -> math_mul inlet 0
        graph.connect_cable(ring_mod, 0, math_mul, 0).expect("Ringmod to mul");
        // Connect env_follower -> math_mul inlet 1
        graph.connect_cable(env_follower, 0, math_mul, 1).expect("Env to mul");

        graph.audio_output_node_l = Some(math_mul);
        graph.audio_output_node_r = Some(math_mul);
        graph.compile();
        graph
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topological_audio_processing() {
        let mut graph = ModularPatchGraph::new(44100.0);

        let osc = graph
            .add_node(PatchNodeKind::Oscillator {
                freq: 440.0,
                shape: PatchOscShape::Sine,
            })
            .expect("Osc");

        let filter = graph
            .add_node(PatchNodeKind::StateVariableFilter {
                cutoff: 800.0,
                resonance: 1.0,
                mode: PatchFilterMode::Lowpass,
            })
            .expect("Filter");

        let delay = graph
            .add_node(PatchNodeKind::DelayLine {
                delay_samples: 10,
                feedback: 0.2,
            })
            .expect("Delay");

        graph.connect_cable(osc, 0, filter, 0).expect("Osc->Filter");
        graph.connect_cable(filter, 0, delay, 0).expect("Filter->Delay");

        graph.audio_output_node_l = Some(delay);
        graph.audio_output_node_r = Some(delay);

        // Process 64 frames
        let mut rendered = Vec::new();
        for _ in 0..64 {
            let (l, r, _) = graph.process_frame(0.0, 0.0);
            assert!(l.is_finite());
            assert!(r.is_finite());
            rendered.push(l);
        }

        // Energy must be generated after delay fills
        let energy: f32 = rendered[15..].iter().map(|s| s * s).sum();
        assert!(energy > 1e-4, "Topological DSP chain produced zero energy");
    }

    #[test]
    fn test_lom_parameter_modulation_output() {
        let mut graph = ModularPatchGraph::preset_lfo_filter_modulator();

        let mut modulations = Vec::new();
        for _ in 0..100 {
            let (_, _, lom) = graph.process_frame(0.0, 0.0);
            assert!(lom.is_some(), "LOM parameter mapper must yield modulation");
            let (dev, param, val) = lom.unwrap();
            assert_eq!(dev, 1);
            assert_eq!(param, 2);
            assert!(val >= 200.0 && val <= 2000.0, "Value out of bounds: {}", val);
            modulations.push(val);
        }

        // LFO must modulate the value across frames
        let min_seen = modulations.iter().cloned().fold(f32::INFINITY, f32::min);
        let max_seen = modulations.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        assert!(max_seen - min_seen > 10.0, "LOM modulation value remained static");
    }

    #[test]
    fn test_cycle_safety() {
        let mut graph = ModularPatchGraph::new(44100.0);

        let node_a = graph
            .add_node(PatchNodeKind::MathAdd { operand: 0.1 })
            .expect("A");
        let node_b = graph
            .add_node(PatchNodeKind::MathMul { factor: 0.5 })
            .expect("B");

        // Feedback cycle: A -> B and B -> A
        graph.connect_cable(node_a, 0, node_b, 0).expect("A->B");
        graph.connect_cable(node_b, 0, node_a, 0).expect("B->A");

        graph.audio_output_node_l = Some(node_b);
        graph.audio_output_node_r = Some(node_b);
        graph.compile();

        // Process frames: must not hang, panic or stack overflow
        for _ in 0..128 {
            let (l, r, _) = graph.process_frame(0.0, 0.0);
            assert!(l.is_finite());
            assert!(r.is_finite());
        }
    }

    #[test]
    fn test_preset_builders() {
        let mut lfo_preset = ModularPatchGraph::preset_lfo_filter_modulator();
        let (l1, r1, lom1) = lfo_preset.process_frame(0.0, 0.0);
        assert!(l1.is_finite() && r1.is_finite());
        assert!(lom1.is_some());

        let mut ring_preset = ModularPatchGraph::preset_fm_ring_effect();
        let (l2, r2, _) = ring_preset.process_frame(0.0, 0.0);
        assert!(l2.is_finite() && r2.is_finite());
    }

    #[test]
    fn test_32_nodes_and_64_cables_capacity_limits() {
        let mut graph = ModularPatchGraph::new(48000.0);
        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.cable_count(), 0);

        // Add exactly 32 nodes
        let mut node_ids = Vec::new();
        for i in 0..MAX_PATCH_NODES {
            let res = graph.add_node(PatchNodeKind::MathAdd { operand: i as f32 });
            assert!(res.is_ok(), "Node {} should be added successfully", i);
            node_ids.push(res.unwrap());
        }
        assert_eq!(graph.node_count(), 32);
        assert_eq!(graph.active_nodes_count(), 32);

        // 33rd node must return NodeCapacityExceeded
        let overflow_node = graph.add_node(PatchNodeKind::Oscillator {
            freq: 440.0,
            shape: PatchOscShape::Sine,
        });
        assert_eq!(overflow_node, Err(PatchGraphError::NodeCapacityExceeded));

        // Connect 64 cables (2 cables between each adjacent pair in ring/chain)
        let mut cable_ids = Vec::new();
        for i in 0..MAX_PATCH_CABLES {
            let from = (i % MAX_PATCH_NODES) as u32;
            let to = ((i + 1) % MAX_PATCH_NODES) as u32;
            let outlet = ((i / MAX_PATCH_NODES) % 4) as u32;
            let inlet = ((i / MAX_PATCH_NODES) % 4) as u32;
            let res = graph.connect_cable(from, outlet, to, inlet);
            assert!(res.is_ok(), "Cable {} should be connected successfully", i);
            cable_ids.push(res.unwrap());
        }
        assert_eq!(graph.cable_count(), 64);
        assert_eq!(graph.active_cables_count(), 64);

        // 65th cable must return CableCapacityExceeded
        let overflow_cable = graph.connect_cable(0, 0, 1, 0);
        assert_eq!(overflow_cable, Err(PatchGraphError::CableCapacityExceeded));

        // Process frames through 32-node 64-cable saturated graph
        for _ in 0..32 {
            let (l, r, _) = graph.process_frame(0.5, 0.5);
            assert!(l.is_finite());
            assert!(r.is_finite());
        }
    }

    #[test]
    fn test_dynamic_cable_routing_and_disconnection() {
        let mut graph = ModularPatchGraph::new(44100.0);

        let osc = graph
            .add_node(PatchNodeKind::Oscillator {
                freq: 440.0,
                shape: PatchOscShape::Sine,
            })
            .unwrap();
        let mul = graph
            .add_node(PatchNodeKind::MathMul { factor: 2.0 })
            .unwrap();

        // Connect osc -> mul
        let cable_id = graph.connect_cable(osc, 0, mul, 0).unwrap();
        graph.audio_output_node_l = Some(mul);
        graph.audio_output_node_r = Some(mul);

        // Render frames with cable connected
        let mut connected_energy = 0.0_f32;
        for _ in 0..50 {
            let (l, _, _) = graph.process_frame(0.0, 0.0);
            connected_energy += l * l;
        }
        assert!(connected_energy > 0.1, "Signal must flow through connected cable");

        // Dynamically disconnect cable
        assert!(graph.disconnect_cable(cable_id).is_ok());
        assert_eq!(graph.cable_count(), 0);

        // Render frames: with cable disconnected, mul inlet is 0, output must be 0
        let mut disconnected_energy = 0.0_f32;
        for _ in 0..50 {
            let (l, _, _) = graph.process_frame(0.0, 0.0);
            disconnected_energy += l * l;
        }
        assert_eq!(disconnected_energy, 0.0, "Signal must cease after cable disconnection");

        // Reconnect dynamically
        let new_cable = graph.connect_cable(osc, 0, mul, 0).unwrap();
        assert_eq!(graph.cable_count(), 1);
        let mut reconnected_energy = 0.0_f32;
        for _ in 0..50 {
            let (l, _, _) = graph.process_frame(0.0, 0.0);
            reconnected_energy += l * l;
        }
        assert!(reconnected_energy > 0.1, "Signal must resume after cable reconnection");
        let _ = new_cable;
    }

    #[test]
    fn test_node_removal_and_pruning() {
        let mut graph = ModularPatchGraph::new(44100.0);

        let n1 = graph.add_node(PatchNodeKind::MathAdd { operand: 1.0 }).unwrap();
        let n2 = graph.add_node(PatchNodeKind::MathAdd { operand: 2.0 }).unwrap();
        let n3 = graph.add_node(PatchNodeKind::MathAdd { operand: 3.0 }).unwrap();

        let c1 = graph.connect_cable(n1, 0, n2, 0).unwrap();
        let c2 = graph.connect_cable(n2, 0, n3, 0).unwrap();

        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.cable_count(), 2);

        // Remove node n2: both c1 (pointing to n2) and c2 (originating from n2) must be pruned
        assert!(graph.remove_node(n2).is_ok());
        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.cable_count(), 0);
        assert!(graph.get_cable(c1).is_none());
        assert!(graph.get_cable(c2).is_none());
        assert!(graph.get_node(n2).is_none());
    }

    #[test]
    fn test_node_and_cable_activation_bypassing() {
        let mut graph = ModularPatchGraph::new(44100.0);

        let osc = graph
            .add_node(PatchNodeKind::Oscillator {
                freq: 500.0,
                shape: PatchOscShape::Square,
            })
            .unwrap();
        let mul = graph
            .add_node(PatchNodeKind::MathMul { factor: 1.0 })
            .unwrap();
        let cable = graph.connect_cable(osc, 0, mul, 0).unwrap();
        graph.audio_output_node_l = Some(mul);

        // Active: output non-zero
        let (l, _, _) = graph.process_frame(0.0, 0.0);
        assert!(l.abs() > 0.5);

        // Mute cable: output becomes 0
        graph.set_cable_active(cable, false).unwrap();
        assert!(!graph.get_cable(cable).unwrap().active);
        let (l_muted, _, _) = graph.process_frame(0.0, 0.0);
        assert_eq!(l_muted, 0.0);

        // Unmute cable
        graph.set_cable_active(cable, true).unwrap();
        let (l_unmuted, _, _) = graph.process_frame(0.0, 0.0);
        assert!(l_unmuted.abs() > 0.5);

        // Deactivate node osc: output becomes 0
        graph.set_node_active(osc, false).unwrap();
        let (l_node_bypassed, _, _) = graph.process_frame(0.0, 0.0);
        assert_eq!(l_node_bypassed, 0.0);
    }

    #[test]
    fn test_all_oscillator_waveforms_and_frequency_modulation() {
        let shapes = [
            PatchOscShape::Sine,
            PatchOscShape::Saw,
            PatchOscShape::Square,
            PatchOscShape::Triangle,
        ];

        for shape in shapes {
            let mut graph = ModularPatchGraph::new(44100.0);
            let osc = graph
                .add_node(PatchNodeKind::Oscillator { freq: 440.0, shape })
                .unwrap();
            graph.audio_output_node_l = Some(osc);

            let mut min_val = f32::INFINITY;
            let mut max_val = f32::NEG_INFINITY;
            for _ in 0..100 {
                let (val, _, _) = graph.process_frame(0.0, 0.0);
                assert!(val.is_finite());
                min_val = min_val.min(val);
                max_val = max_val.max(val);
            }
            assert!(min_val >= -1.01 && min_val <= -0.5, "Shape {:?} min {} out of bounds", shape, min_val);
            assert!(max_val <= 1.01 && max_val >= 0.5, "Shape {:?} max {} out of bounds", shape, max_val);
        }

        // Test FM (inlet 0 modulating frequency)
        let mut fm_graph = ModularPatchGraph::new(44100.0);
        let mod_osc = fm_graph
            .add_node(PatchNodeKind::Oscillator {
                freq: 50.0,
                shape: PatchOscShape::Sine,
            })
            .unwrap();
        let carrier_osc = fm_graph
            .add_node(PatchNodeKind::Oscillator {
                freq: 440.0,
                shape: PatchOscShape::Sine,
            })
            .unwrap();
        fm_graph.connect_cable(mod_osc, 0, carrier_osc, 0).unwrap();
        fm_graph.audio_output_node_l = Some(carrier_osc);

        for _ in 0..64 {
            let (val, _, _) = fm_graph.process_frame(0.0, 0.0);
            assert!(val.is_finite());
        }
    }

    #[test]
    fn test_all_filter_modes_and_multi_outlet() {
        let modes = [
            PatchFilterMode::Lowpass,
            PatchFilterMode::Highpass,
            PatchFilterMode::Bandpass,
            PatchFilterMode::Notch,
        ];

        for mode in modes {
            let mut graph = ModularPatchGraph::new(44100.0);
            let osc = graph
                .add_node(PatchNodeKind::Oscillator {
                    freq: 1000.0,
                    shape: PatchOscShape::Saw,
                })
                .unwrap();
            let flt = graph
                .add_node(PatchNodeKind::StateVariableFilter {
                    cutoff: 1200.0,
                    resonance: 2.0,
                    mode,
                })
                .unwrap();
            graph.connect_cable(osc, 0, flt, 0).unwrap();
            graph.audio_output_node_l = Some(flt);

            for _ in 0..50 {
                let (val, _, _) = graph.process_frame(0.0, 0.0);
                assert!(val.is_finite());
            }

            // Verify multi-outlet access (outlets 1=bp, 2=hp, 3=lp)
            assert!(graph.get_node_outlet(flt, 0).is_some());
            assert!(graph.get_node_outlet(flt, 1).is_some());
            assert!(graph.get_node_outlet(flt, 2).is_some());
            assert!(graph.get_node_outlet(flt, 3).is_some());
        }
    }

    #[test]
    fn test_envelope_follower_dynamics() {
        let mut graph = ModularPatchGraph::new(44100.0);
        let env = graph
            .add_node(PatchNodeKind::EnvFollower {
                attack_ms: 2.0,
                release_ms: 20.0,
            })
            .unwrap();
        graph.audio_input_node = Some(env);
        graph.audio_output_node_l = Some(env);

        // Step input 1.0: envelope rises towards 1.0
        let mut env_out = 0.0_f32;
        for _ in 0..200 {
            let (val, _, _) = graph.process_frame(1.0, 1.0);
            env_out = val;
        }
        assert!(env_out > 0.8, "Envelope follower should track attack: {}", env_out);

        // Silence input 0.0: envelope decays
        for _ in 0..4000 {
            let (val, _, _) = graph.process_frame(0.0, 0.0);
            env_out = val;
        }
        assert!(env_out < 0.05, "Envelope follower should decay: {}", env_out);
    }

    #[test]
    fn test_ring_mod_internal_and_external_carrier() {
        // Internal carrier
        let mut graph = ModularPatchGraph::new(44100.0);
        let ring = graph
            .add_node(PatchNodeKind::RingMod {
                carrier_freq: 220.0,
            })
            .unwrap();
        graph.audio_input_node = Some(ring);
        graph.audio_output_node_l = Some(ring);

        let mut energy = 0.0_f32;
        for _ in 0..50 {
            let (val, _, _) = graph.process_frame(1.0, 1.0);
            energy += val * val;
        }
        assert!(energy > 0.1, "RingMod internal carrier should produce signal");

        // External carrier via inlet 1
        let mut ext_graph = ModularPatchGraph::new(44100.0);
        let sig = ext_graph
            .add_node(PatchNodeKind::MathAdd { operand: 0.5 })
            .unwrap();
        let carrier = ext_graph
            .add_node(PatchNodeKind::MathAdd { operand: 0.8 })
            .unwrap();
        let ext_ring = ext_graph
            .add_node(PatchNodeKind::RingMod { carrier_freq: 0.0 })
            .unwrap();
        ext_graph.connect_cable(sig, 0, ext_ring, 0).unwrap();
        ext_graph.connect_cable(carrier, 0, ext_ring, 1).unwrap();
        ext_graph.audio_output_node_l = Some(ext_ring);

        let (val, _, _) = ext_graph.process_frame(0.0, 0.0);
        assert!((val - 0.4).abs() < 1e-4, "External carrier ring mod (0.5 * 0.8 = 0.4): {}", val);
    }

    #[test]
    fn test_lom_param_mapper_and_observation() {
        let mut graph = ModularPatchGraph::new(44100.0);
        let mapper = graph
            .add_node(PatchNodeKind::LomParamMapper {
                target_track: 2,
                target_device: 3,
                target_param: 4,
                min_val: 10.0,
                max_val: 100.0,
            })
            .unwrap();
        graph.audio_input_node = Some(mapper);

        // Input -1.0 -> maps to min_val 10.0
        let (_, _, lom1) = graph.process_frame(-1.0, -1.0);
        assert_eq!(lom1, Some((3, 4, 10.0)));
        assert_eq!(graph.last_lom_target, Some((2, 3, 4, 10.0)));

        // Input +1.0 -> maps to max_val 100.0
        let (_, _, lom2) = graph.process_frame(1.0, 1.0);
        assert_eq!(lom2, Some((3, 4, 100.0)));
        assert_eq!(graph.last_lom_target, Some((2, 3, 4, 100.0)));

        // Input 0.0 -> maps to midpoint 55.0
        let (_, _, lom3) = graph.process_frame(0.0, 0.0);
        assert_eq!(lom3, Some((3, 4, 55.0)));
        assert_eq!(graph.last_lom_target, Some((2, 3, 4, 55.0)));

        // Real-time observation of outlet
        let observed = graph.get_node_outlet(mapper, 0);
        assert_eq!(observed, Some(55.0));
    }

    #[test]
    fn test_process_buffer_zero_alloc() {
        let mut graph = ModularPatchGraph::preset_lfo_filter_modulator();
        let input_l = [0.1_f32; 128];
        let input_r = [0.1_f32; 128];
        let mut output_l = [0.0_f32; 128];
        let mut output_r = [0.0_f32; 128];

        let count = graph.process_buffer(&input_l, &input_r, &mut output_l, &mut output_r);
        assert_eq!(count, 128);

        for i in 0..128 {
            assert!(output_l[i].is_finite());
            assert!(output_r[i].is_finite());
        }
    }

    #[test]
    fn test_invalid_node_and_cable_error_handling() {
        let mut graph = ModularPatchGraph::new(44100.0);
        let n = graph.add_node(PatchNodeKind::MathMul { factor: 1.0 }).unwrap();

        // Connect from invalid node
        assert_eq!(
            graph.connect_cable(99, 0, n, 0),
            Err(PatchGraphError::InvalidNodeId(99))
        );
        // Connect to invalid node
        assert_eq!(
            graph.connect_cable(n, 0, 99, 0),
            Err(PatchGraphError::InvalidNodeId(99))
        );
        // Invalid outlet
        assert_eq!(
            graph.connect_cable(n, 4, n, 0),
            Err(PatchGraphError::InvalidOutlet(4))
        );
        // Invalid inlet
        assert_eq!(
            graph.connect_cable(n, 0, n, 4),
            Err(PatchGraphError::InvalidInlet(4))
        );
        // Disconnect nonexistent cable
        assert_eq!(
            graph.disconnect_cable(10),
            Err(PatchGraphError::CableNotFound(10))
        );
        // Remove nonexistent node
        assert_eq!(
            graph.remove_node(99),
            Err(PatchGraphError::InvalidNodeId(99))
        );
    }
}
