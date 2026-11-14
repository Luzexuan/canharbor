// SPDX-License-Identifier: AGPL-3.0-or-later
import type { Session } from "./api";
import { formatTime } from "./i18n/format";
import { stateText, text } from "./i18n/messages";

/**
 * What is known about the selected channel and when it was last confirmed.
 * Opens by itself while the state or a control renewal is uncertain.
 */
export function StatusDetails({
  state,
  queryStale,
  lastQuery,
  lastRenewal,
  renewalUnknown,
  streamDisconnected,
  checkDisabled,
  onCheck,
}: {
  state: Session["state"] | undefined;
  queryStale: boolean;
  lastQuery: number | null;
  lastRenewal: number | null;
  renewalUnknown: boolean;
  streamDisconnected: boolean;
  checkDisabled: boolean;
  onCheck: () => void;
}) {
  return (
    <details className="status-details" open={queryStale || renewalUnknown}>
      <summary>{text("main:connection_and_control_details")}</summary>
      <p data-testid="channel-status-certainty">
        {queryStale || !lastQuery
          ? text(
              "main:current_channel_state_unknown_last_reported_value_a_disconnected",
              { value: stateText(state ?? "unavailable") },
            )
          : text("main:last_reported_channel_state_value", {
              value: stateText(state ?? "unavailable"),
            })}
      </p>
      <p>
        {text("main:last_query_and_renewal_value", {
          query: lastQuery
            ? formatTime(new Date(lastQuery))
            : text("main:not_yet_confirmed"),
          renewal: lastRenewal
            ? formatTime(new Date(lastRenewal))
            : text("main:not_yet_confirmed"),
        })}
      </p>
      {streamDisconnected && (
        <p>
          {text(
            "main:event_stream_disconnected_the_channel_may_remain_running_while",
          )}
        </p>
      )}
      {renewalUnknown && (
        <p role="alert">
          {text(
            "main:control_renewal_outcome_unknown_renewal_is_paused_the_channel",
          )}
        </p>
      )}
      <button disabled={checkDisabled} onClick={onCheck}>
        {text("main:check_final_status")}
      </button>
    </details>
  );
}
