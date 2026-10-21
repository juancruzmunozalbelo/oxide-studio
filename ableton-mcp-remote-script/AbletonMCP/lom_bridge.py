# -*- coding: utf-8 -*-
"""
Ableton Live 12 LOM Bridge Module (Python 3.11).

Provides safe command execution, LOM inspection, parameter mutation,
and standalone mock objects for testing without Live running.
"""

from __future__ import annotations
import math
import sys
from typing import Any, Dict, List, Optional, Tuple, Union


# ---------------------------------------------------------------------------
# Mock LOM Objects for Standalone Testing
# ---------------------------------------------------------------------------

class MockParameter:
    """Mock Live.DeviceParameter.DeviceParameter."""

    def __init__(self, name: str, value: float = 0.5, min_val: float = 0.0, max_val: float = 1.0):
        self.name = name
        self._value = float(value)
        self.min = float(min_val)
        self.max = float(max_val)

    @property
    def value(self) -> float:
        return self._value

    @value.setter
    def value(self, val: float):
        self._value = max(self.min, min(self.max, float(val)))

    def __repr__(self) -> str:
        return f"<MockParameter '{self.name}'={self._value:.3f} [{self.min}..{self.max}]>"


class MockDevice:
    """Mock Live.Device.Device."""

    def __init__(self, name: str, class_name: str, parameters: Optional[List[MockParameter]] = None):
        self.name = name
        self.class_name = class_name
        self.class_display_name = name
        if parameters is None:
            self.parameters = [
                MockParameter("Device On", 1.0, 0.0, 1.0),
                MockParameter("Frequency", 440.0, 20.0, 20000.0),
                MockParameter("Resonance", 0.5, 0.0, 1.0),
                MockParameter("Gain", 0.0, -70.0, 6.0),
            ]
        else:
            self.parameters = parameters

    def __repr__(self) -> str:
        return f"<MockDevice '{self.name}' ({self.class_name}) params={len(self.parameters)}>"


class MockClip:
    """Mock Live.Clip.Clip."""

    def __init__(self, length_beats: float = 4.0, name: str = "Clip"):
        self.length = float(length_beats)
        self.name = name
        self.is_playing = False
        self.notes: List[Tuple[int, float, float, int, bool]] = []

    def set_notes(self, notes: List[Tuple[int, float, float, int, bool]]):
        self.notes = list(notes)

    def add_new_notes(self, notes: List[Tuple[int, float, float, int, bool]]):
        self.notes.extend(notes)

    def get_notes_extended(self, start_pitch: int, pitch_span: int, start_time: float, time_span: float):
        result = []
        for n in self.notes:
            pitch, start, dur, vel, mute = n
            if (start_pitch <= pitch < start_pitch + pitch_span and
                    start_time <= start < start_time + time_span):
                result.append(n)
        return tuple(result)

    def __repr__(self) -> str:
        return f"<MockClip '{self.name}' len={self.length} notes={len(self.notes)}>"


class MockClipSlot:
    """Mock Live.ClipSlot.ClipSlot."""

    def __init__(self, slot_index: int):
        self.slot_index = slot_index
        self._clip: Optional[MockClip] = None
        self.is_playing = False

    @property
    def has_clip(self) -> bool:
        return self._clip is not None

    @property
    def clip(self) -> Optional[MockClip]:
        return self._clip

    def create_clip(self, length_beats: float) -> MockClip:
        self._clip = MockClip(length_beats, name=f"Clip {self.slot_index + 1}")
        return self._clip

    def delete_clip(self):
        self._clip = None
        self.is_playing = False

    def fire(self):
        if self._clip:
            self.is_playing = True
            self._clip.is_playing = True

    def stop(self):
        self.is_playing = False
        if self._clip:
            self._clip.is_playing = False

    def __repr__(self) -> str:
        return f"<MockClipSlot {self.slot_index} has_clip={self.has_clip}>"


