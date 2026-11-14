// SPDX-License-Identifier: AGPL-3.0-or-later
import type { BusEvent } from "./api";
import { text } from "./i18n/messages";

export type BusDiagnostic = Extract<BusEvent, { type: "bus_diagnostic" }>;

/** The recent bus diagnostic events of the stream; nothing when there are none. */
export function BusDiagnostics({
  diagnostics,
}: {
  diagnostics: BusDiagnostic[];
}) {
  if (!diagnostics.length) return null;
  return (
    <section aria-label={text("main:bus_diagnostics")}>
      <h2>{text("main:bus_diagnostics")}</h2>
      <p role="status">
        {text("main:recent_bus_diagnostic_events_value", {
          number: diagnostics.length,
        })}
      </p>
      <details>
        <summary>{text("main:diagnostic_details")}</summary>
        <ul>
          {diagnostics.map((item) => (
            <li key={item.sequence}>
              {item.overflow
                ? text("main:receive_overflow_observations_may_be_missing")
                : item.error_classes & 0x40
                  ? text(
                      "main:bus_off_inspect_the_bus_before_explicitly_restarting",
                    )
                  : text("main:bus_diagnostic_inspect_adapter_details")}{" "}
              ·{" "}
              {text("main:classes_value", {
                classes: item.error_classes.toString(16),
              })}{" "}
              · {item.raw_data_hex}
              {item.overflow
                ? ` · ${text("main:receive_loss_reported_count_unknown")}`
                : ""}{" "}
              · {item.timestamp_quality}
            </li>
          ))}
        </ul>
      </details>
    </section>
  );
}
