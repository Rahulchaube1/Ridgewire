mod media;
mod state;
mod adapters;
mod transport;
use anyhow::{bail, ensure, Result};
use clap::{Parser, Subcommand};
use futures::{stream, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use transport::Transport;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}
#[derive(Subcommand)]
enum Cmd {
    /// Print the channel and action catalog.
    Channels,
    /// Report tool availability, configuration and disabled channels.
    Doctor,
    /// Enable a channel for this user.
    Add { channel: String },
    /// Disable a channel for this user.
    Remove { channel: String },
    /// Read a public URL using the corresponding channel.
    Read { url: String },
    /// Preview automatic URL routing without a network request.
    Route { url: String },
    /// Run a named channel action.
    Run {
        channel: String,
        action: String,
        target: Option<String>,
        #[arg(long, default_value_t=5)] limit: u8,
    },
    /// Read up to 256 JSON jobs from standard input and emit ordered JSON Lines.
    Batch {
        #[arg(long,default_value_t=4,value_parser=clap::value_parser!(u8).range(1..=16))]
        jobs: u8,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    channel: String,
    action: String,
    #[serde(default)] target: String,
    #[serde(default="default_limit")] limit: u8,
}
fn default_limit() -> u8 { 5 }

fn batch_input(reader: impl BufRead) -> Result<Vec<Job>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    reader.take(1_048_577).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 1_048_576, "Batch exceeds one MiB");
    let mut jobs = Vec::new();
    for (i,line) in std::str::from_utf8(&bytes)?.lines().enumerate() {
        if line.trim().is_empty() { continue }
        ensure!(jobs.len() < 256, "Batch exceeds 256 jobs");
        jobs.push(serde_json::from_str(line).map_err(|e| anyhow::anyhow!("Line {}: {}",i+1,e))?);
    }
    Ok(jobs)
}
async fn execute(http: &Transport, disabled: &state::Disabled, job: Job) -> Value {
    let result = async {
        ensure!((1..=50).contains(&job.limit), "Limit must be between 1 and 50");
        ensure!(job.target.len() <= 8192, "Target exceeds 8192 bytes");
        ensure!(!disabled.contains(&job.channel), "Channel disabled. Run ridgewire add {}", job.channel);
        adapters::run(http,&job).await
    }.await;
    match result {
        Ok(data) => json!({"ok":true,"channel":job.channel,"action":job.action,"data":data}),
        Err(e) => json!({"ok":false,"channel":job.channel,"action":job.action,"error":e.to_string()}),
    }
}
fn emit(v: &Value) -> Result<()> {
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out,v)?;
    writeln!(out)?;
    Ok(())
}
async fn launch() -> Result<bool> {
    let cli = Cli::parse();
    if matches!(&cli.command,Cmd::Channels) {
        emit(&json!(adapters::CATALOG))?;
        return Ok(true)
    }
    match &cli.command {
        Cmd::Add{channel} | Cmd::Remove{channel} => {
            let enable = matches!(&cli.command,Cmd::Add{..});
            state::set_enabled(channel,enable)?;
            emit(&json!({"ok":true,"channel":channel,"enabled":enable}))?;
            return Ok(true)
        }
        Cmd::Route { url } => {
            let job = adapters::route(url)?;
            emit(&json!({"channel":job.channel,"action":job.action,"target":job.target}))?;
            return Ok(true)
        }
        Cmd::Doctor => { emit(&state::doctor()?)?; return Ok(true) }
        _ => {}
    }
    let disabled=state::read()?;
    let http = Transport::new()?;
    let (jobs,width) = match cli.command {
        Cmd::Read{url} => (vec![adapters::route(&url)?],1),
        Cmd::Run{channel,action,target,limit} => (vec![Job{channel,action,target:target.unwrap_or_default(),limit}],1),
        Cmd::Batch{jobs} => (batch_input(std::io::stdin().lock())?,jobs),
        _ => bail!("Unexpected command"),
    };
    let mut results=stream::iter(jobs.into_iter().map(|j|execute(&http,&disabled,j))).buffered(width as usize);
    let mut ok=true;
    while let Some(v)=results.next().await {
        ok &= v["ok"] == true;
        emit(&v)?;
    }
    Ok(ok)
}
#[tokio::main]
async fn main() {
    match launch().await {
        Ok(true) => (),
        Ok(false) => std::process::exit(1),
        Err(e) => { eprintln!("{}",json!({"ok":false,"error":e.to_string()})); std::process::exit(2); }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn batch_validation() {
        assert!(batch_input(&b"{\"action\":\"hot\",\"channel\":\"v2ex\"}\n"[..]).is_ok());
        assert!(batch_input(&b"{\"channel\":\"web\"}"[..]).is_err());
        assert!(batch_input(vec![32;1_048_577].as_slice()).is_err());
    }
}
