use crate::{
    config::Manifest,
    registry::{Registry, Workspace},
};
use serde::Serialize;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug, Serialize)]
pub struct ResolvedContext {
    pub workspace: Workspace,
    pub source_exists: bool,
    pub runtime_dir: PathBuf,
    pub runtime_created: bool,
    pub state: &'static str,
    /// Names reserved by adoption, not evidence of a live route or listener.
    pub planned_urls: BTreeMap<String, String>,
    pub capabilities: Capabilities,
    pub caveats: Vec<&'static str>,
}
#[derive(Debug, Serialize)]
pub struct Capabilities {
    pub workspace_registry: bool,
    pub routing: bool,
    pub supervision: bool,
    pub tasks: bool,
    pub sharing: bool,
    pub data_isolation_verified: bool,
}
impl ResolvedContext {
    pub fn new(registry: &Registry, manifest: &Manifest, workspace: Workspace) -> Self {
        let planned_urls = manifest
            .services
            .iter()
            .map(|(name, service)| {
                let host = if name == &manifest.default_service {
                    workspace.hostname.clone()
                } else {
                    format!(
                        "{}.{}",
                        service.subdomain.as_deref().unwrap_or(name),
                        workspace.hostname
                    )
                };
                (name.clone(), format!("https://{host}"))
            })
            .collect();
        Self {
            source_exists: workspace.source.is_dir(),
            runtime_dir: registry.runtime_path(&workspace),
            workspace,
            runtime_created: false,
            state: "registered",
            planned_urls,
            capabilities: Capabilities {
                workspace_registry: true,
                routing: false,
                supervision: false,
                tasks: false,
                sharing: false,
                data_isolation_verified: false,
            },
            caveats: vec![
                "Inspection does not start services or verify planned URLs; use proxy status and status for separate ownership/intent reports",
                "Data isolation requires project bindings; recorded intent is not live health. Automatic packaging and network sharing remain unsupported",
            ],
        }
    }
}
