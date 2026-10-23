//! Integration tests for importing real-world and synthetic Ableton Live 12 projects.

use std::fs;
use std::path::Path;

use oxide_project::als::from_als_xml_with_base_dir;
use oxide_project::DAWProject;
use oxide_timeline::ClipPattern;

#[test]
fn test_external_als_import_if_configured() {
    let Ok(als_path_str) = std::env::var("OXIDE_TEST_ALS_PATH") else {
        return;
    };
    let real_als_path = Path::new(&als_path_str);
    if !real_als_path.exists() {
        return;
    }

    let project = DAWProject::load_als(real_als_path)
        .expect("DAWProject::load_als must successfully load real-world Ableton Live 12 ALS");

    // 1. Assert BPM == 135.0
    assert!(
        (project.transport.bpm - 135.0).abs() < 1e-3,
        "Expected BPM 135.0, got {}",
        project.transport.bpm
    );

    // 2. Assert tracks >= 20 (there are 30 Audio/MIDI/Return tracks after GroupTrack filtering)
    assert!(
        project.tracks.len() >= 20,
        "Expected >= 20 tracks, got {}",
        project.tracks.len()
    );

    // 3. Assert total imported MIDI notes > 1000
    let mut total_notes = 0usize;
    for track in &project.tracks {
        for arr_clip in &track.arrangement_clips {
            if let ClipPattern::Midi(m) = &arr_clip.pattern {
                total_notes += m.notes.len();
            }
        }
    }
    println!("Total imported MIDI notes in arrangement clips: {}", total_notes);
    assert!(
        total_notes > 1000,
        "Expected total imported MIDI notes > 1000, got {}",
        total_notes
    );

    // 4. Assert that audio tracks resolved real .wav paths on disk
    let mut resolved_wav_count = 0usize;
    let mut clips_with_pcm_samples = 0usize;
    for track in &project.tracks {
        for arr_clip in &track.arrangement_clips {
            if let ClipPattern::Audio(data) = &arr_clip.pattern {
                if Path::new(&data.file_path).exists() {
                    resolved_wav_count += 1;
                    if !data.samples.is_empty() {
                        clips_with_pcm_samples += 1;
                    }
                }
            }
        }
        for s_clip_opt in &track.session_clips {
            if let Some(s_clip) = s_clip_opt {
                if let ClipPattern::Audio(data) = &s_clip.pattern {
                    if Path::new(&data.file_path).exists() {
                        resolved_wav_count += 1;
                        if !data.samples.is_empty() {
                            clips_with_pcm_samples += 1;
                        }
                    }
                }
            }
        }
    }

    println!(
        "Resolved real WAV paths on disk: {}, clips with loaded PCM samples: {}",
        resolved_wav_count, clips_with_pcm_samples
    );
    assert!(
        resolved_wav_count > 0,
        "Expected audio tracks to resolve real .wav paths on disk, got 0"
    );
    assert!(
        clips_with_pcm_samples > 0,
        "Expected at least one audio clip to have populated AudioClipData.samples"
    );

    // 5. Verify session clips auto-populated for Arrangement-only projects
    let tracks_with_session_clips = project
        .tracks
        .iter()
        .filter(|t| !t.session_clips.is_empty())
        .count();
    println!("Tracks with auto-populated session clips: {}", tracks_with_session_clips);
    assert!(
        tracks_with_session_clips > 0,
        "Expected tracks with arrangement clips to auto-populate up to 8 session clips"
    );
}

