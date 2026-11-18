use super::*;

#[test]
fn compares_relative_keys_without_treating_multipart_etags_as_content_hashes() {
    let row = |key: &str, size, etag: &str| ObjectRow {
        key: key.into(),
        size: Some(size),
        etag: Some(etag.into()),
        folder: false,
        modified: None,
        storage_class: None,
    };
    let source = [
        row("src/a.txt", 12, "abc"),
        row("src/b.txt", 10, "ebf-2"),
        row("src/c.txt", 5, "def"),
    ];
    let target = [
        row("dst/a.txt", 12, "abc"),
        row("dst/b.txt", 10, "fgh-3"),
        row("dst/d.txt", 5, "def"),
    ];
    let report = compare_rows(&source, "src/", &target, "dst/");
    assert_eq!(
        (
            report.matching,
            report.different_etag,
            report.source_only,
            report.target_only
        ),
        (1, 1, 1, 1)
    );
}
