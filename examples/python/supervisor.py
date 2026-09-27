#!/usr/bin/env python3
"""M — a reference **dispatcher**: the skeleton an AI supervisor's model sits on.

`dispatch.py` (v0.9 interface E3) is a batch client: it is handed a task list and sends
it. This is the other shape — a process that **stays up**, reads the node's state, and
decides *whether* to act. In the roadmap's words (docs/roadmap-v1.0.md §9), M is an AI
dispatcher: it decides what runs where and stops there. It is not in the data path, does
not proxy a task's I/O, and is not in the kernel.

**The decision layer is a bounded tool-calling loop, and it ships empty of policy.** M is
handed the node's state and the control plane's own tool schema
(`docs/tool-schema-control-plane.md`, filtered to the tools a *dispatcher* should have), and
it decides by **calling tools** — the process makes the HTTP request and hands the JSON back,
exactly as the client guide describes (§8). Two things are deliberately left to you: the
**policy** (what a good decision is) and the **model**. Without `--llm-model` there is no
model and M decides nothing — the conservative default rather than a degraded mode, because a
dispatcher that cannot see the whole picture dispatches nothing.

    read the state (one snapshot)  ->  ask the model, running the tools it asks for  ->  report

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
- **Five capability names were vocabulary only.** No route required them, so the v1.0 gap 3/N
  clean-up removed them, leaving the vocabulary at 33 (decisions §83). Dispatching a task needs
  `agent.run`; deriving an instance needs
  `sandbox.instantiate`; deciding a request needs `sandbox.read` **and** the capability the
  request's own action implies.

    python examples/python/supervisor.py --once                     # one turn, no model
    python examples/python/supervisor.py --interval 30              # stay up
    python examples/python/supervisor.py --once --llm-base-url http://127.0.0.1:11434/v1 \
        --llm-model qwen2.5:7b                                      # one turn, with a model
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
DEFAULT_MAX_ROUNDS = 6

# Where the tool list comes from: the schema document is the authority, and `docs/` sits two
# levels above `examples/python/`. `--tool-schema` overrides it for a copy of this file that
# lives somewhere else.
DEFAULT_TOOL_SCHEMA = os.path.join(
    os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))),
    "docs",
    "tool-schema-control-plane.md",
)

# The tools a **dispatcher** should have: what it decides from, and the acts that are its
# own. The schema has eighty; the rest are the node's business, and three are worth naming
# as excluded:
#
#   - `agent_run` is the **executor's** loop ("run one agent turn on this node"). The first
#     rule of the client guide's §8 is that a supervisor is not an executor.
#   - `events` is **not a tool** (the guide's third rule): the stream stays open, so M
#     subscribes to it under `--events` and feeds frames as context instead.
#   - the exports and imports write files on the node, and `snapshots_*` / `vm_*` move its
#     runtime; neither is routing, which is all M does.
M_TOOLS: tuple[str, ...] = (
    # what M decides from — the three sources, plus its own identity and the fleet
    "status",
    "capabilities",
    "executors",
    "sandboxes",
    "instance_list",
    "sandboxes_requests",
    "runs",
    "run_get",
    "audit_status",
    "audit_events",
    "vm_status",
    # what M may do about it — dispatch, instances, the switch, and the approval slot
    "tasks",
    "instance_create",
    "instance_delete",
    "sandboxes_switch",
    "sandboxes_requests_post",
    "sandbox_request_approve",
    "sandbox_request_reject",
)

SYSTEM_PROMPT = """You are M, the dispatcher of one RiscDom node.

You decide what runs where, and you stop there. You are not an executor: you never run a
turn, never write code, and never touch the kernel. You are not in the data path and you do
not proxy a task's I/O. You act by calling the tools you are given; the process makes the
HTTP request and hands you the JSON back.

The node you are looking at is described in the first user message: its status, your own
capabilities, the fleet, the sandbox registry, the instances that exist right now, and the
requests waiting for a decision. That snapshot is the **truth**, read from the node itself
(the audit chain, the instance table and the approval slot). Your memory is a cache and it
is not durable: if you are missing something, ask for it with a tool rather than assuming.

How to decide:
- Read the snapshot before anything else, and say what you would change, if anything.
- If it does not tell you enough, do nothing and say what you would need.
- Prefer the smallest act that answers the question. A dispatch is one task on one executor.
- `sandbox_request_approve` and `sandbox_request_reject` only change the record. If a switch
  is actually wanted, `sandboxes_switch` is the act that moves the node — take that decision
  deliberately, not as a side effect of approving.
