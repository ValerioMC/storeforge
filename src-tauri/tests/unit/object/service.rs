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

fn object_drawer_client(endpoint: &str) -> Client {
    let config = aws_sdk_s3::Config::builder()
        .behavior_version_latest()
        .credentials_provider(aws_sdk_s3::config::Credentials::new(
            "fixture-key",
            "fixture-secret",
            None,
            None,
            "fixture",
        ))
        .region(aws_sdk_s3::config::Region::new("us-east-1"))
        .endpoint_url(endpoint)
        .force_path_style(true)
        .retry_config(aws_sdk_s3::config::retry::RetryConfig::disabled())
        .build();
    Client::from_conf(config)
}

fn object_drawer_mock(
    responses: Vec<(&'static str, String, Vec<u8>)>,
) -> (String, std::thread::JoinHandle<Vec<String>>) {
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        thread,
        time::{Duration, Instant},
    };

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let requests = thread::spawn(move || {
        responses
            .into_iter()
            .map(|(status, headers, body)| {
                let deadline = Instant::now() + Duration::from_secs(4);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            thread::sleep(Duration::from_millis(3))
                        }
                        Err(error) => panic!("Missing mock request: {error}"),
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = String::new();
                let mut reader = BufReader::new(&mut stream);
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                        break;
                    }
                    request.push_str(&line);
                }
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\n{headers}Connection: close\r\n\r\n"
                )
                .unwrap();
                stream.write_all(&body).unwrap();
                request
            })
            .collect()
    });
    (endpoint, requests)
}

#[tokio::test]
async fn object_drawer_requests_a_bounded_range_for_the_exact_historical_text_object() {
    let (endpoint, worker) = object_drawer_mock(vec![
        ("200 OK", "Content-Length: 72000\r\nContent-Type: text/csv; charset=utf-8\r\nETag: \"fixture\"\r\n".into(), vec![]),
        ("206 Partial Content", "Content-Length: 65536\r\nContent-Range: bytes 0-65535/72000\r\n".into(), vec![b'x'; 65536]),
    ]);
    let object = ObjectId {
        bucket: "archive".into(),
        key: "logs// / report.csv ".into(),
        version_id: Some("version /+".into()),
    };
    let preview = ObjectService::preview(&object_drawer_client(&endpoint), &object)
        .await
        .unwrap();
    let requests = worker.join().unwrap();
    assert_eq!(preview.content_type, "text/csv");
    assert!(preview.truncated);
    assert!(!preview.oversized);
    assert_eq!(STANDARD.decode(preview.data).unwrap().len(), 65536);
    assert!(
        requests
            .iter()
            .all(|request| request.contains("versionId=version%20%2F%2B"))
    );
    assert!(
        requests
            .iter()
            .all(|request| request.contains("/archive/logs//%20/%20report.csv%20"))
    );
    assert!(
        requests[1]
            .to_ascii_lowercase()
            .contains("range: bytes=0-65535")
    );
    assert!(
        requests[1]
            .to_ascii_lowercase()
            .contains("if-match: \"fixture\"")
    );
}

#[tokio::test]
async fn object_drawer_checks_object_and_media_bounds_before_exposing_payloads() {
    let (endpoint, worker) = object_drawer_mock(vec![
        (
            "200 OK",
            "Content-Length: 52428801\r\nContent-Type: video/mp4\r\n".into(),
            vec![],
        ),
        (
            "200 OK",
            "Content-Length: 4\r\nContent-Type: application/octet-stream\r\n".into(),
            vec![],
        ),
        ("200 OK", "Content-Length: 4\r\n".into(), b"%PDF".to_vec()),
    ]);
    let client = object_drawer_client(&endpoint);
    let mut object = ObjectId {
        bucket: "archive".into(),
        key: "large.mp4".into(),
        version_id: None,
    };
    let large = ObjectService::preview(&client, &object).await.unwrap();
    assert!(large.oversized);
    assert!(large.data.is_empty());
    object.key = "first.pdf".into();
    let pdf = ObjectService::preview(&client, &object).await.unwrap();
    assert_eq!(pdf.content_type, "application/pdf");
    assert!(matches!(pdf.kind, PreviewKind::Pdf));
    assert_eq!(STANDARD.decode(pdf.data).unwrap(), b"%PDF");
    let requests = worker.join().unwrap();
    assert!(!requests[2].to_ascii_lowercase().contains("range:"));
}

#[tokio::test]
async fn object_drawer_metadata_replacement_sends_content_headers_and_literal_copy_source() {
    let response = b"<CopyObjectResult><ETag>&quot;fixture&quot;</ETag></CopyObjectResult>";
    let (endpoint, worker) = object_drawer_mock(vec![
        (
            "200 OK",
            "Content-Length: 8\r\nETag: \"fixture\"\r\n".into(),
            vec![],
        ),
        (
            "200 OK",
            format!("Content-Length: {}\r\n", response.len()),
            response.to_vec(),
        ),
    ]);
    let object = ObjectId {
        bucket: "archive".into(),
        key: "/logs// / report.csv ".into(),
        version_id: None,
    };
    let update = MetadataUpdate {
        content_type: Some("text/csv".into()),
        cache_control: Some("private".into()),
        content_disposition: Some("attachment".into()),
        content_encoding: Some("gzip".into()),
        content_language: Some("en".into()),
        expires: Some("2026-11-20T12:00:00Z".into()),
        metadata: [("pass".into(), "018".into())].into(),
    };
    ObjectService::change(
        &object_drawer_client(&endpoint),
        &object,
        ObjectChange::SetMetadata { update },
    )
    .await
    .unwrap();
    let requests = worker.join().unwrap();
    let copy = requests[1].to_ascii_lowercase();
    for header in [
        "x-amz-copy-source: archive//logs//%20/%20report.csv%20",
        "x-amz-copy-source-if-match: \"fixture\"",
        "x-amz-meta-pass: 018",
        "content-type: text/csv",
        "cache-control: private",
        "content-disposition: attachment",
        "content-encoding: gzip",
        "content-language: en",
        "expires: fri, 20 nov 2026 12:00:00 gmt",
    ] {
        assert!(copy.contains(header), "Missing header: {header}");
    }
}

#[tokio::test]
async fn object_drawer_restores_the_exact_version_and_returns_the_provider_version() {
    let response = b"<CopyObjectResult><ETag>&quot;fixture&quot;</ETag></CopyObjectResult>";
    let (endpoint, worker) = object_drawer_mock(vec![(
        "200 OK",
        format!(
            "Content-Length: {}\r\nx-amz-version-id: restored +/\r\n",
            response.len()
        ),
        response.to_vec(),
    )]);
    let source = ObjectId {
        bucket: "archive".into(),
        key: "/logs// / report.csv ".into(),
        version_id: Some("historical /+".into()),
    };
    let restored = ObjectService::change_with_version(
        &object_drawer_client(&endpoint),
        &source,
        ObjectChange::RestoreVersion,
    )
    .await
    .unwrap();
    assert_eq!(restored.as_deref(), Some("restored +/"));
    let copy = worker.join().unwrap().pop().unwrap().to_ascii_lowercase();
    assert!(copy.contains(
        "x-amz-copy-source: archive//logs//%20/%20report.csv%20?versionid=historical%20%2f%2b"
    ));
}
