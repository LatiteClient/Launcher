use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BuildKind {
    #[default]
    Release,
    Nightly,
    Debug,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InjectRequest {
    pub dll_path: Option<String>,
    pub build: Option<BuildKind>,
}
