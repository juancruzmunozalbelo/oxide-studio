//! LilyPond-like notation parser for electronic sequences and drum grooves.
//!
//! Reverse-engineered and inspired by Ableton Learning Synths / Live 12
//! (`/Users/juancruz/ableton-rea/src/engine/sequencer.ts`).

use oxide_timeline::{ClipPattern, DrumStep, DrumStepPattern, MidiNote, MidiPattern};

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedItem {
    Pitch { note: u8 },
    Rest,
    Drum { pad_idx: u8 },
}

#[derive(Debug, Clone)]
pub struct ParsedStep {
    pub items: Vec<ParsedItem>,
    pub duration_beats: f64,
}

#[derive(Debug, Clone)]
pub struct ParsedSequence {
    pub tempo: f64,
    pub voices: Vec<Vec<ParsedStep>>,
}

/// Parse single note/drum token, e.g. "c'", "bf,", "fs", "bd", "sn", "hh", "r".
fn parse_single_pitch_or_drum(token: &str) -> ParsedItem {
    let lower = token.trim().to_lowercase();
    match lower.as_str() {
        "r" => return ParsedItem::Rest,
        "bd" => return ParsedItem::Drum { pad_idx: 0 },
        "rim" => return ParsedItem::Drum { pad_idx: 1 },
        "sn" => return ParsedItem::Drum { pad_idx: 2 },
        "clap" => return ParsedItem::Drum { pad_idx: 3 },
        "hh" | "ch" => return ParsedItem::Drum { pad_idx: 6 },
        "oh" => return ParsedItem::Drum { pad_idx: 10 },
        _ => {}
    }

    let mut chars = lower.chars().peekable();
    let letter = match chars.next() {
        Some(c) => c,
        None => return ParsedItem::Rest,
    };

    let base_semitone = match letter {
        'c' => 0,
        'd' => 2,
        'e' => 4,
        'f' => 5,
        'g' => 7,
        'a' => 9,
        'b' => 11,
        _ => return ParsedItem::Rest,
    };

    let mut acc = 0i32;
    let mut octave = 4i32; // Default base octave = 4 (MIDI C4 = 60, C3 = 48)

    while let Some(&ch) = chars.peek() {
        match ch {
            's' => {
                acc += 1;
                chars.next();
            }
            'f' => {
                acc -= 1;
                chars.next();
            }
            '\'' => {
                octave += 1;
                chars.next();
            }
            ',' => {
                octave -= 1;
                chars.next();
            }
            _ => break,
        }
    }

    let midi_note = ((octave + 1) * 12 + base_semitone + acc).clamp(0, 127) as u8;
    ParsedItem::Pitch { note: midi_note }
}

/// Parse rhythm specification from denominator string, dot, and tuplet.
fn parse_rhythm(denom_str: &str, has_dot: bool, tuplet: Option<f64>, sticky: f64) -> f64 {
    let base_beats = if denom_str.is_empty() {
        sticky
    } else if let Ok(d) = denom_str.parse::<f64>() {
        if d > 0.0 {
            4.0 / d
        } else {
            sticky
        }
    } else {
        sticky
    };

    let dotted = if has_dot { base_beats * 1.5 } else { base_beats };
    if let Some(t) = tuplet {
        if t > 0.0 {
            dotted / t
        } else {
            dotted
        }
    } else {
        dotted
    }
}

