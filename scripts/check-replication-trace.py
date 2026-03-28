#!/usr/bin/env python3
import json
import sys
from pathlib import Path

AUTO_TERMINALS = {"auto_accepted", "auto_rejected", "auto_compensated", "none"}
RESERVATION_STATES = {
    "not_applicable",
    "reserved",
    "failed_retryable",
    "failed_non_retryable",
    "retry_exhausted",
}
COMPENSATION_STATES = {"not_applicable", "queued", "completed"}


def winner(current, challenger):
    if current is None:
        return challenger
    if challenger["rank"] > current["rank"]:
        return challenger
    if challenger["rank"] < current["rank"]:
        return current
    return challenger if challenger["op_id"] > current["op_id"] else current


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: check-replication-trace.py <trace.json>")
        return 2

    trace_path = Path(sys.argv[1])
    events = json.loads(trace_path.read_text(encoding="utf-8"))
    if not isinstance(events, list):
        print("trace root must be a JSON array")
        return 2

    heads = {}
    errors = []

    for idx, raw in enumerate(events):
        required = {"resource_key", "op_id", "rank", "terminal_state"}
        missing = required.difference(raw.keys())
        if missing:
            errors.append(f"event[{idx}] missing keys: {sorted(missing)}")
            continue

        event = {
            "resource_key": str(raw["resource_key"]),
            "op_id": str(raw["op_id"]),
            "rank": int(raw["rank"]),
            "terminal_state": str(raw["terminal_state"]),
            "reservation_status": str(raw.get("reservation_status", "not_applicable")),
            "compensation_status": str(raw.get("compensation_status", "not_applicable")),
        }

        if event["terminal_state"] not in AUTO_TERMINALS:
            errors.append(
                f"event[{idx}] has non-auto terminal_state={event['terminal_state']}"
            )
        if event["reservation_status"] not in RESERVATION_STATES:
            errors.append(
                f"event[{idx}] has invalid reservation_status={event['reservation_status']}"
            )
        if event["compensation_status"] not in COMPENSATION_STATES:
            errors.append(
                f"event[{idx}] has invalid compensation_status={event['compensation_status']}"
            )
        reservation_failed = event["reservation_status"] in {
            "failed_retryable",
            "failed_non_retryable",
            "retry_exhausted",
        }
        if reservation_failed and event["terminal_state"] != "auto_rejected":
            errors.append(
                f"event[{idx}] reservation failed must imply auto_rejected terminal"
            )
        if event["terminal_state"] == "auto_compensated" and event[
            "compensation_status"
        ] not in {"queued", "completed"}:
            errors.append(
                f"event[{idx}] auto_compensated requires queued/completed compensation"
            )

        key = event["resource_key"]
        prior = heads.get(key)
        if reservation_failed:
            new_head = prior
        else:
            new_head = winner(prior, event)
            heads[key] = new_head

        expected = raw.get("expected_winner_op_id")
        if expected is not None:
            actual = "none" if new_head is None else new_head["op_id"]
            if str(expected) != actual:
                errors.append(f"event[{idx}] expected winner {expected}, got {actual}")

    if errors:
        print("trace check failed:")
        for err in errors:
            print(f"  - {err}")
        return 1

    print("trace check passed.")
    for resource_key in sorted(heads):
        head = heads[resource_key]
        print(f"  {resource_key}: winner={head['op_id']} rank={head['rank']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
