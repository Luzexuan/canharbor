// SPDX-License-Identifier: AGPL-3.0-or-later
import type { DbcDatabase } from "./dbc";
import { signalCell } from "./DbcControl";
import { formatNumber } from "./i18n/format";
import { text } from "./i18n/messages";
import type { Aggregate } from "./monitor";

/**
 * Retained counts and the latest frame per identifier, direction and type.
 * With a `database`, a further column shows the signals of that latest frame.
 */
export function AggregateTable({
  groups,
  database,
}: {
  groups: Aggregate[];
  database?: DbcDatabase | null;
}) {
  return (
    <div className="table-scroll aggregate-table">
      <table>
        <thead>
          <tr>
            <th>{text("main:id_hex")}</th>
            <th>{text("main:width")}</th>
            <th>{text("main:direction")}</th>
            <th>{text("main:type")}</th>
            <th>{text("main:observed_count")}</th>
            <th>{text("main:latest_data_hex")}</th>
            {database && <th>{text("main:signals")}</th>}
          </tr>
        </thead>
        <tbody>
          {groups.map((group) => (
            <tr key={group.key}>
              <td>{group.latest.frame.id.toString(16).toUpperCase()}</td>
              <td>
                {group.latest.frame.extended
                  ? text("main:29_bit")
                  : text("main:11_bit")}
              </td>
              <td>{group.latest.direction.toUpperCase()}</td>
              <td>{group.latest.frame.kind}</td>
              <td>{formatNumber(group.count)}</td>
              <td>
                {group.latest.frame.data_hex.match(/.{2}/g)?.join(" ") || "—"}
              </td>
              {database && <td>{signalCell(database, group.latest.frame)}</td>}
            </tr>
          ))}
        </tbody>
      </table>
      {!groups.length && (
        <p>{text("main:no_id_groups_match_counts_start_at_subscription_or")}</p>
      )}
    </div>
  );
}
