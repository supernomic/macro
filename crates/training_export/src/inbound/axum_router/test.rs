use super::{RunExportRequest, default_sharing};
use crate::domain::model::{Projection, SharingMode};

#[test]
fn default_sharing_is_disabled() {
    assert_eq!(default_sharing(), SharingMode::Disabled);
}

#[test]
fn omitted_sharing_mode_deserializes_as_disabled() {
    let body: RunExportRequest =
        serde_json::from_str(r#"{"projection":"training_export"}"#).expect("request json");
    assert_eq!(body.projection, Projection::TrainingExport);
    assert_eq!(body.sharing_mode, SharingMode::Disabled);
}

#[test]
fn explicit_full_sharing_mode_is_preserved() {
    let body: RunExportRequest =
        serde_json::from_str(r#"{"projection":"training_export","sharing_mode":"full"}"#)
            .expect("request json");
    assert_eq!(body.sharing_mode, SharingMode::Full);
}
