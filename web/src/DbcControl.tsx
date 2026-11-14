// SPDX-License-Identifier: AGPL-3.0-or-later
import { useState } from "react";
import { DbcDatabase, DbcError } from "./dbc";
import { text } from "./i18n/messages";

/**
 * The optional DBC file of the frame monitor. `input` is the file control and
 * `status` the messages that follow the filters; `database` is null until a
 * file has been accepted.
 */
export function useDbc() {
  // Display only, and kept in this page: a reload starts without a database.
  const [dbc, setDbc] = useState<{
    name: string;
    database: DbcDatabase;
  } | null>(null);
  const [dbcError, setDbcError] = useState("");
  async function loadDbc(file: File) {
    setDbcError("");
    try {
      // Checked before reading, so an oversized file is never loaded.
      if (file.size > 16 * 1024 * 1024) throw new DbcError("too_large", 0, "");
      setDbc({
        name: file.name,
        database: DbcDatabase.parse(new Uint8Array(await file.arrayBuffer())),
      });
    } catch (error) {
      // A file that cannot be used replaces nothing.
      setDbcError(
        !(error instanceof DbcError)
          ? text("main:the_dbc_file_could_not_be_read")
          : error.kind === "too_large"
            ? text("main:the_dbc_file_exceeds_16_mib_or_the_signal_limit")
            : error.line === 0
              ? text("main:dbc_not_supported_value", { reason: error.reason })
              : error.kind === "unsupported"
                ? text("main:dbc_line_not_supported_value", {
                    line: error.line,
                    reason: error.reason,
                  })
                : text("main:dbc_line_not_read_value", {
                    line: error.line,
                    reason: error.reason,
                  }),
      );
    }
  }
  return {
    database: dbc?.database ?? null,
    input: (
      <label>
        {text("main:dbc_file_optional")}
        <input
          type="file"
          accept=".dbc"
          onChange={(e) => {
            const selected = e.target.files?.[0];
            if (selected) void loadDbc(selected);
            e.target.value = "";
          }}
        />
      </label>
    ),
    status: (
      <>
        {dbcError && <p role="alert">{dbcError}</p>}
        {dbc && (
          <p data-testid="dbc-status">
            {text("main:dbc_loaded_value", {
              name: dbc.name,
              messages: dbc.database.messages,
            })}{" "}
            <button
              onClick={() => {
                setDbc(null);
                setDbcError("");
              }}
            >
              {text("main:remove_dbc")}
            </button>
          </p>
        )}
      </>
    ),
  };
}
