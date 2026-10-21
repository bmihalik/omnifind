#!/usr/bin/env python3
"""Exercise the real MCP example subprocess; no third-party Python packages or public network.
Usage: python3 examples/mcp_smoke.py /absolute/path/to/target/debug/examples/mcp
Concept: Béla István MIHALIK. Code generation: Perplexity AI.
"""
import argparse
import json
import os
import queue
import subprocess
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


class Mcp:
    def __init__(self, binary, extract=False):
        env = dict(os.environ, OMNIFIND_MCP_ALLOW_EXTRACT="1" if extract else "0",
                   OMNIFIND_MCP_TIMEOUT_SECONDS="5")
        self.stderr = tempfile.TemporaryFile(mode="w+t")
        self.proc = subprocess.Popen([binary], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=self.stderr, text=True, encoding="utf-8", env=env)
        self.messages = queue.Queue()
        self.next_id = 0
        self.request_meta = None
        self.thread = threading.Thread(target=self._read, daemon=True)
        self.thread.start()

    def _read(self):
        for line in self.proc.stdout:
            try:
                self.messages.put(json.loads(line))
            except ValueError as exc:
                self.messages.put(exc)
        self.messages.put(EOFError("server stdout closed"))

    def notify(self, method, params=None):
        value = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            value["params"] = params
        self.proc.stdin.write(json.dumps(value) + "\n")
        self.proc.stdin.flush()

    def call(self, method, params=None):
        self.next_id += 1
        value = {"jsonrpc": "2.0", "id": self.next_id, "method": method}
        if self.request_meta is not None:
            params = dict(params or {}, _meta=self.request_meta)
        if params is not None:
            value["params"] = params
        self.proc.stdin.write(json.dumps(value) + "\n")
        self.proc.stdin.flush()
        while True:
            message = self.messages.get(timeout=15)
            if isinstance(message, Exception):
                raise message
            assert message.get("jsonrpc") == "2.0", message
            if message.get("id") == self.next_id:
                return message
            assert "id" not in message, message

    def initialize(self, version):
        result = self.call("initialize", {
            "protocolVersion": version, "capabilities": {},
            "clientInfo": {"name": "omnifind-local-smoke", "version": "1.0"}
        })["result"]
        assert result["protocolVersion"] == version, result
        assert "tools" in result["capabilities"]
        self.notify("notifications/initialized")
        return result

    def close(self):
        if self.proc.stdin and not self.proc.stdin.closed:
            self.proc.stdin.close()
        try:
            code = self.proc.wait(timeout=10)
            assert code == 0, f"server exit {code}"
        finally:
            if self.proc.poll() is None:
                self.proc.kill()
                self.proc.wait(timeout=5)
            self.thread.join(timeout=2)
            self.proc.stdout.close()
            self.stderr.close()


def tool(client, name, arguments):
    return client.call("tools/call", {"name": name, "arguments": arguments})


def data(message):
    result = message["result"]
    assert json.loads(result["content"][0]["text"]) == result["structuredContent"]
    return result["structuredContent"]


class Fixture(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/missing":
            self.send_response(404)
            self.end_headers()
            return
        links = "".join(f'<a href="/link/{i}">Link</a>' for i in range(55))
        html = f'<html><title>MCP fixture</title><body><article>Árvíztűrő test {links}</article></body></html>'
        body = html.encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_args):
        pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary")
    binary = os.path.abspath(parser.parse_args().binary)
    records = []
    for version in ["2025-06-18", "2025-11-25"]:
        client = Mcp(binary)
        try:
            client.initialize(version)
            records.append(f"PASS: stdio initialize/initialized ({version})")
            assert client.call("ping")["result"] == {}
            tools = client.call("tools/list")["result"]["tools"]
            assert len(tools) == 4
            assert all(t["annotations"]["readOnlyHint"] for t in tools)
            search = next(t for t in tools if t["name"] == "omnifind_search")
            assert search["inputSchema"]["properties"]["limit"]["maximum"] == 20
            records.append(f"PASS: ping, four default tools, bounds and annotations ({version})")
            denied = tool(client, "omnifind_extract", {"url": "https://example.com"})
            assert "error" in denied or denied["result"].get("isError") is True
            invalid = tool(client, "omnifind_search", {"query": " ", "limit": 1})
            assert invalid["result"]["isError"] is True
            assert data(invalid)["error"]["code"] == "invalid_input"
            for arguments in [{"query":"test","limit":21}, {"query":"test","limit":0}]:
                assert tool(client, "omnifind_books", arguments)["result"]["isError"] is True
            for name, args in [
                ("omnifind_search", {"query": "test", "engine": "unknown"}),
                ("omnifind_search", {"query": "test", "unknown": 1}),
                ("omnifind_books", {}),
                ("unknown_tool", {})
            ]:
                reply = tool(client, name, args)
                assert "error" in reply or reply["result"].get("isError") is True
            assert client.call("ping")["result"] == {}
            records.append(f"PASS: disabled tool, argument and unknown-tool errors; server stays alive ({version})")
        finally:
            client.close()
        records.append(f"PASS: clean EOF shutdown ({version})")

    client = Mcp(binary)
    try:
        client.request_meta = {
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientCapabilities": {},
            "io.modelcontextprotocol/clientInfo": {"name": "omnifind-local-smoke", "version": "1.0"}
        }
        discovery = client.call("server/discover", {})["result"]
        assert "2026-07-28" in discovery["supportedVersions"]
        assert len(client.call("tools/list", {})["result"]["tools"]) == 4
        reply = tool(client, "omnifind_search", {"query": " "})
        assert reply["result"]["isError"] is True
        assert data(reply)["error"]["code"] == "invalid_input"
        records.append("PASS: 2026-07-28 discovery, per-request metadata, tool listing and invocation")
    finally:
        client.close()

    http = ThreadingHTTPServer(("127.0.0.1", 0), Fixture)
    thread = threading.Thread(target=http.serve_forever, daemon=True)
    thread.start()
    url = f"http://127.0.0.1:{http.server_address[1]}"
    client = Mcp(binary, extract=True)
    try:
        client.initialize("2025-11-25")
        assert len(client.call("tools/list")["result"]["tools"]) == 5
        reply = tool(client, "omnifind_extract", {"url": url, "max_text_chars": 4})
        assert reply["result"]["isError"] is False
        result = data(reply)
        assert result["page"]["title"] == "MCP fixture"
        assert result["page"]["text"] == "Árví"
        assert len(result["page"]["links"]) == 50
        assert result["text_truncated"] and result["links_truncated"]
        records.append("PASS: opt-in extraction over actual stdio + local HTTP; Unicode and link truncation")
        reply = tool(client, "omnifind_extract", {"url": url + "/missing"})
        assert reply["result"]["isError"] is True
        assert data(reply)["error"]["status"] == 404
        records.append("PASS: real HTTP 404 becomes a tool error, not empty success")
        for value in ["file:///etc/passwd", "https://user:secret@example.com"]:
            reply = tool(client, "omnifind_extract", {"url": value})
            assert reply["result"]["isError"] is True
            assert data(reply)["error"]["code"] == "invalid_input"
        records.append("PASS: non-HTTP and credential-bearing extraction URLs rejected")
    finally:
        client.close()
        http.shutdown()
        http.server_close()
        thread.join(timeout=2)
    records.append("PASS: opt-in server clean EOF shutdown; stdout is JSON-RPC only")
    print("\n".join(records))


if __name__ == "__main__":
    main()