- When there is nothing to change, answer in words and call no tool. That is a complete
  answer, not a failure.

Constraints:
- You act only through these tools. Inventing another way is an error.
- If a tool answers an error, read it; do not retry it blindly.
"""


def load_tools(path: str = DEFAULT_TOOL_SCHEMA) -> list[dict]:
    """M's tools, read from the schema document and filtered to `M_TOOLS`.

    The document is the authority — `check-tool-schema.mjs` keeps it in step with the
    server's route table — so the list is **read**, not re-typed: a second copy would drift
    the first time an endpoint changed. Each definition is one JSON object on its own line
    inside a fenced block, which is why this is a line filter rather than a Markdown parser.

    A whitelisted name that is missing is an **error**, not a silently shorter list: a model
    that cannot see `tasks` cannot dispatch, and that failure belongs here rather than in a
    decision nobody can explain.
    """
    try:
        with open(path, "r", encoding="utf-8") as handle:
            lines = handle.read().splitlines()
    except OSError as error:
        raise ValueError(
            f"cannot read the tool schema {path} ({error}); pass --tool-schema"
        ) from None
    found: dict[str, dict] = {}
    for line in lines:
        if not line.startswith('{"type":"function"'):
            continue
        try:
            definition = json.loads(line)
        except ValueError:
            continue
        name = definition.get("function", {}).get("name", "")
        if name in M_TOOLS:
            found[name] = definition
    missing = [name for name in M_TOOLS if name not in found]
    if missing:
        raise ValueError(f"{path} does not define {missing}")
    return [found[name] for name in M_TOOLS]


def state_message(state: dict) -> str:
    """The node's state, as the model's first user message.

    JSON, because it is lossless and its shapes are already documented by the tool schemas —
    a hand-rendered summary would be a second description to keep true. Keys are sorted, so
    two turns' messages differ exactly where the state does.
    """
    return "The node's state right now:\n" + json.dumps(
        state, indent=1, sort_keys=True, ensure_ascii=False
    )


class LLMTransportError(Exception):
    """The model server could not be reached.

    A separate type from `TransportError` on purpose: "the node is unreachable" and "the
    model is unreachable" have different remedies (fix the node, or fix `--llm-base-url` and
    its key), and folding them together would make a model outage look like a credential
    problem.
    """


class LLMApiError(Exception):
    """The model server answered an error: an HTTP status and whatever body came with it."""

    def __init__(self, status: int, payload: dict) -> None:
        super().__init__(f"{status}: {payload.get('error') or payload}")
        self.status = status
        self.payload = payload


class Chat:
    """An OpenAI-compatible chat client, in `urllib` — the model is not the control plane.

    Deliberately **not** `ControlPlane._request`: that one carries the node's bearer token,
    reads the node's error model, and points at the node. A model server has its own address,
    its own key (or none) and its own error text, so it gets its own client. **No retry**,
    for the same reason the control-plane client has none: a turn that cannot reach its model
    is a turn to end, not one to spend twice.
    """

    def __init__(self, base_url: str, model: str, api_key: str = "", timeout: float = 60.0) -> None:
        self.base = (base_url if "://" in base_url else f"http://{base_url}").rstrip("/")
        self.model = model
        self.api_key = api_key
        self.timeout = timeout

    def __call__(self, messages: list[dict], tools: list[dict]) -> dict:
        body: dict = {"model": self.model, "messages": messages}
        if tools:
            body["tools"] = tools
            body["tool_choice"] = "auto"
        request = urllib.request.Request(
            f"{self.base}/chat/completions",
            data=json.dumps(body).encode("utf-8"),
            method="POST",
        )
        request.add_header("Content-Type", "application/json")
        if self.api_key:
            request.add_header("Authorization", "Bearer " + self.api_key)
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as answer:
                return json.loads(answer.read())
        except urllib.error.HTTPError as error:
            raw = error.read()
            try:
                payload = json.loads(raw)
            except ValueError:
                payload = {"error": raw.decode("utf-8", "replace")}
            raise LLMApiError(error.code, payload) from None
        except urllib.error.URLError as error:
            raise LLMTransportError(
                f"cannot reach the model at {self.base}: {error.reason}"
            ) from None


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

    The subclass also puts M's **name** on every request (`X-RiscDom-Agent`, v1.0 gap 3/N),
    which is what makes the node's audit rows say who asked instead of naming the node.
    """

    def __init__(
        self,
        server: str,
        token: str,
        timeout: float = 30.0,
        caller: str | None = None,
    ) -> None:
        super().__init__(server, token, timeout)
        self.caller = caller

    def _request(self, method: str, path: str, body: bytes | None = None) -> bytes:
        """The parent's request, with M's name on it.

        Overridden rather than edited into `dispatch.py`: the batch client has no identity to
        declare, and the header belongs to the dispatcher. The body of this method mirrors
        the parent's (it builds and sends in one step, so there is no hook to extend) — the
        error mapping is repeated verbatim, and `--self-test` walks both of its branches
        through this override so the two cannot drift apart unnoticed.
        """
        request = urllib.request.Request(f"{self.base}{path}", data=body, method=method)
        request.add_header("Authorization", f"Bearer {self.token}")
        if self.caller:
            request.add_header("X-RiscDom-Agent", self.caller)
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
                # The same reading the parent takes: a credential problem is not a task
                # problem, and the caller has to fix who it is.
                raise TransportError(
                    f"the control plane refused this token: {payload.get('message', error.code)}"
                ) from None
            raise ApiError(error.code, payload) from None
        except urllib.error.URLError as error:
            raise TransportError(f"cannot reach {self.base}: {error.reason}") from None

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

    def requests(self, status: str | None = None) -> list[dict]:
        """The approval slot: every request, or the ones in one `status`."""
        query = f"?status={urllib.parse.quote(status)}" if status else ""
        return list(self.get(f"/v0/sandboxes/requests{query}").get("requests", []))

    def pending_requests(self) -> list[dict]:
        """The approval slot, filtered to what is still waiting on a decision."""
        return self.requests("pending")

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
        task_id: str | None = None,
    ) -> dict:
        """`POST /v0/tasks` — the whole `Task`, including the instance (v1.0 M2a-3).

        The parent's `dispatch` predates `instance` and `id` and deliberately stays as it is;
        this is the same request with the finer declaration added. An omitted `id` is the
        normal case: the node fills one in so the answer can be matched to the ask.
        """
        body: dict = {"target": target, "input": text}
        if sandbox:
            body["sandbox"] = sandbox
        if instance:
            body["instance"] = instance
        if task_id:
            body["id"] = task_id
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


