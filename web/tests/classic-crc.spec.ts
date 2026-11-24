// SPDX-License-Identifier: AGPL-3.0-or-later
import { test, expect } from "./isolated-service";
import { decode } from "../src/contract";
import { execFileSync } from "node:child_process";
import { nativeBinary } from "./native";
import type { StressValidation } from "../src/generated/api";
import example from "../../examples/stress/classic-crc32.json" with { type: "json" };

test("Classic CRC32-v1 uses the shared request schema", () => {
  expect(() => decode("stress_plan", example)).not.toThrow();
  for (const source of [-1, 4294967296, "0"])
    expect(() =>
      decode("stress_plan", {
        ...example,
        traffic: { ...example.traffic, source },
      }),
    ).toThrow();
  expect(() =>
    decode("stress_plan", {
      ...example,
      traffic: { ...example.traffic, typo: true },
    }),
  ).toThrow();
  for (const run_uuid of ["not-a-uuid", example.traffic.run_uuid.toUpperCase()])
    expect(() =>
      decode("stress_plan", {
        ...example,
        traffic: { ...example.traffic, run_uuid },
      }),
    ).toThrow();
});

test("CRC main and reverse flow identities are editable and invalidate preflight", async ({
  page,
  isolatedRuntime,
}) => {
  const url = execFileSync(
    nativeBinary,
    ["--runtime-dir", isolatedRuntime, "web", "open", "--print"],
    { encoding: "utf8" },
  ).trim();
  await page.goto(url);
  await page
    .getByRole("button", { name: "Stress testing", exact: true })
    .click();
  await page.getByText("Advanced stress settings", { exact: true }).click();
  const panel = page.getByRole("region", { name: "Stress testing" });
  await panel
    .getByLabel("Stress protocol", { exact: true })
    .selectOption("crc");
  await panel
    .getByLabel("CRC run UUID", { exact: true })
    .fill(example.traffic.run_uuid);
  await panel.getByLabel("CRC source number", { exact: true }).fill("0");
  const validate = panel.getByRole("button", {
    name: "Validate stress plan",
    exact: true,
  });
  let resolve!: (value: {
    status: number;
    traffic: unknown;
    validation: StressValidation;
  }) => void;
  let reject!: (error: unknown) => void;
  const response = new Promise<{
    status: number;
    traffic: unknown;
    validation: StressValidation;
  }>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  await page.route("**/api/v1/stress/validate", async (route) => {
    if (route.request().method() !== "POST") return route.continue();
    try {
      // Capture the real response before forwarding it; Chromium may release
      // its response-body buffer once the application has consumed the stream.
      const accepted = await route.fetch();
      const validation = decode("stress_validation", await accepted.json());
      const traffic = route.request().postDataJSON().traffic;
      await route.fulfill({ response: accepted });
      resolve({ status: accepted.status(), traffic, validation });
    } catch (error) {
      reject(error);
      await route.abort();
    }
  });
  await validate.click();
  const accepted = await response;
  expect(accepted.status).toBe(200);
  expect(accepted.traffic).toEqual(example.traffic);
  expect(accepted.validation.version).toBe(12);
  await expect(page.getByTestId("stress-validation")).toBeVisible();
  await panel.getByLabel("CRC source number", { exact: true }).fill("7");
  await expect(page.getByTestId("stress-validation")).toHaveCount(0);
  await panel
    .getByRole("button", { name: "Add reverse flow", exact: true })
    .click();
  const flow = panel.getByRole("group", { name: "Flow reverse-1" });
  await expect(
    flow.getByRole("checkbox", {
      name: "Classic CRC32-v1 (8 bytes)",
      exact: true,
    }),
  ).toBeChecked();
  await expect(flow.getByLabel("CRC run UUID", { exact: true })).toHaveValue(
    example.traffic.run_uuid,
  );
  await expect(
    flow.getByLabel("CRC source number", { exact: true }),
  ).toHaveValue("0");
});
