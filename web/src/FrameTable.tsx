// SPDX-License-Identifier: AGPL-3.0-or-later
import { useState, type ReactNode } from "react";
import type { BusEvent } from "./api";
import { hexBytes, signalText, type DbcDatabase } from "./dbc";
import { text } from "./i18n/messages";

export type FrameBusEvent = Extract<BusEvent, { type: "frame_received" }>;
// Rows rendered beyond each edge of the visible part of the list.
const ROW_OVERSCAN = 12;

/**
 * The recent-frames table. Only the rows near the scroll position are
 * rendered; spacer rows keep the scroll height of the whole list, and all rows
 * have the same height. `origin` is the timestamp that elapsed times are
 * relative to. With a `database`, a further column shows the decoded signals
 * of the rendered rows. Children follow the table, for the empty states.
 */
export function FrameTable({
  rows: visibleRows,
  origin,
  hidden,
  database,
  children,
}: {
  rows: FrameBusEvent[];
  origin: string | null;
  hidden: boolean;
  database?: DbcDatabase | null;
  children?: ReactNode;
}) {
  const [frameViewport, setFrameViewport] = useState({ top: 0, height: 480 });
  const [rowHeight, setRowHeight] = useState(36);
  const measureRow = (row: HTMLTableRowElement | null) => {
    const height = row?.getBoundingClientRect().height;
    if (height && Math.abs(height - rowHeight) > 0.5) setRowHeight(height);
  };
  const firstRow = Math.min(
    Math.max(0, visibleRows.length - 1),
    Math.max(0, Math.floor(frameViewport.top / rowHeight) - ROW_OVERSCAN),
  );
  const lastRow = Math.min(
    visibleRows.length,
    firstRow + Math.ceil(frameViewport.height / rowHeight) + 2 * ROW_OVERSCAN,
  );
  const columns = database ? 8 : 7;
  const signals = (row: FrameBusEvent) => {
    // Remote frames carry no data.
    const decoded =
      row.frame.kind === "classic_remote"
        ? null
        : database?.decode(
            row.frame.id,
            row.frame.extended,
            hexBytes(row.frame.data_hex),
          );
    if (!decoded) return "—";
    return decoded.truncated
      ? `${signalText(decoded)} ${text("main:signals_beyond_the_payload_value", { number: decoded.truncated })}`
      : signalText(decoded);
  };
  return (
    <div
      className="table-scroll"
      hidden={hidden}
      onScroll={(event) => {
        const { scrollTop, clientHeight } = event.currentTarget;
        setFrameViewport((old) =>
          old.top === scrollTop && old.height === clientHeight
            ? old
            : { top: scrollTop, height: clientHeight },
        );
      }}
    >
      <table aria-rowcount={visibleRows.length + 1}>
        <thead>
          <tr>
            <th>{text("main:sequence")}</th>
            <th>{text("main:elapsed_s")}</th>
            <th>{text("main:dir")}</th>
            <th>{text("main:id_hex")}</th>
            <th>{text("main:type")}</th>
            <th>DLC</th>
            <th>{text("main:data_hex")}</th>
            {database && <th>{text("main:signals")}</th>}
          </tr>
        </thead>
        <tbody>
          {firstRow > 0 && (
            <tr className="row-spacer" aria-hidden="true">
              <td colSpan={columns} style={{ height: firstRow * rowHeight }} />
            </tr>
          )}
          {visibleRows.slice(firstRow, lastRow).map((row, offset) => (
            <tr
              key={`${row.sequence}-${firstRow + offset}`}
              aria-rowindex={firstRow + offset + 2}
              ref={offset === 0 ? measureRow : undefined}
            >
              <td>{row.sequence}</td>
              <td
                title={text("main:value_reported_value_s_epoch_value", {
                  timestamp_quality: row.timestamp_quality,
                  value: row.device_raw_timestamp_us ?? "unavailable",
                  timestamp_epoch: row.timestamp_epoch,
                })}
              >
                {(
                  Number(
                    (BigInt(row.host_monotonic_ns) -
                      BigInt(origin ?? row.host_monotonic_ns)) /
                      1000000n,
                  ) / 1000
                ).toFixed(3)}
              </td>
              <td>
                <span className={`direction ${row.direction}`}>
                  {row.direction.toUpperCase()}
                </span>
              </td>
              <td className="frame-id">
                {row.frame.id
                  .toString(16)
                  .toUpperCase()
                  .padStart(row.frame.extended ? 8 : 3, "0")}
              </td>
              <td>
                {row.frame.kind === "fd_data"
                  ? "FD"
                  : row.frame.kind === "classic_remote"
                    ? "RTR"
                    : "CAN"}
                {row.frame.brs ? " · BRS" : ""}
                {row.frame.esi ? " · ESI" : ""}
              </td>
              <td>{row.frame.dlc}</td>
              <td>{row.frame.data_hex.match(/.{2}/g)?.join(" ") || "—"}</td>
              {database && <td className="frame-signals">{signals(row)}</td>}
            </tr>
          ))}
          {lastRow < visibleRows.length && (
            <tr className="row-spacer" aria-hidden="true">
              <td
                colSpan={columns}
                style={{
                  height: (visibleRows.length - lastRow) * rowHeight,
                }}
              />
            </tr>
          )}
        </tbody>
      </table>
      {children}
    </div>
  );
}
