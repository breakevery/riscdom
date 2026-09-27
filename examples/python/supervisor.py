#!/usr/bin/env python3
"""M — a reference **dispatcher**: the skeleton an AI supervisor's model sits on.

`dispatch.py` (v0.9 interface E3) is a batch client: it is handed a task list and sends
it. This is the other shape — a process that **stays up**, reads the node's state, and
decides *whether* to act. In the roadmap's words (docs/roadmap-v1.0.md §9), M is an AI
dispatcher: it decides what runs where and stops there. It is not in the data path, does
not proxy a task's I/O, and is not in the kernel.

**This batch is the skeleton, not the brain.** The decision layer is `decide()` below and
it is a **stub**: it returns `None`, which means "do nothing". That is deliberate — the
conservative state is the loop's *starting point*, not an error path — and the LLM loop
that fills it in is the next batch (M2c-2). What is here is everything around it:

    read the state (one snapshot)  ->  decide  ->  act  ->  report

`import dispatch` is the point, not a shortcut: the credential reading, the HTTP client
and the error taxonomy (a refused credential is not a failed request) are already the
reference supervisor's, and a second copy of the wire format would be a second thing to
keep true. Python puts this script's directory on `sys.path`, so `python
examples/python/supervisor.py` finds it.

**Four things this is not.** It is not an executor (the tools inside one are a different,
much smaller set, and not callable over HTTP); it does not offer `GET /v0/events` to a
model as a tool (the stream stays open, so it is context, not a tool call); it does not
hold the truth about what is running (that lives on the node); and it does not treat its
own memory as durable.

**Known boundaries, stated rather than hidden** (reconnaissance of the M2c batch):

- **M has no identity of its own in the audit chain.** A token client acts as `operator`,
  so M's rows and a person's look the same; the two `m.sandbox.*` rows the node writes
  carry `actor: "host"`.
- **A decision on a sandbox request is not written to the chain.** `approve` / `reject`
  announce a `sandbox:request` event and record nothing durable.
- **The instance table and the pending-approval slot are in memory.** A node restart
  loses both; the **audit chain is the only durable source**.
- **The audit read has no window and no pagination** — `GET /v0/audit/events` takes
  `limit` (required), `actor` and `action_prefix`, so "everything since X" is an export,
  not a query.
- **Five capability names are vocabulary only** (`task.dispatch`, `task.dispatch.remote`,
  `sandbox.instantiate.remote`, `audit.read.remote`, `request.approve`): no route requires
  them. Dispatching a task needs `agent.run`; deriving an instance needs
  `sandbox.instantiate`; deciding a request needs `sandbox.read` **and** the capability the
  request's own action implies.

    python examples/python/supervisor.py --once                     # one conservative turn
    python examples/python/supervisor.py --interval 30              # stay up
    python examples/python/supervisor.py --events                   # just the event stream
    python examples/python/supervisor.py --self-test                # offline, no server

Exit codes, the same table `dispatch.py` uses: 0 the turn completed (acted, or had nothing
to do), 1 the turn did not complete (the state could not be read — M took no action), 2 a
usage error, 3 the control plane could not be reached or refused the credential.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from typing import NamedTuple

# The sibling module lives beside this file, and a script's directory is **not** always on
# `sys.path`: `python -P` / `PYTHONSAFEPATH=1` leaves it off, and so does `python -m`.
# (The interpreter this project bundles runs with the safe path on, which is how the
# reconnaissance found it.) Two lines make the reuse true however the file is started.
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

# The transport, the credential and the errors are the reference supervisor's already.
import dispatch
from dispatch import ApiError, ControlPlane, TransportError

# `dispatch.py`'s table, reused rather than re-invented: a reference implementation that
# invents a second convention teaches the wrong thing twice.
EXIT_OK = dispatch.EXIT_OK
EXIT_TURN_FAILED = dispatch.EXIT_TASK_FAILED
EXIT_USAGE = dispatch.EXIT_USAGE
EXIT_TRANSPORT = dispatch.EXIT_TRANSPORT

DEFAULT_SERVER = dispatch.DEFAULT_SERVER
DEFAULT_INTERVAL_SECONDS = 30.0
RECONNECT_SECONDS = 2.0


class Action(NamedTuple):
    """One thing M decided to do: a tool name and its arguments.

    The names are the control-plane tool schema's (`docs/tool-schema-control-plane.md`),
    which is the list M's model is offered. Keeping the decision in this shape is what
    makes `decide()` swappable: M2c-2 replaces that function, not this file's plumbing.
    """

    tool: str
    arguments: dict


class Supervisor(ControlPlane):
    """The client a dispatcher needs: every read it decides from, and every write it may.

    `dispatch.py`'s `ControlPlane` carries the three things a batch client uses
    (`executors` / `dispatch` / `events`). M reads a dozen more and writes a handful, so
    they are added **here, in a subclass** — `dispatch.py` is imported, never edited, and
    stays the one description of the wire format.
    """

    # ----- reads ---------------------------------------------------------------

    def get(self, path: str) -> dict | list:
        """One `GET`, parsed. Raises `TransportError` / `ApiError`, like the parent's."""
        return json.loads(self._request("GET", path))

    def status(self) -> dict:
        return self.get("/v0/status")

    def capabilities(self) -> list[str]:
        return list(self.get("/v0/capabilities").get("capabilities", []))

    def sandboxes(self) -> dict:
        """`{sandboxes: [...], current, default}` — the registry, not the live state."""
        return self.get("/v0/sandboxes")

    def instances(self, definition: str) -> list[dict]:
        """The instances **of one definition**; a name nobody has is an `ApiError` 404."""
        quoted = urllib.parse.quote(definition, safe="")
        return list(self.get(f"/v0/sandboxes/{quoted}/instances").get("instances", []))

    def pending_requests(self) -> list[dict]:
        """The approval slot, filtered to what is still waiting on a decision."""
        return list(
            self.get("/v0/sandboxes/requests?status=pending").get("requests", [])
        )

    def runs(self, limit: int = 20) -> list[dict]:
        return list(self.get(f"/v0/runs?limit={int(limit)}"))

    def audit_status(self) -> dict:
        return self.get("/v0/audit/status")

    def audit_events(
        self, limit: int = 50, actor: str | None = None, action_prefix: str | None = None
    ) -> list[dict]:
        """The chain's recent rows.

        Note the boundary: `limit` is the whole of the windowing this endpoint has. A
        supervisor rebuilding context from scratch reads the export, not a page.
        """
        query = {"limit": int(limit), "actor": actor, "action_prefix": action_prefix}
        pairs = urllib.parse.urlencode(
            {key: value for key, value in query.items() if value is not None}
        )
        return list(self.get(f"/v0/audit/events?{pairs}"))

    def vm_status(self) -> dict:
        return self.get("/v0/vm/status")

    def snapshot(self) -> dict:
        """Everything a turn decides from, read in one place and failing as one.

        The aggregation is the conservative loop's whole safety property: if any read
        raises, the caller has nothing to act on, so it acts on nothing. The thin
        wrappers above stay public so a decision layer — or a test — can ask one question
        without paying for the rest.
        """
        registry = self.sandboxes()
        names = [row.get("name", "") for row in registry.get("sandboxes", [])]
        instances: dict[str, list[dict]] = {}
        for name in names:
            if name:
                instances[name] = self.instances(name)
        return {
            "status": self.status(),
            "capabilities": self.capabilities(),
            "executors": self.executors(),
            "sandboxes": registry,
            "instances": instances,
            "pending_requests": self.pending_requests(),
        }

    # ----- controls ------------------------------------------------------------

    def dispatch_task(
        self,
        target: str,
        text: str,
        sandbox: str | None = None,
        instance: str | None = None,
    ) -> dict:
        """`POST /v0/tasks` — the whole `Task`, including the instance (v1.0 M2a-3).

        The parent's `dispatch` predates `instance` and deliberately stays as it is;
        this is the same request with the finer declaration added.
        """
        body: dict = {"target": target, "input": text}
        if sandbox:
            body["sandbox"] = sandbox
        if instance:
            body["instance"] = instance
        return json.loads(self._request("POST", "/v0/tasks", json.dumps(body).encode("utf-8")))

    def spawn_instance(self, definition: str) -> dict:
        """`POST /v0/sandboxes/{name}/instances` — derive one, changing nothing else."""
        quoted = urllib.parse.quote(definition, safe="")
        return json.loads(
            self._request("POST", f"/v0/sandboxes/{quoted}/instances", b"{}")
        )

    def reap_instance(self, definition: str, instance_id: str) -> None:
        """`DELETE /v0/sandboxes/{name}/instances/{id}` — both halves must agree."""
        quoted = urllib.parse.quote(definition, safe="")
        key = urllib.parse.quote(instance_id, safe="")
        self._request("DELETE", f"/v0/sandboxes/{quoted}/instances/{key}")

    def switch_sandbox(self, name: str) -> dict:
        """`POST /v0/sandboxes/switch` — the takeover, which is a different act."""
        return json.loads(
            self._request(
                "POST", "/v0/sandboxes/switch", json.dumps({"name": name}).encode("utf-8")
            )
        )

    def ask_sandbox(self, action: str, sandbox: str | None = None, reason: str | None = None) -> str:
        """`POST /v0/sandboxes/requests` — ask, never decide (that is another actor's)."""
        body: dict = {"action": action}
        if sandbox:
            body["sandbox"] = sandbox
        if reason:
            body["reason"] = reason
        answer = json.loads(
            self._request(
                "POST", "/v0/sandboxes/requests", json.dumps(body).encode("utf-8")
            )
        )
        return answer.get("id", "")

    def decide_request(self, request_id: str, approve: bool) -> dict:
        """`POST .../{id}/approve|reject` — the decision, and its record back."""
        key = urllib.parse.quote(request_id, safe="")
        verdict = "approve" if approve else "reject"
        return json.loads(
            self._request("POST", f"/v0/sandboxes/requests/{key}/{verdict}", b"{}")
        )

    # ----- the event stream ----------------------------------------------------

    def follow(
        self,
        on_frame,
        stop: threading.Event,
        last_event_id: str | None = None,
    ) -> str | None:
        """Subscribe to `/v0/events`, **remembering the last `id:`**, and return it.

        The inherited `ControlPlane.events` deliberately drops `id:` (it does not resume).
        A supervisor that must not silently miss a frame is the case that does, so this is
        the one place the reader is written twice — twenty lines, and the divergence is
        stated here rather than discovered later. The returned id is what a reconnect
        sends as `Last-Event-ID`, which is how the server replays from there instead of
        answering with a `gap` frame.
        """
        request = urllib.request.Request(f"{self.base}/v0/events")
        request.add_header("Authorization", f"Bearer {self.token}")
        request.add_header("Accept", "text/event-stream")
        if last_event_id:
            request.add_header("Last-Event-ID", last_event_id)
        latest = last_event_id
        try:
            with urllib.request.urlopen(request, timeout=None) as stream:
                for line in stream:
                    if stop.is_set():
                        return latest
                    text = line.decode("utf-8", "replace").rstrip("\r\n")
                    if text.startswith("id:"):
                        latest = text[len("id:") :].strip()
                        continue
                    if not text.startswith("data:"):
                        continue
                    try:
                        on_frame(json.loads(text[len("data:") :].strip()))
                    except ValueError:
                        continue
        except (urllib.error.URLError, OSError):
            # Context, not the answer: losing the stream must not stop the loop.
            return latest
        return latest


