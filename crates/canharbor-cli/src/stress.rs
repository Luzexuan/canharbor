// SPDX-License-Identifier: AGPL-3.0-or-later
use canharbor_client::LocalClient;
use clap::{Args, Subcommand};
use serde_json::{Value, json};
use std::{error::Error, path::PathBuf, time::Duration};
#[derive(Subcommand)]
pub enum Command {
    /// Generate an all-to-all Classic plan without contacting the service
    Generate(Generate),
    /// Read retained run events after a revision
    Events {
        id: String,
        #[arg(long, default_value_t = 0)]
        after_revision: u64,
    },
    /// One bounded JSON RPC over stdin/stdout, normally carried by SSH.
    Agent,
    /// Run an explicit multi-host plan with bounded coordination
    Coordinate {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        #[arg(help = "Explicitly authorize active CAN operation; sending requires this flag")]
        active: bool,
        #[arg(long)]
        #[arg(help = "Destination file path")]
        output: PathBuf,
    },
    /// List retained stress runs
    List,
    /// List evidence files for a run
    Artifacts { id: String },
    /// Validate a JSON plan without starting transmission
    Validate {
        #[arg(long)]
        plan: PathBuf,
    },
    /// Validate and run a plan with explicit --active authorization
    Run {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        #[arg(help = "Explicitly authorize active CAN operation; sending requires this flag")]
        active: bool,
        #[arg(long)]
        #[arg(help = "Unique ID for this logical request; retain it for read-only recovery")]
        request_id: Option<String>,
    },
    /// Read run state and progress
    Status { id: String },
    /// Request a stop and inspect final cleanup status
    Stop { id: String },
    /// Stop one flow while preserving other active flows
    StopFlow { id: String, flow: String },
    /// Read or save the run report
    Report {
        id: String,
        #[arg(long)]
        #[arg(help = "Destination file path")]
        output: Option<PathBuf>,
    },
    /// Delete a terminal run and its evidence
    Delete { id: String },
}

#[derive(Args)]
pub struct Generate {
    /// Repeat for each sender; every other selected channel is its receiver
    #[arg(long, required = true)]
    channel: Vec<String>,
    /// Optional alias per channel, in the same order
    #[arg(long)]
    alias: Vec<String>,
    /// First hexadecimal CAN ID; one consecutive ID is assigned per sender
    #[arg(long, default_value = "100", value_parser = crate::input::hex_u32)]
    first_id: u32,
    /// Use 29-bit CAN identifiers
    #[arg(long)]
    extended: bool,
    /// First decimal run tag; one consecutive tag is assigned per sender
    #[arg(long, default_value_t = 1)]
    first_run_tag: u32,
    /// Nominal CAN bitrate in bit/s
    #[arg(long, default_value_t = 500000)]
    bitrate: u32,
    /// Fixed total frames/s across all selected senders
    #[arg(long, requires = "duration_seconds", conflicts_with = "stage")]
    aggregate_fps: Option<u32>,
    /// Fixed load duration in seconds
    #[arg(long, requires = "aggregate_fps", conflicts_with = "stage")]
    duration_seconds: Option<u32>,
    /// Repeat aggregate FPS:SECONDS for each staircase stage
    #[arg(long, value_parser = parse_stage, required_unless_present = "aggregate_fps")]
    stage: Vec<canharbor_stress::Stage>,
}

fn parse_stage(value: &str) -> Result<canharbor_stress::Stage, String> {
    let (fps, seconds) = value
        .split_once(':')
        .ok_or("stage must be aggregate FPS:SECONDS")?;
    Ok(canharbor_stress::Stage {
        name: format!("{fps}-fps"),
        fps: fps.parse().map_err(|_| "stage fps must be a u32 integer")?,
        duration_seconds: seconds
            .parse()
            .map_err(|_| "stage seconds must be a u32 integer")?,
        warmup_seconds: 0,
    })
}

