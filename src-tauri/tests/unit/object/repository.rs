use super::*;

#[test]
fn copy_source_keeps_literal_path_segments_and_version() {
    let object = ObjectId {
        bucket: "reports".into(),
        key: "/ Q4//#budget +.csv".into(),
        version_id: Some("abc/+=".into()),
    };
    assert_eq!(
        copy_source(&object),
        "reports//%20Q4//%23budget%20%2B.csv?versionId=abc%2F%2B%3D"
    );
}