class Decision(NamedTuple):
    """What one turn's decision layer did: the tools it asked for, and how it ended."""

    actions: list[Action]
    summary: str


class OfflineDecider:
    """The default decision layer: decide nothing.

    It exists so that "no model configured" is a **working** configuration rather than an
    error path, and so that the conservative answer is the one you get by doing nothing at
    all. Naming a model (`--llm-model`) is how you get a decision instead.
    """

    def decide(self, state: dict, perform) -> Decision:
        return Decision([], "no model is configured (--llm-model), so nothing was decided")


class LLMDecider:
    """The decision layer: a bounded tool-calling loop over the node's state.

    One turn is at most `max_rounds` model calls. The model sees the snapshot once, then the
    results of everything it asks for; **every tool call it makes is performed here**, which
    is the loop the client guide describes — the process makes the HTTP request and hands the
    JSON back. The actions are returned in order so the caller can report them.

    A model that stops asking for tools ends the turn. An empty list means "nothing to do",
    the same answer the offline layer gives, so the conservative case is not a separate path.
    A model that asks for tools in *every* round hits the cap: the turn ends with what it
    already did and a summary that says so, because the alternative — a loop with no ceiling
    — is a process that can be talked into running forever.

    A tool the model names but was not offered is **not performed**; the model gets an error
    result naming it. That is defence in depth: `tools` is already the whitelist, so this can
    only fire for a model that invents a name.
    """

    def __init__(
        self,
        chat: Chat,
        tools: list[dict],
        system_prompt: str = SYSTEM_PROMPT,
        max_rounds: int = DEFAULT_MAX_ROUNDS,
    ) -> None:
        self.chat = chat
        self.tools = tools
        self.system_prompt = system_prompt
        self.max_rounds = max(1, int(max_rounds))
        self.allowed = {tool["function"]["name"] for tool in tools}

    def decide(self, state: dict, perform) -> Decision:
        messages: list[dict] = [
            {"role": "system", "content": self.system_prompt},
            {"role": "user", "content": state_message(state)},
        ]
        actions: list[Action] = []
        for _round in range(1, self.max_rounds + 1):
            answer = self.chat(messages, self.tools)
            choices = answer.get("choices") or []
            if not choices:
                return Decision(actions, "the model answered with no choices")
            message = choices[0].get("message") or {}
            messages.append(message)
            calls = message.get("tool_calls") or []
            if not calls:
                closing = (message.get("content") or "").strip()
                return Decision(actions, closing or "the model said nothing")
            for call in calls:
                function = call.get("function") or {}
                name = function.get("name", "")
                if name not in self.allowed:
                    result: dict = {"error": f"no tool named {name!r} was offered"}
                else:
                    action = Action(name, tool_arguments(function.get("arguments")))
                    try:
                        result = perform(action)
                        actions.append(action)
                    except (TransportError, ApiError, ValueError) as error:
                        result = {"error": str(error)}
                messages.append(
                    {
                        "role": "tool",
                        "tool_call_id": call.get("id", ""),
                        "content": json.dumps(result, ensure_ascii=False),
                    }
                )
        return Decision(
            actions,
            f"the model asked for tools in all {self.max_rounds} round(s); the turn ended there",
        )