# --------------------------------------------------------------------------------------
# The decision layer — a stub, on purpose
# --------------------------------------------------------------------------------------


def decide(state: dict) -> Action | None:
    """What to do about `state` — **the stub this batch ships**.

    It returns `None`, and `None` means *do nothing*. That is the start of the design and
    not a placeholder for an error: a dispatcher that cannot see the whole picture
    dispatches nothing, so the conservative answer is also the default one, and turning
    the loop on cannot by itself change a node.

    The next batch (M2c-2) replaces this function's body with a model call over the tool
    schema — the tool list is `docs/tool-schema-control-plane.md`, the state is `state`,
    and the answer is an `Action`. Note what that shape already fixes: a decision is a
    **request** (`Action`), never a side effect, and `state` is everything the model may
    see. `state["capabilities"]` is the caller's own vocabulary — M learns what it may do
    from the node rather than from a hard-coded list.
    """
    return None


def perform(supervisor: Supervisor, action: Action) -> dict:
    """Carry out an `Action`: the tool name decides which endpoint is asked.

    Unreachable while `decide` returns `None`, and written anyway: M2c-2 supplies the
    decisions, and the plumbing between a decision and a request is not the interesting
    part of that batch. The names are the tool schema's, so the two cannot drift apart
    without a test noticing (`--self-test` drives each one against a fake node).
    """
    arguments = action.arguments
    if action.tool == "tasks":
        return supervisor.dispatch_task(
            arguments["target"],
            arguments["input"],
            arguments.get("sandbox"),
            arguments.get("instance"),
        )
    if action.tool == "sandbox_instance_create":
        return supervisor.spawn_instance(arguments["name"])
    if action.tool == "sandbox_instance_reap":
        supervisor.reap_instance(arguments["name"], arguments["instance_id"])
        return {"deleted": True}
    if action.tool == "sandboxes_switch":
        return supervisor.switch_sandbox(arguments["name"])
    if action.tool == "sandbox_request":
        return {"id": supervisor.ask_sandbox(
            arguments["action"], arguments.get("sandbox"), arguments.get("reason")
        )}
    if action.tool == "sandbox_request_approve":
        return supervisor.decide_request(arguments["request_id"], True)
    if action.tool == "sandbox_request_reject":
        return supervisor.decide_request(arguments["request_id"], False)
    raise ValueError(f"unknown tool {action.tool!r}")


