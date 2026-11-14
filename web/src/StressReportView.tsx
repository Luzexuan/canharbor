// SPDX-License-Identifier: AGPL-3.0-or-later
// The report of one stress run. Rendering only: the panel owns the run state
// and the requests.
import type { StressArtifacts, StressReport } from "./generated/api";
import { formatNumber } from "./i18n/format";
import { stateText, text } from "./i18n/messages";
import { defaultCriteria, terminal } from "./stressShared";

export function StressReportView({
  report,
  artifacts,
  busy,
  stop,
  stopFlow,
}: {
  report: StressReport;
  artifacts: StressArtifacts | null;
  busy: boolean;
  stop: () => Promise<void>;
  stopFlow: (name: string) => Promise<void>;
}) {
  const history = report.history ?? [];
  const series = history.slice(1).map((point, i) => {
    const before = history[i];
    const seconds = (point.elapsed_ms - before.elapsed_ms) / 1000;
    return {
      t: point.elapsed_ms / 1000,
      fps: seconds > 0 ? (point.submitted - before.submitted) / seconds : 0,
      target:
        report.adaptive?.reserved_peak_fps ??
        report.group?.budget.peak_transmit_fps ??
        point.target_fps,
    };
  });
  const maxY = Math.max(1, ...series.flatMap((p) => [p.fps, p.target]));
  const tMin = series[0]?.t ?? 0;
  const tMax = Math.max(tMin + 1, series.at(-1)?.t ?? 1);
  const points = (key: "fps" | "target") =>
    series
      .map(
        (p) =>
          `${50 + ((p.t - tMin) / (tMax - tMin)) * 690},${180 - (p[key] / maxY) * 150}`,
      )
      .join(" ");
  return (
    <div data-testid="stress-report">
      <div className="panel-heading">
        <h3>
          {text("stress:run_value", {
            id: report.id.slice(0, 8),
            state: stateText(report.state),
          })}
        </h3>
        <button
          disabled={busy || terminal(report.state)}
          onClick={() => void stop()}
        >
          {text("stress:stop_stress_test")}
        </button>
      </div>
      <div className="stress-totals">
        <strong>
          {text("stress:submitted_value", {
            submitted: formatNumber(report.submitted),
            planned: formatNumber(report.planned),
          })}
        </strong>
        <span>
          {text("stress:skipped_value", {
            skipped: report.skipped,
            failed: report.failed,
            unknown: report.unknown,
          })}
        </span>
        <span>
          {text("stress:verdict_value", {
            verdict: stateText(report.verdict),
            complete: report.evidence.complete
              ? text("stress:complete")
              : text("stress:not_complete"),
          })}
        </span>
        <span>
          {text("stress:scheduling_quantum_value", {
            quantum_ms: report.scheduling_quantum_ns / 1e6,
            rx_lag_events: report.rx_lag_events,
            queue_high_water: report.evidence.queue_high_water,
          })}
        </span>
        <span>
          {text("stress:process_peak_rss_value", {
            process_peak_rss_bytes:
              history.at(-1)?.process_peak_rss_bytes != null
                ? text("stress:value_mib", {
                    value: (
                      (history.at(-1)?.process_peak_rss_bytes ?? 0) / 1048576
                    ).toFixed(1),
                  })
                : text("stress:unavailable"),
            counters_complete: report.counters_complete
              ? text("stress:current")
              : text("stress:checkpoint_only"),
          })}
        </span>
      </div>
      <details>
        <summary>{text("stress:frozen_acceptance_criteria")}</summary>
        <pre>
          {JSON.stringify(report.plan.criteria ?? defaultCriteria, null, 2)}
        </pre>
      </details>
      {report.assessment && (
        <div data-testid="stress-assessment">
          {text("stress:criteria_and_workload_value", {
            criteria: report.assessment.passed
              ? text("stress:met")
              : text("stress:not_met"),
            workload: report.assessment.workload_met
              ? text("stress:met")
              : text("stress:not_met"),
          })}
          {report.assessment.failures.length > 0 && (
            <ul>
              {report.assessment.failures.map((f, index) => (
                <li key={index}>
                  {f.criterion}
                  {f.channel ? ` · ${f.channel}` : ""}
                  {f.stage != null
                    ? ` · ${text("stress:stage_value", { stage: f.stage })}`
                    : ""}
                  : {f.actual}
                  {f.denominator != null ? ` / ${f.denominator}` : ""}
                  {" · "}
                  {text("stress:limit_value", { limit: f.limit })}
                  {f.denominator != null ? " ppm" : ""}
                </li>
              ))}
            </ul>
          )}
          <p>
            {text(
              "stress:criteria_met_is_separate_from_complete_evidence_and_completed",
            )}
          </p>
        </div>
      )}
      {report.provenance && (
        <details data-testid="stress-provenance">
          <summary>
            {text("stress:software_and_selected_library_source_evidence")}
          </summary>
          <p>
            {text("stress:version_value", {
              package_version: report.provenance.package_version,
              os: report.provenance.os,
              architecture: report.provenance.architecture,
              scope: report.provenance.scope,
            })}
          </p>
          <p>
            {text("stress:service_executable_sha_256")}{" "}
            {report.provenance.executable.sha256 ?? text("stress:unknown_2")} ·{" "}
            {stateText(report.provenance.executable.status)}
          </p>
          {report.provenance.channels.map((c) => (
            <div key={c.channel}>
              <p>
                {text("stress:firmware_value", {
                  channel: c.channel,
                  backend: c.backend,
                  device_id: c.device_id,
                  firmware: c.firmware,
                })}
              </p>
              {c.libraries.length ? (
                c.libraries.map((f, index) => (
                  <p key={index}>
                    {text("stress:sha_256_value", {
                      name: f.name,
                      sha256: f.sha256 ?? text("stress:unknown_2"),
                      bytes: f.bytes ?? text("stress:unknown_2"),
                      status: stateText(f.status),
                    })}
                  </p>
                ))
              ) : (
                <p>
                  {text(
                    "stress:selected_dynamic_library_evidence_unavailable_not_applicable",
                  )}
                </p>
              )}
            </div>
          ))}
          {report.plan.topology && (
            <pre>{JSON.stringify(report.plan.topology, null, 2)}</pre>
          )}
        </details>
      )}
      {report.timing && (
        <div data-testid="stress-timing">
          <h3>{text("stress:sampled_host_timing")}</h3>
          <p>
            {text("stress:every_value", {
              scope: report.timing.scope,
              sample_every: report.plan.timing?.sample_every,
              window_high_water: report.timing.window_high_water,
              max_pending: report.plan.timing?.max_pending,
              pending: report.timing.pending,
              evicted: report.timing.evicted,
              abandoned: report.timing.abandoned,
              finalized: report.timing.finalized
                ? text("stress:finalized")
                : text("stress:in_progress"),
            })}
          </p>
          <p>
            {text("stress:request_sampling_value", {
              requests_sampled: report.timing.request.submitted,
              requests_known:
                report.plan.rtt?.role === "responder" ? 0 : report.submitted,
              responses_sampled: report.timing.response.submitted,
              responses_known: report.rtt_report?.responder_submitted ?? 0,
            })}
          </p>
          {report.timing.error && <p role="alert">{report.timing.error}</p>}
          {(
            [
              ["Request", report.timing.request],
              ["Response", report.timing.response],
            ] as const
          ).map(([name, t]) => (
            <div key={name}>
              <p>
                {text("stress:measured_submissions_submitted_value", {
                  name: text(
                    name === "Request" ? "stress:request" : "stress:response",
                  ),
                  samples: t.samples,
                  submitted: t.submitted,
                  failed: t.failed,
                  unknown: t.unknown,
                  worker_queue_unknown: t.worker_queue_unknown,
                })}
              </p>
              <p>
                {(
                  [
                    "preparation",
                    "dispatch_wait",
                    "worker_queue",
                    "submit_call",
                    "completion_wait",
                    "total",
                  ] as const
                )
                  .map(
                    (key) =>
                      `${text(`stress:${key}`)}: p99 ${t[key].p99_upper_ns == null ? text("stress:unknown_2") : `${(t[key].p99_upper_ns / 1e6).toFixed(3)} ms`}, max ${t[key].samples === 0 ? text("stress:unknown_2") : `${(t[key].max_ns / 1e6).toFixed(3)} ms`}, n=${t[key].samples}`,
                  )
                  .join(" · ")}
              </p>
            </div>
          ))}
          {report.timing.receivers.map((t) => (
            <div key={t.channel}>
              <p>
                {text("stress:matched_value", {
                  channel: t.channel,
                  matched: t.matched,
                  submitted: report.timing!.request.submitted,
                  coverage: report.timing!.request.submitted
                    ? text("stress:value_3", {
                        value: (
                          (t.matched * 100) /
                          report.timing!.request.submitted
                        ).toFixed(2),
                      })
                    : text("stress:unknown_2"),
                  sampled_rx: t.sampled_rx,
                  unmatched_rx: t.unmatched_rx,
                  missing_sampled_submitted: t.missing_sampled_submitted,
                })}
              </p>
              <p>
                {text("stress:signed_rx_latency_negative_samples_value", {
                  samples: t.latency.negative_magnitude.samples,
                  percentiles: [
                    t.latency.p50_upper_ns,
                    t.latency.p95_upper_ns,
                    t.latency.p99_upper_ns,
                  ]
                    .map((n) =>
                      n == null
                        ? text("stress:unknown_2")
                        : `${(n / 1e6).toFixed(3)} ms`,
                    )
                    .join(" / "),
                  minimum:
                    t.latency.min_ns == null
                      ? text("stress:unknown_2")
                      : text("stress:value_ms", {
                          value: (t.latency.min_ns / 1e6).toFixed(3),
                        }),
                  maximum:
                    t.latency.max_ns == null
                      ? text("stress:unknown_2")
                      : text("stress:value_ms", {
                          value: (t.latency.max_ns / 1e6).toFixed(3),
                        }),
                })}
              </p>
            </div>
          ))}
          <p>
            {text(
              "stress:coverage_is_limited_to_sampled_sequences_and_the_retained",
            )}
          </p>
          <details>
            <summary>
              {text("stress:timing_histograms_and_sample_counts")}
            </summary>
            <pre>{JSON.stringify(report.timing, null, 2)}</pre>
          </details>
        </div>
      )}
      {report.group && (
        <div data-testid="stress-group-report">
          <h3>{text("stress:concurrent_flow_results")}</h3>
          <p>
            {text("stress:group_evidence_value", {
              evidence_scope: report.group.evidence_scope,
              complete: report.evidence.complete
                ? text("stress:complete")
                : text("stress:writing_or_incomplete"),
              peak_transmit_fps: report.group.budget.peak_transmit_fps,
              start_ns:
                report.group.start_ns == null
                  ? text("stress:not_released")
                  : text("stress:value_s", {
                      value: (report.group.start_ns / 1e9).toFixed(3),
                    }),
            })}
          </p>
          {report.group.budget.devices.map((d) => (
            <p key={d.device_id}>
              {text("stress:reserved_value", {
                device_id: d.device_id,
                channels: d.channels.join(", "),
                peak_transmit_fps: d.peak_transmit_fps,
              })}
            </p>
          ))}
          {report.group.flows.map((f) => (
            <article key={f.name} data-testid={`stress-flow-${f.name}`}>
              <h4>
                {text("stress:can_0x_value", {
                  name: f.name,
                  id: f.report.plan.can_id.toString(16).toUpperCase(),
                  width: f.report.plan.extended
                    ? text("stress:extended")
                    : text("stress:standard"),
                })}
              </h4>
              <p>
                {text("stress:verifier_value", {
                  sender: f.report.plan.sender,
                  receivers: f.report.plan.receivers.join(", "),
                  state: stateText(f.report.state),
                  verdict: stateText(f.report.verdict),
                })}
              </p>
              <p>
                {text("stress:submitted_2_value", {
                  submitted: f.report.submitted,
                  planned: f.report.planned,
                  skipped: f.report.skipped,
                  failed: f.report.failed,
                  unknown: f.report.unknown,
                })}
              </p>
              {f.report.receivers.map((r) => (
                <p key={r.channel}>
                  {text("stress:unique_value", {
                    channel: r.channel,
                    unique: r.unique,
                    missing_expected: r.missing_expected,
                    missing_submitted: r.missing_submitted,
                    duplicates: r.duplicates,
                    reordered: r.reordered,
                    crc_errors: r.crc_errors,
                    pattern_errors: r.pattern_errors,
                  })}
                </p>
              ))}
              <LoadDetails report={f.report} />
              <ClassicPatternDetails report={f.report} />
              <ResourceDetails report={f.report} />
              {f.report.rtt_report && (
                <p>
                  {text("stress:rtt_samples_value", {
                    samples: f.report.rtt_report.latency.samples,
                    responder_submitted:
                      f.report.rtt_report.responder_submitted,
                    responder_unknown: f.report.rtt_report.responder_unknown,
                  })}
                </p>
              )}
              {f.report.timing?.receivers.map((r) => (
                <p key={r.channel}>
                  {text("stress:timing_value", {
                    channel: r.channel,
                    matched: r.matched,
                    submitted: f.report.timing!.request.submitted,
                    unmatched_rx: r.unmatched_rx,
                  })}
                </p>
              ))}
              {report.group!.stop_requests[f.name] != null && (
                <p>
                  {text("stress:stop_requested_at_value", {
                    name: (report.group!.stop_requests[f.name] / 1e9).toFixed(
                      3,
                    ),
                  })}
                </p>
              )}
              <button
                disabled={
                  terminal(f.report.state) ||
                  f.report.state.startsWith("saving_") ||
                  report.group!.stop_requests[f.name] != null
                }
                onClick={() => void stopFlow(f.name)}
              >
                {text("stress:stop_flow_value", {
                  name: f.name,
                })}
              </button>
              <details>
                <summary>
                  {text("stress:frozen_flow_plan_criteria_and_evidence")}
                </summary>
                <pre>{JSON.stringify(f.report, null, 2)}</pre>
              </details>
            </article>
          ))}
          <p>
            {text(
              "stress:group_totals_include_locally_generated_rtt_responses_a_skipped",
            )}
          </p>
        </div>
      )}
      {report.integrity && (
        <p>
          {text("stress:payload_verification_value", {
            integrity: report.integrity,
          })}
        </p>
      )}
      <LoadDetails report={report} />
      {report.adaptive && (
        <div data-testid="stress-adaptive">
          <h3>{text("stress:adaptive_exploration_and_fixed_measurement")}</h3>
          <p>{report.adaptive.scope}</p>
          <p>
            {text("stress:phase_value", {
              phase: report.adaptive.phase,
              selected_fps: report.adaptive.selected_fps ?? text("stress:none"),
              reserved_planned: report.adaptive.reserved_planned,
              reserved_peak_fps: report.adaptive.reserved_peak_fps,
            })}
          </p>
          {report.adaptive.steps.map((s) => (
            <article key={s.name} data-testid={`stress-adaptive-${s.name}`}>
              <h4>
                {text("stress:input_value", {
                  name: s.name,
                  fps: s.report.plan.fps,
                  state: stateText(s.report.state),
                })}
              </h4>
              <p>
                {text("stress:submitted_2_value_2", {
                  submitted: s.report.submitted,
                  planned: s.report.planned,
                  skipped: s.report.skipped,
                  verdict: stateText(s.report.verdict),
                })}
              </p>
              <LoadDetails report={s.report} />
              <p>
                {text("stress:rx_evidence_interval_value", {
                  sample_every: s.report.plan.evidence?.sample_every ?? 0,
                })}
              </p>
              {s.decision ? (
                <p>
                  {text("stress:criteria_2_value", {
                    passed: s.decision.criteria.passed
                      ? text("stress:met")
                      : text("stress:not_met"),
                    qualified: String(s.decision.qualified),
                    reason: stateText(s.decision.reason),
                    next: s.decision.next ?? text("stress:none"),
                  })}
                </p>
              ) : (
                <p>{text("stress:decision_pending")}</p>
              )}
              <ClassicPatternDetails report={s.report} />
              <ResourceDetails report={s.report} />
              <details>
                <summary>
                  {text("stress:frozen_step_actual_load_and_decision_evidence")}
                </summary>
                <pre>{JSON.stringify(s, null, 2)}</pre>
              </details>
            </article>
          ))}
        </div>
      )}
      <ClassicPatternDetails report={report} />
      <ResourceDetails report={report} />
      {report.rtt_report && (
        <p>
          {text("stress:rtt_initiator_host_observations_value", {
            samples: report.rtt_report.latency.samples,
            p99:
              report.rtt_report.latency.p99_upper_ns == null
                ? text("stress:unavailable")
                : text("stress:value_ms", {
                    value: (
                      report.rtt_report.latency.p99_upper_ns / 1e6
                    ).toFixed(3),
                  }),
            timed_out: report.rtt_report.timed_out,
            responder_submitted: report.rtt_report.responder_submitted,
            responder_unknown: report.rtt_report.responder_unknown,
          })}
        </p>
      )}
      {report.plan.coordination && (
        <p>
          {text("stress:coordinated_group_value", {
            group_id: report.plan.coordination.group_id,
            participant: report.plan.coordination.participant,
            clock_quality: report.plan.coordination.clock_quality,
          })}
        </p>
      )}
      {report.lifecycle?.termination && (
        <p role="status">
          {text("stress:stopped_value", {
            code: stateText(report.lifecycle.termination.code),
            detail: report.lifecycle.termination.detail,
          })}
        </p>
      )}
      {report.lifecycle?.stop_requested_ns != null && (
        <p>
          {text("stress:stop_requested_at_value_2", {
            stop_requested_ns: (
              report.lifecycle.stop_requested_ns / 1e9
            ).toFixed(3),
            last_attempt_sequence:
              report.lifecycle.last_attempt_sequence ?? text("stress:none"),
            unknown_sequence: report.unknown_sequence ?? text("stress:none"),
          })}
        </p>
      )}
      {report.recovery?.status && (
        <p>
          {text(
            report.recovery.counters_are_lower_bounds
              ? "stress:evidence_verification_lower_bounds_value"
              : "stress:evidence_verification_value",
            {
              status: stateText(report.recovery.status),
              sample_coverage: report.recovery.sample_coverage,
              submission_coverage:
                report.recovery.submission_coverage ??
                text("stress:legacy_aggregates"),
              valid_prefix_bytes: formatNumber(
                report.recovery.valid_prefix_bytes,
              ),
              damaged_files: report.recovery.damaged_files,
            },
          )}
        </p>
      )}
      {report.backend_cleanup?.map((c) => (
        <p key={c.channel}>
          {text("stress:backend_cleanup_value", {
            channel: c.channel,
            scope: c.scope,
            backend_unconfirmed:
              c.backend_unconfirmed ?? text("stress:unknown_2"),
          })}
        </p>
      ))}
      {report.stability?.status && (
        <p>
          {text("stress:memory_monitor_value", {
            status: stateText(report.stability.status),
            max_growth_bytes: (
              report.stability.max_growth_bytes / 1048576
            ).toFixed(1),
          })}
        </p>
      )}
      {series.length > 0 && (
        <figure>
          <figcaption>
            {text("stress:host_submission_rate_frames_s_observed_vs_value", {
              adaptive:
                report.group || report.adaptive
                  ? text("stress:reserved_peak_2")
                  : text("stress:target"),
              t_min: Math.round(tMax - tMin),
            })}
          </figcaption>
          <svg
            viewBox="0 0 780 220"
            role="img"
            aria-label={text("stress:stress_submission_rate_chart")}
          >
            <line x1="50" y1="180" x2="740" y2="180" />
            <line x1="50" y1="30" x2="50" y2="180" />
            <text x="5" y="32">
              {Math.round(maxY)}
            </text>
            <text x="25" y="180">
              0
            </text>
            <text x="50" y="198">
              {Math.round(tMin)}
            </text>
            <text x="710" y="198">
              {Math.round(tMax)}
            </text>
            <text x="320" y="210">
              {text("stress:elapsed_time_seconds")}
            </text>
            <polyline className="stress-target" points={points("target")} />
            <polyline className="stress-observed" points={points("fps")} />
          </svg>
          <small>
            {text("stress:solid_observed_host_submissions_dashed_value", {
              adaptive:
                report.group || report.adaptive
                  ? text(
                      "stress:conservative_aggregate_reserved_peak_not_a_dynamic_target",
                    )
                  : text("stress:target"),
            })}
          </small>
        </figure>
      )}
      {report.receivers.length > 0 && (
        <table>
          <caption>{text("stress:peer_rx_observations")}</caption>
          <thead>
            <tr>
              <th>{text("stress:receiver")}</th>
              <th>{text("stress:unique_2")}</th>
              <th>{text("stress:expected_not_observed")}</th>
              <th>{text("stress:submitted_missing")}</th>
              <th>{text("stress:duplicates_2")}</th>
              <th>{text("stress:reordered_2")}</th>
              <th>{text("stress:evidence_2")}</th>
            </tr>
          </thead>
          <tbody>
            {report.receivers.map((r) => (
              <tr key={r.channel}>
                <td>{r.channel}</td>
                <td>{r.unique}</td>
                <td>{r.missing_expected}</td>
                <td>
                  {terminal(report.state)
                    ? r.missing_submitted
                    : text("stress:pending_2")}
                </td>
                <td>{r.duplicates}</td>
                <td>{r.reordered}</td>
                <td>
                  {text("stress:crc_errors_value", {
                    evidence_complete: r.evidence_complete
                      ? text("stress:complete")
                      : text("stress:incomplete"),
                    crc_errors: r.crc_errors ?? 0,
                    pattern_errors: r.pattern_errors ?? 0,
                  })}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <details>
        <summary>{text("stress:stage_results")}</summary>
        {report.stages.map((s, i) => (
          <p key={i}>
            {text("stress:frames_s_2_value", {
              name: s.name,
              phase: s.warmup
                ? text("stress:warmup")
                : text("stress:measurement"),
              fps: s.fps,
              submitted: s.submitted,
              planned: s.count,
              skipped: s.skipped,
              received_unique: s.received_unique.join(", "),
            })}
          </p>
        ))}
      </details>
      {report.errors.map((e, i) => (
        <p key={i} role="alert">
          {e}
        </p>
      ))}
      {artifacts && (
        <details>
          <summary>
            {text("stress:evidence_files_value", {
              bytes: formatNumber(report.evidence.bytes),
            })}
          </summary>
          <ul>
            {artifacts.files.map((f) => (
              <li key={f.name}>
                <a
                  download
                  href={`/api/v1/stress/runs/${report.id}/artifacts/${encodeURIComponent(f.name)}`}
                >
                  {f.name}
                </a>{" "}
                ·{" "}
                {text("stress:bytes_2_value", {
                  bytes: f.bytes,
                })}{" "}
                <small>
                  {text("stress:sha_256_2_value", {
                    sha256: f.sha256,
                  })}
                </small>
              </li>
            ))}
          </ul>
        </details>
      )}
      <p>
        {text(
          "stress:refresh_only_reconnects_the_display_pausing_or_closing_this",
        )}
      </p>
    </div>
  );
}
function ClassicPatternDetails({ report }: { report: StressReport }) {
  if (report.plan.traffic?.format !== "classic_pattern_v1" || report.group)
    return null;
  const tx = report.classic_pattern_transmit;
  return (
    <div data-testid="stress-classic-pattern">
      <h4>{text("stress:classic_pattern_v1_independent_checkpoint")}</h4>
      <p>
        {text("stress:physical_planned_frames_value", {
          planned: report.planned,
          checkpoints: report.planned / 2,
          data: report.planned / 2,
          control_id: report.plan.traffic.checkpoint_can_id
            .toString(16)
            .toUpperCase(),
          max_pending: report.plan.traffic.max_pending,
        })}
      </p>
      {tx && (
        <p>
          {text("stress:submitted_checkpoints_data_value", {
            checkpoint_submitted: tx.checkpoint_submitted,
            data_submitted: tx.data_submitted,
            checkpoint_skipped: tx.checkpoint_skipped,
            data_skipped: tx.data_skipped,
            checkpoint_failed: tx.checkpoint_failed,
            data_failed: tx.data_failed,
            checkpoint_unknown: tx.checkpoint_unknown,
            data_unknown: tx.data_unknown,
          })}
        </p>
      )}
      {report.receivers.map(
        (r) =>
          r.classic_pattern && (
            <p key={r.channel}>
              {text("stress:checkpoints_2_value", {
                channel: r.channel,
                checkpoints_unique: r.classic_pattern.checkpoints_unique,
                data_verified_unique: r.classic_pattern.data_verified_unique,
                missing_checkpoints: r.classic_pattern.missing_checkpoints,
                unverified_data: r.classic_pattern.unverified_data,
                ambiguous_data: r.classic_pattern.ambiguous_data,
                invalid_envelopes: r.classic_pattern.invalid_envelopes,
                pending_data: r.classic_pattern.pending_data,
                window_high_water: r.classic_pattern.window_high_water,
                checkpoint_evictions: r.classic_pattern.checkpoint_evictions,
                finalized: r.classic_pattern.finalized
                  ? text("stress:finalized")
                  : text("stress:provisional"),
              })}
            </p>
          ),
      )}
    </div>
  );
}

function LoadDetails({ report }: { report: StressReport }) {
  const l = report.load;
  if (!l) return null;
  return (
    <p data-testid="stress-observed-load">
      {text("stress:observed_host_load_value", {
        actual_fps_milli: (l.actual_fps_milli / 1000).toFixed(3),
        submitted: l.submitted,
        skipped: l.skipped,
        start_ns: (Math.max(0, l.end_ns - l.start_ns) / 1e9).toFixed(3),
        scheduled_duration_ns: (l.scheduled_duration_ns / 1e9).toFixed(3),
        completed: l.completed
          ? text("stress:completed")
          : text("stress:interrupted"),
      })}
    </p>
  );
}

function ResourceDetails({ report }: { report: StressReport }) {
  if (
    (!report.plan.tracking && !report.plan.queues) ||
    !report.receivers.length
  )
    return null;
  return (
    <details open data-testid="stress-resources">
      <summary>{text("stress:tracking_and_queue_observations")}</summary>
      {report.receivers.map((r) => (
        <div key={r.channel}>
          <p>
            {r.channel}:{" "}
            {r.tracking
              ? text(
                  "stress:sliding_capacity_value_base_value_finalized_through_value_value",
                  {
                    capacity: r.tracking.capacity,
                    base: r.tracking.base,
                    finalized_through: r.tracking.finalized_through,
                    value: r.tracking.finalized
                      ? text("stress:historyFrozen")
                      : text("stress:activeWindow"),
                    unique_retired: r.tracking.unique_retired,
                    finalized_missing_expected:
                      r.tracking.finalized_missing_expected,
                    finalized_missing_submitted:
                      r.tracking.finalized_missing_submitted,
                    late_after_window: r.tracking.late_after_window,
                    future_outside_window: r.tracking.future_outside_window,
                  },
                )
              : text("stress:exact_bitmap")}
          </p>
          {r.queues && (
            <div>
              <p>
                {text("stress:sampled_rx_depth_high_water_value", {
                  observed_high_water: r.queues.observed_high_water,
                  samples: r.queues.samples,
                  p99:
                    r.queues.residence.p99_upper_ns == null
                      ? text("stress:unavailable")
                      : text("stress:value_ms", {
                          value: (
                            r.queues.residence.p99_upper_ns / 1e6
                          ).toFixed(3),
                        }),
                  invalid_timestamps: r.queues.invalid_timestamps,
                })}
              </p>
              <details data-testid="stress-residence-distribution">
                <summary>
                  {text("stress:residence_distribution_value", {
                    channel: r.channel,
                  })}
                </summary>
                <p>
                  {text("stress:valid_value", {
                    samples: r.queues.residence.samples,
                    negative: r.queues.residence.negative,
                    overflow: r.queues.residence.overflow,
                    max_ms: (r.queues.residence.max_ns / 1e6).toFixed(3),
                    p50_upper_ns:
                      r.queues.residence.p50_upper_ns ??
                      text("stress:unknown_2"),
                    p95_upper_ns:
                      r.queues.residence.p95_upper_ns ??
                      text("stress:unknown_2"),
                  })}
                </p>
                <ol>
                  {r.queues.residence.bins.map((n, i) => (
                    <li key={i}>
                      {text("stress:ns_2_value", {
                        limit_ns: 1000 * 2 ** i,
                        samples: n,
                      })}
                    </li>
                  ))}
                </ol>
              </details>
            </div>
          )}
        </div>
      ))}
    </details>
  );
}
