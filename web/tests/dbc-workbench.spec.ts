import { nativeBinary } from "./native";
// SPDX-License-Identifier: AGPL-3.0-or-later
import { test, expect } from "./isolated-service";
import { execFileSync } from "node:child_process";
test("a loaded DBC file adds decoded signals to the frame table", async ({
  page,
  isolatedRuntime,
}) => {
  const cli = (...args: string[]) =>
    execFileSync(nativeBinary, ["--runtime-dir", isolatedRuntime, ...args], {
      encoding: "utf8",
    });
  const send = (id: string, data: string) =>
    expect(
      cli(
        "send",
        "--channel",
        "virtual:0",
        "--id",
        id,
        "--data",
        data,
        "--active",
      ),
    ).toContain("virtual_bus_accepted");
  await page.goto(cli("web", "open", "--print").trim());
  await expect(page.getByText("Browser paired", { exact: true })).toBeVisible();
  await page
    .getByRole("combobox", { name: "Channel", exact: true })
    .selectOption("virtual:1");
  await page
    .getByRole("button", { name: "Open and listen", exact: true })
    .click();
  await expect(
    page.getByText("You have control", { exact: true }),
  ).toBeVisible();
  send("123", "C8003C02");
  send("124", "0102");
  await expect(
    page.getByRole("cell", { name: "C8 00 3C 02", exact: true }),
  ).toBeVisible();
  // Without a database there is no signal column.
  const header = page.getByRole("columnheader", {
    name: "Signals",
    exact: true,
  });
  await expect(header).toHaveCount(0);

  const file = page.getByLabel("DBC file (optional)");
  // A file that cannot be used reports the line and adds nothing.
  await file.setInputFiles({
    name: "broken.dbc",
    mimeType: "application/octet-stream",
    buffer: Buffer.from("BO_ 291 Engine: 8 ECU\n SG_ Speed : 0|0@1+ (1,0)\n"),
  });
  await expect(page.getByRole("alert")).toContainText("DBC line 2");
  await expect(header).toHaveCount(0);

  await file.setInputFiles({
    name: "vehicle.dbc",
    mimeType: "application/octet-stream",
    buffer: Buffer.from(
      'BO_ 291 Engine: 8 ECU\n SG_ Speed : 0|16@1+ (0.5,0) [0|0] "km/h" X\n' +
        ' SG_ Temp : 16|8@1+ (1,-40) [0|0] "degC" X\n SG_ Gear : 24|8@1+ (1,0) [0|0] "" X\n' +
        ' SG_ Late : 56|8@1+ (1,0) [0|0] "" X\n' +
        'VAL_ 291 Gear 2 "Drive" ;\n',
    ),
  });
  await expect(page.getByTestId("dbc-status")).toContainText(
    "vehicle.dbc: 1 message(s)",
  );
  await expect(page.getByRole("alert")).toHaveCount(0);
  await expect(header).toBeVisible();
  // Frames already in the table are decoded, not only later ones.
  await expect(
    page.getByRole("cell", {
      name: "Engine: Speed=100 km/h, Temp=20 degC, Gear=2 (Drive) (1 signal(s) beyond the payload)",
      exact: true,
    }),
  ).toBeVisible();
  // The frame the file does not describe has an empty signal cell.
  const other = page.getByRole("row").filter({ hasText: "01 02" });
  await expect(other.getByRole("cell").last()).toHaveText("—");

  // The by-identifier view shows the signals of the latest frame per group.
  await page
    .getByRole("combobox", { name: "Monitor view", exact: true })
    .selectOption("ids");
  const groups = page.locator(".aggregate-table");
  await expect(
    groups.getByRole("columnheader", { name: "Signals", exact: true }),
  ).toBeVisible();
  await expect(
    groups.getByRole("row").filter({ hasText: "C8 00 3C 02" }),
  ).toContainText("Engine: Speed=100 km/h");
  await expect(
    groups
      .getByRole("row")
      .filter({ hasText: "01 02" })
      .getByRole("cell")
      .last(),
  ).toHaveText("—");
  await page
    .getByRole("combobox", { name: "Monitor view", exact: true })
    .selectOption("frames");

  // The offline viewer uses the same database and has the control while it
  // is the open view.
  await page
    .getByRole("button", { name: "Offline analysis", exact: true })
    .click();
  const offline = page.getByRole("region", { name: "Offline capture viewer" });
  await expect(offline.getByTestId("dbc-status")).toContainText("vehicle.dbc");
  await expect(page.getByLabel("DBC file (optional)")).toHaveCount(1);
  const event = (sequence: number, id: number, data_hex: string) =>
    JSON.stringify({
      record: "event",
      event: {
        type: "frame_received",
        sequence: String(sequence),
        host_monotonic_ns: String(sequence * 1000),
        timestamp_quality: "host_only",
        device_raw_timestamp_us: null,
        device_timestamp_us: null,
        timestamp_epoch: "0",
        raw_dlc: null,
        direction: "rx",
        frame: {
          kind: "classic_data",
          id,
          extended: false,
          brs: false,
          esi: false,
          data_hex,
          dlc: data_hex.length / 2,
        },
      },
    });
  await offline.getByLabel("Local capture file").setInputFiles({
    name: "capture.ndjson",
    mimeType: "application/x-ndjson",
    buffer: Buffer.from(
      [
        JSON.stringify({
          record: "header",
          schema_version: 1,
          capture_id: "c",
          owner: "o",
          server_instance_id: "s",
          session_epoch: "s1@1",
          created_unix_ms: "1",
          config: {},
        }),
        event(1, 0x123, "C8003C02"),
        event(2, 0x124, "0102"),
        JSON.stringify({
          record: "footer",
          complete: true,
          events: "2",
          dropped: "0",
          reason: null,
        }),
      ].join("\n") + "\n",
    ),
  });
  await expect(
    offline.getByRole("columnheader", { name: "Signals", exact: true }),
  ).toBeVisible();
  await expect(
    offline.getByRole("row").filter({ hasText: "C8003C02" }),
  ).toContainText("Engine: Speed=100 km/h, Temp=20 degC, Gear=2 (Drive)");
  await expect(
    offline
      .getByRole("row")
      .filter({ hasText: "0102" })
      .getByRole("cell")
      .last(),
  ).toHaveText("—");

  // Removing the database from here removes the column in every view.
  await offline
    .getByRole("button", { name: "Remove DBC", exact: true })
    .click();
  await expect(
    offline.getByRole("columnheader", { name: "Signals", exact: true }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: "Live traffic", exact: true }).click();
  await expect(header).toHaveCount(0);
  await expect(page.getByTestId("dbc-status")).toHaveCount(0);
});
