// SPDX-License-Identifier: AGPL-3.0-or-later
use canharbor_core::runtime::Runtime;
use canharbor_stress::{Plan, Stage, templates::AllToAll};
use canharbor_virtual::VirtualBus;
use std::{collections::HashSet, sync::Arc};

fn options(nodes: usize, rates: &[u32], seconds: u32) -> AllToAll {
    AllToAll {
        channels: (0..nodes).map(|n| format!("virtual:{n}")).collect(),
        aliases: vec![],
        first_can_id: 0x100,
        extended: false,
        first_run_tag: 1,
        bitrate: 500_000,
        stages: rates
            .iter()
            .map(|fps| Stage {
                name: format!("{fps}-fps"),
                fps: *fps,
                duration_seconds: seconds,
                warmup_seconds: 0,
            })
            .collect(),
    }
}

#[test]
fn six_senders_have_thirty_peer_links_and_aggregate_staircase_rates() {
    let rates = [600, 1500, 3000, 4500, 6000];
    let plan = options(6, &rates, 30).generate().unwrap();
    let flows = plan.flow_plans();
    assert_eq!(flows.len(), 6);
    let links = flows
        .iter()
        .flat_map(|(_, flow)| {
            assert_eq!(flow.receivers.len(), 5);
            flow.receivers.iter().map(|receiver| {
                assert_ne!(receiver, &flow.sender);
                (flow.sender.clone(), receiver.clone())
            })
        })
        .collect::<HashSet<_>>();
    assert_eq!(links.len(), 30);
    assert_eq!(
        flows
            .iter()
            .map(|(_, p)| p.can_id)
            .collect::<HashSet<_>>()
            .len(),
        6
    );
    assert_eq!(
        flows
            .iter()
            .map(|(_, p)| p.run_tag)
            .collect::<HashSet<_>>()
            .len(),
        6
    );
    for (index, expected) in rates.into_iter().enumerate() {
        assert_eq!(
            flows.iter().map(|(_, p)| p.stages[index].fps).sum::<u32>(),
            expected
        );
    }
    assert_eq!(plan.compile().unwrap().count, 468_000);
    assert_eq!(plan.group_budget().unwrap().peak_transmit_fps, 6000);
    let runtime = Runtime::new(Arc::new(VirtualBus::with_channel_count(256, 6)));
    plan.validate(&runtime).unwrap();
    let json = serde_json::to_string(&plan).unwrap();
    assert_eq!(serde_json::from_str::<Plan>(&json).unwrap(), plan);
}

#[test]
fn fixed_600_fps_for_600_seconds_is_divided_across_six_senders() {
    let plan = options(6, &[600], 600).generate().unwrap();
    assert_eq!(plan.compile().unwrap().count, 360_000);
    assert_eq!(plan.compile().unwrap().duration_ns, 600_000_000_000);
    for (_, flow) in plan.flow_plans() {
        assert_eq!(flow.stages[0].fps, 100);
        assert_eq!(flow.compile().unwrap().count, 60_000);
    }
}

#[test]
fn two_and_eight_node_bounds_and_extended_identifiers_work() {
    for nodes in [2, 8] {
        let mut options = options(nodes, &[nodes as u32 * 100], 1);
        options.extended = true;
        options.first_can_id = 0x1fff_ffff - nodes as u32 + 1;
        options.aliases = (0..nodes).map(|n| format!("Adapter {n}")).collect();
        let plan = options.generate().unwrap();
        assert_eq!(plan.channels().len(), nodes);
        assert_eq!(plan.topology.unwrap().aliases.len(), nodes);
    }
}

#[test]
fn invalid_channels_and_aliases_are_rejected() {
    for nodes in [0, 1, 9] {
        assert!(options(nodes, &[600], 1).generate().is_err());
    }
    for channel in [
        "",
        "virtual:0",
        " trailing ",
        "bad\nchannel",
        &"x".repeat(257),
    ] {
        let mut options = options(2, &[200], 1);
        options.channels[1] = channel.into();
        assert!(options.generate().is_err(), "{channel:?}");
    }
    for aliases in [vec!["one"], vec!["same", "same"], vec!["one", "\n"]] {
        let mut options = options(2, &[200], 1);
        options.aliases = aliases.into_iter().map(String::from).collect();
        assert!(options.generate().is_err());
    }
}

#[test]
fn identifier_and_run_tag_ranges_do_not_wrap() {
    for (first_can_id, extended) in [(0x7ff, false), (0x1fff_ffff, true), (u32::MAX, true)] {
        let mut options = options(2, &[200], 1);
        options.first_can_id = first_can_id;
        options.extended = extended;
        assert!(options.generate().is_err());
    }
    let mut options = options(2, &[200], 1);
    options.first_run_tag = u32::MAX;
    assert!(options.generate().unwrap_err().contains("run tag range"));
}

#[test]
fn static_and_resource_budgets_are_checked_without_a_runtime() {
    for rates in [vec![], vec![0], vec![601], vec![20_004], vec![600; 33]] {
        assert!(options(6, &rates, 30).generate().is_err(), "{rates:?}");
    }
    assert!(options(6, &[600], 0).generate().is_err());
    let mut invalid_bitrate = options(6, &[600], 30);
    invalid_bitrate.bitrate = 0;
    assert!(invalid_bitrate.generate().is_err());
    assert!(
        options(6, &[600], 72 * 3600)
            .generate()
            .unwrap_err()
            .contains("evidence")
    );
}

#[test]
fn checked_in_six_node_example_matches_the_generator() {
    let mut options = options(6, &[600, 1500, 3000, 4500, 6000], 30);
    options.bitrate = 1_000_000;
    let expected = options.generate().unwrap();
    let example: Plan = serde_json::from_str(include_str!(
        "../../../examples/stress/six-node-staircase.json"
    ))
    .unwrap();
    assert_eq!(example, expected);
}
