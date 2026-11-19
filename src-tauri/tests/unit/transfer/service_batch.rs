use super::*;
use crate::object::service::tests::{object_drawer_client, object_drawer_mock};

#[tokio::test]
async fn browser_batch_partial_move_keeps_an_unverified_source() {
    let (source_endpoint, source_worker) = object_drawer_mock(vec![
        (
            "200 OK",
            "Content-Length: 1\r\nETag: \"one\"\r\n".into(),
            vec![],
        ),
        (
            "200 OK",
            "Content-Length: 1\r\nETag: \"one\"\r\n".into(),
            vec![b'x'],
        ),
        (
            "200 OK",
            "Content-Length: 1\r\nETag: \"one\"\r\n".into(),
            vec![],
        ),
        ("204 No Content", "Content-Length: 0\r\n".into(), vec![]),
        (
            "200 OK",
            "Content-Length: 1\r\nETag: \"two\"\r\n".into(),
            vec![],
        ),
        (
            "200 OK",
            "Content-Length: 1\r\nETag: \"two\"\r\n".into(),
            vec![b'y'],
        ),
    ]);
    let (target_endpoint, target_worker) = object_drawer_mock(vec![
        ("404 Not Found", "Content-Length: 0\r\n".into(), vec![]),
        (
            "200 OK",
            "Content-Length: 0\r\nETag: \"copied-one\"\r\n".into(),
            vec![],
        ),
        (
            "200 OK",
            "Content-Length: 1\r\nETag: \"copied-one\"\r\n".into(),
            vec![],
        ),
        ("404 Not Found", "Content-Length: 0\r\n".into(), vec![]),
        (
            "200 OK",
            "Content-Length: 0\r\nETag: \"copied-two\"\r\n".into(),
            vec![],
        ),
        (
            "200 OK",
            "Content-Length: 1\r\nETag: \"wrong\"\r\n".into(),
            vec![],
        ),
    ]);
    let source = object_drawer_client(&source_endpoint);
    let target = object_drawer_client(&target_endpoint);
    let plan = CopyPlan {
        source: Location {
            connection: Uuid::new_v4(),
            bucket: "source".into(),
            prefix: "root/".into(),
        },
        target: Location {
            connection: Uuid::new_v4(),
            bucket: "target".into(),
            prefix: "dest/".into(),
        },
        objects: vec![],
        move_source: true,
        rename_to: None,
        overwrite: false,
    };
    let directory = tempfile::tempdir().unwrap();
    let jobs = JobRepository::open(directory.path().into()).unwrap();
    let id = Uuid::new_v4();
    jobs.insert(Job {
        id,
        label: "Move objects".into(),
        status: JobStatus::Running,
        started_at: Utc::now(),
        finished_at: None,
        total: 2,
        succeeded: 0,
        failed: 0,
        failures: vec![],
        successful_items: vec![],
        bytes_processed: 0,
        current_bytes: 0,
        current: None,
        cancel_requested: false,
    })
    .unwrap();
    let progress = Progress { id, jobs: &jobs };
    for key in ["root/ a ", "root/b"] {
        let object = object_id(&plan.source, key.into());
        let destination_key = copy_target(&object, &plan).unwrap();
        let result = copy_item(&source, &target, &object, &plan, destination_key, &progress).await;
        progress.record_bytes(key, result).unwrap();
    }
    progress.finish(Ok(())).unwrap();
    let requests = source_worker.join().unwrap();
    let target_requests = target_worker.join().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.starts_with("DELETE "))
            .count(),
        1
    );
    assert!(requests[3].starts_with("DELETE /source/root/%20a%20"));
    assert!(target_requests[1].starts_with("PUT /target/dest/%20a%20"));
    let result = jobs.list().unwrap();
    assert_eq!(result[0].successful_items, ["root/ a "]);
    assert_eq!(result[0].failures[0].item, "root/b");
    assert_eq!(result[0].bytes_processed, 1);
}
