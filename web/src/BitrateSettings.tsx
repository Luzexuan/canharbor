// SPDX-License-Identifier: AGPL-3.0-or-later
import type { Capabilities } from "./generated/api";
import { text } from "./i18n/messages";

/**
 * Nominal and CAN FD data bitrate inputs with the timing capabilities the
 * selected channel reports. Values and their validity are owned by the caller.
 */
export function BitrateSettings({
  simulated,
  fdAvailable,
  capabilities,
  capabilityError,
  nominalBitrate,
  onNominalBitrate,
  validNominal,
  fdEnabled,
  onFdEnabled,
  dataBitrate,
  onDataBitrate,
  validData,
  rateIssue,
}: {
  simulated: boolean | undefined;
  fdAvailable: boolean;
  capabilities: Capabilities | null;
  capabilityError: unknown;
  nominalBitrate: number;
  onNominalBitrate: (value: number) => void;
  validNominal: boolean;
  fdEnabled: boolean;
  onFdEnabled: (value: boolean) => void;
  dataBitrate: number;
  onDataBitrate: (value: number) => void;
  validData: boolean;
  rateIssue: string | null;
}) {
  return (
    <div className="fixed-setting">
      <label>
        {text("main:nominal_bitrate")}
        <input
          aria-label={text("main:nominal_bitrate")}
          aria-invalid={!validNominal}
          aria-describedby={!validNominal ? "nominal-error" : undefined}
          type="number"
          list="nominal-rates"
          min="1"
          max="10000000"
          disabled={simulated}
          value={simulated ? 500000 : nominalBitrate}
          onChange={(e) => onNominalBitrate(Number(e.target.value))}
        />
      </label>
      <datalist id="nominal-rates">
        {(capabilities?.fixed_nominal_bitrates.length
          ? capabilities.fixed_nominal_bitrates
          : [125000, 250000, 500000, 1000000]
        ).map((rate) => (
          <option key={rate} value={rate}>
            {text("main:kbit_s_value", { value: rate / 1000 })}
          </option>
        ))}
      </datalist>
      {!validNominal && (
        <p id="nominal-error">
          {text("main:enter_a_whole_nominal_bitrate_from_1_to_10000000")}
        </p>
      )}
      <label>
        <input
          type="checkbox"
          checked={fdEnabled}
          disabled={!fdAvailable}
          onChange={(e) => onFdEnabled(e.target.checked)}
        />
        {text("main:enable_can_fd")}
      </label>
      {fdEnabled && (
        <label>
          {text("main:fd_data_bitrate")}
          <input
            aria-label={text("main:fd_data_bitrate")}
            aria-invalid={!validData}
            aria-describedby={!validData ? "data-bitrate-error" : undefined}
            type="number"
            list="data-rates"
            min="1"
            max="20000000"
            disabled={simulated}
            value={simulated ? 2000000 : dataBitrate}
            onChange={(e) => onDataBitrate(Number(e.target.value))}
          />
        </label>
      )}
      <datalist id="data-rates">
        {(capabilities?.fixed_data_bitrates.length
          ? capabilities.fixed_data_bitrates
          : [1000000, 2000000, 4000000, 8000000]
        ).map((rate) => (
          <option key={rate} value={rate}>
            {text("main:mbit_s_value", { value: rate / 1000000 })}
          </option>
        ))}
      </datalist>
      {rateIssue && <p role="alert">{rateIssue}</p>}
      {!!capabilityError && (
        <p>
          {capabilityError instanceof Error
            ? capabilityError.message
            : String(capabilityError)}
        </p>
      )}
      {capabilities && (
        <details>
          <summary>{text("main:device_timing_capabilities")}</summary>
          <p>
            {capabilities.fixed_nominal_bitrates.length
              ? text("main:supported_nominal_rates_value_bit_s", {
                  value: capabilities.fixed_nominal_bitrates.join(", "),
                })
              : text(
                  "main:suggested_rates_are_not_a_compatibility_claim_the_device",
                )}
          </p>
          {capabilities.nominal_timing && (
            <p>
              {text("main:nominal_timing_value", {
                ...capabilities.nominal_timing,
              })}
            </p>
          )}
          {fdEnabled && (
            <p>
              {capabilities.fixed_data_bitrates.length
                ? text("main:supported_fd_data_rates_value_bit_s", {
                    value: capabilities.fixed_data_bitrates.join(", "),
                  })
                : text("main:fd_rates_require_device_timing_validation")}
            </p>
          )}
        </details>
      )}
      {!validData && (
        <p id="data-bitrate-error">
          {text("main:enter_a_whole_data_bitrate_from_1_to_20000000")}
        </p>
      )}
      <small>
        {simulated
          ? text("main:virtual_bus_fixed_500_kbit_s_2_mbit_s")
          : text("main:hardware_compatibility_has_not_been_verified")}
      </small>
    </div>
  );
}
