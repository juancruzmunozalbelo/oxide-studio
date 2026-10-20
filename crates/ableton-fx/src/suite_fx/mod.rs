//! Live 12 Suite Audio Effects module.

pub mod auto_shift;
pub mod beat_repeat;
pub mod chorus_ensemble;
pub mod corpus;
pub mod echo;
pub mod erosion;
pub mod hybrid_reverb;
pub mod phaser_flanger;
pub mod redux;
pub mod spectral_resonator;
pub mod utility;

pub use auto_shift::AutoShift;
pub use beat_repeat::BeatRepeat;
pub use chorus_ensemble::{ChorusEnsemble, ChorusMode};
pub use corpus::{Corpus, CorpusResonatorType};
pub use echo::Echo;
pub use erosion::{Erosion, ErosionMode};
pub use hybrid_reverb::{HybridReverb, HybridReverbMode};
pub use phaser_flanger::{PhaserFlanger, PhaserFlangerMode};
pub use redux::Redux;
pub use spectral_resonator::SpectralResonator;
pub use utility::Utility;
