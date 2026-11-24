// SPDX-License-Identifier: AGPL-3.0-or-later
import { useEffect, useRef, useState } from "react";
import { ErrorRecovery } from "./ErrorRecovery";
import { FlowEditor } from "./FlowEditor";
import { api, ApiFailure, type Auth, type Device } from "./api";
import type {
  StressArtifacts,
  StressFlow,
  StressPlan,
  StressReport,
  StressRunSummary,
  StressValidation,
} from "./generated/api";
import { useTranslation } from "./i18n";
import { errorSummary } from "./i18n/errors";
import { LocalizedError, stateText, text } from "./i18n/messages";
import { StressReportView } from "./StressReportView";
import { defaultCriteria, terminal } from "./stressShared";
export function StressPanel({
  auth,
  devices,
}: {
  auth: Auth;
  devices: Device[];
}) {
  useTranslation();
  const [trackingMode, setTrackingMode] = useState("exact");
  const [trackingCapacity, setTrackingCapacity] = useState(4096);
  const [queueEnabled, setQueueEnabled] = useState(false);
  const [queueEvery, setQueueEvery] = useState(100);
  const [flowsText, setFlowsText] = useState("[]");
  const [timingEnabled, setTimingEnabled] = useState(false);
  const [timingEvery, setTimingEvery] = useState(100);
  const [timingPending, setTimingPending] = useState(4096);
  const [busLabel, setBusLabel] = useState("");
  const [wiring, setWiring] = useState("");
  const [aliases, setAliases] = useState<Record<string, string>>({});
  const [criteria, setCriteria] = useState(defaultCriteria);
  const [fd, setFd] = useState(false);
  const [pattern, setPattern] = useState(false);
  const [crc, setCrc] = useState(false);
  const [crcRunUuid, setCrcRunUuid] = useState<string>(() =>
    crypto.randomUUID(),
  );
  const [crcSource, setCrcSource] = useState(0);
  const [checkpointId, setCheckpointId] = useState("124");
  const [patternWindow, setPatternWindow] = useState(256);
  const [dataBitrate, setDataBitrate] = useState(2000000);
  const [lengths, setLengths] = useState("32,48,64");
  const [brsMode, setBrsMode] = useState("mixed");
  const [rttEnabled, setRttEnabled] = useState(false);
  const [responseId, setResponseId] = useState("124");
  const [rttTimeout, setRttTimeout] = useState(100);
  const [sender, setSender] = useState("virtual:0");
  const [receivers, setReceivers] = useState(["virtual:1"]);
  const [kind, setKind] = useState("fixed");
  const [shapeKind, setShapeKind] = useState("uniform");
  const [burstFrames, setBurstFrames] = useState(8);
  const [intervalSeed, setIntervalSeed] = useState(42);
  const [jitterPpm, setJitterPpm] = useState(500000);
  const [exploreSeconds, setExploreSeconds] = useState(5);
  const [measureSeconds, setMeasureSeconds] = useState(30);
  const [minObservedPpm, setMinObservedPpm] = useState(950000);
  const [fps, setFps] = useState(100);
  const [count, setCount] = useState(100);
  const [rates, setRates] = useState("100,500,1000");
  const [seconds, setSeconds] = useState(60);
  const [warmup, setWarmup] = useState(2);
  const [bitrate, setBitrate] = useState(500000);
  const [canId, setCanId] = useState("123");
  const [ack, setAck] = useState(true);
  const [disk, setDisk] = useState(256);
  const [rssLimit, setRssLimit] = useState(0);
  const [rssWarmup, setRssWarmup] = useState(60);
  const [sample, setSample] = useState(0);
  const [tag, setTag] = useState(
    () => crypto.getRandomValues(new Uint32Array(1))[0],
  );
  const [active, setActive] = useState(false);
  const [checkedPlan, setCheckedPlan] = useState<{
    signature: string;
    flowsText: string;
    validation: StressValidation;
  } | null>(null);
  const [runs, setRuns] = useState<StressRunSummary[]>([]);
  const [selected, setSelected] = useState("");
  const [report, setReport] = useState<StressReport | null>(null);
  const [artifacts, setArtifacts] = useState<StressArtifacts | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [pending, setPending] = useState<string | null>(null);
  const [scope, setScope] = useState("");
  const selectedRef = useRef(selected);
  selectedRef.current = selected;
  function applyTemplate(template: "check" | "staircase" | "soak") {
    setKind(template === "check" ? "fixed" : template);
    setFps(100);
    setCount(100);
    setRates("100,500,1000");
    setSeconds(60);
    setWarmup(2);
    setBitrate(500000);
    setCanId("123");
    setFd(false);
    setPattern(false);
    setCrc(false);
    setRttEnabled(false);
    setTrackingMode("exact");
    setQueueEnabled(false);
    setTimingEnabled(false);
    setFlowsText("[]");
    setCriteria(defaultCriteria);
    setShapeKind("uniform");
    setDisk(256);
    setRssLimit(0);
    setSample(0);
    setAck(true);
    setActive(false);
    setCheckedPlan(null);
  }
  const declaredAliases = [sender, ...receivers]
    .filter((channel) => aliases[channel]?.trim())
    .map((channel) => ({ channel, alias: aliases[channel].trim() }));
  let flows: StressFlow[] = [];
  let flowError: unknown = null;
  try {
    const parsed: unknown = JSON.parse(flowsText);
    if (
      !Array.isArray(parsed) ||
      parsed.some((v) => !v || typeof v !== "object" || Array.isArray(v))
    )
      throw new LocalizedError(
        "stress:additional_flows_must_be_a_json_array_of_objects",
      );
    if (
      parsed.some(
        (value) =>
          typeof value.name !== "string" ||
          typeof value.sender !== "string" ||
          !Number.isInteger(value.can_id) ||
          !Array.isArray(value.receivers) ||
          value.receivers.some((id: unknown) => typeof id !== "string") ||
          (value.stages !== undefined && !Array.isArray(value.stages)),
      )
    )
      throw new LocalizedError(
        "stress:each_flow_needs_a_name_sender_numeric_can_id",
      );
    flows = parsed as StressFlow[];
  } catch (e) {
    flowError = e;
  }
  const patternActive =
    pattern || flows.some((f) => f.traffic?.format === "classic_pattern_v1");
  const plan: StressPlan = {
    ...(trackingMode === "sliding"
      ? { tracking: { format: "sliding" as const, capacity: trackingCapacity } }
      : {}),
    ...(queueEnabled ? { queues: { sample_every: queueEvery } } : {}),
    ...(kind === "adaptive"
      ? {
          adaptive: {
            rates: rates.split(",").map(Number),
            exploration_seconds: exploreSeconds,
            measurement_seconds: measureSeconds,
            min_observed_ppm: minObservedPpm,
          },
        }
      : {}),
    ...(shapeKind === "burst"
      ? { shape: { format: "burst" as const, frames: burstFrames } }
      : shapeKind === "seeded"
        ? {
            shape: {
              format: "seeded_intervals" as const,
              seed: intervalSeed,
              jitter_ppm: jitterPpm,
            },
          }
        : {}),
    ...(flows.length ? { flows } : {}),
    ...(timingEnabled
      ? { timing: { sample_every: timingEvery, max_pending: timingPending } }
      : {}),
    ...(busLabel || wiring || declaredAliases.length
      ? { topology: { bus: busLabel, wiring, aliases: declaredAliases } }
      : {}),
    criteria,
    ...(crc
      ? {
          traffic: {
            format: "classic_crc32_v1" as const,
            run_uuid: crcRunUuid,
            source: crcSource,
          },
        }
      : {}),
    ...(pattern
      ? {
          traffic: {
            format: "classic_pattern_v1" as const,
            checkpoint_can_id: /^[0-9a-f]+$/i.test(checkpointId)
              ? parseInt(checkpointId, 16)
              : -1,
            seed: tag,
            stream: 1,
            max_pending: patternWindow,
          },
        }
      : {}),
    ...(fd
      ? {
          traffic: {
            format: "fd_v1" as const,
            data_bitrate: dataBitrate,
            stream: 1,
            epoch: tag || 1,
            seed: tag,
            variants: lengths.split(",").map((n, i) => ({
              length: Number(n),
              brs: brsMode === "on" || (brsMode === "mixed" && i % 2 === 1),
            })),
          },
        }
      : {}),
    ...(fd && rttEnabled
      ? {
          rtt: {
            role: "loopback" as const,
            response_can_id: /^[0-9a-f]+$/i.test(responseId)
              ? parseInt(responseId, 16)
              : -1,
            timeout_ms: rttTimeout,
            max_inflight: 128,
          },
        }
      : {}),
    sender,
    receivers,
    can_id: /^[0-9a-f]+$/i.test(canId) ? parseInt(canId, 16) : -1,
    extended: false,
    run_tag: tag,
    bitrate,
    receivers_active: ack,
    drain_ms: 200,
    count: kind === "fixed" ? count : 0,
    fps: kind === "fixed" ? fps : 0,
    stages:
      kind === "fixed" || kind === "adaptive"
        ? []
        : (kind === "soak" ? [fps] : rates.split(",").map(Number)).map(
            (rate, index) => ({
              name: `${kind}-${index + 1}`,
              fps: rate,
              duration_seconds: seconds,
              warmup_seconds: warmup,
            }),
          ),
    ...(rssLimit > 0
      ? {
          stability: {
            max_rss_growth_bytes: rssLimit * 1048576,
            warmup_seconds: rssWarmup,
          },
        }
      : {}),
    evidence: {
      max_bytes: disk * 1024 * 1024,
      segment_bytes: 4 * 1024 * 1024,
      sample_every: patternActive ? 1 : sample,
    },
  };
  const signature = JSON.stringify(plan);
  // Invalidate during render, rather than allowing a deferred effect to clear
  // a newer response or briefly authorize a plan edited since validation.
  const validation =
    checkedPlan?.signature === signature && checkedPlan.flowsText === flowsText
      ? checkedPlan.validation
      : null;
  if (checkedPlan && !validation) setCheckedPlan(null);
  useEffect(() => {
    let live = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    void (async () => {
      try {
        const version = await api("version", "/version");
        if (!live) return;
        const key = `canharbor:stress:${version.server_instance_id}:${auth.client_id}`;
        setScope(key);
        try {
          setPending(sessionStorage.getItem(key));
        } catch {
          /* in-memory only */
        }
      } catch (e) {
        if (live) setError(e);
      }
    })();
    const poll = async () => {
      try {
        const list = await api("stress_runs", "/stress/runs");
        if (!live) return;
        setRuns(list.runs);
        const id =
          selectedRef.current ||
          list.runs.find((r) => !terminal(r.state))?.id ||
          list.runs.at(-1)?.id;
        if (id) {
          if (!selectedRef.current) {
            selectedRef.current = id;
            setSelected(id);
          }
          const value = await api(
            "stress_report",
            `/stress/runs/${encodeURIComponent(id)}`,
          );
          if (!live || selectedRef.current !== id) return;
          setReport(value);
        }
      } catch (e) {
        if (live) setError(e);
      }
      if (live) timer = setTimeout(poll, 500);
    };
    void poll();
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [auth]);
  useEffect(() => {
    setArtifacts(null);
    if (!report || !terminal(report.state)) return;
    let live = true;
    void api("stress_artifacts", `/stress/runs/${report.id}/artifacts`)
      .then((value) => {
        if (live) setArtifacts(value);
      })
      .catch((e) => {
        if (live) setError(e);
      });
    return () => {
      live = false;
    };
  }, [report?.id, report?.state]);
  function addReverse() {
    if (plan.adaptive || flowError || !receivers.length || flows.length >= 7)
      return;
    const used = new Set([
      plan.can_id,
      ...(plan.traffic?.format === "classic_pattern_v1"
        ? [plan.traffic.checkpoint_can_id]
        : []),
      ...(plan.rtt ? [plan.rtt.response_can_id] : []),
      ...flows.flatMap((f) => [
        f.can_id,
        ...(f.traffic?.format === "classic_pattern_v1"
          ? [f.traffic.checkpoint_can_id]
          : []),
        ...(f.rtt ? [f.rtt.response_can_id] : []),
      ]),
    ]);
    let id = (plan.can_id + 1) & 0x7ff;
    while (used.has(id)) id = (id + 1) & 0x7ff;
    let runTag: number;
    do {
      runTag = crypto.getRandomValues(new Uint32Array(1))[0];
    } while (runTag === tag || flows.some((f) => f.run_tag === runTag));
    const usedNames = new Set(flows.map((f) => f.name));
    let index = flows.length + 1;
    while (usedNames.has(`reverse-${index}`)) index++;
    let traffic = plan.traffic;
    if (traffic?.format === "classic_crc32_v1") {
      const crcUuid = traffic.run_uuid;
      const usedSources = new Set(
        [plan, ...flows].flatMap((f) =>
          f.traffic?.format === "classic_crc32_v1" &&
          f.traffic.run_uuid === crcUuid
            ? [f.traffic.source]
            : [],
        ),
      );
      let source = 0;
      while (usedSources.has(source)) source++;
      traffic = { ...traffic, source };
    }
    if (traffic?.format === "classic_pattern_v1") {
      let checkpoint = (id + 1) & 0x7ff;
      while (used.has(checkpoint) || checkpoint === id)
        checkpoint = (checkpoint + 1) & 0x7ff;
      traffic = { ...traffic, checkpoint_can_id: checkpoint };
    }
    const flow: StressFlow = {
      name: `reverse-${index}`,
      sender: receivers[0],
      receivers: [sender],
      can_id: id,
      extended: false,
      run_tag: runTag,
      count: plan.count,
      fps: plan.fps,
      stages: plan.stages,
      ...(plan.tracking ? { tracking: plan.tracking } : {}),
      ...(plan.queues ? { queues: plan.queues } : {}),
      ...(plan.shape ? { shape: plan.shape } : {}),
      criteria: plan.criteria,
      ...(traffic ? { traffic } : {}),
      ...(plan.timing ? { timing: plan.timing } : {}),
    };
    setFlowsText(JSON.stringify([...flows, flow], null, 2));
  }
  async function stopFlow(name: string) {
    if (!report) return;
    try {
      setReport(
        await api(
          "stress_report",
          `/stress/runs/${report.id}/flows/${encodeURIComponent(name)}/stop`,
          "POST",
          {},
          auth,
        ),
      );
    } catch (e) {
      setError(e);
    }
  }
  async function check() {
    if (flowError) {
      setError(flowError);
      return;
    }
    setBusy(true);
    setError("");
    try {
      const validation = await api(
        "stress_validation",
        "/stress/validate",
        "POST",
        plan,
        auth,
      );
      setCheckedPlan({ signature, flowsText, validation });
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  }
  function clearPending() {
    setPending(null);
    try {
      sessionStorage.removeItem(scope);
    } catch {
      /* no credentials saved */
    }
  }
  function renewRunIdentity() {
    setTag(crypto.getRandomValues(new Uint32Array(1))[0]);
    const uuids = new Map([[crcRunUuid, crypto.randomUUID()]]);
    setCrcRunUuid((current) =>
      current === crcRunUuid ? uuids.get(crcRunUuid)! : current,
    );
    if (
      !flowError &&
      flows.some((f) => f.traffic?.format === "classic_crc32_v1")
    ) {
      const next = flows.map((f) => {
        if (f.traffic?.format !== "classic_crc32_v1") return f;
        const prior = f.traffic.run_uuid;
        if (!uuids.has(prior)) uuids.set(prior, crypto.randomUUID());
        return { ...f, traffic: { ...f.traffic, run_uuid: uuids.get(prior)! } };
      });
      setFlowsText((current) =>
        current === flowsText ? JSON.stringify(next, null, 2) : current,
      );
    }
  }
  async function start() {
    if (!validation || !active || pending || !scope) return;
    setBusy(true);
    setError("");
    const request = crypto.randomUUID();
    setPending(request);
    try {
      sessionStorage.setItem(scope, request);
    } catch {
      /* recovery remains in memory */
    }
    try {
      const value = await api(
        "stress_report",
        "/stress/runs",
        "POST",
        {
          client_request_id: request,
          plan_hash: validation.plan_hash,
          active: true,
          plan: validation.plan,
        },
        auth,
      );
      setSelected(value.id);
      selectedRef.current = value.id;
      setReport(value);
      clearPending();
      setActive(false);
      renewRunIdentity();
    } catch (e) {
      if (e instanceof ApiFailure && !e.unknown) {
        clearPending();
        setError(e);
      } else {
        setError(
          new LocalizedError("stress:unknownStart", { detail: String(e) }),
        );
      }
    } finally {
      setBusy(false);
    }
  }
  async function recover() {
    if (!pending) return;
    setBusy(true);
    setError("");
    try {
      const value = await api(
        "stress_report",
        `/stress/requests/${encodeURIComponent(pending)}`,
      );
      setSelected(value.id);
      selectedRef.current = value.id;
      setReport(value);
      clearPending();
      setActive(false);
      renewRunIdentity();
    } catch (e) {
      setError(
        new LocalizedError("stress:unknownRecovery", { detail: String(e) }),
      );
    } finally {
      setBusy(false);
    }
  }
  async function refreshStatus() {
    setBusy(true);
    try {
      const list = await api("stress_runs", "/stress/runs");
      setRuns(list.runs);
      if (selectedRef.current)
        setReport(
          await api(
            "stress_report",
            `/stress/runs/${encodeURIComponent(selectedRef.current)}`,
          ),
        );
      setError(null);
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  }
  async function stop() {
    if (!report) return;
    setBusy(true);
    setError("");
    try {
      setReport(
        await api(
          "stress_report",
          `/stress/runs/${report.id}/stop`,
          "POST",
          {},
          auth,
        ),
      );
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  }
  const channels = devices.flatMap((d) =>
    d.channels.map((c) => ({ ...c, simulated: d.simulated })),
  );
  return (
    <section
      id="stress"
      className="stress-panel"
      aria-label={text("stress:stress_testing")}
    >
      <div className="panel-heading">
        <div>
          <div className="section-label">
            {text("stress:stress_classic_can")}
          </div>
          <h2>{text("stress:stress_testing")}</h2>
          <p>
            {text(
              "stress:service_owned_execution_10_ms_scheduling_quantum_peer_rx",
            )}
          </p>
        </div>
      </div>
      <p>
        {text(
          "stress:use_an_isolated_test_bus_normal_receivers_participate_in",
        )}
      </p>
      <fieldset disabled={busy}>
        <legend>{text("stress:quick_setup")}</legend>
        <p>
          {text(
            "stress:templates_replace_the_current_plan_settings_review_channels_and",
          )}
        </p>
        <button onClick={() => applyTemplate("check")}>
          {text("stress:short_receive_check")}
        </button>
        <button onClick={() => applyTemplate("staircase")}>
          {text("stress:rate_staircase")}
        </button>
        <button onClick={() => applyTemplate("soak")}>
          {text("stress:one_minute_soak")}
        </button>
      </fieldset>
      <fieldset disabled={busy}>
        <legend>{text("stress:test_plan")}</legend>
        <div className="stress-grid">
          <label>
            {text("stress:stress_sender")}
            <select
              aria-label={text("stress:stress_sender")}
              value={sender}
              onChange={(e) => {
                const value = e.target.value;
                setSender(value);
                setReceivers((current) => current.filter((id) => id !== value));
              }}
            >
              {channels.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.label} ·{" "}
                  {c.simulated
                    ? text("stress:simulated")
                    : text("stress:hardware")}
                </option>
              ))}
            </select>
          </label>
          <label>
            {text("stress:workload")}
            <select
              aria-label={text("stress:stress_workload")}
              value={kind}
              onChange={(e) => setKind(e.target.value)}
            >
              <option value="fixed">{text("stress:fixed_frame_count")}</option>
              <option value="staircase">{text("stress:staircase")}</option>
              <option value="soak">{text("stress:long_duration_soak")}</option>
              <option value="adaptive">
                {text("stress:predeclared_adaptive_exploration")}
              </option>
            </select>
          </label>
          {kind !== "adaptive" && (
            <label>
              {text("stress:target_frames_s")}
              <input
                aria-label={text("stress:stress_target_fps")}
                type="number"
                min="1"
                max="20000"
                value={fps}
                onChange={(e) => setFps(Number(e.target.value))}
              />
            </label>
          )}
          {kind === "adaptive" ? (
            <>
              <label>
                {text("stress:candidate_rates_increasing_physical_frames_s")}
                <input
                  aria-label={text("stress:stress_adaptive_rates")}
                  value={rates}
                  onChange={(e) => setRates(e.target.value)}
                />
              </label>
              <label>
                {text("stress:exploration_seconds_per_candidate")}
                <input
                  aria-label={text("stress:stress_adaptive_explore_seconds")}
                  type="number"
                  min="1"
                  value={exploreSeconds}
                  onChange={(e) => setExploreSeconds(Number(e.target.value))}
                />
              </label>
              <label>
                {text("stress:fixed_measurement_seconds")}
                <input
                  aria-label={text("stress:stress_adaptive_measure_seconds")}
                  type="number"
                  min="1"
                  value={measureSeconds}
                  onChange={(e) => setMeasureSeconds(Number(e.target.value))}
                />
              </label>
              <label>
                {text("stress:minimum_observed_host_rate_ppm")}
                <input
                  aria-label={text("stress:stress_adaptive_observed_ppm")}
                  type="number"
                  min="1"
                  max="1000000"
                  value={minObservedPpm}
                  onChange={(e) => setMinObservedPpm(Number(e.target.value))}
                />
              </label>
              <p>
                {text(
                  "stress:all_candidates_and_criteria_are_frozen_before_authorization_exploration",
                )}
              </p>
            </>
          ) : kind === "fixed" ? (
            <label>
              {text("stress:frame_count")}
              <input
                aria-label={text("stress:stress_frame_count")}
                type="number"
                min="1"
                value={count}
                onChange={(e) => setCount(Number(e.target.value))}
              />
            </label>
          ) : (
            <>
              {kind === "staircase" && (
                <label>
                  {text("stress:stage_rates_frames_s")}
                  <input
                    aria-label={text("stress:stress_stage_rates")}
                    value={rates}
                    onChange={(e) => setRates(e.target.value)}
                  />
                </label>
              )}
              <label>
                {text("stress:seconds_per_measurement_stage")}
                <input
                  aria-label={text("stress:stress_stage_seconds")}
                  type="number"
                  min="1"
                  value={seconds}
                  onChange={(e) => setSeconds(Number(e.target.value))}
                />
              </label>
              <label>
                {text("stress:warmup_seconds_per_stage")}
                <input
                  aria-label={text("stress:stress_warmup_seconds")}
                  type="number"
                  min="0"
                  value={warmup}
                  onChange={(e) => setWarmup(Number(e.target.value))}
                />
              </label>
            </>
          )}
          <label>
            {text("stress:load_shape")}
            <select
              aria-label={text("stress:stress_load_shape")}
              value={shapeKind}
              onChange={(e) => setShapeKind(e.target.value)}
            >
              <option value="uniform">{text("stress:uniform")}</option>
              <option value="burst">{text("stress:bounded_burst")}</option>
              <option value="seeded">
                {text("stress:fixed_seed_intervals")}
              </option>
            </select>
          </label>
          {shapeKind === "burst" && (
            <label>
              {text("stress:maximum_physical_frames_per_burst")}
              <input
                aria-label={text("stress:stress_burst_frames")}
                type="number"
                min="1"
                max="200"
                value={burstFrames}
                onChange={(e) => setBurstFrames(Number(e.target.value))}
              />
            </label>
          )}
          {shapeKind === "seeded" && (
            <>
              <label>
                {text("stress:interval_seed")}
                <input
                  aria-label={text("stress:stress_interval_seed")}
                  type="number"
                  min="0"
                  max="4294967295"
                  value={intervalSeed}
                  onChange={(e) => setIntervalSeed(Number(e.target.value))}
                />
              </label>
              <label>
                {text("stress:maximum_slot_jitter_ppm")}
                <input
                  aria-label={text("stress:stress_interval_jitter")}
                  type="number"
                  min="1"
                  max="500000"
                  value={jitterPpm}
                  onChange={(e) => setJitterPpm(Number(e.target.value))}
                />
              </label>
            </>
          )}
          {shapeKind !== "uniform" && (
            <p>
              {text("stress:shapes_use_a_10_ms_host_scheduler_quantum_burst")}
            </p>
          )}
          <label>
            {text("stress:frame_protocol")}
            <select
              aria-label={text("stress:stress_protocol")}
              value={crc ? "crc" : pattern ? "pattern" : fd ? "fd" : "classic"}
              onChange={(e) => {
                setFd(e.target.value === "fd");
                setPattern(e.target.value === "pattern");
                setCrc(e.target.value === "crc");
              }}
            >
              <option value="classic">{text("stress:classic_sequence")}</option>
              <option value="crc">{text("stress:classic_crc32_v1")}</option>
              <option value="pattern">
                {text("stress:classic_pattern_v1_checkpoint")}
              </option>
              <option value="fd">
                {text("stress:can_fd_crc_mixed_lengths")}
              </option>
            </select>
          </label>
          {crc && (
            <>
              <label>
                {text("stress:crc_run_uuid")}
                <input
                  aria-label={text("stress:crc_run_uuid")}
                  value={crcRunUuid}
                  onChange={(e) => setCrcRunUuid(e.target.value)}
                />
              </label>
              <label>
                {text("stress:crc_source")}
                <input
                  aria-label={text("stress:crc_source")}
                  type="number"
                  min="0"
                  max="4294967295"
                  value={crcSource}
                  onChange={(e) => setCrcSource(Number(e.target.value))}
                />
              </label>
              <p>{text("stress:crc_wire_scope")}</p>
            </>
          )}
          {pattern && (
            <>
              <label>
                {text("stress:checkpoint_can_id_hex")}
                <input
                  aria-label={text("stress:stress_checkpoint_can_id")}
                  value={checkpointId}
                  onChange={(e) => setCheckpointId(e.target.value)}
                />
              </label>
              <label>
                {text("stress:correlation_window")}
                <input
                  aria-label={text("stress:stress_pattern_window")}
                  type="number"
                  min="1"
                  max="4096"
                  value={patternWindow}
                  onChange={(e) => setPatternWindow(Number(e.target.value))}
                />
              </label>
              <p>
                {text(
                  "stress:count_and_rate_include_both_checkpoint_and_data_frames",
                )}
              </p>
            </>
          )}
          {fd && (
            <>
              <label>
                {text("stress:fd_data_bitrate")}
                <input
                  aria-label={text("stress:stress_data_bitrate")}
                  type="number"
                  value={dataBitrate}
                  onChange={(e) => setDataBitrate(Number(e.target.value))}
                />
              </label>
              <label>
                {text("stress:fd_lengths_bytes_comma_separated")}
                <input
                  aria-label={text("stress:stress_fd_lengths")}
                  value={lengths}
                  onChange={(e) => setLengths(e.target.value)}
                />
              </label>
              <label>
                {text("stress:fd_bitrate_switching")}
                <select
                  aria-label={text("stress:stress_brs")}
                  value={brsMode}
                  onChange={(e) => setBrsMode(e.target.value)}
                >
                  <option value="mixed">
                    {text("stress:alternate_off_on")}
                  </option>
                  <option value="on">{text("stress:all_on")}</option>
                  <option value="off">{text("stress:all_off")}</option>
                </select>
              </label>
              <label>
                {text("stress:enable_explicit_rtt_responder")}
                <input
                  aria-label={text("stress:stress_rtt")}
                  type="checkbox"
                  checked={rttEnabled}
                  onChange={(e) => setRttEnabled(e.target.checked)}
                />
              </label>
              {rttEnabled && (
                <>
                  <label>
                    {text("stress:rtt_response_id_hex")}
                    <input
                      aria-label={text("stress:stress_response_id")}
                      value={responseId}
                      onChange={(e) => setResponseId(e.target.value)}
                    />
                  </label>
                  <label>
                    {text("stress:rtt_timeout_ms")}
                    <input
                      aria-label={text("stress:stress_rtt_timeout")}
                      type="number"
                      value={rttTimeout}
                      onChange={(e) => setRttTimeout(Number(e.target.value))}
                    />
                  </label>
                </>
              )}
            </>
          )}
          <label>
            {text("stress:nominal_bitrate_bit_s")}
            <input
              aria-label={text("stress:stress_bitrate")}
              type="number"
              value={bitrate}
              onChange={(e) => setBitrate(Number(e.target.value))}
            />
          </label>
          <label>
            {text("stress:standard_can_id_hex")}
            <input
              aria-label={text("stress:stress_can_id")}
              value={canId}
              onChange={(e) => setCanId(e.target.value)}
            />
          </label>
          <label>
            {text("stress:evidence_reservation_mib")}
            <select
              aria-label={text("stress:stress_evidence_budget")}
              value={disk}
              onChange={(e) => setDisk(Number(e.target.value))}
            >
              {[64, 256, 512, 1024, 2048].map((n) => (
                <option key={n}>{n}</option>
              ))}
            </select>
          </label>
          <label>
            {text("stress:sample_every_n_matching_frames_0_off")}
            <input
              aria-label={text("stress:stress_sample_interval")}
              type="number"
              min="0"
              value={patternActive ? 1 : sample}
              disabled={patternActive}
              onChange={(e) => setSample(Number(e.target.value))}
            />
          </label>
          <label>
            {text("stress:maximum_process_rss_growth_mib_0_off")}
            <input
              aria-label={text("stress:stress_rss_growth_limit")}
              type="number"
              min="0"
              value={rssLimit}
              onChange={(e) => setRssLimit(Number(e.target.value))}
            />
          </label>
          <label>
            {text("stress:rss_baseline_warmup_seconds")}
            <input
              aria-label={text("stress:stress_rss_warmup")}
              type="number"
              min="0"
              value={rssWarmup}
              onChange={(e) => setRssWarmup(Number(e.target.value))}
            />
          </label>
        </div>
        <details>
          <summary>{text("stress:advanced_stress_settings")}</summary>
          <p>{text("stress:these_values_remain_part_of_the_plan_when_this")}</p>
          <fieldset>
            <legend>
              {text("stress:acceptance_criteria_frozen_when_validated")}
            </legend>
            <p>
              {text(
                "stress:ratios_use_integer_ppm_1000000_100_submission_ratio_applies",
              )}
            </p>
            <div className="stress-grid">
              {(
                [
                  [
                    "min_submitted_ppm",
                    text("stress:minimum_scheduled_submission_ratio_ppm"),
                  ],
                  [
                    "max_scheduler_skipped",
                    text("stress:maximum_scheduler_skipped"),
                  ],
                  [
                    "max_missing_expected_ppm",
                    text("stress:maximum_planned_rx_missing_ppm"),
                  ],
                  [
                    "max_missing_submitted_ppm",
                    text("stress:maximum_submitted_rx_missing_ppm"),
                  ],
                  [
                    "max_duplicates",
                    text("stress:maximum_duplicates_per_receiver"),
                  ],
                  [
                    "max_reordered",
                    text("stress:maximum_reordered_per_receiver"),
                  ],
                  [
                    "max_payload_errors",
                    text("stress:maximum_payload_errors_per_receiver"),
                  ],
                  [
                    "max_out_of_range",
                    text("stress:maximum_out_of_range_per_receiver"),
                  ],
                ] as const
              ).map(([key, label]) => (
                <label key={key}>
                  {label}
                  <input
                    aria-label={text("stress:stress_criterion_value", {
                      key: key,
                    })}
                    type="number"
                    min={key === "min_submitted_ppm" ? 1 : 0}
                    max={
                      key === "min_submitted_ppm"
                        ? 1000000
                        : key.endsWith("ppm")
                          ? 999999
                          : 4294967295
                    }
                    step="1"
                    value={criteria[key]}
                    onChange={(e) =>
                      setCriteria({
                        ...criteria,
                        [key]: Number(e.target.value),
                      })
                    }
                  />
                </label>
              ))}
              <label>
                {text("stress:maximum_dispatch_lateness_ms_blank_no_limit")}
                <input
                  aria-label={text("stress:stress_criterion_dispatch_lateness")}
                  type="number"
                  min="0"
                  max="259200000"
                  step="1"
                  value={
                    criteria.max_dispatch_lateness_ns == null
                      ? ""
                      : criteria.max_dispatch_lateness_ns / 1e6
                  }
                  onChange={(e) =>
                    setCriteria({
                      ...criteria,
                      max_dispatch_lateness_ns:
                        e.target.value === ""
                          ? null
                          : Number(e.target.value) * 1e6,
                    })
                  }
                />
              </label>
            </div>
          </fieldset>
          <fieldset>
            <legend>{text("stress:tracking_and_receive_queues")}</legend>
            <label>
              {text("stress:receive_tracking")}{" "}
              <select
                aria-label={text("stress:stress_tracking_mode")}
                value={trackingMode}
                onChange={(e) => setTrackingMode(e.target.value)}
              >
                <option value="exact">
                  {text("stress:exact_bitmap_finite_plan")}
                </option>
                <option value="sliding">{text("stress:sliding_window")}</option>
              </select>
            </label>
            {trackingMode === "sliding" && (
              <label>
                {text("stress:window_slots")}{" "}
                <input
                  aria-label={text("stress:stress_tracking_capacity")}
                  type="number"
                  min="64"
                  max="1048576"
                  value={trackingCapacity}
                  onChange={(e) => setTrackingCapacity(Number(e.target.value))}
                />
              </label>
            )}
            <p>{text("stress:window_slots_must_be_a_power_of_two_evicted")}</p>
            <label>
              <input
                aria-label={text("stress:enable_stress_queue_sampling")}
                type="checkbox"
                checked={queueEnabled}
                onChange={(e) => setQueueEnabled(e.target.checked)}
              />{" "}
              {text("stress:sample_receive_queue_depth_and_residence")}
            </label>
            {queueEnabled && (
              <label>
                {text("stress:every_eligible_rx")}{" "}
                <input
                  aria-label={text("stress:stress_queue_sample_interval")}
                  type="number"
                  min="1"
                  value={queueEvery}
                  onChange={(e) => setQueueEvery(Number(e.target.value))}
                />
              </label>
            )}
            <p>
              {text(
                "stress:observed_depth_is_a_sampled_lower_bound_including_other",
              )}
            </p>
          </fieldset>
          <fieldset>
            <legend>{text("stress:host_timing_and_source_evidence")}</legend>
            <label className="stress-check">
              <input
                aria-label={text("stress:enable_stress_host_timing")}
                type="checkbox"
                checked={timingEnabled}
                onChange={(e) => setTimingEnabled(e.target.checked)}
              />
              {text(
                "stress:record_sampled_host_timing_additional_evidence_budget",
              )}
            </label>
            {timingEnabled && (
              <div className="stress-grid">
                <label>
                  {text("stress:measure_every_nth_sequence")}
                  <input
                    aria-label={text("stress:stress_timing_sample_interval")}
                    type="number"
                    min="1"
                    value={timingEvery}
                    onChange={(e) => setTimingEvery(Number(e.target.value))}
                  />
                </label>
                <label>
                  {text("stress:maximum_pending_timing_records")}
                  <input
                    aria-label={text("stress:stress_timing_window")}
                    type="number"
                    min="1"
                    max="4096"
                    value={timingPending}
                    onChange={(e) => setTimingPending(Number(e.target.value))}
                  />
                </label>
              </div>
            )}
            <p>
              {text(
                "stress:host_rx_minus_backend_call_completion_may_be_negative",
              )}
            </p>
            <div className="stress-grid">
              <label>
                {text("stress:declared_bus_label")}
                <input
                  aria-label={text("stress:stress_declared_bus")}
                  maxLength={128}
                  value={busLabel}
                  onChange={(e) => setBusLabel(e.target.value)}
                />
              </label>
              <label>
                {text("stress:declared_wiring_and_termination")}
                <input
                  aria-label={text("stress:stress_declared_wiring")}
                  maxLength={1024}
                  value={wiring}
                  onChange={(e) => setWiring(e.target.value)}
                />
              </label>
              {[sender, ...receivers].map((channel) => (
                <label key={channel}>
                  {text("stress:role_alias_value", {
                    channel: channel,
                  })}
                  <input
                    aria-label={text("stress:stress_role_alias_value", {
                      channel: channel,
                    })}
                    maxLength={64}
                    value={aliases[channel] ?? ""}
                    onChange={(e) =>
                      setAliases({ ...aliases, [channel]: e.target.value })
                    }
                  />
                </label>
              ))}
            </div>
            <p>
              {text("stress:declarations_are_saved_with_the_plan_they_are_not")}
            </p>
          </fieldset>
          <fieldset>
            <legend>{text("stress:concurrent_flows_on_this_runtime")}</legend>
            <p>
              {text("stress:the_form_defines_primary_add_up_to_seven_explicit")}
            </p>
            <button
              disabled={
                !!plan.adaptive ||
                !!flowError ||
                !receivers.length ||
                flows.length >= 7
              }
              onClick={addReverse}
            >
              {text("stress:add_reverse_flow")}
            </button>
            {!flowError &&
              flows.map((flow, index) => (
                <FlowEditor
                  key={index}
                  flow={flow}
                  channels={channels}
                  onChange={(value) =>
                    setFlowsText(
                      JSON.stringify(
                        flows.map((item, i) => (i === index ? value : item)),
                        null,
                        2,
                      ),
                    )
                  }
                  onRemove={() =>
                    setFlowsText(
                      JSON.stringify(
                        flows.filter((_, i) => i !== index),
                        null,
                        2,
                      ),
                    )
                  }
                />
              ))}
            <details>
              <summary>{text("stress:advanced_json")}</summary>
              <label>
                {text("stress:additional_flows_json")}
                <textarea
                  aria-label={text("stress:stress_additional_flows")}
                  rows={12}
                  maxLength={65536}
                  value={flowsText}
                  onChange={(e) => setFlowsText(e.target.value)}
                  spellCheck={false}
                />
              </label>
            </details>
            {!!flowError && <p role="alert">{errorSummary(flowError)}</p>}
            <p>{text("stress:each_flow_freezes_its_own_count_rate_id_tag")}</p>
          </fieldset>
        </details>
        <fieldset>
          <legend>{text("stress:expected_receivers")}</legend>
          {channels
            .filter((c) => c.id !== sender)
            .map((c) => (
              <label key={c.id} className="stress-check">
                <input
                  type="checkbox"
                  checked={receivers.includes(c.id)}
                  onChange={(e) =>
                    setReceivers(
                      e.target.checked
                        ? [...receivers, c.id]
                        : receivers.filter((id) => id !== c.id),
                    )
                  }
                />
                {c.label}
              </label>
            ))}
        </fieldset>
        <label className="stress-check">
          <input
            type="checkbox"
            checked={ack}
            onChange={(e) => setAck(e.target.checked)}
          />
          {text(
            "stress:normal_receiver_mode_participates_in_can_ack_no_generated",
          )}
        </label>
        <button onClick={() => void check()}>
          {text("stress:validate_stress_plan")}
        </button>
        {validation && (
          <p data-testid="stress-validation">
            {text("stress:plan_validated_tracking_value", {
              tracking_bytes: (validation.tracking_bytes / 1048576).toFixed(2),
              memory_limit_bytes: (
                validation.memory_limit_bytes / 1048576
              ).toFixed(0),
              disk: disk,
              max_fps: validation.max_fps,
            })}
          </p>
        )}
        {validation?.load_budget && (
          <p data-testid="stress-load-budget">
            {text("stress:frozen_reservation_up_to_value", {
              reserved_planned: validation.load_budget.reserved_planned,
              reserved_peak_fps: validation.load_budget.reserved_peak_fps,
              maximum_quantum_frames:
                validation.load_budget.maximum_quantum_frames,
              estimated_evidence_bytes: (
                validation.load_budget.estimated_evidence_bytes / 1048576
              ).toFixed(2),
            })}
          </p>
        )}
        {validation?.group_budget && (
          <p data-testid="stress-group-budget">
            {text("stress:group_value", {
              flow_count: validation.group_budget.flow_count,
              channels: validation.group_budget.channels.length,
              devices: validation.group_budget.devices.length,
              peak_transmit_fps: validation.group_budget.peak_transmit_fps,
              maximum_inflight: validation.group_budget.maximum_inflight,
              memory_reserve_bytes: (
                (validation.group_budget.tracking_bytes +
                  validation.group_budget.memory_reserve_bytes) /
                1048576
              ).toFixed(1),
              estimated_evidence_bytes: (
                validation.group_budget.estimated_evidence_bytes / 1048576
              ).toFixed(1),
            })}
          </p>
        )}
        <label className="stress-check">
          <input
            aria-label={text("stress:authorize_stress_transmission")}
            type="checkbox"
            checked={active}
            onChange={(e) => setActive(e.target.checked)}
          />
          {text("stress:i_authorize_transmission_on_the_selected_test_bus")}
        </label>
        <button
          onClick={() => void start()}
          disabled={
            !validation || !active || !!pending || !scope || !!flowError
          }
        >
          {text("stress:start_stress_test")}
        </button>
      </fieldset>
      {pending && (
        <div role="status">
          {text("stress:unresolved_start_request_value", {
            pending: pending,
          })}{" "}
          <button disabled={busy} onClick={() => void recover()}>
            {text("stress:recover_stress_start_result")}
          </button>
        </div>
      )}
      {!!error && (
        <ErrorRecovery
          error={error}
          context="stress"
          busy={busy}
          onDismiss={() => setError(null)}
          onCheck={() => void refreshStatus()}
        />
      )}
      {runs.length > 0 && (
        <label>
          {text("stress:recorded_run")}
          <select
            aria-label={text("stress:stress_run")}
            value={selected}
            onChange={(e) => {
              setSelected(e.target.value);
              selectedRef.current = e.target.value;
              setReport(null);
            }}
          >
            {runs.map((r) => (
              <option key={r.id} value={r.id}>
                {r.id.slice(0, 8)} · {stateText(r.state)} ·{" "}
                {stateText(r.verdict)}
              </option>
            ))}
          </select>
        </label>
      )}
      {report && (
        <StressReportView
          report={report}
          artifacts={artifacts}
          busy={busy}
          stop={stop}
          stopFlow={stopFlow}
        />
      )}
    </section>
  );
}
