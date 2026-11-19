use super::*;

#[test]
fn rgw_mutations_require_the_ceph_provider_and_confirmation() {
    let service = AdministrationService::new().unwrap();
    let operation = service
        .operations
        .iter()
        .find(|operation| operation.label == "Repair bucket index")
        .unwrap();
    let values = BTreeMap::from([("bucket".into(), "archive".into())]);
    assert!(validate_request(operation, values.clone(), false, Provider::Ceph).is_err());
    assert!(validate_request(operation, values.clone(), true, Provider::Aws).is_err());
    assert_eq!(
        validate_request(operation, values.clone(), true, Provider::Ceph).unwrap(),
        values
    );
}

#[test]
fn iam_roles_topics_preserves_an_explicit_empty_sns_attribute() {
    let service = AdministrationService::new().unwrap();
    let operation = service
        .operation("notifications-update-topic-attribute")
        .unwrap();
    let values = BTreeMap::from([
        (
            "TopicArn".into(),
            "arn:aws:sns:us-east-1:123456789012:events".into(),
        ),
        ("AttributeName".into(), "DisplayName".into()),
        ("AttributeValue".into(), String::new()),
    ]);
    let accepted = validate_request(operation, values.clone(), true, Provider::Aws).unwrap();
    assert_eq!(accepted, values);
    let mut omitted = values;
    omitted.remove("AttributeValue");
    assert!(validate_request(operation, omitted, true, Provider::Aws).is_err());
}