class MockTrack:
    """Mock Live.Track.Track."""

    def __init__(self, name: str, is_audio: bool = False, is_return: bool = False):
        self.name = name
        self.is_audio = is_audio
        self.is_return = is_return
        self.is_foldable = False
        self.mute = False
        self.solo = False
        self.arm = False
        self.devices: List[MockDevice] = []
        self.clip_slots: List[MockClipSlot] = [MockClipSlot(i) for i in range(8)]

        if not is_return and not is_audio:
            # Default synth device
            self.devices.append(MockDevice("Wavetable", "InstrumentDevice", [
                MockParameter("Device On", 1.0, 0.0, 1.0),
                MockParameter("Osc 1 Pos", 0.25, 0.0, 1.0),
                MockParameter("Filter Cutoff", 2500.0, 20.0, 20000.0),
                MockParameter("Filter Res", 0.4, 0.0, 1.0),
            ]))
        elif is_return:
            self.devices.append(MockDevice("Hybrid Reverb", "AudioEffectDevice", [
                MockParameter("Device On", 1.0, 0.0, 1.0),
                MockParameter("Decay Time", 2.5, 0.1, 20.0),
                MockParameter("Dry/Wet", 1.0, 0.0, 1.0),
            ]))

    def stop_all_clips(self):
        for slot in self.clip_slots:
            slot.stop()

    def __repr__(self) -> str:
        return f"<MockTrack '{self.name}' devices={len(self.devices)} slots={len(self.clip_slots)}>"


class MockScene:
    """Mock Live.Scene.Scene."""

    def __init__(self, name: str, song: MockSong):
        self.name = name
        self._song = song
        self.is_triggered = False

    def fire(self):
        self.is_triggered = True
        # Fire corresponding clip in each track
        idx = self._song.scenes.index(self) if self in self._song.scenes else 0
        for trk in self._song.tracks:
            if idx < len(trk.clip_slots):
                trk.clip_slots[idx].fire()

    def __repr__(self) -> str:
        return f"<MockScene '{self.name}'>"


class MockBrowser:
    """Mock Live.Application.Browser."""

    def __init__(self):
        self.loaded_items: List[str] = []

    def load_item(self, item_or_uri: Any):
        self.loaded_items.append(str(item_or_uri))
        return True


class MockApplication:
    """Mock Live.Application.Application."""

    def __init__(self):
        self.browser = MockBrowser()
        self.version = "12.4.6"

    def get_version_string(self) -> str:
        return self.version


class MockSong:
    """Mock Live.Song.Song."""

    def __init__(self):
        self.tempo: float = 124.0
        self.is_playing: bool = False
        self.current_song_time: float = 0.0
        self.loop: bool = True
        self.signature_numerator: int = 4
        self.signature_denominator: int = 4
        self.scale_name: str = "Dorian"
        self.root_note: int = 5  # F
        self.tracks: List[MockTrack] = [
            MockTrack("1 Synth", is_audio=False),
            MockTrack("2 Drums", is_audio=False),
            MockTrack("3 Audio", is_audio=True),
        ]
        self.return_tracks: List[MockTrack] = [
            MockTrack("A Reverb", is_return=True),
            MockTrack("B Delay", is_return=True),
        ]
        self.scenes: List[MockScene] = [MockScene(f"Scene {i + 1}", self) for i in range(8)]

    def create_midi_track(self, index: int = -1) -> MockTrack:
        trk = MockTrack(f"{len(self.tracks) + 1} MIDI", is_audio=False)
        if index < 0 or index >= len(self.tracks):
            self.tracks.append(trk)
        else:
            self.tracks.insert(index, trk)
        return trk

    def create_audio_track(self, index: int = -1) -> MockTrack:
        trk = MockTrack(f"{len(self.tracks) + 1} Audio", is_audio=True)
        if index < 0 or index >= len(self.tracks):
            self.tracks.append(trk)
        else:
            self.tracks.insert(index, trk)
        return trk

    def create_return_track(self) -> MockTrack:
        char = chr(ord('A') + len(self.return_tracks))
        trk = MockTrack(f"{char} Return", is_return=True)
        self.return_tracks.append(trk)
        return trk

    def start_playing(self):
        self.is_playing = True

    def stop_playing(self):
        self.is_playing = False
        self.current_song_time = 0.0

    def __repr__(self) -> str:
        return f"<MockSong tempo={self.tempo} tracks={len(self.tracks)} returns={len(self.return_tracks)}>"


# ---------------------------------------------------------------------------
# LOM Command Evaluator & Dispatch Functions
# ---------------------------------------------------------------------------

