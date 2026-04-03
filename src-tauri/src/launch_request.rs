use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InjectRequest {
    pub dll_path: Option<String>,
}