def run_once(supervisor: Supervisor, out=print) -> int:
    """One conservative turn, and the whole of this batch's behaviour.

    The order is the design: **read first, decide second, act third**. A read that fails
    ends the turn — `supervisor.snapshot()` raising is the *only* thing standing between
    M and a control call, which is why it is one call that fails as a whole rather than a
    dozen that fail one at a time. Acting on a picture that is missing a piece is how a
    supervisor destroys work it cannot see.
    """
    try:
        state = supervisor.snapshot()
    except (TransportError, ApiError, ValueError) as error:
        out(f"supervisor: cannot read the node's state ({error}); taking no action")
        return EXIT_TURN_FAILED

    action = decide(state)
    if action is None:
        out(
            "supervisor: nothing to do"
            f" ({len(state['pending_requests'])} pending request(s),"
            f" {len(state['executors'])} executor(s),"
            f" {sum(len(v) for v in state['instances'].values())} instance(s) seen)"
        )
        return EXIT_OK

    try:
        answer = perform(supervisor, action)
    except (TransportError, ApiError, ValueError) as error:
        out(f"supervisor: {action.tool} failed ({error})")
        return EXIT_TURN_FAILED
    out(f"supervisor: {action.tool} -> {json.dumps(answer)}")
    return EXIT_OK


