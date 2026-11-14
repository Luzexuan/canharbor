// SPDX-License-Identifier: AGPL-3.0-or-later
import { formatNumber } from "./i18n/format";
import { text } from "./i18n/messages";
import type { Aggregate } from "./monitor";

/** Retained counts and the latest frame per identifier, direction and type. */
export function AggregateTable({ groups }: { groups: Aggregate[] }) {
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
