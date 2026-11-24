// SPDX-License-Identifier: AGPL-3.0-or-later
//! Offline generators for existing stress plans; rates here are aggregate rates.
use crate::{Flow, Plan, RoleAlias, Stage, TopologyDeclaration};
use std::collections::HashSet;

pub struct AllToAll {
    pub channels: Vec<String>,
    /// Optional aliases in the same order as channels.
    pub aliases: Vec<String>,
    pub first_can_id: u32,
    pub extended: bool,
    pub first_run_tag: u32,
    pub bitrate: u32,
    /// Each stage's fps is the total across all senders.
    pub stages: Vec<Stage>,
}

impl AllToAll {
    /// Generate a Classic plan and check its static/resource bounds without a backend.
    /// Channel availability and bitrate capabilities still require `Plan::validate`.
    pub fn generate(self) -> Result<Plan, String> {
        let nodes = self.channels.len();
        if !(2..=8).contains(&nodes) {
            return Err("all-to-all requires 2..8 channels".into());
        }
        let mut seen = HashSet::new();
        for channel in &self.channels {
            if channel.is_empty()
                || channel.len() > 256
                || channel.trim() != channel
                || channel.chars().any(char::is_control)
                || !seen.insert(channel)
            {
                return Err("channels must be distinct, nonempty IDs of at most 256 bytes without surrounding whitespace or control characters".into());
            }
        }
        if !self.aliases.is_empty() && self.aliases.len() != nodes {
            return Err("provide either no aliases or one alias per channel".into());
        }
        let last_id = self
            .first_can_id
            .checked_add(nodes as u32 - 1)
            .ok_or("CAN ID range overflow")?;
        canharbor_core::CanId::new(last_id, self.extended)
            .map_err(|_| "consecutive CAN IDs exceed the selected identifier width")?;
        self.first_run_tag
            .checked_add(nodes as u32 - 1)
            .ok_or("run tag range overflow")?;
        if self.stages.is_empty() || self.stages.len() > 32 {
            return Err("provide 1..32 fixed-rate or staircase stages".into());
        }
        let mut stages = self.stages;
        for stage in &mut stages {
            if stage.fps == 0 || !stage.fps.is_multiple_of(nodes as u32) {
                return Err(
                    "each aggregate fps must be positive and divisible by the channel count".into(),
                );
            }
            stage.fps /= nodes as u32;
        }
        let mut flows = self
            .channels
            .iter()
            .enumerate()
            .map(|(index, sender)| Flow {
                tracking: Default::default(),
                queues: None,
                shape: Default::default(),
                name: format!("node-{}", index + 1),
                sender: sender.clone(),
                receivers: self
                    .channels
                    .iter()
                    .filter(|channel| *channel != sender)
                    .cloned()
                    .collect(),
                can_id: self.first_can_id + index as u32,
                extended: self.extended,
                run_tag: self.first_run_tag + index as u32,
                count: 0,
                fps: 0,
                stages: stages.clone(),
                traffic: Default::default(),
                criteria: Default::default(),
                timing: None,
                rtt: None,
            })
            .collect::<Vec<_>>();
        let primary = flows.remove(0);
        let topology = (!self.aliases.is_empty()).then(|| TopologyDeclaration {
            aliases: self
                .channels
                .into_iter()
                .zip(self.aliases)
                .map(|(channel, alias)| RoleAlias { channel, alias })
                .collect(),
            ..Default::default()
        });
        let plan = Plan {
            tracking: primary.tracking,
            queues: primary.queues,
            adaptive: None,
            shape: primary.shape,
            flows,
            timing: primary.timing,
            topology,
            criteria: primary.criteria,
            traffic: primary.traffic,
            rtt: primary.rtt,
            coordination: None,
            sender: primary.sender,
            receivers: primary.receivers,
            can_id: primary.can_id,
            extended: primary.extended,
            run_tag: primary.run_tag,
            stages: primary.stages,
            evidence: Default::default(),
            count: 0,
            fps: 0,
            bitrate: self.bitrate,
            receivers_active: true,
            drain_ms: 200,
            stability: Default::default(),
        };
        plan.compile()?;
        Ok(plan)
    }
}