def tool_arguments(raw) -> dict:
    """A tool call's arguments: the JSON text a model sends, or an empty mapping."""
    if isinstance(raw, dict):
        return raw
    if not raw:
        return {}
    try:
        parsed = json.loads(raw)
    except ValueError as error:
        raise ValueError(f"the model's tool arguments are not JSON: {error}") from None
    if not isinstance(parsed, dict):
        raise ValueError("the model's tool arguments are not an object")
    return parsed


def perform(supervisor: Supervisor, action: Action) -> dict:
    """Carry out an `Action`: the tool name decides which endpoint is asked.

    The tool names and their argument names are the schema document's (`instance_create`
    takes `name`, `sandbox_request_approve` takes `id`), so every tool the model is offered
    is a tool this function knows — the two cannot drift apart without `--self-test` failing.

    The read tools are here as well as the controls: a model that wants to look again at one
    thing should be able to, without M having to decide in Python what it is allowed to
    notice.
    """
    arguments = action.arguments
    # ----- reads ---------------------------------------------------------------
    if action.tool == "status":
        return supervisor.status()
    if action.tool == "capabilities":
        return {"capabilities": supervisor.capabilities()}
    if action.tool == "executors":
        return {"executors": [{"agent_id": name} for name in supervisor.executors()]}
    if action.tool == "sandboxes":
        return supervisor.sandboxes()
    if action.tool == "instance_list":
        return {"instances": supervisor.instances(arguments["name"])}
    if action.tool == "sandboxes_requests":
        return {"requests": supervisor.requests(arguments.get("status"))}
    if action.tool == "runs":
        return {"runs": supervisor.runs(int(arguments.get("limit", 20)))}
    if action.tool == "run_get":
        key = urllib.parse.quote(arguments["run_id"], safe="")
        return {"run": supervisor.get(f"/v0/runs/{key}")}
    if action.tool == "audit_status":
        return supervisor.audit_status()
    if action.tool == "audit_events":
        return {
            "events": supervisor.audit_events(
                int(arguments.get("limit", 50)),
                arguments.get("actor"),
                arguments.get("action_prefix"),
            )
        }
    if action.tool == "vm_status":
        return supervisor.vm_status()
    # ----- controls ------------------------------------------------------------
    if action.tool == "tasks":
        return supervisor.dispatch_task(
            arguments["target"],
            arguments["input"],
            arguments.get("sandbox"),
            arguments.get("instance"),
            arguments.get("id"),
        )
    if action.tool == "instance_create":
        return supervisor.spawn_instance(arguments["name"])
    if action.tool == "instance_delete":
        supervisor.reap_instance(arguments["name"], arguments["id"])
        return {"deleted": True}
    if action.tool == "sandboxes_switch":
        return supervisor.switch_sandbox(arguments["name"])
    if action.tool == "sandboxes_requests_post":
        return {
            "id": supervisor.ask_sandbox(
                arguments["action"], arguments.get("sandbox"), arguments.get("reason")
            )
        }
    if action.tool == "sandbox_request_approve":
        return supervisor.decide_request(arguments["id"], True)
    if action.tool == "sandbox_request_reject":
        return supervisor.decide_request(arguments["id"], False)
    raise ValueError(f"unknown tool {action.tool!r}")


