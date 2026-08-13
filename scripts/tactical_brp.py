"""CLI tooling to drive the tactical server/client over the Bevy Remote
Protocol (BRP) for headless, CLI-only testing.

Assumes the caller already has a SpacetimeDB + tactical server + one or more
headless tactical clients running with `--brp-port` set (e.g. via
`just tactical-isolated`, `just tactical brp_port=...`, and
`just client-headless brp_port=...`). This script does not spawn or manage
those processes - see `just tactical-brp-smoke-test` for the wiring.
"""

from __future__ import annotations

import argparse
import json
import sys
import time
import urllib.error
import urllib.request
from typing import Any

# Every BRP-queryable type as a typed class (CharacterId, ClientPlayer,
# Transform, ...), each extending BevyComponent or BevyResource, plus the
# handful of plain nested dataclasses their fields reference (e.g.
# PlayerInputRequest). See adventuresim_brp_lib.py's own docstring, and `just
# generate-brp-types`. BrpClient's component/resource-shaped methods below
# take and return these, not raw type-path strings or dicts.
from adventuresim_brp_lib import *  # noqa: F401,F403
from adventuresim_brp_lib import BevyComponent, BevyResource


class BrpError(RuntimeError):
    pass


class BrpClient:
    """Thin JSON-RPC 2.0 client for a Bevy Remote Protocol HTTP endpoint."""

    def __init__(self, port: int, host: str = "127.0.0.1", timeout: float = 5.0):
        self.url = f"http://{host}:{port}"
        self.timeout = timeout
        self._next_id = 1

    def call(self, method: str, params: dict[str, Any] | None = None) -> Any:
        request_id = self._next_id
        self._next_id += 1
        body = json.dumps(
            {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}
        ).encode("utf-8")
        request = urllib.request.Request(
            self.url, data=body, headers={"content-type": "application/json"}
        )
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                payload = json.loads(response.read())
        except urllib.error.URLError as error:
            raise BrpError(f"{method} against {self.url} failed: {error}") from error
        if "error" in payload:
            raise BrpError(f"{method} against {self.url} returned error: {payload['error']}")
        return payload.get("result")

    def query(
        self,
        components: list[type[BevyComponent]] | None = None,
        option: list[type[BevyComponent]] | None = None,
        has: list[type[BevyComponent]] | None = None,
        with_: list[type[BevyComponent]] | None = None,
        without: list[type[BevyComponent]] | None = None,
    ) -> list[dict[str, Any]]:
        data: dict[str, Any] = {}
        if components:
            data["components"] = [c.type_path for c in components]
        if option:
            data["option"] = [c.type_path for c in option]
        if has:
            data["has"] = [c.type_path for c in has]
        params: dict[str, Any] = {"data": data}
        filter_: dict[str, Any] = {}
        if with_:
            filter_["with"] = [c.type_path for c in with_]
        if without:
            filter_["without"] = [c.type_path for c in without]
        if filter_:
            params["filter"] = filter_
        return self.call("world.query", params)

    def get_components(self, entity: int, components: list[type[BevyComponent]]) -> dict[type[BevyComponent], BevyComponent]:
        result = self.call(
            "world.get_components", {"entity": entity, "components": [c.type_path for c in components]}
        )
        raw = result["components"]
        return {
            component_type: component_type.from_brp(raw[component_type.type_path])
            for component_type in components
            if component_type.type_path in raw
        }

    def insert_resource(self, resource: BevyResource) -> None:
        self.call("world.insert_resources", {"resource": resource.type_path, "value": resource.to_brp()})

    def remove_resource(self, resource_type: type[BevyResource]) -> None:
        self.call("world.remove_resources", {"resource": resource_type.type_path})

    def despawn(self, entity: int) -> None:
        self.call("world.despawn_entity", {"entity": entity})


def find_entity_with_component(client: BrpClient, component_type: type[BevyComponent]) -> int | None:
    matches = client.query(with_=[component_type])
    if not matches:
        return None
    return matches[0]["entity"]


def wait_for_entity_with_component(
    client: BrpClient, component_type: type[BevyComponent], timeout: float
) -> int:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        entity = find_entity_with_component(client, component_type)
        if entity is not None:
            return entity
        time.sleep(0.25)
    raise BrpError(f"timed out waiting for an entity with {component_type.type_path}")


def call_command(args: argparse.Namespace) -> int:
    client = BrpClient(args.port)
    params = json.loads(args.params) if args.params else None
    result = client.call(args.method, params)
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


def smoke_test_command(args: argparse.Namespace) -> int:
    server = BrpClient(args.server_brp_port)
    client = BrpClient(args.client_brp_port)

    print("Checking BRP is reachable on server and client...")
    server.call("rpc.discover")
    client.call("rpc.discover")

    print(f"Waiting up to {args.timeout}s for {ClientPlayer.type_path} to appear on the client...")
    player_entity = wait_for_entity_with_component(client, ClientPlayer, args.timeout)
    print(f"Found local player entity {player_entity}")

    before = client.get_components(player_entity, [Transform])[Transform]
    before_x = before.translation[0]
    print(f"Starting translation.x = {before_x}")

    print("Driving forward movement via PlayerInputOverride...")
    client.insert_resource(
        PlayerInputOverride(value=PlayerInputRequest(movement=[0.0, 1.0], look=[0.0, 0.0], jump=False, weapon_guard="Lowered"))
    )

    deadline = time.monotonic() + args.timeout
    after_x = before_x
    while time.monotonic() < deadline:
        time.sleep(0.25)
        after = client.get_components(player_entity, [Transform])[Transform]
        after_x = after.translation[0]
        if abs(after_x - before_x) >= args.min_delta:
            break

    print("Clearing PlayerInputOverride...")
    client.insert_resource(PlayerInputOverride(value=None))

    moved = abs(after_x - before_x)
    if moved < args.min_delta:
        print(
            f"FAIL: translation.x moved {moved:.4f} (< {args.min_delta}) - "
            f"{before_x} -> {after_x}",
            file=sys.stderr,
        )
        return 1

    print(f"PASS: translation.x moved {moved:.4f} ({before_x} -> {after_x})")
    return 0


def create_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    call_parser = sub.add_parser("call", help="Issue a raw BRP JSON-RPC call")
    call_parser.add_argument("port", type=int)
    call_parser.add_argument("method")
    call_parser.add_argument("--params", help="JSON-encoded params object")
    call_parser.set_defaults(func=call_command)

    smoke_parser = sub.add_parser(
        "smoke-test",
        help="Connect, find the local player, drive movement, and assert it moved",
    )
    smoke_parser.add_argument("--server-brp-port", type=int, required=True)
    smoke_parser.add_argument("--client-brp-port", type=int, required=True)
    smoke_parser.add_argument("--timeout", type=float, default=15.0)
    smoke_parser.add_argument("--min-delta", type=float, default=0.1)
    smoke_parser.set_defaults(func=smoke_test_command)

    return parser


def main() -> int:
    args = create_parser().parse_args()
    try:
        return args.func(args)
    except BrpError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