#[test]
fn test_synthetic_live12_arrangement_and_linear_volume_import() {
    let tmp_dir = std::env::temp_dir().join("oxide_test_live12_synthetic");
    let samples_dir = tmp_dir.join("Samples").join("Imported");
    let _ = fs::create_dir_all(&samples_dir);

    // Create a valid 44-byte 16-bit PCM WAV sample for relative path testing
    let test_wav_path = samples_dir.join("drop_vocal.wav");
    let mut wav_bytes = Vec::new();
    wav_bytes.extend_from_slice(b"RIFF");
    wav_bytes.extend_from_slice(&(44u32 + 32u32 - 8u32).to_le_bytes());
    wav_bytes.extend_from_slice(b"WAVE");
    wav_bytes.extend_from_slice(b"fmt ");
    wav_bytes.extend_from_slice(&16u32.to_le_bytes()); // fmt size
    wav_bytes.extend_from_slice(&1u16.to_le_bytes());  // PCM
    wav_bytes.extend_from_slice(&2u16.to_le_bytes());  // stereo
    wav_bytes.extend_from_slice(&44100u32.to_le_bytes());
    wav_bytes.extend_from_slice(&(44100u32 * 4).to_le_bytes()); // byte rate
    wav_bytes.extend_from_slice(&4u16.to_le_bytes());  // block align
    wav_bytes.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    wav_bytes.extend_from_slice(b"data");
    wav_bytes.extend_from_slice(&32u32.to_le_bytes()); // 8 frames stereo
    for i in 0..16 {
        let val = ((i as f32 / 16.0) * 16384.0) as i16;
        wav_bytes.extend_from_slice(&val.to_le_bytes());
    }
    fs::write(&test_wav_path, &wav_bytes).expect("Failed to write test WAV");

    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Ableton MajorVersion="5" MinorVersion="12.0_12203" SchemaChangeCount="3" Creator="Ableton Live 12.2.1" Revision="c0d59beb0465cc1bcc75830dbff2b10a48082a7d">
  <LiveSet>
    <Tempo>
      <Manual Value="135.0000"/>
    </Tempo>
    <MasterTrack Id="0">
      <DeviceChain>
        <Mixer>
          <Volume><Manual Value="0.85"/></Volume>
          <Pan><Manual Value="0.0"/></Pan>
        </Mixer>
      </DeviceChain>
    </MasterTrack>
    <Tracks>
      <GroupTrack Id="33">
        <Name><EffectiveName Value="1-Group"/></Name>
      </GroupTrack>
      <MidiTrack Id="16">
        <Name><EffectiveName Value="7-ADRKT_WTTB4_Synth_10"/></Name>
        <DeviceChain>
          <Mixer>
            <Volume><Manual Value="0.85"/></Volume>
            <Pan><Manual Value="-0.2"/></Pan>
          </Mixer>
          <MainSequencer>
            <ClipSlotList>
              <ClipSlot Id="0"/>
            </ClipSlotList>
            <ClipTimeable>
              <ArrangerAutomation>
                <Events>
                  <MidiClip Id="42" Time="32">
                    <CurrentStart Value="32.0"/>
                    <CurrentEnd Value="40.0"/>
                    <Name Value="Lead Motif"/>
                    <Notes>
                      <KeyTracks>
                        <KeyTrack>
                          <MidiKey Value="60"/>
                          <Notes>
                            <MidiNoteEvent Time="0.0" Duration="1.0" Velocity="100.0" Probability="1.0"/>
                            <MidiNoteEvent Time="2.0" Duration="1.5" Velocity="115.0" Probability="0.9"/>
                          </Notes>
                        </KeyTrack>
                      </KeyTracks>
                    </Notes>
                  </MidiClip>
                </Events>
              </ArrangerAutomation>
            </ClipTimeable>
          </MainSequencer>
        </DeviceChain>
      </MidiTrack>
      <AudioTrack Id="27">
        <Name><EffectiveName Value="16-Vocal Hook"/></Name>
        <DeviceChain>
          <Mixer>
            <Volume><Manual Value="0.0000"/></Volume>
            <Pan><Manual Value="0.1"/></Pan>
          </Mixer>
          <MainSequencer>
            <ClipSlotList>
              <ClipSlot Id="0"/>
            </ClipSlotList>
            <Sample>
              <ArrangerAutomation>
                <Events>
                  <AudioClip Id="88" Time="0">
                    <CurrentStart Value="0.0"/>
                    <CurrentEnd Value="16.0"/>
                    <Name Value="Drop Vocal"/>
                    <SampleRef>
                      <FileRef>
                        <Path Value="C:/Users/Producer/Music/Techno Session Project/Samples/Imported/drop_vocal.wav"/>
                        <RelativePath Value="Samples/Imported/drop_vocal.wav"/>
                      </FileRef>
                    </SampleRef>
                  </AudioClip>
                </Events>
              </ArrangerAutomation>
            </Sample>
          </MainSequencer>
        </DeviceChain>
      </AudioTrack>
    </Tracks>
  </LiveSet>
</Ableton>
"#;

    let project = from_als_xml_with_base_dir(xml, Some(&tmp_dir))
        .expect("Synthetic XML with base_dir must parse successfully");

    // 1. Verify tempo
    assert!((project.transport.bpm - 135.0).abs() < 1e-3);

    // 2. Verify volume conversion (Ableton 0.85 linear -> 0.0 dB)
    assert!(
        project.master_track.volume_db.abs() < 0.05,
        "Master volume 0.85 linear must convert to ~0.0 dB, got {}",
        project.master_track.volume_db
    );

    // 3. Verify GroupTrack skipped cleanly
    assert_eq!(project.tracks.len(), 2, "Expected 2 tracks (GroupTrack skipped)");

    // 4. Verify MIDI track
    let midi_trk = &project.tracks[0];
    assert_eq!(midi_trk.id, 16);
    assert_eq!(midi_trk.name, "7-ADRKT_WTTB4_Synth_10");
    assert!(midi_trk.mixer.volume_db.abs() < 0.05);
    assert_eq!(midi_trk.arrangement_clips.len(), 1);
    let arr_mclip = &midi_trk.arrangement_clips[0];
    assert_eq!(arr_mclip.timeline_start_beat, 32.0);
    assert_eq!(arr_mclip.duration_beats, 8.0);
    if let ClipPattern::Midi(pat) = &arr_mclip.pattern {
        assert_eq!(pat.notes.len(), 2);
        assert_eq!(pat.notes[0].pitch, 60);
    } else {
        panic!("Expected Midi pattern");
    }
    // Auto-populated session clips
    assert_eq!(midi_trk.session_clips.len(), 1);
    assert!(midi_trk.session_clips[0].is_some());

    // 5. Verify Audio track with relative sample resolution
    let audio_trk = &project.tracks[1];
    assert_eq!(audio_trk.id, 27);
    assert_eq!(audio_trk.name, "16-Vocal Hook");
    assert_eq!(audio_trk.mixer.volume_db, -60.0);
    assert_eq!(audio_trk.arrangement_clips.len(), 1);
    let arr_aclip = &audio_trk.arrangement_clips[0];
    assert_eq!(arr_aclip.timeline_start_beat, 0.0);
    assert_eq!(arr_aclip.duration_beats, 16.0);
    if let ClipPattern::Audio(data) = &arr_aclip.pattern {
        assert!(
            Path::new(&data.file_path).exists(),
            "Sample path must be resolved against base_dir: {}",
            data.file_path
        );
        assert!(
            !data.samples.is_empty(),
            "Audio clip must contain loaded PCM samples from disk"
        );
        assert_eq!(data.sample_rate, 44100);
        assert_eq!(data.channels, 2);
    } else {
        panic!("Expected Audio pattern");
    }

    // Auto-populated session clips
    assert_eq!(audio_trk.session_clips.len(), 1);
    assert!(audio_trk.session_clips[0].is_some());

    let _ = fs::remove_dir_all(&tmp_dir);
}