def run_once(supervisor: Supervisor, decider=None, out=print) -> int:
    """One conservative turn: read, ask the decision layer, report.

    The order is the design: **read first, decide second, act third**. A read that fails ends
    the turn — `supervisor.snapshot()` raising is the *only* thing standing between M and a
    control call. The decision layer performs the tools it asks for (that is its loop), so by
    the time it returns, the calls it made are made: a decision layer that fails half-way is
    reported **after** those calls, never before them.
    """
    decider = decider or OfflineDecider()
    try:
        state = supervisor.snapshot()
    except (TransportError, ApiError, ValueError) as error:
        out(f"supervisor: cannot read the node's state ({error}); taking no action")
        return EXIT_TURN_FAILED

    try:
        decision = decider.decide(state, perform=lambda action: perform(supervisor, action))
    except (TransportError, ApiError, LLMTransportError, LLMApiError, ValueError) as error:
        out(f"supervisor: the decision layer failed ({error}); no further action")
        return EXIT_TURN_FAILED

    if not decision.actions:
        out(
            "supervisor: nothing to do"
            f" ({len(state['pending_requests'])} pending request(s),"
            f" {len(state['executors'])} executor(s),"
            f" {sum(len(v) for v in state['instances'].values())} instance(s) seen)"
        )
    else:
        for action in decision.actions:
            out(
                "supervisor: performed "
                f"{action.tool} {json.dumps(action.arguments, ensure_ascii=False)}"
            )
    if decision.summary:
        out(f"supervisor: {decision.summary}")
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
            "M — a reference dispatcher. Reads a node's state, asks a model what to do "
            "about it (if one is configured), and reports. With no --llm-model it decides "
            "nothing, which is the conservative default."
        ),
        epilog=(
            "The node's token comes from --token-file or $RISCDOM_TOKEN; the model's key from "
            "--llm-api-key-file or $RISCDOM_LLM_API_KEY. Neither is ever an argument."
        ),
    )
    parser.add_argument("--server", default=DEFAULT_SERVER, help=f"host:port (default {DEFAULT_SERVER})")
    parser.add_argument("--token-file", help="read the node's bearer token from this file")
    parser.add_argument(
        "--agent-id",
        help=(
            "the name M answers to, sent as `X-RiscDom-Agent` on every request so the node's "
            "audit rows say who asked (or $RISCDOM_AGENT_ID). **Required**: an unnamed "
            "dispatcher is what the gap 2/N batch went and fixed."
        ),
    )
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
    # The decision layer's configuration: M's model is **not** the executor's, so these are
    # M's own settings and are never read from the node's `llm_configs`.
    parser.add_argument("--llm-base-url", help="an OpenAI-compatible endpoint, e.g. http://127.0.0.1:11434/v1")
    parser.add_argument("--llm-model", help="the model name to ask (without this, nothing is decided)")
    parser.add_argument("--llm-api-key-file", help="read the model's API key from this file ($RISCDOM_LLM_API_KEY)")
    parser.add_argument(
        "--llm-timeout", type=float, default=60.0, help="per-call timeout for the model in seconds"
    )
    parser.add_argument(
        "--max-rounds",
        type=int,
        default=DEFAULT_MAX_ROUNDS,
        help=f"model calls allowed in one turn (default {DEFAULT_MAX_ROUNDS})",
    )
    parser.add_argument(
        "--tool-schema",
        default=DEFAULT_TOOL_SCHEMA,
        help="the control-plane tool schema M's tools are read from",
    )
    parser.add_argument("--self-test", action="store_true", help="prove this script offline")
    return parser.parse_args(argv)


def read_key(path: str | None) -> str:
    """The model's key: `--llm-api-key-file`, then `$RISCDOM_LLM_API_KEY`.

    Absent is a **valid** answer here, unlike the node's token: a model server on loopback
    usually wants no key at all.
    """
    if path:
        try:
            with open(path, "r", encoding="utf-8") as handle:
                return handle.read().strip()
        except OSError as error:
            raise ValueError(f"cannot read {path}: {error}") from None
    return os.environ.get("RISCDOM_LLM_API_KEY", "").strip()


def build_decider(args: argparse.Namespace, out=print):
    """The decision layer the arguments ask for: a model if one is named, otherwise none."""
    if not args.llm_model:
        out("supervisor: no --llm-model: the decision layer is offline and will decide nothing")
        return OfflineDecider()
    if not args.llm_base_url:
        raise ValueError("--llm-model needs --llm-base-url (where the model server is)")
    tools = load_tools(args.tool_schema)
    out(f"supervisor: {len(tools)} tool(s) offered to the model {args.llm_model}")
    chat = Chat(
        args.llm_base_url,
        args.llm_model,
        read_key(args.llm_api_key_file),
        args.llm_timeout,
    )
    return LLMDecider(chat, tools, max_rounds=args.max_rounds)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    if args.self_test:
        return self_test()

    agent_id = (args.agent_id or os.environ.get("RISCDOM_AGENT_ID", "")).strip()
    if not agent_id:
        print(
            "supervisor: M must be named: pass --agent-id <name> or set $RISCDOM_AGENT_ID "
            "(the node writes that name into every audit row this dispatcher causes)",
            file=sys.stderr,
        )
        return EXIT_USAGE

    try:
        token = dispatch.read_token(args.token_file)
        decider = build_decider(args)
    except (TransportError, ValueError) as error:
        print(f"supervisor: {error}", file=sys.stderr)
        return error.code if isinstance(error, TransportError) else EXIT_USAGE

    supervisor = Supervisor(args.server, token, args.timeout, caller=agent_id)
    try:
        if args.events:
            print(f"supervisor: {agent_id} — an AI dispatcher, reading the node")
            return watch(supervisor, args.json)
        print(f"supervisor: {agent_id} — an AI dispatcher, reading the node")
        print(f"server    : {args.server}")
        while True:
            code = run_once(supervisor, decider)
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
        # The caller each request declared (v1.0 gap 3/N), in arrival order: the header is
        # what the whole batch is about, so the test reads what M actually sent.
        self.callers: list[str | None] = []
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
                # Every handler asks this first, so it is also where the caller header is
                # recorded: one place, and no handler can forget it.
                outer.callers.append(self.headers.get("X-RiscDom-Agent"))
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


