use crate::{Job,transport::{on,tool,website,Transport}};
use anyhow::{bail,ensure,Context,Result};
use serde_json::{json,Value};

pub const CATALOG:&[(&str,&[&str])]=&[
    ("web",&["read","search"]),("github",&["read","search"]),
    ("v2ex",&["hot","node","read","user"]),("xueqiu",&["quote","search","hot_posts","hot_stocks"]),
    ("youtube",&["read","search","subtitle","transcribe"]),("bilibili",&["read","search","hot","subtitle"]),
    ("xiaoyuzhou",&["transcribe"]),("boss",&["search","read"]),("facebook",&["search","profile"]),
    ("instagram",&["search","profile","posts"]),("linkedin",&["jobs","read"]),
    ("reddit",&["search","read","subreddit"]),("twitter",&["search","profile","tweets","thread"]),
    ("xiaohongshu",&["search","read","comments"]),
];
pub fn route(input:&str)->Result<Job>{
    let url=website(input)?;
    let host=url.host_str().unwrap_or_default();
    let (channel,action)=match host {
        h if on(h,"github.com")=>("github","read"),
        h if on(h,"v2ex.com") && url.path().starts_with("/t/")=>("v2ex","read"),
        h if on(h,"youtube.com") || on(h,"youtu.be")=>("youtube","read"),
        h if on(h,"bilibili.com") && url.path().contains("/video/")=>("bilibili","read"),
        h if on(h,"reddit.com") && url.path().contains("/comments/")=>("reddit","read"),
        h if (on(h,"x.com")||on(h,"twitter.com")) && url.path().contains("/status/")=>("twitter","thread"),
        h if on(h,"xiaohongshu.com")=>("xiaohongshu","read"),
        h if on(h,"linkedin.com") && url.path().contains("/jobs/view/")=>("linkedin","read"),
        h if on(h,"zhipin.com") && url.path().contains("/job_detail/")=>("boss","read"),
        _=>("web","read"),
    };
    Ok(Job{channel:channel.into(),action:action.into(),target:input.into(),limit:5})
}
fn segment(s:&str)->Result<&str>{
    ensure!(!s.is_empty() && s!="." && s!=".." && s.bytes().all(|c|c.is_ascii_alphanumeric() || b"._-".contains(&c)),"Invalid identifier");
    Ok(s)
}
fn topic_id(s:&str)->Result<&str>{
    let id=s.split("/t/").last().unwrap_or(s).split(['?','#','/']).next().unwrap_or("");
    ensure!(!id.is_empty() && id.bytes().all(|c|c.is_ascii_digit()),"Expected a numeric topic ID");
    Ok(id)
}
fn list(v:&Value,n:u8)->Result<Value>{
    Ok(json!(v.as_array().context("Expected an array from the provider")?.iter().take(n as usize).collect::<Vec<_>>()))
}
async fn github(h:&Transport,action:&str,t:&str,n:u8)->Result<Value>{
    let get=|path:&str| {
        let req=h.0.get(format!("https://api.github.com{path}"));
        match std::env::var("GITHUB_TOKEN"){Ok(token)=>req.bearer_auth(token),Err(_)=>req}
    };
    if action=="search" {
        let v=h.json(get("/search/repositories").query(&[("q",t),("per_page",&n.to_string())])).await?;
        return list(&v["items"],n)
    }
    let url=if t.contains("://"){website(t)?}else{website(&format!("https://github.com/{t}"))?};
    ensure!(url.host_str()==Some("github.com"),"Expected a github.com URL");
    let parts:Vec<_>=url.path().trim_matches('/').split('/').collect();
    ensure!(parts.len()>=2,"Expected owner/repository");
    let path=format!("/repos/{}/{}",segment(parts[0])?,segment(parts[1])?);
    if parts.len()==2 {
        let info=h.json(get(&path)).await?;
        let response=get(&format!("{path}/readme")).header("Accept","application/vnd.github.raw+json").send().await.map_err(|e|e.without_url())?;
        let readme=if response.status()==404 {Value::Null} else {
            Value::String(String::from_utf8(Transport::body(response).await?)?)
        };
        return Ok(json!({"repository":info,"readme":readme}))
    }
    ensure!(parts.len()==4 && matches!(parts[2],"issues"|"pull") && parts[3].parse::<u64>().is_ok(),"Expected a repository, issue or pull request");
    let path=format!("{path}/issues/{}",parts[3]);
    let issue=h.json(get(&path)).await?;
    let comments=h.json(get(&format!("{path}/comments")).query(&[("per_page",n)])).await?;
    Ok(json!({"issue":issue,"comments":comments}))
}
async fn native(h:&Transport,j:&Job)->Result<Value>{
    let (a,t,n)=(j.action.as_str(),j.target.as_str(),j.limit);
    match (j.channel.as_str(),a) {
        ("github",_)=>github(h,a,t,n).await,
        ("web","read")=>{
            let url=website(t)?;
            let body=h.bytes(h.0.get(format!("https://r.jina.ai/{url}")).header("Accept","text/plain")).await?;
            Ok(json!({"url":t,"text":String::from_utf8(body)?}))
        }
        ("web","search")=>{
            let key=std::env::var("EXA_API_KEY").context("Set EXA_API_KEY for web search")?;
            let req=h.0.post("https://api.exa.ai/search").header("x-api-key",key)
                .json(&json!({"query":t,"numResults":n,"type":"auto"}));
            let v=h.json(req).await?;
            list(&v["results"],n)
        }
        ("v2ex",_)=>{
            let base="https://www.v2ex.com/api";
            let req=match a {
                "hot"=>h.0.get(format!("{base}/topics/hot.json")),
                "user"=>h.0.get(format!("{base}/members/show.json")).query(&[("username",t)]),
                "node"=>h.0.get(format!("{base}/topics/show.json")).query(&[("node_name",t)]),
                _=>h.0.get(format!("{base}/topics/show.json")).query(&[("id",topic_id(t)?)]),
            };
            let v=h.json(req).await?;
            if a=="user"{ensure!(v["id"].is_number(),"User unavailable");return Ok(v)}
            if a=="read"{
                ensure!(v[0]["id"].is_number(),"Topic unavailable");
                let replies=h.json(h.0.get(format!("{base}/replies/show.json")).query(&[("topic_id",topic_id(t)?),("page","1")])).await?;
                return Ok(json!({"topic":v[0],"replies":list(&replies,n)?}))
            }
            list(&v,n)
        }
        ("xueqiu",_)=>{
            let size=n.to_string();
            let (url,params,pointer)=match a{
                "quote"=>("https://stock.xueqiu.com/v5/stock/quote.json",vec![("symbol",t),("extend","detail")],"/data/quote"),
                "search"=>("https://xueqiu.com/stock/search.json",vec![("code",t),("size",size.as_str())],"/stocks"),
                "hot_posts"=>("https://xueqiu.com/v4/statuses/public_timeline_by_category.json",vec![("count",size.as_str()),("category","-1"),("since_id","-1"),("max_id","-1")],"/list"),
                _=>("https://stock.xueqiu.com/v5/stock/hot_stock/list.json",vec![("size",size.as_str()),("type","10")],"/data/items"),
            };
            let v=h.json(h.0.get(url).query(&params)).await?;
            let data=v.pointer(pointer).filter(|x|!x.is_null()).context("Provider did not return the requested data")?;
            if a=="quote"{Ok(data.clone())}else{list(data,n)}
        }
        _=>bail!("Unknown native channel"),
    }
}
pub async fn run(h:&Transport,j:&Job)->Result<Value>{
    let (c,a,t)=(j.channel.as_str(),j.action.as_str(),j.target.trim());
    ensure!(CATALOG.iter().any(|(name,actions)|*name==c && actions.contains(&a)),"Unknown action. Run ridgewire channels");
    ensure!(!t.is_empty() || matches!(a,"hot"|"hot_posts"|"hot_stocks"),"A target is required");
    ensure!(!t.starts_with('-') && !t.contains('\0'),"Invalid target");
    if matches!(c,"github"|"web"|"v2ex"|"xueqiu"){return native(h,j).await}
    if matches!(c,"youtube"|"xiaoyuzhou"){return crate::media::run(h,c,a,t,j.limit).await}
    let limit=j.limit.to_string();
    let mut target=t.to_owned();
    let mut command=a;
    if c=="bilibili"{
        let parts:Vec<&str>=match a{
            "search"=>vec!["search",t,"--type","video","-n",&limit,"--json"],
            "hot"=>vec!["hot","-n",&limit,"--json"],
            _=>{
                let id=t.split("/video/").last().unwrap_or(t).split(['?','#','/']).next().unwrap_or("");
                ensure!(id.starts_with("BV") && id.len()==12 && id.bytes().all(|b|b.is_ascii_alphanumeric()),"Expected a Bilibili BV identifier");
                if a=="subtitle"{vec!["video",id,"--subtitle","--json"]}else{vec!["video",id,"--json"]}
            }
        };
        return Ok(serde_json::from_slice(&tool("bili",&parts,60).await?)?)
    }
    match (c,a){
        ("instagram","posts")=>command="user",
        ("linkedin","jobs")=>command="search",
        ("linkedin","read")=>command="job-detail",
        ("xiaohongshu","read")=>command="note",
        ("boss","read")=>{
            let u=website(t)?;
            ensure!(on(u.host_str().unwrap_or(""),"zhipin.com"),"Expected a Boss job URL");
            target=segment(u.path().strip_prefix("/job_detail/").and_then(|p|p.strip_suffix(".html")).context("Invalid job URL")?)?.into();
            command="detail";
        }
        ("twitter","thread")=>{
            let id=t.split("/status/").last().unwrap_or(t).split(['?','#','/']).next().unwrap_or("");
            ensure!(!id.is_empty() && id.bytes().all(|c|c.is_ascii_digit()),"Expected a post ID");
            target=id.into();
        }
        (_, "profile"|"tweets")=>target=segment(t.trim_start_matches('@'))?.into(),
        ("reddit","subreddit")=>target=segment(t.trim_start_matches("r/"))?.into(),
        _=>{}
    }
    ensure!(!target.starts_with('-'),"Invalid identifier");
    let mut args=vec![c,command,target.as_str()];
    if matches!(a,"search"|"subreddit"|"jobs"|"posts"|"comments"|"tweets") || (c=="reddit" && a=="read"){args.extend(["--limit",&limit])}
    if c=="xiaohongshu" && a=="comments"{args.extend(["--with-replies","true"])}
    args.extend(["-f","json"]);
    let data:Value=serde_json::from_slice(&tool("opencli",&args,90).await?)?;
    ensure!(data.get("ok")!=Some(&Value::Bool(false)),"OpenCLI reported a provider failure");
    Ok(data.get("data").cloned().unwrap_or(data))
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test] fn routes_validate_authority(){
        assert_eq!(route("https://github.com/example/repo").unwrap().channel,"github");
        assert_eq!(route("https://github.com.example.org/a/b").unwrap().channel,"web");
        assert!(route("file:///etc/hosts").is_err());
        assert!(route("https://user:pass@example.com").is_err());
    }
    #[test] fn identifiers(){
        assert!(segment("..").is_err());
        assert_eq!(topic_id("https://v2ex.com/t/123#r1").unwrap(),"123");
        assert!(topic_id("12x").is_err());
    }
}
