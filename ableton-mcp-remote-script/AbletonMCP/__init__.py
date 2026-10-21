# -*- coding: utf-8 -*-
"""
Ableton Live 12 Model Context Protocol (MCP) Remote Script.

Connects Ableton Live 12 Suite to the `ableton-mcp` dual-target server
via a high-speed JSON-RPC 2.0 TCP bridge on 127.0.0.1:9877.
"""

from __future__ import annotations
import json
import logging
import queue
import socket
import sys
import threading
import time
from typing import Any, Dict, Optional, Tuple

# Try importing Live's ControlSurface framework; provide fallback for standalone execution
try:
    from _Framework.ControlSurface import ControlSurface  # type: ignore
except ImportError:
    try:
        from ableton.v3.control_surface import ControlSurface  # type: ignore
    except ImportError:
        class ControlSurface:  # type: ignore
            """Fallback ControlSurface base class for standalone testing."""
            def __init__(self, c_instance=None):
                self._c_instance = c_instance

            def song(self):
                return getattr(self, "_mock_song", None)

            def application(self):
                return getattr(self, "_mock_app", None)

            def schedule_message(self, delay, callback):
                callback()

            def update(self):
                pass

            def disconnect(self):
                pass

from . import lom_bridge

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger("AbletonMCP")

DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = 9877


class AbletonMCP(ControlSurface):
    """Main Ableton Live 12 MIDI Remote Script Control Surface."""

    def __init__(self, c_instance=None):
        super().__init__(c_instance)
        self._c_instance = c_instance
        self._is_live_environment = c_instance is not None

        # Standalone mock objects when not in real Live
        if not self._is_live_environment:
            self._mock_song = lom_bridge.MockSong()
            self._mock_app = lom_bridge.MockApplication()

        # Thread synchronization
        self._running = True
        self._request_queue: queue.Queue = queue.Queue()
        self._server_socket: Optional[socket.socket] = None
        self._client_threads: list[threading.Thread] = []

        # Start TCP server daemon
        self._server_thread = threading.Thread(target=self._run_server, daemon=True, name="AbletonMCP-Server")
        self._server_thread.start()
        logger.info(f"AbletonMCP initialized (live_env={self._is_live_environment}) listening on {DEFAULT_HOST}:{DEFAULT_PORT}")

    def get_song(self):
        """Retrieve real Song or mock Song."""
        if hasattr(self, "song") and callable(self.song):
            s = self.song()
            if s is not None:
                return s
        return getattr(self, "_mock_song", None)

    def get_application(self):
        """Retrieve real Application or mock Application."""
        if hasattr(self, "application") and callable(self.application):
            app = self.application()
            if app is not None:
                return app
        return getattr(self, "_mock_app", None)

    def _run_server(self):
        """Main TCP socket listening loop."""
        try:
            self._server_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            self._server_socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            self._server_socket.bind((DEFAULT_HOST, DEFAULT_PORT))
            self._server_socket.listen(5)
            self._server_socket.settimeout(0.5)
            logger.info(f"AbletonMCP TCP server bound to {DEFAULT_HOST}:{DEFAULT_PORT}")
        except Exception as e:
            logger.error(f"Failed to bind AbletonMCP TCP socket on port {DEFAULT_PORT}: {e}")
            return

        while self._running:
            try:
                client_sock, client_addr = self._server_socket.accept()
                t = threading.Thread(
                    target=self._handle_client,
                    args=(client_sock, client_addr),
                    daemon=True,
                    name=f"AbletonMCP-Client-{client_addr[1]}",
                )
                t.start()
                self._client_threads.append(t)
            except socket.timeout:
                continue
            except Exception as e:
                if self._running:
                    logger.error(f"Accept error: {e}")
                break

    def _handle_client(self, client_sock: socket.socket, client_addr: Tuple[str, int]):
        """Handle line-delimited JSON-RPC 2.0 communication with a connected client."""
        client_sock.settimeout(1.0)
        buffer = ""
        while self._running:
            try:
                data = client_sock.recv(4096)
                if not data:
                    break
                buffer += data.decode("utf-8")
                while "\n" in buffer:
                    line, buffer = buffer.split("\n", 1)
                    line = line.strip()
                    if not line:
                        continue
                    response_str = self._process_request_line(line)
                    client_sock.sendall((response_str + "\n").encode("utf-8"))
            except socket.timeout:
                continue
            except (ConnectionResetError, BrokenPipeError):
                break
            except Exception as e:
                logger.error(f"Client read error from {client_addr}: {e}")
                break

        try:
            client_sock.close()
        except Exception:
            pass

    def _process_request_line(self, line: str) -> str:
        """Parse JSON-RPC 2.0 and dispatch with thread synchronization."""
        try:
            req = json.loads(line)
        except json.JSONDecodeError as e:
            return json.dumps({
                "jsonrpc": "2.0",
                "id": None,
                "error": {"code": -32700, "message": f"Parse error: {str(e)}"},
            })

        req_id = req.get("id")
        method = req.get("method", "")
        params = req.get("params", {})
        if not isinstance(params, dict):
            params = {}

        # If running inside real Live, execute on main UI thread using schedule_message
        if self._is_live_environment and hasattr(self, "schedule_message"):
            done_event = threading.Event()
            result_container: Dict[str, Any] = {}

            def run_on_main_thread():
                try:
                    song = self.get_song()
                    app = self.get_application()
                    res = lom_bridge.execute_lom_command(song, app, method, params)
                    result_container["result"] = res
                except Exception as ex:
                    result_container["error"] = str(ex)
                finally:
                    done_event.set()

            try:
                self.schedule_message(1, run_on_main_thread)
                # Wait up to 1.5s for main thread to dispatch
                if not done_event.wait(timeout=1.5):
                    return json.dumps({
                        "jsonrpc": "2.0",
                        "id": req_id,
                        "error": {"code": -32000, "message": "LOM main thread execution timed out"},
                    })
            except Exception as ex:
                return json.dumps({
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "error": {"code": -32603, "message": f"Dispatch failure: {str(ex)}"},
                })

            if "error" in result_container:
                return json.dumps({
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "error": {"code": -32603, "message": result_container["error"]},
                })
            return json.dumps({
                "jsonrpc": "2.0",
                "id": req_id,
                "result": result_container.get("result", {}),
            })
        else:
            # Standalone mock execution
            try:
                song = self.get_song()
                app = self.get_application()
                result = lom_bridge.execute_lom_command(song, app, method, params)
                return json.dumps({
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "result": result,
                })
            except Exception as e:
                return json.dumps({
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "error": {"code": -32603, "message": str(e)},
                })

    def disconnect(self):
        """Clean shutdown invoked by Ableton Live when switching remote scripts or exiting."""
        self._running = False
        if self._server_socket:
            try:
                self._server_socket.close()
            except Exception:
                pass
        super().disconnect()
        logger.info("AbletonMCP disconnected cleanly.")


def create_instance(c_instance):
    """Factory entrypoint invoked by Ableton Live 12."""
    return AbletonMCP(c_instance)


if __name__ == "__main__":
    print(f"Starting AbletonMCP Standalone Mock Server on {DEFAULT_HOST}:{DEFAULT_PORT}...")
    surface = AbletonMCP(c_instance=None)
    try:
        while True:
            time.sleep(1.0)
    except KeyboardInterrupt:
        print("\nStopping AbletonMCP server...")
        surface.disconnect()
