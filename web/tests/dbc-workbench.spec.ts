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

  await page.getByRole("button", { name: "Remove DBC", exact: true }).click();
  await expect(header).toHaveCount(0);
  await expect(page.getByTestId("dbc-status")).toHaveCount(0);
});