def watch(supervisor: Supervisor, as_json: bool, out=print, stop: threading.Event | None = None) -> int:
    """`--events`: subscribe, print, and resume from the last id if the stream drops.

    Read-only on purpose: the stream is *context* (the client guide's §8 says a supervisor
    feeds what arrives to its model rather than offering the stream as a tool), so this
    mode never calls a control endpoint.
    """
    stop = stop or threading.Event()

    def emit(frame: dict) -> None:
        if as_json:
            out(json.dumps(frame))
        else:
            name = frame.get("event") or frame.get("kind")
            out(f"{name}  {json.dumps(frame.get('payload', {}))}")

    last_id: str | None = None
    while not stop.is_set():
        last_id = supervisor.follow(emit, stop, last_event_id=last_id)
        if stop.is_set():
            break
        # The stream ended (a node restart, a dropped socket): wait, then resume from the
        # last id seen. `Last-Event-ID` is what keeps the gap silent instead of fatal.
        stop.wait(RECONNECT_SECONDS)
    return EXIT_OK


# --------------------------------------------------------------------------------------
# Arguments and entry point
# --------------------------------------------------------------------------------------


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="supervisor.py",
        description=(
            "M — a reference dispatcher. Reads a node's state, decides what to do, and "
            "acts. The decision layer is a stub in this batch: the default is to do "
            "nothing."
        ),
        epilog="The token comes from --token-file or $RISCDOM_TOKEN; never from an argument.",
    )
    parser.add_argument("--server", default=DEFAULT_SERVER, help=f"host:port (default {DEFAULT_SERVER})")
    parser.add_argument("--token-file", help="read the bearer token from this file")
    parser.add_argument(
        "--interval",
        type=float,
        default=DEFAULT_INTERVAL_SECONDS,
        help=f"seconds between turns when staying up (default {DEFAULT_INTERVAL_SECONDS:g})",
    )
    parser.add_argument("--once", action="store_true", help="take one turn and exit")
    parser.add_argument("--events", action="store_true", help="subscribe to the event stream instead of turning")
    parser.add_argument("--timeout", type=float, default=30.0, help="per-request timeout in seconds")
    parser.add_argument("--json", action="store_true", help="print frames as JSON (with --events)")
    parser.add_argument("--self-test", action="store_true", help="prove this script offline")
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    if args.self_test:
        return self_test()

    try:
        token = dispatch.read_token(args.token_file)
    except TransportError as error:
        print(f"supervisor: {error}", file=sys.stderr)
        return error.code

    supervisor = Supervisor(args.server, token, args.timeout)
    try:
        if args.events:
            print("supervisor: an AI dispatcher's skeleton — reading the node, deciding nothing yet")
            return watch(supervisor, args.json)
        print("supervisor: an AI dispatcher's skeleton — reading the node, deciding nothing yet")
        print(f"server    : {args.server}")
        while True:
            code = run_once(supervisor)
            if args.once or code != EXIT_OK:
                return code
            time.sleep(args.interval)
    except KeyboardInterrupt:
        return EXIT_OK


