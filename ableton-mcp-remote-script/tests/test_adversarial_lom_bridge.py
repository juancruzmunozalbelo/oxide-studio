# -*- coding: utf-8 -*-
"""Adversarial Stress Test Suite for AbletonMCP Remote Script and lom_bridge."""

import unittest
import json
import socket
import time
import os
import sys
import threading

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))

from AbletonMCP import lom_bridge, AbletonMCP, DEFAULT_HOST, DEFAULT_PORT


class TestAdversarialLomBridge(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # Start standalone AbletonMCP server on DEFAULT_PORT (9877)
        cls.surface = AbletonMCP(c_instance=None)
        # Give server thread time to bind
        time.sleep(0.1)

    @classmethod
    def tearDownClass(cls):
        cls.surface.disconnect()
        time.sleep(0.1)

    def _send_line(self, line: str, timeout: float = 2.0) -> str:
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.settimeout(timeout)
        s.connect((DEFAULT_HOST, DEFAULT_PORT))
        if not line.endswith("\n"):
            line += "\n"
        s.sendall(line.encode("utf-8"))
        resp = ""
        while "\n" not in resp:
            chunk = s.recv(4096).decode("utf-8")
            if not chunk:
                break
            resp += chunk
        s.close()
        return resp.strip()

    def _rpc_call(self, method: str, params: dict, req_id=1, timeout: float = 2.0) -> dict:
        req = json.dumps({
            "jsonrpc": "2.0",
            "id": req_id,
            "method": method,
            "params": params,
        })
        resp_line = self._send_line(req, timeout=timeout)
        return json.loads(resp_line)

    # ------------------------------------------------------------------------
    # 1. Standalone All-Method Happy Path & Boundary Clamping
    # ------------------------------------------------------------------------

    def test_all_15_lom_methods_roundtrip(self):
        # 1. ping
        r = self._rpc_call("ping", {})
        self.assertEqual(r["result"]["status"], "ok")

        # 2. get_song_info
        r = self._rpc_call("get_song_info", {})
        self.assertIn("tempo", r["result"])
        self.assertIn("track_count", r["result"])

        # 3. set_tempo (with normal, negative, zero, extreme values)
        r = self._rpc_call("set_tempo", {"bpm": 140.0})
        self.assertEqual(r["result"]["tempo"], 140.0)

        # Tempo must be clamped to [20.0, 999.0]
        r_low = self._rpc_call("set_tempo", {"bpm": -100.0})
        self.assertEqual(r_low["result"]["tempo"], 20.0)

        r_high = self._rpc_call("set_tempo", {"bpm": 5000.0})
        self.assertEqual(r_high["result"]["tempo"], 999.0)

        # 4. set_scale
        r = self._rpc_call("set_scale", {"root_note": 14, "scale_name": "Phrygian"})
        # 14 % 12 == 2 (D)
        self.assertEqual(r["result"]["root_note"], 2)
        self.assertEqual(r["result"]["scale_name"], "Phrygian")

        # 5. create_midi_track, create_audio_track, create_return_track
        r_m = self._rpc_call("create_midi_track", {"index": 0})
        self.assertEqual(r_m["result"]["index"], 0)

        r_a = self._rpc_call("create_audio_track", {"index": -1})
        self.assertIn("track_count", r_a["result"])

        r_r = self._rpc_call("create_return_track", {})
        self.assertIn("return_track_count", r_r["result"])

        # 6. load_browser_item
        r_b = self._rpc_call("load_browser_item", {"track_index": 0, "uri": "query:synths/wavetable"})
        self.assertTrue(r_b["result"]["loaded"])

        # 7. create_clip & add_notes
        r_c = self._rpc_call("create_clip", {"track_index": 0, "slot_index": 0, "length_beats": 8.0})
        self.assertTrue(r_c["result"]["created"])

        # Notes as dict list
        r_n1 = self._rpc_call("add_notes", {
            "track_index": 0,
            "slot_index": 0,
            "notes": [
                {"pitch": 48, "start_beat": 0.0, "duration_beats": 1.0, "velocity": 100},
                {"pitch": 55, "start_beat": 2.0, "duration_beats": 2.0, "velocity": 90, "mute": True},
            ]
        })
        self.assertEqual(r_n1["result"]["notes_added"], 2)

        # Notes as tuple/list format: [pitch, start, dur, vel, mute]
        r_n2 = self._rpc_call("add_notes", {
            "track_index": 0,
            "slot_index": 0,
            "notes": [
                [60, 4.0, 1.0, 110, False],
                [64, 5.0, 0.5],
            ]
        })
        self.assertEqual(r_n2["result"]["notes_added"], 2)

        # 8. fire_clip, stop_clip, fire_scene
        r_fc = self._rpc_call("fire_clip", {"track_index": 0, "slot_index": 0})
        self.assertTrue(r_fc["result"]["fired"])

        r_sc = self._rpc_call("stop_clip", {"track_index": 0, "slot_index": 0})
        self.assertTrue(r_sc["result"]["stopped"])

        r_sc_all = self._rpc_call("stop_clip", {"track_index": 0})
        self.assertTrue(r_sc_all["result"]["stopped"])

        r_fs = self._rpc_call("fire_scene", {"scene_index": 0})
        self.assertTrue(r_fs["result"]["fired"])

        # 9. get_track_devices & set_device_param
        r_gd = self._rpc_call("get_track_devices", {"track_index": 0})
        self.assertGreaterEqual(r_gd["result"]["device_count"], 0)

        # 10. exec_lom_query expressions & statements
        r_q1 = self._rpc_call("exec_lom_query", {"query": "len(tracks)"})
        self.assertGreaterEqual(r_q1["result"]["result"], 1)

        r_q2 = self._rpc_call("exec_lom_query", {"query": "out = [t.name for t in tracks]"})
        self.assertIn("out", r_q2["result"]["result"])

    # ------------------------------------------------------------------------
    # 2. Adversarial JSON & Wire Faults
    # ------------------------------------------------------------------------

    def test_malformed_json_and_syntax_errors(self):
        # Truncated JSON
        resp = self._send_line('{"jsonrpc": "2.0", "method":')
        data = json.loads(resp)
        self.assertIn("error", data)
        self.assertEqual(data["error"]["code"], -32700)

        # Empty line should return nothing and keep connection alive
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.settimeout(1.0)
        s.connect((DEFAULT_HOST, DEFAULT_PORT))
        s.sendall(b"   \n\n\t\n")
        # Send valid request immediately after
        s.sendall(b'{"jsonrpc":"2.0","id":99,"method":"ping","params":{}}\n')
        line = s.recv(4096).decode("utf-8")
        s.close()
        self.assertIn('"id": 99', line)
        self.assertIn('"status": "ok"', line)

    def test_unknown_method_error(self):
        r = self._rpc_call("non_existent_lom_method_xyz", {})
        self.assertIn("error", r)
        self.assertEqual(r["error"]["code"], -32603)
        self.assertIn("Unknown LOM method", r["error"]["message"])

    def test_out_of_bounds_parameters(self):
        # Track out of bounds
        r = self._rpc_call("create_clip", {"track_index": 99999, "slot_index": 0})
        self.assertIn("error", r)
        self.assertIn("out of range", r["error"]["message"])

        # Slot out of bounds
        r2 = self._rpc_call("create_clip", {"track_index": 0, "slot_index": 99999})
        self.assertIn("error", r2)
        self.assertIn("out of range", r2["error"]["message"])

        # Scene out of bounds
        r3 = self._rpc_call("fire_scene", {"scene_index": 99999})
        self.assertIn("error", r3)
        self.assertIn("out of range", r3["error"]["message"])

        # Device param on non-existent device
        r4 = self._rpc_call("set_device_param", {"track_index": 0, "device_index": 999, "param_index": 0, "value": 1.0})
        self.assertIn("error", r4)
        self.assertIn("out of range", r4["error"]["message"])

    def test_exec_lom_query_adversarial_queries(self):
        # Division by zero
        r1 = self._rpc_call("exec_lom_query", {"query": "1 / 0"})
        self.assertIn("error", r1)
        self.assertIn("division by zero", r1["error"]["message"])

        # Syntax error in both eval and exec
        r2 = self._rpc_call("exec_lom_query", {"query": "def !!!"})
        self.assertIn("error", r2)

        # Attribute error
        r3 = self._rpc_call("exec_lom_query", {"query": "song.non_existent_property_foo_bar"})
        self.assertIn("error", r3)
        self.assertIn("has no attribute", r3["error"]["message"])

        # Circular reference in exec statement
        r4 = self._rpc_call("exec_lom_query", {"query": "a = []; a.append(a)"})
        # Should either serialize safe representation or return handled recursion error without crashing
        self.assertTrue("result" in r4 or "error" in r4)

    # ------------------------------------------------------------------------
    # 3. Network Stress & High Concurrency
    # ------------------------------------------------------------------------

    def test_rapid_connect_disconnect_burst(self):
        # Open and immediately close 20 TCP sockets
        for _ in range(20):
            s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            s.connect((DEFAULT_HOST, DEFAULT_PORT))
            s.close()

        # Verify server is still operational
        r = self._rpc_call("ping", {})
        self.assertEqual(r["result"]["status"], "ok")

    def test_concurrent_clients(self):
        errors = []

        def worker(client_id):
            try:
                for i in range(5):
                    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                    s.settimeout(2.0)
                    s.connect((DEFAULT_HOST, DEFAULT_PORT))
                    req = json.dumps({
                        "jsonrpc": "2.0",
                        "id": client_id * 100 + i,
                        "method": "ping",
                        "params": {},
                    }) + "\n"
                    s.sendall(req.encode("utf-8"))
                    resp = s.recv(4096).decode("utf-8")
                    s.close()
                    d = json.loads(resp)
                    if d.get("result", {}).get("status") != "ok":
                        errors.append(f"Client {client_id} iteration {i} unexpected response: {resp}")
            except Exception as e:
                errors.append(f"Client {client_id} exception: {e}")

        threads = [threading.Thread(target=worker, args=(c,)) for c in range(8)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        self.assertEqual(errors, [])


if __name__ == "__main__":
    unittest.main()
