use crate::error::Error;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Dep {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct WorkspaceInfo {
    pub name: String,
    pub root: PathBuf,
    pub deps: Vec<Dep>,
}

#[derive(Deserialize)]
struct CargoToml {
    package: Option<Package>,
    dependencies: Option<toml::Table>,
    #[serde(rename = "dev-dependencies")]
    dev_dependencies: Option<toml::Table>,
}

#[derive(Deserialize)]
struct Package {
    name: String,
}

pub fn find_root(start: PathBuf) -> Option<PathBuf> {
    let mut cur = start;
    loop {
        if cur.join("Cargo.toml").exists() {
            return Some(cur);
        }
        match cur.parent() {
            Some(p) => cur = p.to_path_buf(),
            None => return None,
        }
    }
}

pub fn load(root: &Path) -> Result<WorkspaceInfo, Error> {
    let content = std::fs::read_to_string(root.join("Cargo.toml"))?;
    let manifest: CargoToml = toml::from_str(&content)?;

    let name = manifest
        .package
        .map(|p| p.name)
        .unwrap_or_else(|| root.file_name().unwrap_or_default().to_string_lossy().into());

    let mut deps = Vec::new();
    for table in [&manifest.dependencies, &manifest.dev_dependencies]
        .into_iter()
        .flatten()
    {
        for (k, v) in table {
            let ver = match v {
                toml::Value::String(s) => s.clone(),
                toml::Value::Table(t) => t
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("*")
                    .to_string(),
                _ => continue,
            };
            deps.push(Dep { name: k.clone(), version: ver });
        }
    }
    deps.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(WorkspaceInfo { name, root: root.to_path_buf(), deps })
}
