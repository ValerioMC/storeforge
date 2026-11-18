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