/// Parse full LilyPond notation string.
pub fn parse_lilypond(input: &str) -> ParsedSequence {
    let mut text = input.trim();
    let mut tempo = 120.0;

    // Check for \tempo 4 = 126
    if text.starts_with("\\tempo") {
        let rest = text.trim_start_matches("\\tempo").trim();
        if let Some(eq_idx) = rest.find('=') {
            let tempo_part = rest[eq_idx + 1..].trim();
            let mut num_str = String::new();
            for c in tempo_part.chars() {
                if c.is_ascii_digit() || c == '.' {
                    num_str.push(c);
                } else if !num_str.is_empty() {
                    break;
                }
            }
            if let Ok(bpm) = num_str.parse::<f64>() {
                if bpm > 10.0 && bpm < 400.0 {
                    tempo = bpm;
                }
            }
            if let Some(line_end) = rest.find('\n') {
                text = rest[line_end + 1..].trim();
            } else if let Some(space_end) = rest[eq_idx + 1..].find(|c: char| c.is_whitespace() && !num_str.is_empty()) {
                text = rest[eq_idx + 1 + space_end..].trim();
            } else {
                text = "";
            }
        }
    }

    let mut voices = Vec::new();
    let voice_parts = text.split("//");

    for v_str in voice_parts {
        let cleaned = v_str.replace('|', " ");
        let tokens: Vec<&str> = cleaned.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }

        let mut steps = Vec::new();
        let mut sticky_rhythm = 1.0; // 1 beat (quarter note)

        let mut idx = 0;
        while idx < tokens.len() {
            let token = tokens[idx];
            idx += 1;

            if token.starts_with('<') {
                // Chord block: <c e g>4
                let mut chord_str = token.to_string();
                if !token.contains('>') {
                    while idx < tokens.len() {
                        let next_t = tokens[idx];
                        idx += 1;
                        chord_str.push(' ');
                        chord_str.push_str(next_t);
                        if next_t.contains('>') {
                            break;
                        }
                    }
                }
                let (inside, trailing) = match chord_str.find('>') {
                    Some(end_bracket) => (&chord_str[1..end_bracket], &chord_str[end_bracket + 1..]),
                    None => (chord_str.get(1..).unwrap_or(""), ""),
                };

                let (denom, dot, tuplet) = parse_rhythm_suffix(trailing);
                let dur = parse_rhythm(&denom, dot, tuplet, sticky_rhythm);
                sticky_rhythm = dur;

                let mut items = Vec::new();
                for note_tok in inside.split_whitespace() {
                    items.push(parse_single_pitch_or_drum(note_tok));
                }
                steps.push(ParsedStep {
                    items,
                    duration_beats: dur,
                });
            } else {
                // Single token, e.g. "c''4." or "r8" or "bd4"
                let (note_part, denom, dot, tuplet) = split_token_and_rhythm(token);
                let dur = parse_rhythm(&denom, dot, tuplet, sticky_rhythm);
                sticky_rhythm = dur;
                let item = parse_single_pitch_or_drum(&note_part);
                steps.push(ParsedStep {
                    items: vec![item],
                    duration_beats: dur,
                });
            }
        }
        voices.push(steps);
    }

    ParsedSequence { tempo, voices }
}

fn parse_rhythm_suffix(s: &str) -> (String, bool, Option<f64>) {
    let mut denom = String::new();
    let mut has_dot = false;
    let mut tuplet = None;

    let mut parts = s.split('/');
    let first = parts.next().unwrap_or("");
    for c in first.chars() {
        if c.is_ascii_digit() {
            denom.push(c);
        } else if c == '.' {
            has_dot = true;
        }
    }
    if let Some(t_str) = parts.next() {
        if let Ok(t) = t_str.trim().parse::<f64>() {
            tuplet = Some(t);
        }
    }
    (denom, has_dot, tuplet)
}

fn split_token_and_rhythm(token: &str) -> (String, String, bool, Option<f64>) {
    let mut note_part = String::new();
    let mut suffix = String::new();

    let mut in_suffix = false;
    for c in token.chars() {
        if !in_suffix && (c.is_ascii_digit() || c == '/') {
            in_suffix = true;
        }
        if in_suffix {
            suffix.push(c);
        } else {
            note_part.push(c);
        }
    }
    let (denom, dot, tuplet) = parse_rhythm_suffix(&suffix);
    (note_part, denom, dot, tuplet)
}

/// Convert parsed LilyPond sequence into a `MidiPattern` or `DrumStepPattern` clip.
pub fn sequence_to_clip_pattern(parsed: &ParsedSequence) -> ClipPattern {
    // Check if primarily drum triggers
    let mut drum_count = 0;
    let mut pitch_count = 0;
    for v in &parsed.voices {
        for s in v {
            for it in &s.items {
                match it {
                    ParsedItem::Drum { .. } => drum_count += 1,
                    ParsedItem::Pitch { .. } => pitch_count += 1,
                    ParsedItem::Rest => {}
                }
            }
        }
    }

    if drum_count > pitch_count {
        // Construct DrumStepPattern
        let mut drum_pat = DrumStepPattern::new_16_step();
        for v in &parsed.voices {
            let mut beat = 0.0f64;
            for s in v {
                let step_idx = (beat / 0.25).round() as usize;
                for it in &s.items {
                    if let ParsedItem::Drum { pad_idx } = it {
                        if step_idx < 16 {
                            drum_pat.set_step(*pad_idx, step_idx, DrumStep::new(true, 110, 36 + *pad_idx));
                        }
                    }
                }
                beat += s.duration_beats;
            }
        }
        ClipPattern::Drum(drum_pat)
    } else {
        // Construct MidiPattern
        let mut total_beats = 0.0f64;
        let mut notes = Vec::new();

        for v in &parsed.voices {
            let mut beat = 0.0f64;
            for s in v {
                let dur = s.duration_beats;
                for it in &s.items {
                    if let ParsedItem::Pitch { note } = it {
                        notes.push(MidiNote::new(*note, 100, beat, dur * 0.9));
                    }
                }
                beat += dur;
            }
            if beat > total_beats {
                total_beats = beat;
            }
        }

        let rounded_length = if total_beats <= 0.0 {
            4.0
        } else {
            (total_beats / 4.0).ceil() * 4.0
        };

        ClipPattern::Midi(MidiPattern {
            length_beats: rounded_length,
            notes,
        })
    }
}