# --------------------------------------------------------------------------------------
# The self-test: a fake node on loopback, so the script proves itself offline
# --------------------------------------------------------------------------------------


class FakeNode:
    """A node that answers the reads, records every request, and can be told to fail.

    Deliberately small: it exists to prove *this* script — that the snapshot reads what it
    claims, that a failed read stops the turn before any control call, and that the event
    reader resumes with the id it saw. Whether the control plane behaves is its own
    tests' business.
    """

    def __init__(self) -> None:
        from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

        self.token = "self-test-token"
        self.requests: list[str] = []
        self.posted: list[tuple[str, dict]] = []
        self.event_ids_seen: list[str | None] = []
        self.fail_path: str | None = None
        outer = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):  # keep the test output clean
                pass

            def _send(self, status, payload, content_type="application/json"):
                body = json.dumps(payload).encode("utf-8")
                self.send_response(status)
                self.send_header("Content-Type", content_type)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def _authorised(self) -> bool:
                return self.headers.get("Authorization") == f"Bearer {outer.token}"

            def do_GET(self):  # noqa: N802 (http.server's naming)
                path = self.path.split("?")[0]
                outer.requests.append(f"GET {self.path}")
                if not self._authorised():
                    self._send(401, {"code": "unauthorized", "message": "no token"})
                    return
                if path == outer.fail_path:
                    self._send(500, {"code": "internal", "message": "the node is unwell"})
                    return
                if path == "/v0/events":
                    outer.event_ids_seen.append(self.headers.get("Last-Event-ID"))
                    body = (
                        'id: 1-0\ndata: {"version":1,"kind":"hello","event":null,"payload":{}}\n\n'
                        'id: 1-1\ndata: {"version":1,"kind":"event","event":"sandbox:request",'
                        '"payload":{"id":"req-1-1","status":"pending"}}\n\n'
                    ).encode("utf-8")
                    self.send_response(200)
                    self.send_header("Content-Type", "text/event-stream")
                    self.send_header("Content-Length", str(len(body)))
                    self.end_headers()
                    self.wfile.write(body)
                    return
                answers = {
                    "/v0/status": {"status": "ok", "version": "0.9.9", "agents": 1, "agent_id": "dev-1-1"},
                    "/v0/capabilities": {"capabilities": ["agent.run", "sandbox.read"]},
                    "/v0/executors": {"executors": [{"agent_id": "executor-0"}]},
                    "/v0/sandboxes": {
                        "sandboxes": [{"name": "blink"}],
                        "current": None,
                        "default": "default",
                    },
                    "/v0/sandboxes/blink/instances": {"instances": []},
                    "/v0/sandboxes/requests": {"requests": []},
                    "/v0/runs": [],
                    "/v0/audit/status": {"count": 3, "chain": {"Intact": {"length": 3}}},
                    "/v0/audit/events": [],
                    "/v0/vm/status": {"running": False, "since_ms": None},
                }
                if path in answers:
                    self._send(200, answers[path])
                    return
                self._send(404, {"code": "not_found", "message": f"no endpoint {path}"})

            def do_POST(self):  # noqa: N802
                outer.requests.append(f"POST {self.path}")
                if not self._authorised():
                    self._send(401, {"code": "unauthorized", "message": "no token"})
                    return
                length = int(self.headers.get("Content-Length") or 0)
                body = json.loads(self.rfile.read(length) or b"{}")
                outer.posted.append((self.path, body))
                if self.path == "/v0/tasks":
                    self._send(
                        200,
                        {
                            "task_id": "task-1",
                            "agent_id": f"{body.get('target')}-child-1",
                            "outcome": {"Final": {"content": "done", "iterations": 1}},
                        },
                    )
                    return
                if self.path == "/v0/sandboxes/blink/instances":
                    self._send(201, {"instance_id": "local-1-2", "definition": "blink", "vm_started_at_ms": None})
                    return
                if self.path == "/v0/sandboxes/requests":
                    self._send(201, {"id": "req-1-2"})
                    return
                if self.path == "/v0/sandboxes/switch":
                    self._send(200, {"from": None, "to": "blink"})
                    return
                if self.path.endswith("/approve") or self.path.endswith("/reject"):
                    self._send(200, {"id": "req-1-1", "status": "approved"})
                    return
                self._send(404, {"code": "not_found", "message": f"no endpoint {self.path}"})

            def do_DELETE(self):  # noqa: N802
                outer.requests.append(f"DELETE {self.path}")
                if not self._authorised():
                    self._send(401, {"code": "unauthorized", "message": "no token"})
                    return
                self.send_response(204)
                self.send_header("Content-Length", "0")
                self.end_headers()

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)

    def start(self) -> str:
        self.thread.start()
        return f"127.0.0.1:{self.server.server_address[1]}"

    def stop(self) -> None:
        self.server.shutdown()
        self.server.server_close()

    def posts(self) -> list[str]:
        return [path for path, _ in self.posted]


