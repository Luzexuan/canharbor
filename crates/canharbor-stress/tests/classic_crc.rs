// SPDX-License-Identifier: AGPL-3.0-or-later
use canharbor_core::runtime::Runtime;
use canharbor_stress::*;
use canharbor_virtual::VirtualBus;
use std::{sync::Arc, time::Duration};
fn plan() -> Plan {
    serde_json::from_str(include_str!("../../../examples/stress/classic-crc32.json")).unwrap()
}
async fn end(run: &Run) -> Report {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let r = run.snapshot();
            if matches!(r.state.as_str(), "completed" | "failed" | "aborted")
                || r.state.starts_with("saving_")
            {
                return r;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap()
}
#[tokio::test(start_paused = true)]
async fn two_to_eight_nodes_reconcile_peer_links_and_raw_crc_replay() {
    for nodes in 2..=8u8 {
        let mut p = plan();
        p.count = 4;
        p.fps = 10;
        p.receivers = (1..nodes).map(|n| format!("virtual:{n}")).collect();
        p.flows=(1..nodes).map(|n|serde_json::from_value(serde_json::json!({"name":format!("source-{n}"),"sender":format!("virtual:{n}"),"receivers":(0..nodes).filter(|m|*m!=n).map(|m|format!("virtual:{m}")).collect::<Vec<_>>(),"can_id":291+u32::from(n),"extended":false,"run_tag":17+u32::from(n),"count":4,"fps":10,"traffic":{"format":"classic_crc32_v1","run_uuid":"00112233-4455-4677-8899-aabbccddeeff","source":n}})).unwrap()).collect();
        let runtime = Runtime::new(Arc::new(VirtualBus::with_channel_count(8192, nodes)));
        let (sink, mut rx) = EvidenceSink::channel(8192);
        let run = Executor::new(runtime.clone())
            .start_recorded(
                p.clone(),
                true,
                uuid::Uuid::new_v4().to_string(),
                Some(sink),
                None,
            )
            .unwrap();
        let mut report = end(&run).await;
        assert_eq!(report.verdict, "pass", "{report:?}");
        assert_eq!(report.version, 12);
        let group = report.group.as_ref().unwrap();
        assert_eq!(
            group
                .flows
                .iter()
                .map(|f| f.report.receivers.len())
                .sum::<usize>(),
            usize::from(nodes) * usize::from(nodes - 1)
        );
        for flow in &group.flows {
            assert_eq!(flow.report.integrity, "crc32_ieee_sequence_v1");
            assert!(flow.report.receivers.iter().all(|r| r.unique == 4
                && r.missing_submitted == 0
                && r.crc_errors == 0
                && r.short_sequence_only == 0));
        }
        assert!(runtime.list().await.is_empty());
        report.state = "completed".into();
        let mut replay = Replay::new(&p).unwrap();
        let mut events = vec![];
        while let Ok(event) = rx.try_recv() {
            replay.event(&event).unwrap();
            events.push(event);
        }
        replay.verify_report(&report).unwrap();
        let mut old = report.clone();
        old.version = 11;
        assert!(replay.verify_report(&old).is_err());
        assert!(!old.passing_invariants());
        let mut removed = false;
        let mut damaged = Replay::new(&p).unwrap();
        let mut rejected = false;
        for event in &events {
            if !removed
                && matches!(event,EvidenceEvent::FlowRecord{event,..} if matches!(event.as_ref(),EvidenceEvent::ClassicCrcFrame{..}))
            {
                removed = true;
                continue;
            }
            if damaged.event(event).is_err() {
                rejected = true;
                break;
            }
        }
        assert!(removed);
        assert!(rejected || damaged.verify_report(&report).is_err());
        if nodes == 2 {
            let mut corrupted = events.clone();
            let raw = corrupted
                .iter_mut()
                .find_map(|e| match e {
                    EvidenceEvent::FlowRecord { event, .. } => match event.as_mut() {
                        EvidenceEvent::ClassicCrcFrame { data_hex, .. } => Some(data_hex),
                        _ => None,
                    },
                    _ => None,
                })
                .unwrap();
            let last = raw.pop().unwrap();
            raw.push(if last == '0' { '1' } else { '0' });
            let mut altered = Replay::new(&p).unwrap();
            let rejected = corrupted.iter().any(|e| altered.event(e).is_err());
            assert!(rejected || altered.verify_report(&report).is_err());
        }
    }
}
#[test]
fn duplicate_identity_budgets_and_adaptive_epochs_are_checked() {
    let mut p = plan();
    let mut flow = serde_json::to_value(&p).unwrap();
    for key in ["bitrate", "receivers_active", "drain_ms", "evidence"] {
        flow.as_object_mut().unwrap().remove(key);
    }
    flow["name"] = "reverse".into();
    flow["sender"] = "virtual:1".into();
    flow["receivers"] = serde_json::json!(["virtual:0"]);
    flow["can_id"] = 292.into();
    flow["run_tag"] = 18.into();
    p.flows.push(serde_json::from_value(flow).unwrap());
    assert!(p.compile().unwrap_err().contains("identity"));
    if let Traffic::ClassicCrc32V1 { source, .. } = &mut p.flows[0].traffic {
        *source = 1;
    }
    assert!(p.compile().is_ok());
    p.traffic = Traffic::Classic;
    assert_eq!(
        Report::initial(&p, true, "mixed".into()).unwrap().version,
        12
    );
    let mut p = plan();
    p.fps = 2001;
    assert!(p.compile().is_err());
    p.fps = 0;
    p.count = 0;
    p.adaptive = Some(AdaptiveOptions {
        rates: vec![10, 20],
        exploration_seconds: 1,
        measurement_seconds: 1,
        min_observed_ppm: 1_000_000,
    });
    p.compile().unwrap();
    let steps = p.identity_plans();
    assert_eq!(steps.len(), 3);
    for (index, step) in steps.iter().enumerate() {
        assert_eq!(step.traffic.classic_crc_identity().unwrap().1, 0);
        for previous in &steps[..index] {
            assert_ne!(previous.traffic, step.traffic);
            assert!(step.decode(&previous.frame(0), false).is_err());
        }
    }
    if let Traffic::ClassicCrc32V1 { run_uuid, .. } = &mut p.traffic {
        *run_uuid = "ffffffff-ffff-ffff-ffff-ffffffffffff".into();
    }
    assert!(p.compile().unwrap_err().contains("UUID"));
}

#[test]
fn six_sources_at_600_aggregate_fps_for_600_seconds_keep_raw_budget_gate() {
    let mut p = plan();
    p.count = 60_000;
    p.fps = 100;
    p.receivers = (1..6).map(|n| format!("virtual:{n}")).collect();
    p.evidence.max_bytes = 2 * 1024 * 1024 * 1024;
    p.flows = (1..6)
        .map(|n| {
            let mut flow = serde_json::to_value(&p).unwrap();
            for key in [
                "bitrate",
                "receivers_active",
                "drain_ms",
                "evidence",
                "flows",
            ] {
                flow.as_object_mut().unwrap().remove(key);
            }
            flow["name"] = format!("source-{n}").into();
            flow["sender"] = format!("virtual:{n}").into();
            flow["receivers"] = serde_json::json!(
                (0..6)
                    .filter(|m| *m != n)
                    .map(|m| format!("virtual:{m}"))
                    .collect::<Vec<_>>()
            );
            flow["can_id"] = (291 + n).into();
            flow["run_tag"] = (17 + n).into();
            flow["traffic"]["source"] = n.into();
            serde_json::from_value(flow).unwrap()
        })
        .collect();
    assert!(p.compile().unwrap_err().contains("2000 records/s"));
    p.evidence.sample_every = 0;
    let compiled = p.compile().unwrap();
    assert_eq!(compiled.count, 360_000);
    assert_eq!(compiled.duration_ns, 600_000_000_000);
    assert_eq!(p.group_budget().unwrap().peak_transmit_fps, 600);
}

#[tokio::test(start_paused = true)]
async fn adaptive_crc_epochs_with_timing_and_ordered_tracking_replay() {
    let mut p = plan();
    p.count = 0;
    p.fps = 0;
    p.adaptive = Some(AdaptiveOptions {
        rates: vec![2, 4],
        exploration_seconds: 1,
        measurement_seconds: 1,
        min_observed_ppm: 1_000_000,
    });
    p.timing = Some(TimingOptions {
        sample_every: 1,
        max_pending: 16,
    });
    p.tracking = Tracking::Sliding { capacity: 64 };
    p.queues = Some(QueueOptions { sample_every: 1 });
    let runtime = Runtime::new(Arc::new(VirtualBus::default()));
    let (sink, mut rx) = EvidenceSink::channel(4096);
    let run = Executor::new(runtime.clone())
        .start_recorded(
            p.clone(),
            true,
            uuid::Uuid::new_v4().to_string(),
            Some(sink),
            None,
        )
        .unwrap();
    let mut report = end(&run).await;
    report.state = "completed".into();
    assert_eq!(report.verdict, "pass", "{report:?}");
    assert_eq!(report.version, 12);
    let a = report.adaptive.as_ref().unwrap();
    assert_eq!(a.selected_fps, Some(4));
    assert_eq!(a.steps.len(), 3);
    for step in &a.steps {
        assert_eq!(step.report.version, 12);
        assert_eq!(step.report.integrity, "crc32_ieee_sequence_v1");
        assert_eq!(step.report.receivers[0].unique, step.report.submitted);
        assert_eq!(step.report.receivers[0].short_sequence_only, 0);
    }
    let mut replay = Replay::new(&p).unwrap();
    while let Ok(event) = rx.try_recv() {
        replay.event(&event).unwrap();
    }
    replay.verify_report(&report).unwrap();
    assert!(runtime.list().await.is_empty());
}

#[tokio::test(start_paused = true)]
async fn mixed_legacy_and_crc_reports_keep_leaf_versions_and_parent_v12() {
    let mut p = plan();
    p.count = 2;
    p.fps = 2;
    let mut flow = serde_json::to_value(&p).unwrap();
    for key in [
        "bitrate",
        "receivers_active",
        "drain_ms",
        "evidence",
        "traffic",
    ] {
        flow.as_object_mut().unwrap().remove(key);
    }
    flow["name"] = "legacy".into();
    flow["can_id"] = 292.into();
    flow["run_tag"] = 18.into();
    p.flows.push(serde_json::from_value(flow).unwrap());
    let (sink, mut rx) = EvidenceSink::channel(4096);
    let run = Executor::new(Runtime::new(Arc::new(VirtualBus::default())))
        .start_recorded(
            p.clone(),
            true,
            uuid::Uuid::new_v4().to_string(),
            Some(sink),
            None,
        )
        .unwrap();
    let mut r = end(&run).await;
    r.state = "completed".into();
    assert_eq!(r.verdict, "pass", "{r:?}");
    assert_eq!(r.version, 12);
    for f in &r.group.as_ref().unwrap().flows {
        assert_eq!(
            f.report.version,
            if f.report.plan.traffic.is_classic_crc() {
                12
            } else {
                8
            }
        );
    }
    let mut replay = Replay::new(&p).unwrap();
    while let Ok(event) = rx.try_recv() {
        replay.event(&event).unwrap();
    }
    replay.verify_report(&r).unwrap();
    r.version = 11;
    assert!(!r.passing_invariants());
    assert!(replay.verify_report(&r).is_err());
}
