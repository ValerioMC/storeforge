use super::*;

#[test]
fn tags_preserve_spaces_but_reject_duplicate_and_empty_keys() {
    let tag = |key: &str| Tag {
        key: key.into(),
        value: " value ".into(),
    };
    assert!(validate_tags(&[tag(" "), tag("  ")]).is_ok());
    assert!(validate_tags(&[tag(" "), tag(" ")]).is_err());
    assert!(validate_tags(&[tag("")]).is_err());
}
