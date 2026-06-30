use serde::Deserialize;

#[derive(Debug, Clone, Default)]
pub struct CrateInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub downloads: u64,
}

#[derive(Debug, Clone, Default)]
pub struct CrateDetail {
    pub name: String,
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
    pub deps: Vec<DepInfo>,
    pub repository: String,
}

#[derive(Debug, Clone)]
pub struct DepInfo {
    pub name: String,
    pub req: String,
}

// ── serde 構造体 ──────────────────────────────────────────────

#[derive(Deserialize)]
struct SearchResp {
    crates: Vec<CrateItem>,
}

#[derive(Deserialize)]
struct CrateItem {
    name: String,
    newest_version: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    downloads: u64,
}

#[derive(Deserialize)]
struct MetaResp {
    #[serde(rename = "crate")]
    krate: MetaCrate,
    #[serde(default)]
    owners: Vec<Owner>,
}

#[derive(Deserialize)]
struct MetaCrate {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    repository: Option<String>,
    #[allow(dead_code)]
    newest_version: String,
}

#[derive(Deserialize)]
struct Owner {
    #[serde(default)]
    name: Option<String>,
    login: String,
}

#[derive(Deserialize)]
struct DepsResp {
    dependencies: Vec<DepItem>,
}

#[derive(Deserialize)]
struct DepItem {
    crate_id: String,
    req: String,
    #[serde(default)]
    kind: String,
}

// ── API 関数 ──────────────────────────────────────────────────

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("cargo-tui/0.1.0 (github.com/cet-t/cargo-tui)")
        .build()
        .unwrap()
}

pub async fn search(query: &str, limit: usize) -> anyhow::Result<Vec<CrateInfo>> {
    let resp: SearchResp = client()
        .get("https://crates.io/api/v1/crates")
        .query(&[("q", query), ("per_page", &limit.to_string())])
        .send()
        .await?
        .json()
        .await?;

    Ok(resp
        .crates
        .into_iter()
        .map(|c| CrateInfo {
            name: c.name,
            version: c.newest_version,
            description: c.description.unwrap_or_default(),
            downloads: c.downloads,
        })
        .collect())
}

pub async fn get_detail(name: &str, version: &str) -> anyhow::Result<CrateDetail> {
    let base = format!("https://crates.io/api/v1/crates/{}", name);

    let (meta_res, deps_res) = tokio::join!(
        client().get(&base).send(),
        client()
            .get(format!("{}/{}/dependencies", base, version))
            .send()
    );

    let meta: MetaResp = meta_res?.json().await?;
    let deps_resp: DepsResp = deps_res?.json().await.unwrap_or(DepsResp { dependencies: vec![] });

    let authors = meta.owners.into_iter().map(|o| o.name.unwrap_or(o.login)).collect();
    let deps = deps_resp
        .dependencies
        .into_iter()
        .filter(|d| d.kind == "normal" || d.kind.is_empty())
        .map(|d| DepInfo { name: d.crate_id, req: d.req })
        .collect();

    Ok(CrateDetail {
        name: meta.krate.name,
        version: version.to_string(),
        description: meta.krate.description.unwrap_or_default(),
        repository: meta.krate.repository.unwrap_or_default(),
        authors,
        deps,
    })
}

pub fn fmt_downloads(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.1}B", n as f64 / 1e9)
    } else if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1e6)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1e3)
    } else {
        n.to_string()
    }
}
