//! Integration tests for Ableton Live 12 .als bidirectional interoperability.

use std::fs;

use oxide_project::{
    load_als, save_als, DAWClip, DAWProject, DAWTrack, MixerChannel,
};
use oxide_timeline::{
    AudioClipData, ClipPattern, DrumStepPattern, MidiNote, MidiPattern, Scene, SessionClip,
};

#[test]
fn test_multitrack_als_roundtrip_with_gzip_magic_inspection() {
    let mut project = DAWProject::new("Oxide Festival Anthem");
    project.transport.bpm = 128.0;
    project.transport.time_signature_numerator = 4;
    project.transport.time_signature_denominator = 4;
    project.transport.loop_enabled = true;
    project.transport.loop_start_beat = 0.0;
    project.transport.loop_end_beat = 32.0;

    project.master_track.volume_db = -0.5;
    project.master_track.pan = 0.0;

    // Track 1: Acid Synth
    let mut synth_track = DAWTrack::new_synth(1, "Acid Lead");
    synth_track.mixer.volume_db = -4.5;
    synth_track.mixer.pan = -0.25;
    synth_track.mixer.mute = false;
    synth_track.mixer.solo = false;
    synth_track.mixer.send_a_db = -12.0;

    let mut acid_pattern = MidiPattern::new(8.0);
    acid_pattern.add_note(MidiNote::new(36, 127, 0.0, 0.5).with_probability(1.0));
    acid_pattern.add_note(MidiNote::new(36, 110, 0.5, 0.5).with_probability(1.0));
    acid_pattern.add_note(MidiNote::new(48, 120, 1.0, 1.0).with_probability(0.85));
    acid_pattern.add_note(MidiNote::new(51, 105, 2.0, 0.75).with_probability(0.9));
    let synth_clip = DAWClip::new(101, "Acid Pattern 1", ClipPattern::Midi(acid_pattern));
    synth_track.session_clips.push(Some(synth_clip));
    project.tracks.push(synth_track);

    // Track 2: 909 Drum Rack
    let mut drum_track = DAWTrack::new_drum_rack(2, "909 Core Kit");
    drum_track.mixer.volume_db = -1.5;
    drum_track.mixer.pan = 0.1;
    let mut drum_pat = DrumStepPattern::new_16_step();
    // 4-on-the-floor kick
    drum_pat.toggle_step(0, 0);
    drum_pat.toggle_step(0, 4);
    drum_pat.toggle_step(0, 8);
    drum_pat.toggle_step(0, 12);
    // Claps on 4 and 12
    drum_pat.toggle_step(3, 4);
    drum_pat.toggle_step(3, 12);
    let drum_clip = DAWClip::new(102, "Driving Techno 909", ClipPattern::Drum(drum_pat));
    drum_track.session_clips.push(Some(drum_clip));
    project.tracks.push(drum_track);

    // Track 3: Vocal / Audio Track
    let mut audio_track = DAWTrack::new_audio(3, "Vocal Stem");
    audio_track.mixer.volume_db = -6.0;
    audio_track.mixer.pan = 0.3;
    let audio_data = AudioClipData::new("/samples/vocals/drop_vocal.wav", 48000, 2, 16.0);
    let audio_clip = SessionClip::new(103, "Drop Vocal Phrase", ClipPattern::Audio(audio_data));
    audio_track.session_clips.push(Some(audio_clip));
    project.tracks.push(audio_track);

    // Scenes
    project.scenes.push(Scene::new(1, "Buildup"));
    project.scenes.push(Scene::new(2, "Peak Drop"));
    project.scenes.push(Scene::new(3, "Outro"));

    // Save to disk
    let test_dir = std::env::temp_dir().join("oxide_als_e2e_tests");
    let _ = fs::create_dir_all(&test_dir);
    let test_file = test_dir.join("festival_anthem.als");

    save_als(&project, &test_file).expect("save_als must write successfully");

    // Inspect Gzip header bytes directly from raw disk read
    let disk_bytes = fs::read(&test_file).expect("File must exist on disk");
    assert!(disk_bytes.len() > 20, "ALS file must contain data");
    assert_eq!(disk_bytes[0], 0x1f, "First byte must match Gzip magic 0x1f");
    assert_eq!(disk_bytes[1], 0x8b, "Second byte must match Gzip magic 0x8b");
    assert_eq!(disk_bytes[2], 0x08, "Third byte must indicate Deflate compression");

    // Load back using DAWProject::load_als and standalone load_als
    let loaded = DAWProject::load_als(&test_file).expect("load_als must parse successfully");
    let _loaded_standalone = load_als(&test_file).expect("standalone load_als must parse successfully");
    let _mixer_ref: &MixerChannel = &loaded.tracks[0].mixer;

    // Verify Project metadata & BPM
    assert_eq!(loaded.metadata.name, "Oxide Festival Anthem");
    assert!((loaded.transport.bpm - 128.0).abs() < 1e-3);
    assert_eq!(loaded.transport.time_signature_numerator, 4);
    assert_eq!(loaded.transport.time_signature_denominator, 4);
    assert!(loaded.transport.loop_enabled);
    assert!((loaded.transport.loop_start_beat - 0.0).abs() < 1e-3);
    assert!((loaded.transport.loop_end_beat - 32.0).abs() < 1e-3);

    // Verify Master Track
    assert!((loaded.master_track.volume_db - (-0.5)).abs() < 1e-3);
    assert!((loaded.master_track.pan - 0.0).abs() < 1e-3);

    // Verify Track Count and Names
    assert_eq!(loaded.tracks.len(), 3);
    assert_eq!(loaded.tracks[0].name, "Acid Lead");
    assert_eq!(loaded.tracks[1].name, "909 Core Kit");
    assert_eq!(loaded.tracks[2].name, "Vocal Stem");

    // Verify Track 1 Mixer
    assert!((loaded.tracks[0].mixer.volume_db - (-4.5)).abs() < 1e-3);
    assert!((loaded.tracks[0].mixer.pan - (-0.25)).abs() < 1e-3);
    assert!(!loaded.tracks[0].mixer.mute);
    assert!((loaded.tracks[0].mixer.send_a_db - (-12.0)).abs() < 1e-3);

    // Verify Track 1 Notes (Pitch, start_beat, duration_beats, velocity, probability)
    let clip0 = loaded.tracks[0].session_clips[0].as_ref().expect("Clip 0 must exist");
    match &clip0.pattern {
        ClipPattern::Midi(pat) => {
            assert_eq!(pat.notes.len(), 4);
            assert_eq!(pat.notes[0].pitch, 36);
            assert_eq!(pat.notes[0].velocity, 127);
            assert!((pat.notes[0].start_beat - 0.0).abs() < 1e-3);
            assert!((pat.notes[0].duration_beats - 0.5).abs() < 1e-3);
            assert!((pat.notes[0].probability - 1.0).abs() < 1e-2);

            assert_eq!(pat.notes[1].pitch, 36);
            assert_eq!(pat.notes[1].velocity, 110);
            assert!((pat.notes[1].start_beat - 0.5).abs() < 1e-3);
            assert!((pat.notes[1].duration_beats - 0.5).abs() < 1e-3);

            assert_eq!(pat.notes[2].pitch, 48);
            assert_eq!(pat.notes[2].velocity, 120);
            assert!((pat.notes[2].start_beat - 1.0).abs() < 1e-3);
            assert!((pat.notes[2].duration_beats - 1.0).abs() < 1e-3);
            assert!((pat.notes[2].probability - 0.85).abs() < 1e-2);

            assert_eq!(pat.notes[3].pitch, 51);
            assert_eq!(pat.notes[3].velocity, 105);
            assert!((pat.notes[3].start_beat - 2.0).abs() < 1e-3);
            assert!((pat.notes[3].duration_beats - 0.75).abs() < 1e-3);
            assert!((pat.notes[3].probability - 0.9).abs() < 1e-2);
        }
        _ => panic!("Expected MIDI clip pattern on track 1"),
    }

    // Verify Track 2 (909 Drums exported as MIDI notes)
    let clip1 = loaded.tracks[1].session_clips[0].as_ref().expect("Clip 1 must exist");
    match &clip1.pattern {
        ClipPattern::Midi(pat) => {
            // 4 kicks + 2 claps = 6 drum hits
            assert_eq!(pat.notes.len(), 6);
            // Kick pitch 36
            let kicks: Vec<&MidiNote> = pat.notes.iter().filter(|n| n.pitch == 36).collect();
            assert_eq!(kicks.len(), 4);
            assert!((kicks[0].start_beat - 0.0).abs() < 1e-3);
            assert!((kicks[1].start_beat - 1.0).abs() < 1e-3);
            assert!((kicks[2].start_beat - 2.0).abs() < 1e-3);
            assert!((kicks[3].start_beat - 3.0).abs() < 1e-3);
        }
        _ => panic!("Expected MIDI clip pattern on track 2"),
    }

    // Verify Track 3 (Audio Clip)
    let clip2 = loaded.tracks[2].session_clips[0].as_ref().expect("Clip 2 must exist");
    match &clip2.pattern {
        ClipPattern::Audio(data) => {
            assert_eq!(data.file_path, "/samples/vocals/drop_vocal.wav");
        }
        _ => panic!("Expected Audio clip pattern on track 3"),
    }

    // Verify Scenes
    assert_eq!(loaded.scenes.len(), 3);
    assert_eq!(loaded.scenes[0].name, "Buildup");
    assert_eq!(loaded.scenes[1].name, "Peak Drop");
    assert_eq!(loaded.scenes[2].name, "Outro");

    // Clean up
    let _ = fs::remove_file(test_file);
}

