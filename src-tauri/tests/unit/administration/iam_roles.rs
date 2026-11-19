use super::*;

#[test]
fn iam_roles_topics_preserves_repeated_ceph_tag_keys_and_empty_values() {
    let fields = BTreeMap::from([
        ("RoleName".into(), "reporting".into()),
        (
            "Tags".into(),
            r#"[{"key":"team","value":"a"},{"key":"team","value":""}]"#.into(),
        ),
    ]);
    let batches = role_tag_batches(fields.clone(), Provider::Ceph).unwrap();
    let encoded = &batches[0];
    assert_eq!(encoded["Tags.member.1.Key"], "team");
    assert_eq!(encoded["Tags.member.2.Key"], "team");
    assert_eq!(encoded["Tags.member.1.Value"], "a");
    assert_eq!(encoded["Tags.member.2.Value"], "");
    assert!(!encoded.contains_key("Tags"));
    assert!(role_tag_batches(fields, Provider::Aws).is_err());
}

#[test]
fn iam_roles_topics_refuses_provider_managed_policy_writes() {
    assert!(validate_iam_resource_write("CreatePolicy", &BTreeMap::new(), Provider::Ceph).is_err());
    let fields = BTreeMap::from([(
        "PolicyArn".into(),
        "arn:aws:iam::aws:policy/ReadOnlyAccess".into(),
    )]);
    assert!(
        validate_iam_resource_write("SetDefaultPolicyVersion", &fields, Provider::Aws).is_err()
    );
    assert!(validate_iam_resource_write("CreatePolicy", &BTreeMap::new(), Provider::Aws).is_ok());
}

#[test]
fn iam_roles_topics_batches_ceph_multivalued_tags_without_truncation() {
    let tags: Vec<_> = (0..12)
        .map(|value| serde_json::json!({ "key": "team", "value": value.to_string() }))
        .collect();
    let batches = role_tag_batches(
        BTreeMap::from([
            ("RoleName".into(), "reporting".into()),
            ("Tags".into(), serde_json::to_string(&tags).unwrap()),
        ]),
        Provider::Ceph,
    )
    .unwrap();
    assert_eq!(batches.len(), 2);
    assert_eq!(batches[0]["Tags.member.9.Value"], "8");
    assert_eq!(batches[1]["Tags.member.1.Value"], "9");
    assert_eq!(batches[1]["Tags.member.3.Value"], "11");
}
