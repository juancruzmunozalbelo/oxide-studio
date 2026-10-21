# -*- coding: utf-8 -*-
"""Unit tests for AbletonMCP and lom_bridge."""

import unittest
import json
import socket
import time
import os
import sys

# Add parent directory to sys.path
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))

from AbletonMCP import lom_bridge, AbletonMCP


class TestLomBridge(unittest.TestCase):
    def setUp(self):
        self.song = lom_bridge.MockSong()
        self.app = lom_bridge.MockApplication()

    def test_ping(self):
        res = lom_bridge.execute_lom_command(self.song, self.app, "ping", {})
        self.assertEqual(res["status"], "ok")
        self.assertEqual(res["bridge"], "AbletonMCP")

    def test_get_song_info(self):
        res = lom_bridge.execute_lom_command(self.song, self.app, "get_song_info", {})
        self.assertEqual(res["tempo"], 124.0)
        self.assertEqual(res["track_count"], 3)
        self.assertEqual(res["return_track_count"], 2)
        self.assertEqual(res["scale_name"], "Dorian")
        self.assertEqual(res["root_note"], 5)

    def test_set_tempo_and_scale(self):
        res = lom_bridge.execute_lom_command(self.song, self.app, "set_tempo", {"bpm": 130.5})
        self.assertEqual(res["tempo"], 130.5)
        self.assertEqual(self.song.tempo, 130.5)

        res_s = lom_bridge.execute_lom_command(self.song, self.app, "set_scale", {"root_note": 9, "scale_name": "Aeolian"})
        self.assertEqual(res_s["root_note"], 9)
        self.assertEqual(res_s["scale_name"], "Aeolian")

    def test_track_creation_and_clips(self):
        t_res = lom_bridge.execute_lom_command(self.song, self.app, "create_midi_track", {})
        self.assertEqual(t_res["track_count"], 4)

        c_res = lom_bridge.execute_lom_command(self.song, self.app, "create_clip", {
            "track_index": 3,
            "slot_index": 0,
            "length_beats": 4.0,
        })
        self.assertTrue(c_res["created"])

        n_res = lom_bridge.execute_lom_command(self.song, self.app, "add_notes", {
            "track_index": 3,
            "slot_index": 0,
            "notes": [
                {"pitch": 60, "start_beat": 0.0, "duration_beats": 0.5, "velocity": 100},
                {"pitch": 64, "start_beat": 1.0, "duration_beats": 0.5, "velocity": 110},
            ]
        })
        self.assertEqual(n_res["notes_added"], 2)

        # Check devices
        dev_res = lom_bridge.execute_lom_command(self.song, self.app, "get_track_devices", {"track_index": 0})
        self.assertGreaterEqual(dev_res["device_count"], 1)

        # Set param
        p_res = lom_bridge.execute_lom_command(self.song, self.app, "set_device_param", {
            "track_index": 0,
            "device_index": 0,
            "param_index": 1,
            "value": 0.75,
        })
        self.assertEqual(p_res["value"], 0.75)

    def test_exec_lom_query(self):
        q_res = lom_bridge.execute_lom_command(self.song, self.app, "exec_lom_query", {
            "query": "song.tempo * 2"
        })
        self.assertEqual(q_res["result"], 248.0)


if __name__ == "__main__":
    unittest.main()
