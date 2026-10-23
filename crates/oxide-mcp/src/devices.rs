//! Device instantiation and device chain inspection helpers for MCP.

use oxide_core::device::{AudioEffectDevice, Device, DeviceChain, InstrumentDevice, MidiEffectDevice};
use oxide_drums::{DrumRack, SimplerDevice};
use oxide_dsp::drift::DriftDevice;
use oxide_dsp::meld::MeldDevice;
use oxide_dsp::operator::OperatorDevice;
use oxide_dsp::physical::{AnalogDevice, CollisionDevice, ElectricDevice, TensionDevice};
use oxide_dsp::wavetable::WavetableDevice;
use oxide_fx::midi_fx::{Arpeggiator, Chord, Scale, Velocity};
use oxide_fx::suite_fx::{
    BeatRepeat, Corpus, Echo, HybridReverb, Redux, SpectralResonator, Utility,
};
use oxide_fx::{AutoFilter, EQEight, GlueCompressor, MasterLimiter, MultibandDynamics, Roar};
use serde_json::{json, Value};

pub enum CreatedDevice {
    Midi(Box<dyn MidiEffectDevice>),
    Instrument(Box<dyn InstrumentDevice>),
    Audio(Box<dyn AudioEffectDevice>),
}

/// Factory function to instantiate any Live 12 device by name.
pub fn instantiate_device(device_type: &str, sample_rate: f32) -> Result<CreatedDevice, String> {
    let clean = device_type
        .trim()
        .to_lowercase()
        .replace([' ', '-', '_'], "");

    match clean.as_str() {
        // --- Instruments ---
        "wavetable" => Ok(CreatedDevice::Instrument(Box::new(WavetableDevice::new(sample_rate)))),
        "operator" => Ok(CreatedDevice::Instrument(Box::new(OperatorDevice::new(sample_rate)))),
        "drift" => Ok(CreatedDevice::Instrument(Box::new(DriftDevice::new(sample_rate)))),
        "meld" => Ok(CreatedDevice::Instrument(Box::new(MeldDevice::new(sample_rate)))),
        "analog" => Ok(CreatedDevice::Instrument(Box::new(AnalogDevice::new(sample_rate)))),
        "collision" => Ok(CreatedDevice::Instrument(Box::new(CollisionDevice::new(sample_rate)))),
        "tension" => Ok(CreatedDevice::Instrument(Box::new(TensionDevice::new(sample_rate)))),
        "electric" => Ok(CreatedDevice::Instrument(Box::new(ElectricDevice::new(sample_rate)))),
        "simpler" => Ok(CreatedDevice::Instrument(Box::new(SimplerDevice::new(sample_rate)))),
        "drumrack" | "drums" => Ok(CreatedDevice::Instrument(Box::new(DrumRack::new(sample_rate)))),

        // --- Audio Effects ---
        "eqeight" | "eq8" => Ok(CreatedDevice::Audio(Box::new(EQEight::new(sample_rate)))),
        "gluecompressor" | "glue" => Ok(CreatedDevice::Audio(Box::new(GlueCompressor::new(sample_rate)))),
        "roar" => Ok(CreatedDevice::Audio(Box::new(Roar::new(sample_rate)))),
        "autofilter" => Ok(CreatedDevice::Audio(Box::new(AutoFilter::new(sample_rate)))),
        "ott" | "multibanddynamics" => Ok(CreatedDevice::Audio(Box::new(MultibandDynamics::new(sample_rate)))),
        "hybridreverb" | "reverb" => Ok(CreatedDevice::Audio(Box::new(HybridReverb::new(sample_rate)))),
        "spectralresonator" => Ok(CreatedDevice::Audio(Box::new(SpectralResonator::new(sample_rate)))),
        "echo" | "delay" => Ok(CreatedDevice::Audio(Box::new(Echo::new(sample_rate)))),
        "beatrepeat" => Ok(CreatedDevice::Audio(Box::new(BeatRepeat::new(sample_rate)))),
        "corpus" => Ok(CreatedDevice::Audio(Box::new(Corpus::new(sample_rate)))),
        "redux" => Ok(CreatedDevice::Audio(Box::new(Redux::new()))),
        "utility" => Ok(CreatedDevice::Audio(Box::new(Utility::new(sample_rate)))),
        "limiter" => Ok(CreatedDevice::Audio(Box::new(MasterLimiter::new(sample_rate)))),

        // --- MIDI Effects ---
        "arpeggiator" | "arp" => Ok(CreatedDevice::Midi(Box::new(Arpeggiator::new()))),
        "chord" => Ok(CreatedDevice::Midi(Box::new(Chord::new()))),
        "scale" => Ok(CreatedDevice::Midi(Box::new(Scale::new()))),
        "velocity" => Ok(CreatedDevice::Midi(Box::new(Velocity::new()))),

        _ => Err(format!("Device '{}' not recognized. Supported: Wavetable, Operator, Drift, Meld, Analog, Collision, Tension, Electric, Simpler, DrumRack, EQEight, GlueCompressor, Roar, AutoFilter, OTT, HybridReverb, SpectralResonator, Echo, BeatRepeat, Corpus, Redux, Utility, Limiter, Arpeggiator, Chord, Scale, Velocity", device_type)),
    }
}

/// Attach an instantiated device into a DeviceChain.
pub fn add_device_to_chain(chain: &mut DeviceChain, dev: CreatedDevice, position: Option<usize>) -> usize {
    match dev {
        CreatedDevice::Midi(mfx) => {
            if let Some(pos) = position {
                chain.insert_midi_effect(pos, mfx);
                pos
            } else {
                chain.add_midi_effect(mfx)
            }
        }
        CreatedDevice::Instrument(inst) => {
            chain.set_instrument(Some(inst));
            chain.midi_effects.len()
        }
        CreatedDevice::Audio(afx) => {
            if let Some(pos) = position {
                chain.insert_audio_effect(pos, afx);
                pos
            } else {
                chain.add_audio_effect(afx)
            }
        }
    }
}

/// Serialize a DeviceChain into a structured JSON representation.
pub fn dump_device_chain(chain: &DeviceChain) -> Value {
    let mut devices = Vec::new();
    let mut idx = 0;

    for (m_idx, mfx) in chain.midi_effects.iter().enumerate() {
        devices.push(json!({
            "index": idx,
            "category_index": m_idx,
            "name": mfx.name(),
            "category": "MidiEffect",
            "enabled": mfx.is_enabled(),
            "latency_samples": mfx.latency_samples()
        }));
        idx += 1;
    }

    if let Some(ref inst) = chain.instrument {
        devices.push(json!({
            "index": idx,
            "category_index": 0,
            "name": inst.name(),
            "category": "Instrument",
            "enabled": inst.is_enabled(),
            "latency_samples": inst.latency_samples()
        }));
        idx += 1;
    }

    for (a_idx, afx) in chain.audio_effects.iter().enumerate() {
        devices.push(json!({
            "index": idx,
            "category_index": a_idx,
            "name": afx.name(),
            "category": "AudioEffect",
            "enabled": afx.is_enabled(),
            "latency_samples": afx.latency_samples()
        }));
        idx += 1;
    }

    json!({
        "chain_name": chain.name(),
        "enabled": chain.is_enabled(),
        "total_latency_samples": chain.total_latency_samples(),
        "device_count": devices.len(),
        "devices": devices
    })
}
