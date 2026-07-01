use crate::error::Error;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Which Cargo.toml dependency section a crate belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepKind {
    Normal,
    Dev,
    Build,
}

impl DepKind {
    /// Section header as written in Cargo.toml.
    pub fn section(self) -> &'static str {
        match self {
            DepKind::Normal => "dependencies",
            DepKind::Dev    => "dev-dependencies",
            DepKind::Build  => "build-dependencies",
        }
    }

    /// The `cargo add` / `cargo remove` flag for this section (empty for normal).
    pub fn flag(self) -> Option<&'static str> {
        match self {
            DepKind::Normal => None,
            DepKind::Dev    => Some("--dev"),
            DepKind::Build  => Some("--build"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Dep {
    pub name: String,
    pub version: String,
    pub kind: DepKind,
}

/// Kind of a runnable cargo target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunKind {
    Bin,
    Example,
}

/// A runnable target (binary or example) discovered in the workspace.
#[derive(Debug, Clone)]
pub struct RunTarget {
    pub package: String,
    pub name: String,
    pub kind: RunKind,
}

#[derive(Debug, Clone)]
pub struct WorkspaceInfo {
    pub name: String,
    pub root: PathBuf,
    pub deps: Vec<Dep>,
    pub targets: Vec<RunTarget>,
}

#[derive(Deserialize)]
struct CargoToml {
    package: Option<Package>,
    dependencies: Option<toml::Table>,
    #[serde(rename = "dev-dependencies")]
    dev_dependencies: Option<toml::Table>,
    #[serde(rename = "build-dependencies")]
    build_dependencies: Option<toml::Table>,
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
    let sections = [
        (DepKind::Normal, &manifest.dependencies),
        (DepKind::Dev,    &manifest.dev_dependencies),
        (DepKind::Build,  &manifest.build_dependencies),
    ];
    for (kind, table) in sections {
        let Some(table) = table else { continue };
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
            deps.push(Dep { name: k.clone(), version: ver, kind });
        }
    }
    // Sort by section (normal, dev, build) then name so the grouped view is stable.
    deps.sort_by(|a, b| {
        (a.kind as u8, &a.name).cmp(&(b.kind as u8, &b.name))
    });

    let targets = detect_targets(root);

    Ok(WorkspaceInfo { name, root: root.to_path_buf(), deps, targets })
}

/// Enumerate all runnable targets (bins and examples) across workspace members
/// via `cargo metadata`. Returns an empty vec if cargo is unavailable or the
/// project has nothing runnable (e.g. a library-only crate with no examples).
fn detect_targets(root: &Path) -> Vec<RunTarget> {
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

    let mut targets = Vec::new();
    let Some(packages) = meta.get("packages").and_then(|v| v.as_array()) else {
        return targets;
    };
    for pkg in packages {
        let pkg_name = pkg.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let Some(pkg_targets) = pkg.get("targets").and_then(|v| v.as_array()) else {
            continue;
        };
        for target in pkg_targets {
            let kinds = target.get("kind").and_then(|v| v.as_array());
            let kind = kinds.and_then(|ks| {
                if ks.iter().any(|k| k.as_str() == Some("bin")) {
                    Some(RunKind::Bin)
                } else if ks.iter().any(|k| k.as_str() == Some("example")) {
                    Some(RunKind::Example)
                } else {
                    None
                }
            });
            if let Some(kind) = kind {
                if let Some(name) = target.get("name").and_then(|v| v.as_str()) {
                    targets.push(RunTarget {
                        package: pkg_name.clone(),
                        name: name.to_string(),
                        kind,
                    });
                }
            }
        }
    }
    targets
}