pub fn generate(options: &Generate) -> Result<(), Box<dyn Error>> {
    let stages = if let (Some(fps), Some(duration_seconds)) =
        (options.aggregate_fps, options.duration_seconds)
    {
        vec![canharbor_stress::Stage {
            name: "fixed".into(),
            fps,
            duration_seconds,
            warmup_seconds: 0,
        }]
    } else {
        options.stage.clone()
    };
    let plan = canharbor_stress::templates::AllToAll {
        channels: options.channel.clone(),
        aliases: options.alias.clone(),
        first_can_id: options.first_id,
        extended: options.extended,
        first_run_tag: options.first_run_tag,
        bitrate: options.bitrate,
        stages,
    }
    .generate()
    .map_err(crate::status::input)?;
    crate::print(serde_json::to_value(plan)?);
    Ok(())
}
fn id_path(id: &str) -> Result<String, Box<dyn Error>> {
    let id = uuid::Uuid::parse_str(id)?;
    Ok(format!("/stress/runs/{id}"))
}
pub async fn run(client: &LocalClient, cmd: Command) -> Result<(), Box<dyn Error>> {
    match cmd {
        Command::Generate(_) => {
            unreachable!("offline generation is handled before client creation")
        }
        Command::StopFlow { id, flow } => {
            if flow.is_empty()
                || flow.len() > 32
                || !flow
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            {
                return Err(crate::status::input("invalid flow name"));
            }
            crate::print(
                client
                    .request(
                        "POST",
                        &format!("{}/flows/{flow}/stop", id_path(&id)?),
                        json!({}),
                        None,
                    )
                    .await?,
            );
        }
        Command::Events { id, after_revision } => crate::print(
            client
                .request_with_query(
                    "GET",
                    &format!("{}/events", id_path(&id)?),
                    Value::Null,
                    None,
                    &[("after_revision", &after_revision.to_string())],
                )
                .await?,
        ),
        Command::Agent => {
            use std::io::Read;
            let mut bytes = vec![];
            std::io::stdin()
                .take(1024 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() > 1024 * 1024 {
                return Err(crate::status::input("agent request exceeds 1 MiB"));
            }
            let body: Value = serde_json::from_slice(&bytes)?;
            if body["operation"] == "validate" {
                crate::print(
                    client
                        .request("POST", "/stress/validate", body["plan"].clone(), None)
                        .await?,
                );
            } else {
                crate::print(
                    client
                        .request("POST", "/stress/coordinate", body, None)
                        .await?,
                );
            }
        }
        Command::Coordinate {
            manifest,
            active,
            output,
        } => super::stress_coordinate::run(manifest, active, output).await?,
        Command::Artifacts { id } => crate::print(
            client
                .request(
                    "GET",
                    &format!("{}/artifacts", id_path(&id)?),
                    Value::Null,
                    None,
                )
                .await?,
        ),
        Command::List => crate::print(
            client
                .request("GET", "/stress/runs", Value::Null, None)
                .await?,
        ),
        Command::Validate { plan } => {
            let p: Value = serde_json::from_slice(&std::fs::read(plan)?)?;
            crate::print(client.request("POST", "/stress/validate", p, None).await?);
        }
        Command::Run {
            plan,
            active,
            request_id,
        } => {
            if !active {
                return Err(crate::status::input("stress run requires --active"));
            }
            let p: Value = serde_json::from_slice(&std::fs::read(plan)?)?;
            let validation = client
                .request("POST", "/stress/validate", p.clone(), None)
                .await?;
            let plan_hash = validation["plan_hash"].clone();
            let request = request_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            if crate::i18n::chinese() {
                eprintln!("压力测试请求 ID：{request}；创建响应丢失时请复用此标识");
            } else {
                eprintln!("stress request ID: {request}; reuse it if creation response is lost");
            }
            let value = client
                .request(
                    "POST",
                    "/stress/runs",
                    json!({"plan":p,"plan_hash":plan_hash,"active":active,"client_request_id":request}),
                    None,
                )
                .await?;
            let id = value["id"].as_str().ok_or("missing run ID")?;
            let path = id_path(id)?;
            if crate::i18n::chinese() {
                eprintln!("压力测试运行：{id}；由服务管理，直到截止时间或明确停止");
            } else {
                eprintln!("stress run: {id}; owned by service until deadline or explicit stop");
            }
            loop {
                let value = client.request("GET", &path, Value::Null, None).await?;
                if matches!(
                    value["state"].as_str(),
                    Some("completed" | "failed" | "aborted" | "interrupted")
                ) {
                    let passed = value["verdict"] == "pass";
                    crate::print(value);
                    if !passed {
                        return Err(Box::new(crate::status::Failure::Reported {
                            code: 6,
                            message: "stress run did not pass; inspect report".into(),
                        }));
                    }
                    break;
                }
                tokio::select! {_=tokio::time::sleep(Duration::from_millis(250))=>{},_=tokio::signal::ctrl_c()=>{crate::print(client.request("POST",&format!("{path}/stop"),json!({}),None).await?);return Err(crate::status::input("stop requested; query status for final cleanup"));}}
            }
        }
        Command::Status { id } => crate::print(
            client
                .request("GET", &id_path(&id)?, Value::Null, None)
                .await?,
        ),
        Command::Stop { id } => crate::print(
            client
                .request("POST", &format!("{}/stop", id_path(&id)?), json!({}), None)
                .await?,
        ),
        Command::Delete { id } => {
            client
                .request("DELETE", &id_path(&id)?, Value::Null, None)
                .await?;
        }
        Command::Report { id, output } => {
            let value = client
                .request(
                    "GET",
                    &format!("{}/report", id_path(&id)?),
                    Value::Null,
                    None,
                )
                .await?;
            if let Some(path) = output {
                use std::io::Write;
                let mut f = canharbor_platform::private::create_new(&path)?;
                f.write_all(serde_json::to_string_pretty(&value)?.as_bytes())?;
                f.sync_all()?;
            } else {
                crate::print(value);
            }
        }
    }
    Ok(())
}
