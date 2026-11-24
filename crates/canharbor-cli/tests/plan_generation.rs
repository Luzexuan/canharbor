// SPDX-License-Identifier: AGPL-3.0-or-later
use canharbor_stress::Plan;
use std::process::{Command, Output};

fn generate(channels: usize, args: &[&str]) -> Output {
    let root = std::env::temp_dir().join(format!("ch-offline-plan-{}", uuid::Uuid::new_v4()));
    let mut command = Command::new(env!("CARGO_BIN_EXE_canharbor"));
    command
        .env("CANHARBOR_CONFIG_DIR", root.join("config"))
        .args(["--lang", "en", "--runtime-dir"])
        .arg(&root)
        .args(["stress", "generate"]);
    for index in 0..channels {
        command.args(["--channel", &format!("virtual:{index}")]);
    }
    let output = command.args(args).output().unwrap();
    assert!(
        !root.exists(),
        "offline generation created a runtime directory"
    );
    output
}

#[test]
fn fixed_plan_is_generated_without_a_service() {
    let output = generate(6, &["--aggregate-fps", "600", "--duration-seconds", "600"]);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let plan: Plan = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan.compile().unwrap().count, 360_000);
    assert_eq!(plan.group_budget().unwrap().peak_transmit_fps, 600);
    assert_eq!(
        plan.flow_plans()
            .iter()
            .map(|(_, p)| p.receivers.len())
            .sum::<usize>(),
        30
    );
}

#[test]
fn staircase_and_aliases_are_emitted_in_existing_plan_format() {
    let output = generate(
        2,
        &[
            "--stage",
            "600:30",
            "--stage",
            "1500:30",
            "--stage",
            "3000:30",
            "--stage",
            "4500:30",
            "--stage",
            "6000:30",
            "--first-id",
            "0x1FFFFF00",
            "--extended",
            "--first-run-tag",
            "41",
            "--alias",
            "Left",
            "--alias",
            "Right",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    let plan: Plan = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan.can_id, 0x1fff_ff00);
    assert_eq!(plan.flows[0].can_id, plan.can_id + 1);
    assert_eq!(plan.flows[0].run_tag, 42);
    assert_eq!(plan.stages[4].fps, 3000);
    assert_eq!(plan.topology.unwrap().aliases[1].alias, "Right");
}

#[test]
fn malformed_conflicting_and_over_budget_inputs_return_input_status() {
    for args in [
        vec![],
        vec!["--aggregate-fps", "600"],
        vec!["--duration-seconds", "30"],
        vec!["--stage", "600"],
        vec!["--stage", "600:30:1"],
        vec!["--stage", "0:30"],
        vec!["--stage", "601:30"],
        vec!["--stage", "20004:30"],
        vec!["--stage", "600:0"],
        vec!["--stage", "600:30", "--first-id", "7FF"],
        vec!["--stage", "600:30", "--first-run-tag", "4294967295"],
        vec!["--stage", "600:30", "--alias", "only-one"],
        vec![
            "--stage",
            "600:30",
            "--aggregate-fps",
            "600",
            "--duration-seconds",
            "30",
        ],
    ] {
        let output = generate(6, &args);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        assert!(!output.stderr.is_empty(), "{args:?}");
    }
    let output = generate(1, &["--stage", "600:30"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}
