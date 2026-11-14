// SPDX-License-Identifier: AGPL-3.0-or-later
// Values shared by the stress plan editor and the report view.
export const defaultCriteria = {
  min_submitted_ppm: 1000000,
  max_scheduler_skipped: 0,
  max_missing_expected_ppm: 0,
  max_missing_submitted_ppm: 0,
  max_duplicates: 0,
  max_reordered: 0,
  max_payload_errors: 0,
  max_out_of_range: 0,
  max_dispatch_lateness_ns: null as number | null,
};
export const terminal = (s: string) =>
  ["completed", "failed", "aborted", "interrupted"].includes(s);
