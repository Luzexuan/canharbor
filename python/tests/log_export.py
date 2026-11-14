# SPDX-License-Identifier: AGPL-3.0-or-later
"""Executed by the CLI's offline test: read an exported log with python-can.

The reader is chosen by the file extension (.asc or .blf).

Prints the messages as JSON, or a line starting with "skipped" when python-can
is not installed.
"""
import json
import sys

try:
    import can
except ImportError:
    print("skipped: python-can is not installed")
    raise SystemExit(0)

reader = can.LogReader(sys.argv[1])
# The ASC reader reports times from the start of the measurement. The BLF
# reader adds the start time in the file header.
start = getattr(reader, "start_timestamp", 0.0)
print(
    json.dumps(
        [
            {
                "us": round((message.timestamp - start) * 1_000_000),
                "id": message.arbitration_id,
                "extended": message.is_extended_id,
                "rx": message.is_rx,
                "remote": message.is_remote_frame,
                "fd": message.is_fd,
                "brs": message.bitrate_switch,
                "esi": message.error_state_indicator,
                "dlc": message.dlc,
                "data": message.data.hex(),
                "channel": message.channel,
            }
            for message in reader
        ]
    )
)