#[test]
fn test_raw_xml_auto_detection() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Ableton MajorVersion="5" MinorVersion="12.0_12049" SchemaChangeCount="3" Creator="Oxide Studio" Revision="oxide-12.0">
  <LiveSet>
    <Tempo>
      <Manual Value="140.0"/>
    </Tempo>
    <Tracks>
      <MidiTrack Id="1">
        <Name>
          <EffectiveName Value="Hard Techno Synth"/>
        </Name>
        <DeviceChain>
          <Mixer>
            <Volume><Manual Value="-2.0"/></Volume>
            <Pan><Manual Value="0.0"/></Pan>
            <On><Manual Value="true"/></On>
          </Mixer>
          <MainSequencer>
            <ClipSlotList>
              <ClipSlot Id="0">
                <ClipSlot>
                  <Value>
                    <MidiClip>
                      <Name Value="Lead Riff"/>
                      <Loop>
                        <LoopStart Value="0.0"/>
                        <LoopEnd Value="4.0"/>
                      </Loop>
                      <Notes>
                        <KeyTracks>
                          <KeyTrack>
                            <MidiKey Value="60"/>
                            <Notes>
                              <MidiNoteEvent Time="0.0" Duration="0.5" Velocity="120.0" Probability="1.0"/>
                              <MidiNoteEvent Time="1.0" Duration="0.5" Velocity="110.0" Probability="1.0"/>
                            </Notes>
                          </KeyTrack>
                        </KeyTracks>
                      </Notes>
                    </MidiClip>
                  </Value>
                </ClipSlot>
              </ClipSlot>
            </ClipSlotList>
          </MainSequencer>
        </DeviceChain>
      </MidiTrack>
    </Tracks>
  </LiveSet>