def imported_modules() -> list[str]:
    """Every module `supervisor.py` imports at the top level, by reading this file.

    Read rather than remembered, because "standard library only" is a claim about this
    file and a claim nobody checks is a claim that rots.
    """
    import ast

    tree = ast.parse(open(__file__, "r", encoding="utf-8").read())
    names: list[str] = []
    for node in tree.body:
        if isinstance(node, ast.Import):
            names.extend(alias.name.split(".")[0] for alias in node.names)
        elif isinstance(node, ast.ImportFrom) and node.module:
            names.append(node.module.split(".")[0])
    return sorted(set(names))


def self_test() -> int:
    """Prove the whole read path offline: no server, no executor, loopback only."""
    failures: list[str] = []
    node = FakeNode()
    address = node.start()
    try:
        supervisor = Supervisor(address, node.token, timeout=5.0)

        # 1. The snapshot reads every source it says it does.
        state = supervisor.snapshot()
        for key in ("status", "capabilities", "executors", "sandboxes", "instances", "pending_requests"):
            if key not in state:
                failures.append(f"the snapshot is missing {key!r}")
        read = " ".join(node.requests)
        for path in (
            "/v0/status",
            "/v0/capabilities",
            "/v0/executors",
            "/v0/sandboxes",
            "/v0/sandboxes/blink/instances",
            "/v0/sandboxes/requests?status=pending",
        ):
            if path not in read:
                failures.append(f"the snapshot never read {path}")
        if state["executors"] != ["executor-0"]:
            failures.append(f"the fleet is wrong: {state['executors']}")

        # 2. The stub decides nothing, so a turn sends no control request at all.
        node.requests.clear()
        node.posted.clear()
        code = run_once(supervisor, out=lambda *_: None)
        if code != EXIT_OK:
            failures.append(f"an idle turn must exit {EXIT_OK}, got {code}")
        if node.posted:
            failures.append(f"an idle turn must send no control request: {node.posts()}")

        # 3. The conservative state: a read that fails ends the turn before any control
        #    request, which is the property the whole loop rests on.
        node.fail_path = "/v0/sandboxes"
        node.requests.clear()
        node.posted.clear()
        code = run_once(supervisor, out=lambda *_: None)
        if code != EXIT_TURN_FAILED:
            failures.append(f"an unreadable state must exit {EXIT_TURN_FAILED}, got {code}")
        if node.posted:
            failures.append(f"an unreadable state must send no control request: {node.posts()}")
        node.fail_path = None

        # 4. …and the next turn recovers, without a restart.
        node.requests.clear()
        if run_once(supervisor, out=lambda *_: None) != EXIT_OK:
            failures.append("the turn after a failed read must succeed")

        # 5. The plumbing between a decision and a request: every tool, driven by hand.
        node.posted.clear()
        perform(supervisor, Action("tasks", {"target": "executor-0", "input": "say hi"}))
        perform(supervisor, Action("tasks", {"target": "executor-0", "input": "x", "sandbox": "blink", "instance": "local-1-2"}))
        perform(supervisor, Action("sandbox_instance_create", {"name": "blink"}))
        perform(supervisor, Action("sandbox_instance_reap", {"name": "blink", "instance_id": "local-1-2"}))
        perform(supervisor, Action("sandboxes_switch", {"name": "blink"}))
        perform(supervisor, Action("sandbox_request", {"action": "switch", "sandbox": "blink"}))
        perform(supervisor, Action("sandbox_request_approve", {"request_id": "req-1-1"}))
        perform(supervisor, Action("sandbox_request_reject", {"request_id": "req-1-1"}))
        sent = node.posts()
        for path in (
            "/v0/tasks",
            "/v0/sandboxes/blink/instances",
            "/v0/sandboxes/switch",
            "/v0/sandboxes/requests",
            "/v0/sandboxes/requests/req-1-1/approve",
            "/v0/sandboxes/requests/req-1-1/reject",
        ):
            if path not in sent:
                failures.append(f"{path} was never sent: {sent}")
        if f"DELETE /v0/sandboxes/blink/instances/local-1-2" not in " ".join(node.requests):
            failures.append("the reap never sent its DELETE")
        # The instance travels with the task (v1.0 M2a-3), and only when declared.
        plain = node.posted[0][1]
        finer = node.posted[1][1]
        if "instance" in plain:
            failures.append(f"a task without an instance must not carry one: {plain}")
        if finer.get("instance") != "local-1-2" or finer.get("sandbox") != "blink":
            failures.append(f"the finer task lost its declaration: {finer}")

        # 6. The event stream: frames are parsed *and* the last id is kept, so a
        #    reconnect resumes instead of asking for a replay it cannot describe.
        stop = threading.Event()
        frames: list[dict] = []
        first = supervisor.follow(lambda frame: frames.append(frame), stop)
        if len(frames) != 2:
            failures.append(f"expected two frames, got {len(frames)}")
        if first != "1-1":
            failures.append(f"the last id must be remembered, got {first!r}")
        second = supervisor.follow(lambda frame: frames.append(frame), stop, last_event_id=first)
        if node.event_ids_seen[-1] != "1-1":
            failures.append(f"the resume must send Last-Event-ID: {node.event_ids_seen}")

        # 7. A wrong token is a credential problem, not a state problem.
        try:
            Supervisor(address, "wrong", timeout=5.0).snapshot()
            failures.append("a wrong token must raise")
        except TransportError:
            pass

        # 8. `--events` is read-only, and it stops when it is told to: the whole request
        #    log from the follow calls contains no write.
        node.requests.clear()
        stop = threading.Event()
        watcher = threading.Thread(
            target=watch, args=(supervisor, True), kwargs={"out": lambda *_: None, "stop": stop}
        )
        watcher.start()
        deadline = time.time() + 5.0
        while not node.event_ids_seen and time.time() < deadline:
            time.sleep(0.02)
        stop.set()
        watcher.join(timeout=5.0)
        if watcher.is_alive():
            failures.append("watch did not stop when its stop event was set")
        if any(not line.startswith("GET ") for line in node.requests):
            failures.append(f"--events must not write: {node.requests}")

        # 9. The red line: nothing but the standard library (and the sibling example).
        allowed = set(sys.stdlib_module_names) | {"dispatch"}
        outside = [name for name in imported_modules() if name not in allowed]
        if outside:
            failures.append(f"non-stdlib imports: {outside}")
    finally:
        node.stop()

    if failures:
        print("supervisor self-test: FAILED", file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        return 1
    print("supervisor self-test: OK (fake node, no network beyond loopback)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