def execute_lom_command(song: Any, app: Any, method: str, params: Dict[str, Any]) -> Any:
    """Execute a single LOM operation in a safe manner, returning JSON-serializable output."""
    if method == "ping":
        return {
            "status": "ok",
            "bridge": "AbletonMCP",
            "live_version": getattr(app, "version", "12.4.6"),
        }

    elif method == "get_song_info":
        return {
            "tempo": float(getattr(song, "tempo", 120.0)),
            "is_playing": bool(getattr(song, "is_playing", False)),
            "current_song_time": float(getattr(song, "current_song_time", 0.0)),
            "loop": bool(getattr(song, "loop", False)),
            "signature_numerator": int(getattr(song, "signature_numerator", 4)),
            "signature_denominator": int(getattr(song, "signature_denominator", 4)),
            "scale_name": str(getattr(song, "scale_name", "Major")),
            "root_note": int(getattr(song, "root_note", 0)),
            "track_count": len(getattr(song, "tracks", [])),
            "return_track_count": len(getattr(song, "return_tracks", [])),
            "scene_count": len(getattr(song, "scenes", [])),
        }

    elif method == "set_tempo":
        bpm = float(params.get("bpm", 120.0))
        bpm = max(20.0, min(999.0, bpm))
        song.tempo = bpm
        return {"tempo": float(song.tempo)}

    elif method == "set_scale":
        root_note = int(params.get("root_note", 0)) % 12
        scale_name = str(params.get("scale_name", "Major"))
        song.root_note = root_note
        song.scale_name = scale_name
        return {"root_note": int(song.root_note), "scale_name": str(song.scale_name)}

    elif method == "create_midi_track":
        index = params.get("index", -1)
        if index is None or index < 0 or index > len(song.tracks):
            index = len(song.tracks)
        song.create_midi_track(index)
        return {"index": index, "track_count": len(song.tracks)}

    elif method == "create_audio_track":
        index = params.get("index", -1)
        if index is None or index < 0 or index > len(song.tracks):
            index = len(song.tracks)
        song.create_audio_track(index)
        return {"index": index, "track_count": len(song.tracks)}

    elif method == "create_return_track":
        song.create_return_track()
        return {"return_track_count": len(song.return_tracks)}

    elif method == "load_browser_item":
        track_index = int(params.get("track_index", 0))
        uri = str(params.get("uri", ""))
        browser = getattr(app, "browser", None)
        if browser is not None:
            browser.load_item(uri)
        return {"loaded": True, "track_index": track_index, "uri": uri}

    elif method == "create_clip":
        track_index = int(params.get("track_index", 0))
        slot_index = int(params.get("slot_index", 0))
        length_beats = float(params.get("length_beats", 4.0))

        if track_index >= len(song.tracks):
            raise IndexError(f"Track index {track_index} out of range (total: {len(song.tracks)})")
        track = song.tracks[track_index]
        if slot_index >= len(track.clip_slots):
            raise IndexError(f"Slot index {slot_index} out of range (total: {len(track.clip_slots)})")
        slot = track.clip_slots[slot_index]
        slot.create_clip(length_beats)
        return {
            "created": True,
            "track_index": track_index,
            "slot_index": slot_index,
            "length_beats": length_beats,
        }

    elif method == "add_notes":
        track_index = int(params.get("track_index", 0))
        slot_index = int(params.get("slot_index", 0))
        notes_in = params.get("notes", [])

        if track_index >= len(song.tracks):
            raise IndexError(f"Track index {track_index} out of range")
        track = song.tracks[track_index]
        if slot_index >= len(track.clip_slots):
            raise IndexError(f"Slot index {slot_index} out of range")
        slot = track.clip_slots[slot_index]
        clip = slot.clip
        if clip is None:
            raise ValueError(f"No clip found in track {track_index}, slot {slot_index}")

        # Convert dict or tuple notes into Live note format
        # Note tuple: (pitch, start_time, duration, velocity, mute)
        converted_notes = []
        for n in notes_in:
            if isinstance(n, dict):
                pitch = int(n.get("pitch", 60))
                start = float(n.get("start_beat", n.get("start_time", 0.0)))
                dur = float(n.get("duration_beats", n.get("duration", 1.0)))
                vel = int(n.get("velocity", 100))
                mute = bool(n.get("mute", False))
                converted_notes.append((pitch, start, dur, vel, mute))
            elif isinstance(n, (list, tuple)):
                pitch = int(n[0])
                start = float(n[1])
                dur = float(n[2])
                vel = int(n[3]) if len(n) > 3 else 100
                mute = bool(n[4]) if len(n) > 4 else False
                converted_notes.append((pitch, start, dur, vel, mute))

        if hasattr(clip, "add_new_notes"):
            clip.add_new_notes(converted_notes)
        elif hasattr(clip, "set_notes"):
            clip.set_notes(converted_notes)

        return {
            "notes_added": len(converted_notes),
            "track_index": track_index,
            "slot_index": slot_index,
        }

    elif method == "fire_clip":
        track_index = int(params.get("track_index", 0))
        slot_index = int(params.get("slot_index", 0))
        if track_index >= len(song.tracks):
            raise IndexError(f"Track index {track_index} out of range")
        track = song.tracks[track_index]
        if slot_index >= len(track.clip_slots):
            raise IndexError(f"Slot index {slot_index} out of range")
        track.clip_slots[slot_index].fire()
        return {"fired": True, "track_index": track_index, "slot_index": slot_index}

    elif method == "stop_clip":
        track_index = int(params.get("track_index", 0))
        slot_index = params.get("slot_index")
        if track_index >= len(song.tracks):
            raise IndexError(f"Track index {track_index} out of range")
        track = song.tracks[track_index]
        if slot_index is not None:
            slot_idx = int(slot_index)
            if slot_idx < len(track.clip_slots):
                track.clip_slots[slot_idx].stop()
        else:
            track.stop_all_clips()
        return {"stopped": True, "track_index": track_index}

    elif method == "fire_scene":
        scene_index = int(params.get("scene_index", 0))
        if scene_index >= len(song.scenes):
            raise IndexError(f"Scene index {scene_index} out of range (total: {len(song.scenes)})")
        song.scenes[scene_index].fire()
        return {"fired": True, "scene_index": scene_index}

    elif method == "get_track_devices":
        track_index = int(params.get("track_index", 0))
        if track_index >= len(song.tracks):
            raise IndexError(f"Track index {track_index} out of range")
        track = song.tracks[track_index]
        devices_out = []
        for dev_idx, dev in enumerate(track.devices):
            params_out = []
            for p_idx, p in enumerate(dev.parameters):
                params_out.append({
                    "index": p_idx,
                    "name": getattr(p, "name", f"Param {p_idx}"),
                    "value": float(getattr(p, "value", 0.0)),
                    "min": float(getattr(p, "min", 0.0)),
                    "max": float(getattr(p, "max", 1.0)),
                })
            devices_out.append({
                "index": dev_idx,
                "name": getattr(dev, "name", "Device"),
                "class_name": getattr(dev, "class_name", "UnknownDevice"),
                "class_display_name": getattr(dev, "class_display_name", getattr(dev, "name", "Device")),
                "parameters": params_out,
            })
        return {
            "track_index": track_index,
            "track_name": track.name,
            "device_count": len(devices_out),
            "devices": devices_out,
        }

    elif method == "set_device_param":
        track_index = int(params.get("track_index", 0))
        device_index = int(params.get("device_index", 0))
        param_index = int(params.get("param_index", 0))
        value = float(params.get("value", 0.0))

        if track_index >= len(song.tracks):
            raise IndexError(f"Track index {track_index} out of range")
        track = song.tracks[track_index]
        if device_index >= len(track.devices):
            raise IndexError(f"Device index {device_index} out of range")
        dev = track.devices[device_index]
        if param_index >= len(dev.parameters):
            raise IndexError(f"Parameter index {param_index} out of range")
        param = dev.parameters[param_index]
        param.value = value
        return {
            "track_index": track_index,
            "device_index": device_index,
            "param_index": param_index,
            "name": param.name,
            "value": float(param.value),
        }

    elif method == "exec_lom_query":
        query = str(params.get("query", ""))
        scope = {
            "song": song,
            "app": app,
            "tracks": getattr(song, "tracks", []),
            "returns": getattr(song, "return_tracks", []),
            "scenes": getattr(song, "scenes", []),
            "math": math,
        }
        # Try evaluating as an expression first
        try:
            val = eval(query, {"__builtins__": {"len": len, "range": range, "str": str, "int": int, "float": float, "bool": bool, "list": list, "dict": dict}}, scope)
            return {"query": query, "result": _safe_serialize(val)}
        except SyntaxError:
            # Execute as statements
            loc: Dict[str, Any] = {}
            exec(query, scope, loc)
            return {"query": query, "result": _safe_serialize(loc)}

    else:
        raise NotImplementedError(f"Unknown LOM method: {method}")


def _safe_serialize(val: Any) -> Any:
    """Recursively convert arbitrary Python objects to JSON-serializable primitives."""
    if val is None or isinstance(val, (int, float, str, bool)):
        return val
    elif isinstance(val, (list, tuple)):
        return [_safe_serialize(x) for x in val]
    elif isinstance(val, dict):
        return {str(k): _safe_serialize(v) for k, v in val.items()}
    elif hasattr(val, "name"):
        return str(val)
    else:
        return repr(val)
