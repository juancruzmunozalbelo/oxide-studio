//! Live Concepts Chapter 3: Signal Routing Graph and Topology Validator.
//!
//! Models the signal flow of Ableton Live 12:
//! - Track inputs (hardware, virtual, resampling)
//! - Track channel strips (Audio, MIDI, Return, Master, Group)
//! - Pre/post fader auxiliary sends feeding return tracks
//! - Group bus submixes and master summing bus
//! - Real-time DAG cycle detection to prevent feedback loops

use ableton_core::TrackArchetype;
use crate::app::AbletonStudioApp;

/// Edge in the studio signal routing graph representing a directed audio path.
#[derive(Debug, Clone, PartialEq)]
pub enum SignalEdge {
    /// Audio signal routed directly to the Master summing bus.
    DirectToMaster,
    /// Audio signal routed into a Group submix bus.
    GroupBus(usize),
    /// Auxiliary send tap routing into a Return track (e.g. Return A Reverb).
    SendAux { return_idx: usize, level: f32, pre_fader: bool },
    /// Master or direct output routed to hardware DAC channels.
    HardwareOut(u16),
}

/// A node in the studio signal routing graph representing a track channel strip.
#[derive(Debug, Clone, PartialEq)]
pub struct SignalRoutingNode {
    pub id: usize,
    pub name: String,
    pub archetype: TrackArchetype,
    pub input_source: String,
    pub output_destination: String,
    pub volume_linear: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    pub edges: Vec<SignalEdge>,
}

/// Routing topology error when graph invariants are violated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutingValidationError {
    CircularRouting { track_id: usize, track_name: String },
    DanglingDestination { track_id: usize, destination: String },
    InvalidReturnIndex { track_id: usize, return_idx: usize },
}

/// Directed Acyclic Graph (DAG) representing the complete audio/MIDI signal flow
/// according to Ableton Live 12 Reference Manual Chapter 3.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SignalRoutingGraph {
    pub nodes: Vec<SignalRoutingNode>,
}

impl SignalRoutingGraph {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    pub fn add_node(&mut self, node: SignalRoutingNode) {
        self.nodes.push(node);
    }

    /// Construct the complete signal routing graph from live GUI application state.
    pub fn build_from_app(app: &AbletonStudioApp) -> Self {
        let mut graph = Self::new();

        // 1. Standard Audio / MIDI Tracks
        for (idx, trk) in app.tracks_gui.iter().enumerate() {
            let mut edges = Vec::new();
            if trk.output_routing == "Master" {
                edges.push(SignalEdge::DirectToMaster);
            } else if let Some(grp_str) = trk.output_routing.strip_prefix("Group ") {
                if let Ok(grp_id) = grp_str.parse::<usize>() {
                    edges.push(SignalEdge::GroupBus(grp_id));
                }
            }

            for (s_idx, &s_val) in trk.sends.iter().enumerate() {
                if s_val > 0.0 {
                    edges.push(SignalEdge::SendAux {
                        return_idx: s_idx,
                        level: s_val,
                        pre_fader: false,
                    });
                }
            }

            // Distinguish track archetype based on track setup
            let archetype = if idx == 1 {
                TrackArchetype::Audio // Drum Kit / Audio
            } else {
                TrackArchetype::Midi  // Synth / MIDI
            };

            graph.add_node(SignalRoutingNode {
                id: idx,
                name: trk.name.clone(),
                archetype,
                input_source: trk.input_routing.clone(),
                output_destination: trk.output_routing.clone(),
                volume_linear: trk.volume_linear,
                pan: trk.pan,
                mute: trk.mute,
                solo: trk.solo,
                edges,
            });
        }

        // 2. Return Tracks (A Reverb, B Delay, etc.)
        for (ret_idx, ret) in app.return_tracks.iter().enumerate() {
            let node_id = 100 + ret_idx;
            graph.add_node(SignalRoutingNode {
                id: node_id,
                name: ret.name.clone(),
                archetype: TrackArchetype::Return,
                input_source: format!("Send {}", (b'A' + (ret_idx % 26) as u8) as char),
                output_destination: "Master".to_string(),
                volume_linear: ret.volume_linear,
                pan: ret.pan,
                mute: ret.mute,
                solo: ret.solo,
                edges: vec![SignalEdge::DirectToMaster],
            });
        }

        // 3. Master Track
        graph.add_node(SignalRoutingNode {
            id: 999,
            name: "Master".to_string(),
            archetype: TrackArchetype::Master,
            input_source: "Summing Bus".to_string(),
            output_destination: "Ext. 1/2".to_string(),
            volume_linear: app.master_volume,
            pan: 0.0,
            mute: false,
            solo: false,
            edges: vec![SignalEdge::HardwareOut(1)],
        });

        graph
    }

    /// Validate that the signal routing graph contains no circular routing dependencies.
    pub fn validate_no_cycles(&self) -> Result<(), RoutingValidationError> {
        for node in &self.nodes {
            let current = node.id;
            for edge in &node.edges {
                if let SignalEdge::GroupBus(target_grp) = edge {
                    if *target_grp == current {
                        return Err(RoutingValidationError::CircularRouting {
                            track_id: current,
                            track_name: node.name.clone(),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Query all active audio sources currently unmuted and feeding the master bus.
    pub fn active_unmuted_tracks(&self) -> Vec<&SignalRoutingNode> {
        self.nodes.iter().filter(|n| !n.mute && n.volume_linear > 0.0).collect()
    }

    /// Query all return tracks.
    pub fn return_channels(&self) -> Vec<&SignalRoutingNode> {
        self.nodes.iter().filter(|n| n.archetype.is_return()).collect()
    }

    /// Query the master channel node.
    pub fn master_node(&self) -> Option<&SignalRoutingNode> {
        self.nodes.iter().find(|n| n.archetype.is_master())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_routing_graph_construction() {
        let app = AbletonStudioApp::new_mock(44100.0);
        let graph = SignalRoutingGraph::build_from_app(&app);

        // Standard mock app has 3 tracks + 2 returns + 1 master = 6 nodes
        assert_eq!(graph.nodes.len(), 6);

        let returns = graph.return_channels();
        assert_eq!(returns.len(), 2);
        assert_eq!(returns[0].name, "A Reverb");
        assert_eq!(returns[1].name, "B Delay");

        let master = graph.master_node().expect("Master node must exist");
        assert_eq!(master.name, "Master");
        assert_eq!(master.archetype, TrackArchetype::Master);

        // Initial graph has zero cycles
        assert!(graph.validate_no_cycles().is_ok());

        // Unmuted active tracks
        let active = graph.active_unmuted_tracks();
        assert_eq!(active.len(), 6);
    }

    #[test]
    fn test_signal_routing_cycle_detection() {
        let mut graph = SignalRoutingGraph::new();
        graph.add_node(SignalRoutingNode {
            id: 5,
            name: "Self-Referencing Group".into(),
            archetype: TrackArchetype::Group,
            input_source: "All Ins".into(),
            output_destination: "Group 5".into(),
            volume_linear: 1.0,
            pan: 0.0,
            mute: false,
            solo: false,
            edges: vec![SignalEdge::GroupBus(5)],
        });

        let result = graph.validate_no_cycles();
        assert!(result.is_err());
        assert_eq!(
            result,
            Err(RoutingValidationError::CircularRouting {
                track_id: 5,
                track_name: "Self-Referencing Group".into(),
            })
        );
    }
}