</Ableton>"#;

    // Pass raw uncompressed bytes directly into from_als_bytes
    let project = DAWProject::from_als_bytes(xml.as_bytes()).expect("Auto-detection of raw XML must succeed");
    assert!((project.transport.bpm - 140.0).abs() < 1e-3);
    assert_eq!(project.tracks.len(), 1);
    assert_eq!(project.tracks[0].name, "Hard Techno Synth");
    assert!((project.tracks[0].mixer.volume_db - (-2.0)).abs() < 1e-3);

    let notes = match &project.tracks[0].session_clips[0].as_ref().unwrap().pattern {
        ClipPattern::Midi(p) => &p.notes,
        _ => panic!("Expected Midi"),
    };
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].pitch, 60);
    assert_eq!(notes[1].pitch, 60);
    assert!((notes[0].start_beat - 0.0).abs() < 1e-3);
    assert!((notes[1].start_beat - 1.0).abs() < 1e-3);
}

#[test]
fn test_corrupt_bytes_error_handling() {
    // Random non-XML, non-gzip bytes
    let bad_bytes = [0xde, 0xad, 0xbe, 0xef, 0x00, 0x11, 0x22];
    let res = DAWProject::from_als_bytes(&bad_bytes);
    assert!(res.is_err(), "Invalid bytes must return Err");

    // Fake gzip header with garbage payload
    let fake_gzip = [0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff];
    let res2 = DAWProject::from_als_bytes(&fake_gzip);
    assert!(res2.is_err(), "Corrupt gzip payload must return Err");
}
