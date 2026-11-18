use crate::transport::{on,tool,website,Transport};
use anyhow::{ensure,Context,Result};
use serde_json::{json,Value};
use std::path::Path;

fn youtube(target:&str)->Result<String>{
    if target.len()==11 && target.bytes().all(|c|c.is_ascii_alphanumeric()||b"_-".contains(&c)){
        return Ok(format!("https://www.youtube.com/watch?v={target}"))
    }
    let u=website(target)?;
    let h=u.host_str().unwrap_or("");
    ensure!(on(h,"youtube.com")||h=="youtu.be","Expected a YouTube URL or video ID");
    Ok(u.to_string())
}
async fn file_with_extension(dir:&Path,ext:&str)->Result<std::path::PathBuf>{
    let mut files=tokio::fs::read_dir(dir).await?;
    while let Some(entry)=files.next_entry().await?{
        let p=entry.path();
        if p.extension().and_then(|s|s.to_str())==Some(ext){return Ok(p)}
    }
    anyhow::bail!("Provider did not produce a {ext} file")
}
async fn small_text(path:&Path)->Result<String>{
    ensure!(tokio::fs::metadata(path).await?.len()<=4*1024*1024,"Text exceeds four MiB");
    Ok(tokio::fs::read_to_string(path).await?)
}
pub async fn run(h:&Transport,c:&str,a:&str,t:&str,n:u8)->Result<Value>{
    if a=="search" || a=="read"{
        let url=if a=="search"{format!("ytsearch{n}:{t}")}else{youtube(t)?};
        let mut args=vec!["--ignore-config","--no-playlist","--skip-download","--no-warnings","--print",
            "%(.{id,title,uploader,webpage_url,upload_date,description})j"];
        if a=="search"{args.push("--flat-playlist")}
        args.extend(["--",&url]);
        let rows=tool("yt-dlp",&args,90).await?;
        let result=std::str::from_utf8(&rows)?.lines().map(serde_json::from_str::<Value>).collect::<std::result::Result<Vec<_>,_>>()?;
        return Ok(json!(result))
    }
    let dir=tempfile::tempdir()?;
    if a=="subtitle"{
        let url=youtube(t)?;
        let template=dir.path().join("captions.%(ext)s");
        let lang=std::env::var("RIDGEWIRE_CAPTION_LANGUAGE").unwrap_or_else(|_|"en".into());
        ensure!(!lang.is_empty() && lang.len()<32 && lang.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'-'),"Invalid caption language");
        tool("yt-dlp",&["--ignore-config","--no-playlist","--skip-download","--write-subs","--write-auto-subs","--sub-langs",&lang,"--sub-format","vtt","-o",template.to_str().context("Invalid temporary path")?,"--",&url],120).await?;
        let path=file_with_extension(dir.path(),"vtt").await?;
        return Ok(json!({"language":lang,"format":"vtt","text":small_text(&path).await?}))
    }
    let model=std::env::var("RIDGEWIRE_WHISPER_MODEL").context("Set RIDGEWIRE_WHISPER_MODEL to a local whisper.cpp model file")?;
    ensure!(Path::new(&model).is_file(),"Whisper model file is unavailable");
    let input=dir.path().join("source");
    let input=input.to_str().context("Invalid temporary path")?;
    if c=="youtube"{
        let url=youtube(t)?;
        tool("yt-dlp",&["--ignore-config","--no-playlist","--max-filesize","128M","-f","bestaudio","-o",input,"--",&url],240).await?;
    } else {
        let url=website(t)?;
        ensure!(on(url.host_str().unwrap_or(""),"xiaoyuzhoufm.com") && url.path().starts_with("/episode/"),"Expected a Xiaoyuzhou episode URL");
        let html=String::from_utf8(h.bytes(h.0.get(url)).await?)?;
        let media=html.split('"').find(|s|s.starts_with("https://media.xyzcdn.net/") && (s.contains(".m4a")||s.contains(".mp3"))).context("No public audio URL on this episode")?;
        let audio=website(media)?;
        ensure!(audio.host_str()==Some("media.xyzcdn.net"),"Invalid episode media host");
        tool("curl",&["--disable","--silent","--fail","--max-time","240","--max-filesize","134217728","--proto","=https","--output",input,"--",audio.as_str()],245).await?;
    }
    let wav=dir.path().join("audio.wav");
    let wav=wav.to_str().context("Invalid temporary path")?;
    tool("ffmpeg",&["-nostdin","-v","error","-protocol_whitelist","file,pipe","-i",input,"-t","10801","-vn","-ar","16000","-ac","1","-c:a","pcm_s16le",wav],300).await?;
    ensure!(tokio::fs::metadata(wav).await?.len()<=(10800u64*32000+1024),"Audio exceeds three hours");
    let prefix=dir.path().join("transcript");
    tool("whisper-cli",&["-m",&model,"-f",wav,"-l","auto","-otxt","-of",prefix.to_str().context("Invalid temporary path")?],1800).await?;
    Ok(json!({"text":small_text(&prefix.with_extension("txt")).await?}))
}
