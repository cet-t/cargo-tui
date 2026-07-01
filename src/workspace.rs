use crate::error::Error;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Dep {
    pub name: String,
    pub version: String,
}

/// A runnable binary target discovered in the workspace.
#[derive(Debug, Clone)]
pub struct BinTarget {
    pub package: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct WorkspaceInfo {
    pub name: String,
    pub root: PathBuf,
    pub deps: Vec<Dep>,
    pub bins: Vec<BinTarget>,
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

/// Walk up from `start` until a directory containing `Cargo.toml` is found.
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

/// Load workspace name and dependency list from `<root>/Cargo.toml`.
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

    let bins = detect_bins(root);

    Ok(WorkspaceInfo { name, root: root.to_path_buf(), deps, bins })
}

/// Enumerate all binary targets across workspace members via `cargo metadata`.
/// Returns an empty vec if cargo is unavailable or the project has no bins
/// (e.g. a library-only crate).
fn detect_bins(root: &Path) -> Vec<BinTarget> {
    let output = std::process::Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(root)
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let meta: serde_json::Value = match serde_json::from_slice(&output.stdout) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut bins = Vec::new();
    let Some(packages) = meta.get("packages").and_then(|v| v.as_array()) else {
        return bins;
    };
    for pkg in packages {
        let pkg_name = pkg.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let Some(targets) = pkg.get("targets").and_then(|v| v.as_array()) else {
            continue;
        };
        for target in targets {
            let is_bin = target
                .get("kind")
                .and_then(|v| v.as_array())
                .map(|kinds| kinds.iter().any(|k| k.as_str() == Some("bin")))
                .unwrap_or(false);
            if is_bin {
                if let Some(name) = target.get("name").and_then(|v| v.as_str()) {
                    bins.push(BinTarget { package: pkg_name.clone(), name: name.to_string() });
                }
            }
        }
    }
    bins
}
