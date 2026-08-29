use std::time::Duration;

use anyhow::{anyhow, Result};

pub fn http_agent() -> Result<ureq::Agent> {
    let mut builder = ureq::AgentBuilder::new()
        .user_agent("camoforge")
        .timeout_connect(Duration::from_secs(20))
        // GB 级下载不能设总超时，读超时只防单次 read 僵死。
        .timeout_read(Duration::from_secs(60));
    for key in [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ] {
        let Ok(url) = std::env::var(key) else {
            continue;
        };
        if url.trim().is_empty() {
            continue;
        }
        let proxy =
            ureq::Proxy::new(url.trim()).map_err(|error| anyhow!("代理 {url} 无效: {error}"))?;
        builder = builder.proxy(proxy);
        break;
    }
    Ok(builder.build())
}
