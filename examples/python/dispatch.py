#!/usr/bin/env python3
"""A reference supervisor: dispatch tasks to a node's executors over HTTP.

This is the smallest complete "supervisor" RiscDom ships: a process that is **not** inside
the kernel, holds no model of its own, and drives a node through the control plane. It is
what an AI supervisor would wrap around a model: the tool calls are the control plane's
endpoints (`docs/tool-schema-control-plane.md`), and the HTTP requests below are what a tool
call turns into.

It demonstrates the whole loop:

    GET  /v0/executors   who can be asked
    POST /v0/tasks       ask one of them (synchronously, one task at a time)
    GET  /v0/events      the node's event stream, while the work happens

Standard library only. `urllib.request` is the HTTP client and the event stream is parsed by
hand (read a line, `data:` is the payload, a blank line ends a frame) -- a reference
implementation should not teach a dependency it does not need.

    python examples/python/dispatch.py --server 127.0.0.1:7821 --tasks tasks.jsonl
    python examples/python/dispatch.py --target executor-0 "build the blink example"
    python examples/python/dispatch.py --self-test        # offline, no server, no network

Exit codes, mirroring the CLI's convention: 0 every task answered with a success, 1 at least
one task did not (refused, broken, or a run that failed), 2 a usage error, 3 the control
plane could not be reached or refused the credential.

The Rust sibling is `worker/examples/dispatch.rs`, which drives executor *processes* directly
(stdin/stdout, no HTTP). This one is deliberately the other half of the picture: it treats
the node as a service.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import threading
import urllib.error
import urllib.request

EXIT_OK = 0
EXIT_TASK_FAILED = 1
EXIT_USAGE = 2
EXIT_TRANSPORT = 3

DEFAULT_SERVER = "127.0.0.1:7821"


class TransportError(Exception):
    """The control plane could not be reached, or refused the credential."""

    def __init__(self, message: str, code: int = EXIT_TRANSPORT) -> None:
        super().__init__(message)
        self.code = code


class ApiError(Exception):
    """The control plane answered an error object (the API document's section 4)."""

    def __init__(self, status: int, payload: dict) -> None:
        super().__init__(f"{status} {payload.get('code', 'error')}: {payload.get('message', '')}")
        self.status = status
        self.payload = payload

    @property
    def cause(self) -> str | None:
        cause = self.payload.get("cause")
        return cause if isinstance(cause, str) else None


class ControlPlane:
    """One node's control plane, as an HTTP client.

    The token is never a command-line argument: it comes from a file or from the
    environment, because an argument lands in the shell history and in the process list.
    """

    def __init__(self, server: str, token: str, timeout: float = 60.0) -> None:
        if "://" not in server:
            server = f"http://{server}"
        self.base = server.rstrip("/")
        self.token = token
        self.timeout = timeout

    def _request(self, method: str, path: str, body: bytes | None = None) -> bytes:
        request = urllib.request.Request(f"{self.base}{path}", data=body, method=method)
        request.add_header("Authorization", f"Bearer {self.token}")
        if body is not None:
            request.add_header("Content-Type", "application/json")
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as answer:
                return answer.read()
        except urllib.error.HTTPError as error:
            raw = error.read()
            try:
                payload = json.loads(raw)
            except ValueError:
                payload = {"code": "error", "message": raw.decode("utf-8", "replace")}
            if error.code in (401, 403):
                # A credential problem is not a task problem: the caller has to fix who it
                # is, not what it asked for.
                raise TransportError(
                    f"the control plane refused this token: {payload.get('message', error.code)}"
                ) from None
            raise ApiError(error.code, payload) from None
        except urllib.error.URLError as error:
            raise TransportError(f"cannot reach {self.base}: {error.reason}") from None

    def executors(self) -> list[str]:
        """The executors a task can be routed to. An empty list is a fact, not an error."""
        payload = json.loads(self._request("GET", "/v0/executors"))
        return [row["agent_id"] for row in payload.get("executors", [])]

    def dispatch(self, target: str, text: str, sandbox: str | None = None) -> dict:
        """Dispatch one task and return its `TaskOutcome` (synchronously, like a run)."""
        body: dict = {"target": target, "input": text}
        if sandbox:
            body["sandbox"] = sandbox
        raw = self._request("POST", "/v0/tasks", json.dumps(body).encode("utf-8"))
        return json.loads(raw)

    def events(self, on_frame, stop: threading.Event) -> None:
        """Subscribe to `/v0/events` until `stop` is set, handing each frame to `on_frame`.

        Frames are the API document's: `id: <ts>-<seq>`, `data: <json>`, blank line. Only
        `data:` is parsed -- `id:` exists so a client can resume with `Last-Event-ID`, and
        this example does not resume.
        """
        request = urllib.request.Request(f"{self.base}/v0/events")
        request.add_header("Authorization", f"Bearer {self.token}")
        request.add_header("Accept", "text/event-stream")
        try:
            with urllib.request.urlopen(request, timeout=None) as stream:
                for line in stream:
                    if stop.is_set():
                        return
                    text = line.decode("utf-8", "replace").rstrip("\r\n")
                    if not text.startswith("data:"):
                        continue
                    try:
                        on_frame(json.loads(text[len("data:") :].strip()))
                    except ValueError:
                        continue
        except (urllib.error.URLError, OSError):
            # The stream is context, not the answer: losing it must not fail the dispatch.
            return


def task_lines(text: str) -> list[dict]:
    """One task per line: `{"target": ..., "input": ...}` (JSON lines, like the worker's)."""
    tasks = []
    for number, line in enumerate(text.splitlines(), start=1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        try:
            task = json.loads(line)
        except ValueError as error:
            raise ValueError(f"line {number} is not JSON: {error}") from None
        if not isinstance(task, dict) or not all(
            isinstance(task.get(field), str) for field in ("target", "input")
        ):
            raise ValueError(f"line {number} needs a string `target` and a string `input`")
        tasks.append(task)
    return tasks


def outcome_line(name: str, outcome: dict) -> str:
    """One line per task, in the worker supervisor's shape."""
    inner = outcome.get("outcome") or {}
    kind = next(iter(inner), "none")
    detail = inner.get(kind) or {}
    if kind == "Final":
        return f"{name:<16} ok        final ({detail.get('iterations', '?')} iteration(s))"
    if kind == "MaxIterations":
        return f"{name:<16} ok        max_iterations ({detail.get('iterations', '?')})"
    if kind == "Failed":
        return f"{name:<16} failed    {detail.get('reason', 'no reason')}"
    return f"{name:<16} ok        {kind}"


def report(outcomes: list[tuple[str, dict | None, str | None]]) -> int:
    """Print the tally and return the exit code.

    Three fates, kept apart on purpose: an outcome the executor produced (`ok` / `failed`),
    and a dispatch that never produced one (`refused` -- nobody owns that target -- or
    `broken`, the API document's `500 cause "task"`).
    """
    print(f"\n{'task':<16} {'result':<10} detail")
    ok = failed = refused = broken = 0
    for name, outcome, problem in outcomes:
        if outcome is not None:
            print(outcome_line(name, outcome))
            inner = outcome.get("outcome") or {}
            if next(iter(inner), "") == "Failed":
                failed += 1
            else:
                ok += 1
        elif problem is not None and problem.startswith("404"):
            print(f"{name:<16} refused   {problem}")
            refused += 1
        else:
            print(f"{name:<16} broken    {problem}")
            broken += 1
    print(f"\n{ok} ok, {failed} failed, {refused} refused, {broken} broken")
    return EXIT_OK if failed == 0 and refused == 0 and broken == 0 else EXIT_TASK_FAILED


def dispatch_all(
    plane: ControlPlane, tasks: list[dict], sandbox: str | None, follow: bool
) -> list[tuple[str, dict | None, str | None]]:
    """Send every task, one at a time, and collect what came back."""
    stop = threading.Event()
    reader = None
    if follow:
        reader = threading.Thread(
            target=plane.events,
            args=(
                lambda frame: print(
                    f"  event  {frame.get('event') or frame.get('kind')}  {json.dumps(frame.get('payload', {}))}",
                    file=sys.stderr,
                ),
                stop,
            ),
            daemon=True,
        )
        reader.start()

    results: list[tuple[str, dict | None, str | None]] = []
    for task in tasks:
        name = task["target"]
        try:
            outcome = plane.dispatch(name, task["input"], task.get("sandbox") or sandbox)
            results.append((name, outcome, None))
        except ApiError as error:
            results.append((name, None, f"{error.status} {error.cause or error.payload.get('code')}"))

    stop.set()
    if reader is not None:
        reader.join(timeout=1.0)
    return results


# --------------------------------------------------------------------------------------
# The self-test: a fake control plane on loopback, so the script proves itself offline.
# --------------------------------------------------------------------------------------


def self_test() -> int:
    """Run the whole dispatch path against an in-process fake control plane.

    No server, no worker, no network beyond loopback: the fake answers the three endpoints
    this example uses, and the assertions are about *this* script (its parsing, its routing,
    its error mapping, its exit code), not about the control plane, which its own tests own.
    """
    from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

    token = "self-test-token"
    served: list[str] = []

    class Fake(BaseHTTPRequestHandler):
        def log_message(self, *args):  # keep the test output clean
            pass

        def _send(self, status, payload, content_type="application/json"):
            body = json.dumps(payload).encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_GET(self):  # noqa: N802 (http.server's naming)
            served.append(f"GET {self.path}")
            if self.headers.get("Authorization") != f"Bearer {token}":
                self._send(401, {"code": "unauthorized", "message": "no token"})
                return
            if self.path == "/v0/executors":
                self._send(
                    200, {"executors": [{"agent_id": "executor-0"}, {"agent_id": "executor-1"}]}
                )
                return
            if self.path == "/v0/events":
                body = (
                    'id: 1-0\ndata: {"version":1,"kind":"hello","event":null}\n\n'
                    'id: 1-1\ndata: {"version":1,"kind":"event","event":"agent:final","payload":{}}\n\n'
                ).encode("utf-8")
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
                return
            self._send(404, {"code": "not_found", "message": f"no endpoint {self.path}"})

        def do_POST(self):  # noqa: N802
            served.append(f"POST {self.path}")
            length = int(self.headers.get("Content-Length") or 0)
            body = json.loads(self.rfile.read(length) or b"{}")
            if self.headers.get("Authorization") != f"Bearer {token}":
                self._send(401, {"code": "unauthorized", "message": "no token"})
                return
            if self.path != "/v0/tasks":
                self._send(404, {"code": "not_found", "message": f"no endpoint {self.path}"})
                return
            target = body.get("target")
            if target not in ("executor-0", "executor-1"):
                self._send(
                    404,
                    {
                        "code": "not_found",
                        "message": f"no executor for agent {target}",
                        "cause": "target",
                    },
                )
                return
            if target == "executor-1":
                inner = {"Failed": {"reason": "no model is configured", "iterations": 0}}
            else:
                inner = {"Final": {"content": "done", "iterations": 2}}
            self._send(
                200,
                {"task_id": "task-self-1", "agent_id": f"{target}-child-1", "outcome": inner},
            )

    server = ThreadingHTTPServer(("127.0.0.1", 0), Fake)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    address = f"127.0.0.1:{server.server_address[1]}"

    failures: list[str] = []
    try:
        # The credential comes from the environment in the real run; here it is explicit.
        plane = ControlPlane(address, token, timeout=5.0)

        found = plane.executors()
        if found != ["executor-0", "executor-1"]:
            failures.append(f"executors: expected two, got {found}")

        tasks = [{"target": "executor-0", "input": "say hi"}, {"target": "executor-1", "input": "x"}]
        results = dispatch_all(plane, tasks, None, follow=True)
        code = report(results)
        if code != EXIT_TASK_FAILED:
            failures.append(f"a failed run must exit {EXIT_TASK_FAILED}, got {code}")
        if results[0][1]["outcome"] != {"Final": {"content": "done", "iterations": 2}}:
            failures.append(f"the first outcome is wrong: {results[0][1]}")
        if "GET /v0/events" not in served:
            failures.append("--follow did not subscribe to the event stream")

        # A target nobody owns is refused, and the cause names the parameter.
        refused = dispatch_all(plane, [{"target": "nobody", "input": "x"}], None, follow=False)
        if refused[0][2] != "404 target":
            failures.append(f"an unknown target must be refused as `404 target`: {refused[0][2]}")

        # A wrong token is a credential problem, not a task problem.
        try:
            ControlPlane(address, "wrong", timeout=5.0).executors()
            failures.append("a wrong token must raise")
        except TransportError:
            pass

        # A malformed task list is a usage error, not a crash.
        try:
            task_lines('{"target": "a"}')
            failures.append("a task without an input must raise")
        except ValueError:
            pass

        # A leading BOM (a Windows editor's doing) must not make a task file unusable.
        import tempfile

        with tempfile.NamedTemporaryFile("w", suffix=".jsonl", delete=False, encoding="utf-8-sig") as handle:
            handle.write('{"target": "executor-0", "input": "x"}\n')
            bomby = handle.name
        try:
            with open(bomby, "r", encoding="utf-8-sig") as handle:
                if len(task_lines(handle.read())) != 1:
                    failures.append("a BOM-prefixed task file must still parse")
        finally:
            os.unlink(bomby)
    finally:
        server.shutdown()
        server.server_close()

    if failures:
        print("dispatch self-test: FAILED", file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        return 1
    print("dispatch self-test: OK (fake control plane, no network beyond loopback)")
    return 0


# --------------------------------------------------------------------------------------


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="dispatch.py",
        description="Dispatch tasks to a node's executors over the control plane.",
        epilog="The token comes from --token-file or $RISCDOM_TOKEN; never from an argument.",
    )
    parser.add_argument("tasks", nargs="*", help="task text, one task per argument")
    parser.add_argument("--server", default=DEFAULT_SERVER, help=f"host:port (default {DEFAULT_SERVER})")
    parser.add_argument("--token-file", help="read the bearer token from this file")
    parser.add_argument("--tasks", dest="tasks_file", help="a JSON-lines task file; '-' reads stdin")
    parser.add_argument("--target", help="send every task to this executor (default: round-robin)")
    parser.add_argument("--sandbox", help="declare this sandbox for every task")
    parser.add_argument("--follow", action="store_true", help="print the event stream while dispatching")
    parser.add_argument("--timeout", type=float, default=60.0, help="per-request timeout in seconds")
    parser.add_argument("--json", action="store_true", help="print the raw outcomes instead of a table")
    parser.add_argument("--self-test", action="store_true", help="prove this script offline")
    return parser.parse_args(argv)


def read_token(path: str | None) -> str:
    """`--token-file`, then `$RISCDOM_TOKEN` -- never the command line."""
    if path:
        try:
            with open(path, "r", encoding="utf-8") as handle:
                token = handle.read().strip()
        except OSError as error:
            raise TransportError(f"cannot read {path}: {error}", EXIT_USAGE) from None
        if not token:
            raise TransportError(f"{path} is empty", EXIT_USAGE)
        return token
    token = os.environ.get("RISCDOM_TOKEN", "").strip()
    if not token:
        raise TransportError(
            "no token: pass --token-file <path> or set RISCDOM_TOKEN", EXIT_USAGE
        )
    return token


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    if args.self_test:
        return self_test()

    try:
        tasks: list[dict] = []
        for text in args.tasks:
            tasks.append({"target": args.target or "", "input": text})
        if args.tasks_file:
            if args.tasks_file == "-":
                tasks.extend(task_lines(sys.stdin.read()))
            else:
                # `utf-8-sig`: a Windows editor may write a BOM, and a task file is
                # data, not a protocol -- tolerating it costs nothing.
                with open(args.tasks_file, "r", encoding="utf-8-sig") as handle:
                    tasks.extend(task_lines(handle.read()))
        if not tasks:
            print("dispatch: no tasks (pass task text, or --tasks <file>)", file=sys.stderr)
            return EXIT_USAGE

        # The credential is read after the usage check: "you asked for nothing" is a
        # usage error whatever the token situation is.
        token = read_token(args.token_file)

        plane = ControlPlane(args.server, token, args.timeout)
        if any(not task["target"] for task in tasks):
            # The target is the routing key: there is no "any executor" here, on purpose.
            found = plane.executors()
            if not found:
                print(
                    "dispatch: this node has no executors configured "
                    "(add `executors` to its settings.json, or use /v0/agent/run)",
                    file=sys.stderr,
                )
                return EXIT_TASK_FAILED
            for index, task in enumerate(tasks):
                if not task["target"]:
                    task["target"] = args.target or found[index % len(found)]

        print("supervisor: a client, not an agent (the node does the work)")
        print(f"server    : {args.server}")
        print(f"tasks     : {len(tasks)}")
        results = dispatch_all(plane, tasks, args.sandbox, args.follow)
        if args.json:
            for name, outcome, problem in results:
                print(
                    json.dumps(
                        outcome if outcome is not None else {"target": name, "error": problem}
                    )
                )
            return _json_exit(results)
        return report(results)
    except TransportError as error:
        print(f"dispatch: {error}", file=sys.stderr)
        return error.code
    except ValueError as error:
        print(f"dispatch: {error}", file=sys.stderr)
        return EXIT_USAGE


def _json_exit(results: list[tuple[str, dict | None, str | None]]) -> int:
    """The exit code for `--json`: the report still decides it, but is not printed twice."""
    failed = any(outcome is None for _, outcome, _ in results)
    ran_and_failed = any(
        next(iter((outcome.get("outcome") or {})), "") == "Failed"
        for _, outcome, _ in results
        if outcome is not None
    )
    return EXIT_TASK_FAILED if failed or ran_and_failed else EXIT_OK


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