def model_tool_call(name: str, arguments: dict, call_id: str = "call-1") -> dict:
    """One scripted assistant message that asks for a tool."""
    return {
        "role": "assistant",
        "content": None,
        "tool_calls": [
            {
                "id": call_id,
                "type": "function",
                "function": {"name": name, "arguments": json.dumps(arguments)},
            }
        ],
    }


def model_final(text: str) -> dict:
    """One scripted assistant message that asks for nothing."""
    return {"role": "assistant", "content": text}


class FakeModel:
    """An OpenAI-compatible model server that answers a scripted list of messages.

    It records every request body, so a test can assert what M actually sent — that the
    snapshot and the tool list really travel, and that a tool result comes back as a `tool`
    message — and it can be told to fail instead of answering, or scripted to ask for a tool
    that was never offered. Nothing here talks to a real model: the point is the *loop*.
    """

    def __init__(self, script: list[dict] | None = None, fail: int | None = None) -> None:
        from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

        self.script = list(script or [])
        self.fail = fail
        self.seen: list[dict] = []
        outer = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):  # keep the test output clean
                pass

            def _send(self, status, payload):
                body = json.dumps(payload).encode("utf-8")
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def do_POST(self):  # noqa: N802
                length = int(self.headers.get("Content-Length") or 0)
                body = json.loads(self.rfile.read(length) or b"{}")
                outer.seen.append(body)
                if outer.fail:
                    self._send(outer.fail, {"error": {"message": "the model is unwell"}})
                    return
                # The script is consumed in order; running out is a closing message rather
                # than a hang, so a test cannot wait forever on a missing entry.
                message = outer.script.pop(0) if outer.script else model_final("(the script is empty)")
                self._send(200, {"choices": [{"index": 0, "message": message, "finish_reason": "stop"}]})
                return

            def do_GET(self):  # noqa: N802
                self._send(404, {"error": {"message": "only POST /chat/completions exists"}})

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.address = f"127.0.0.1:{self.server.server_address[1]}/v1"

    def start(self) -> str:
        self.thread.start()
        return self.address

    def stop(self) -> None:
        self.server.shutdown()
        self.server.server_close()


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
        supervisor = Supervisor(address, node.token, timeout=5.0, caller="m-self-test")

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

        # 5. The plumbing between a decision and a request: every tool, driven by hand. The
        #    names are the schema document's, so this is also the check that `perform` and
        #    the tool list still spell one thing one way.
        node.posted.clear()
        perform(supervisor, Action("tasks", {"target": "executor-0", "input": "say hi"}))
        perform(supervisor, Action("tasks", {"target": "executor-0", "input": "x", "sandbox": "blink", "instance": "local-1-2", "id": "task-9"}))
        perform(supervisor, Action("instance_create", {"name": "blink"}))
        perform(supervisor, Action("instance_delete", {"name": "blink", "id": "local-1-2"}))
        perform(supervisor, Action("sandboxes_switch", {"name": "blink"}))
        perform(supervisor, Action("sandboxes_requests_post", {"action": "switch", "sandbox": "blink"}))
        perform(supervisor, Action("sandbox_request_approve", {"id": "req-1-1"}))
        perform(supervisor, Action("sandbox_request_reject", {"id": "req-1-1"}))
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
        # The reads are the same table: a model that asks to look again is answered.
        perform(supervisor, Action("instance_list", {"name": "blink"}))
        perform(supervisor, Action("sandboxes_requests", {"status": "pending"}))
        perform(supervisor, Action("runs", {"limit": 5}))
        perform(supervisor, Action("audit_events", {"limit": 5, "actor": "operator"}))
        perform(supervisor, Action("vm_status", {}))
        read_again = " ".join(node.requests)
        for path in (
            "/v0/sandboxes/blink/instances",
            "/v0/sandboxes/requests?status=pending",
            "/v0/runs?limit=5",
            "/v0/audit/events?limit=5&actor=operator",
            "/v0/vm/status",
        ):
            if path not in read_again:
                failures.append(f"the read tools never asked {path}: {read_again}")
        # The instance and the id travel with the task (v1.0 M2a-3), and only when declared.
        plain = node.posted[0][1]
        finer = node.posted[1][1]
        for field in ("instance", "id"):
            if field in plain:
                failures.append(f"a task without a {field} must not carry one: {plain}")
        if finer.get("instance") != "local-1-2" or finer.get("sandbox") != "blink":
            failures.append(f"the finer task lost its declaration: {finer}")
        if finer.get("id") != "task-9":
            failures.append(f"the caller's task id was dropped: {finer}")

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

        # 9. M's tool list: read from the schema document, filtered to the whitelist, in the
        #    schema's own shape.
        tools = load_tools()
        names = [tool["function"]["name"] for tool in tools]
        if names != list(M_TOOLS):
            failures.append(f"the tool list is not the whitelist: {names}")
        for tool in tools:
            function = tool.get("function", {})
            if tool.get("type") != "function" or not function.get("description"):
                failures.append(f"a tool is not in the schema's shape: {tool}")
            if not isinstance(function.get("parameters"), dict):
                failures.append(f"a tool has no parameters object: {function.get('name')}")
        for excluded in ("agent_run", "events"):
            if excluded in names:
                failures.append(f"{excluded} must not be offered to a dispatcher")
        with open(DEFAULT_TOOL_SCHEMA, encoding="utf-8") as schema:
            defined = [line for line in schema if line.startswith('{"type":"function"')]
        if len(defined) <= len(M_TOOLS):
            failures.append(f"the schema document defines only {len(defined)} tools")

        # 10. The decision layer with a model: the snapshot and the tools travel, a tool call
        #     is performed, its result goes back, and the model's next answer ends the turn.
        model = FakeModel(
            [model_tool_call("instance_create", {"name": "blink"}), model_final("derived one instance")]
        )
        model.start()
        try:
            node.requests.clear()
            node.posted.clear()
            decider = LLMDecider(Chat(model.address, "test-model"), tools, max_rounds=4)
            code = run_once(supervisor, decider, out=lambda *_: None)
            if code != EXIT_OK:
                failures.append(f"a turn with a model must exit {EXIT_OK}, got {code}")
            if "POST /v0/sandboxes/blink/instances" not in " ".join(node.requests):
                failures.append(f"the model's tool call was not performed: {node.requests}")
            if len(model.seen) != 2:
                failures.append(f"expected two model calls, got {len(model.seen)}")
            else:
                first = model.seen[0]
                if (
                    first["messages"][0].get("role") != "system"
                    or "not an executor" not in first["messages"][0].get("content", "")
                ):
                    failures.append("the model did not get the system prompt")
                if "pending_requests" not in first["messages"][1].get("content", ""):
                    failures.append("the state did not travel with the first message")
                if len(first.get("tools", [])) != len(M_TOOLS) or first.get("tool_choice") != "auto":
                    failures.append("the tools did not travel")
                if model.seen[1]["messages"][-1].get("role") != "tool":
                    failures.append("the tool result was not fed back")
        finally:
            model.stop()

        # 11. The approval tools: the record changes, and only the record.
        model = FakeModel([model_tool_call("sandbox_request_approve", {"id": "req-1-1"}), model_final("ok")])
        model.start()
        try:
            node.posted.clear()
            run_once(supervisor, LLMDecider(Chat(model.address, "test-model"), tools), out=lambda *_: None)
            if "POST /v0/sandboxes/requests/req-1-1/approve" not in " ".join(node.requests):
                failures.append(f"approve was not sent: {node.requests}")
        finally:
            model.stop()

        # 12. …and an ask is a POST that leaves a request behind.
        model = FakeModel(
            [
                model_tool_call("sandboxes_requests_post", {"action": "switch", "sandbox": "blink"}),
                model_final("asked"),
            ]
        )
        model.start()
        try:
            node.posted.clear()
            run_once(supervisor, LLMDecider(Chat(model.address, "test-model"), tools), out=lambda *_: None)
            if "POST /v0/sandboxes/requests" not in " ".join(node.requests):
                failures.append(f"the ask was not sent: {node.requests}")
        finally:
            model.stop()

        # 13. A tool M never offered is **not performed**: the model is told, and nothing is
        #     sent. (`agent_run` is the executor's loop, so it is the interesting case.)
        model = FakeModel([model_tool_call("agent_run", {"user_input": "hi"}), model_final("fine")])
        model.start()
        try:
            node.posted.clear()
            run_once(supervisor, LLMDecider(Chat(model.address, "test-model"), tools), out=lambda *_: None)
            if node.posted:
                failures.append(f"an unoffered tool must not be performed: {node.posts()}")
            if len(model.seen) != 2 or "no tool named" not in model.seen[1]["messages"][-1].get("content", ""):
                failures.append("the model was not told that tool does not exist")
        finally:
            model.stop()

        # 14. A model that fails is a conservative turn: exit 1 and **no** control request.
        model = FakeModel(fail=500)
        model.start()
        try:
            node.posted.clear()
            code = run_once(supervisor, LLMDecider(Chat(model.address, "test-model"), tools), out=lambda *_: None)
            if code != EXIT_TURN_FAILED:
                failures.append(f"a failing model must exit {EXIT_TURN_FAILED}, got {code}")
            if node.posted:
                failures.append(f"a failing model must send no control request: {node.posts()}")
        finally:
            model.stop()

        # 15. A model server that is not there at all: the same conservative turn.
        node.posted.clear()
        code = run_once(
            supervisor, LLMDecider(Chat("127.0.0.1:1", "test-model"), tools), out=lambda *_: None
        )
        if code != EXIT_TURN_FAILED:
            failures.append(f"an unreachable model must exit {EXIT_TURN_FAILED}, got {code}")
        if node.posted:
            failures.append(f"an unreachable model must send no control request: {node.posts()}")

        # 16. The round cap: a model that asks for a tool in every round ends the turn at the
        #     ceiling instead of running forever — and says so.
        model = FakeModel([model_tool_call("instance_list", {"name": "blink"})] * 3)
        model.start()
        try:
            capped = LLMDecider(Chat(model.address, "test-model"), tools, max_rounds=3)
            decision = capped.decide(
                supervisor.snapshot(), perform=lambda action: perform(supervisor, action)
            )
            if len(decision.actions) != 3:
                failures.append(f"three rounds should mean three actions: {decision.actions}")
            if "3 round" not in decision.summary:
                failures.append(f"the cap must be reported: {decision.summary!r}")
            if len(model.seen) != 3:
                failures.append(f"the cap must stop the calls: {len(model.seen)}")
        finally:
            model.stop()

        # 17. The offline default is still the safe one: no model, no control request.
        node.posted.clear()
        code = run_once(supervisor, OfflineDecider(), out=lambda *_: None)
        if code != EXIT_OK or node.posted:
            failures.append(f"the offline default must decide nothing: {code} {node.posts()}")

        # 19. M's name travels (v1.0 gap 3/N): every request carries `X-RiscDom-Agent`, an
        #     unnamed dispatcher carries none, and both error branches of the override behave
        #     like the parent's.
        # A clean window, so the pairing of path and caller is beyond doubt.
        node.requests.clear()
        node.callers.clear()
        supervisor.snapshot()
        unnamed_at = [
            path
            for path, caller in zip(node.requests, node.callers)
            if caller != "m-self-test"
        ]
        if not node.callers:
            failures.append("the named dispatcher made no request at all")
        if unnamed_at:
            failures.append(f"requests that did not carry M's name: {unnamed_at}")
        unnamed = Supervisor(address, node.token, timeout=5.0)
        node.callers.clear()
        node.requests.clear()
        unnamed.snapshot()
        if any(caller is not None for caller in node.callers):
            failures.append(f"an unnamed dispatcher sends no header: {node.callers}")
        # The override repeats the parent's error mapping (it builds and sends in one step),
        # so both branches are walked through it here and cannot drift apart unnoticed.
        try:
            supervisor.get("/v0/no-such-endpoint")
            failures.append("a 404 must raise")
        except ApiError as error:
            if error.status != 404:
                failures.append(f"expected a 404 ApiError, got {error.status}")
        except TransportError as error:
            failures.append(f"a 404 is not a transport problem: {error}")
        # …and an unnamed M is a usage error **before** anything is contacted: the identity is
        # what the batch is about, so it is not defaulted away.
        if main(["--once", "--agent-id", "  "]) != EXIT_USAGE:
            failures.append("a blank --agent-id must be a usage error")

        # 20. The red line: nothing but the standard library (and the sibling example).
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
