//! 8-target Modulation Matrix for Wavetable.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModSource {
    None,
    AmpEnv,
    Env2,
    Env3,
    Lfo1,
    Lfo2,
    Velocity,
    KeyTrack,
    PitchBend,
    ModWheel,
    Pressure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModDestination {
    None,
    Osc1Pos,
    Osc2Pos,
    Osc1Warp,
    Osc2Warp,
    Osc1Pitch,
    Osc2Pitch,
    Filter1Cutoff,
    Filter1Res,
    Filter1Morph,
    Filter2Cutoff,
    Filter2Res,
    Filter2Morph,
    Volume,
    Pan,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModSlot {
    pub source: ModSource,
    pub destination: ModDestination,
    pub amount: f32, // [-1.0, 1.0]
}

impl Default for ModSlot {
    fn default() -> Self {
        Self {
            source: ModSource::None,
            destination: ModDestination::None,
            amount: 0.0,
        }
    }
}

pub const NUM_MOD_SLOTS: usize = 8;

#[derive(Debug, Clone)]
pub struct ModMatrix {
    pub slots: [ModSlot; NUM_MOD_SLOTS],
}

impl ModMatrix {
    pub fn new() -> Self {
        Self {
            slots: [ModSlot::default(); NUM_MOD_SLOTS],
        }
    }

    pub fn set_slot(&mut self, index: usize, source: ModSource, destination: ModDestination, amount: f32) {
        if index < NUM_MOD_SLOTS {
            self.slots[index] = ModSlot {
                source,
                destination,
                amount: amount.clamp(-1.0, 1.0),
            };
        }
    }

    /// Evaluates accumulated modulation for a given destination.
    pub fn evaluate_destination(
        &self,
        dest: ModDestination,
        amp_env: f32,
        env2: f32,
        env3: f32,
        lfo1: f32,
        lfo2: f32,
        velocity: f32,
        key_track: f32,
        pitch_bend: f32,
        mod_wheel: f32,
        pressure: f32,
    ) -> f32 {
        let mut total = 0.0f32;
        for slot in &self.slots {
            if slot.destination == dest && slot.source != ModSource::None {
                let src_val = match slot.source {
                    ModSource::None => 0.0,
                    ModSource::AmpEnv => amp_env,
                    ModSource::Env2 => env2,
                    ModSource::Env3 => env3,
                    ModSource::Lfo1 => lfo1,
                    ModSource::Lfo2 => lfo2,
                    ModSource::Velocity => velocity,
                    ModSource::KeyTrack => key_track,
                    ModSource::PitchBend => pitch_bend,
                    ModSource::ModWheel => mod_wheel,
                    ModSource::Pressure => pressure,
                };
                total += src_val * slot.amount;
            }
        }
        total
    }
}
