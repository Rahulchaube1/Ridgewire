use anyhow::{bail, ensure, Context, Result};
use futures::StreamExt;
use reqwest::{Client,RequestBuilder,Url};
use serde_json::Value;
use std::{process::Stdio,time::Duration};
use tokio::{io::AsyncReadExt,process::Command};

const MAX:usize=4*1024*1024;
pub struct Transport(pub Client);
impl Transport {
    pub fn new()->Result<Self>{
        Ok(Self(Client::builder().user_agent(concat!("Ridgewire/",env!("CARGO_PKG_VERSION")))
            .https_only(true).redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(8)).timeout(Duration::from_secs(35)).build()?))
    }
    pub async fn bytes(&self,req:RequestBuilder)->Result<Vec<u8>>{
        let resp=req.send().await.map_err(|e|e.without_url())?;
        Self::body(resp).await
    }
    pub async fn body(resp:reqwest::Response)->Result<Vec<u8>>{
        ensure!(resp.status().is_success(),"Provider returned HTTP {}",resp.status());
        let mut stream=resp.bytes_stream();
        let mut bytes=Vec::new();
        while let Some(chunk)=stream.next().await{
            let chunk=chunk.map_err(|e|e.without_url())?;
            ensure!(bytes.len()+chunk.len()<=MAX,"Provider response exceeds four MiB");
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
    pub async fn json(&self,req:RequestBuilder)->Result<Value>{
        Ok(serde_json::from_slice(&self.bytes(req).await?).context("Invalid JSON from provider")?)
    }
}
pub fn website(input:&str)->Result<Url>{
    let url=Url::parse(input)?;
    ensure!(matches!(url.scheme(),"http"|"https") && url.host_str().is_some() && url.username().is_empty() && url.password().is_none(),"Expected an HTTP(S) URL without credentials");
    Ok(url)
}
pub fn on(host:&str,domain:&str)->bool{host==domain || host.ends_with(&format!(".{domain}"))}
pub async fn tool(program:&str,args:&[&str],seconds:u64)->Result<Vec<u8>>{
    let mut child=Command::new(program).args(args).kill_on_drop(true).stdin(Stdio::null())
        .stdout(Stdio::piped()).stderr(Stdio::null()).spawn().with_context(||format!("Could not start {program}. Check installation and PATH"))?;
    let mut pipe=child.stdout.take().context("Missing output pipe")?.take((MAX+1) as u64);
    let work=async {
        let mut out=Vec::new();
        pipe.read_to_end(&mut out).await?;
        ensure!(out.len()<=MAX,"Tool output exceeds four MiB");
        if !child.wait().await?.success(){bail!("{program} failed. Check the provider and your tool configuration")}
        Ok(out)
    };
    tokio::time::timeout(Duration::from_secs(seconds),work).await.context("Tool timed out")?
}
