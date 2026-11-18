use anyhow::{Context,Result,ensure};
use serde_json::{Value,json};
use std::{collections::BTreeSet,fs,io::{ErrorKind,Write},path::{Path,PathBuf}};

pub type Disabled=BTreeSet<String>;
fn config_dir()->Result<PathBuf>{
    if let Some(p)=std::env::var_os("RIDGEWIRE_CONFIG_DIR"){return Ok(PathBuf::from(p))}
    Ok(dirs::config_dir().context("Cannot determine configuration directory")?.join("ridgewire"))
}
fn load(path:&Path)->Result<Disabled>{
    match fs::read(path){
        Ok(data)=>Ok(serde_json::from_slice(&data).context("Invalid disabled-channels file")?),
        Err(e) if e.kind()==ErrorKind::NotFound=>Ok(Disabled::new()),
        Err(e)=>Err(e.into()),
    }
}
pub fn read()->Result<Disabled>{load(&config_dir()?.join("disabled.json"))}
pub fn set_enabled(channel:&str,enabled:bool)->Result<()>{
    ensure!(crate::adapters::CATALOG.iter().any(|(c,_)|*c==channel),"Unknown channel");
    let dir=config_dir()?;
    fs::create_dir_all(&dir)?;
    let lock=fs::OpenOptions::new().write(true).create(true).truncate(false).open(dir.join("state.lock"))?;
    fs2::FileExt::lock_exclusive(&lock)?;
    let path=dir.join("disabled.json");
    let mut disabled=load(&path)?;
    if enabled{disabled.remove(channel);}else{disabled.insert(channel.into());}
    let mut temp=tempfile::NamedTempFile::new_in(dir)?;
    serde_json::to_writer(&mut temp,&disabled)?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    temp.persist(path)?;
    Ok(())
}
pub fn doctor()->Result<Value>{
    let path=std::env::var_os("PATH").unwrap_or_default();
    let mut tools=serde_json::Map::new();
    for name in ["yt-dlp","ffmpeg","whisper-cli","bili","opencli","curl"]{
        let found=std::env::split_paths(&path).any(|dir|{
            if cfg!(windows){[".exe",".cmd",".bat"].iter().any(|ext|dir.join(format!("{name}{ext}")).is_file())}
            else{dir.join(name).is_file()}
        });
        tools.insert(name.into(),json!(found));
    }
    Ok(json!({"version":env!("CARGO_PKG_VERSION"),"disabled":read()?,"tool_files_on_path":tools,
        "exa_key_set":std::env::var_os("EXA_API_KEY").is_some(),"whisper_model_file":std::env::var_os("RIDGEWIRE_WHISPER_MODEL").is_some_and(|p|PathBuf::from(p).is_file())}))
}
